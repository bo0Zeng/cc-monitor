//! **一台的活卡板**：那台后端拼好的活卡（`live_card` 帧：开 · 加块 · 加字 · 改相 · 撤）折成「某会话某运行此刻有哪几张、各是什么样」的整份 ——
//! 桌面壳（`event_replay`，每台一块）与手机核心（`mobcore::resident`，每台一块）都只调这里，界面 / Kotlin 照整份画、不认 `op`。
//!
//! 对账 · 缺口 · 截头 · 挤谁全在后端（`src/backend/stream/live.rs`）。唯一一条规矩：一张卡的 `n` 不接上一帧 ⇒ 这张先不画，
//! 等那台下一张整卡（`open`）或撤卡（`close`）—— 那台在发件箱满过时自己补整卡，这边从不自己补缺口。

use std::collections::BTreeMap;

use cmd_wire::live::{BlockKind, LiveBlock, LiveCard, LiveOp, LivePhase};
use serde::{Deserialize, Serialize};

/// 一张活卡此刻的样子。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct LiveCardNow {
    /// 卡号（那台整台唯一）。
    #[cfg_attr(test, ts(type = "number"))]
    pub card: u64,
    /// 对账键（与那一轮记录的 `rid` 同值）。
    pub rid: String,
    pub phase: LivePhase,
    /// 正文截过头（只留了尾巴）。
    pub clipped: bool,
    /// 各块，按块号。
    pub blocks: Vec<LiveBlock>,
}

/// 某会话某运行此刻的活卡（整份，先来的在前）：交给界面 / Kotlin 的就是它；空 ⇒ 那里此刻没有卡。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct LiveCards {
    pub sid: String,
    /// 归哪个子运行；缺 ＝ 主运行。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub run: Option<String>,
    pub cards: Vec<LiveCardNow>,
}

/// 哪个会话的哪个运行（`None` ＝ 主运行）。
pub type Where = (String, Option<String>);

#[derive(Debug, Clone)]
struct Card {
    sid: String,
    run: Option<String>,
    /// 下一帧该是第几号。
    next: u64,
    now: LiveCardNow,
    born: u64,
}

/// 一台的活卡板（见模块头注）。
#[derive(Debug, Default)]
pub struct Board {
    cards: BTreeMap<u64, Card>,
    born: u64,
}

impl Board {
    /// **一帧一步**（全仓唯一的折法）。变了 ⇒ 哪个会话哪个运行（照 [`Self::cards_of`] 交那一份整份）；没变 ⇒ `None`。
    pub fn apply(&mut self, f: &LiveCard) -> Option<Where> {
        let at = (f.sid.clone(), f.run.clone());
        if let LiveOp::Open {
            rid,
            phase,
            clipped,
            blocks,
        } = &f.op
        {
            let born = match self.cards.get(&f.card) {
                Some(c) => c.born,
                None => {
                    self.born += 1;
                    self.born
                }
            };
            let now = LiveCardNow {
                card: f.card,
                rid: rid.clone(),
                phase: *phase,
                clipped: *clipped,
                blocks: blocks.clone(),
            };
            let card = Card {
                sid: f.sid.clone(),
                run: f.run.clone(),
                next: f.n + 1,
                now,
                born,
            };
            self.cards.insert(f.card, card);
            return Some(at);
        }
        let c = self.cards.get_mut(&f.card)?;
        let at = (c.sid.clone(), c.run.clone());
        if f.n != c.next || matches!(f.op, LiveOp::Close { .. }) {
            self.cards.remove(&f.card);
            return Some(at);
        }
        c.next += 1;
        let blocks = &mut c.now.blocks;
        match &f.op {
            LiveOp::Block { i, block, tool } => {
                let i = *i as usize;
                while blocks.len() <= i {
                    blocks.push(LiveBlock {
                        kind: BlockKind::Other,
                        text: None,
                        tool: None,
                    });
                }
                blocks[i] = LiveBlock {
                    kind: *block,
                    text: None,
                    tool: tool.clone(),
                };
            }
            LiveOp::Text { i, s } => {
                if let Some(b) = usize::try_from(*i).ok().and_then(|i| blocks.get_mut(i)) {
                    b.text.get_or_insert_with(String::new).push_str(s);
                }
            }
            LiveOp::Awaiting => c.now.phase = LivePhase::Awaiting,
            LiveOp::Open { .. } | LiveOp::Close { .. } => {}
        }
        Some(at)
    }

    /// 某会话某运行此刻的整份（先来的在前）。
    pub fn cards_of(&self, sid: &str, run: Option<&str>) -> LiveCards {
        let mut mine: Vec<&Card> = self
            .cards
            .values()
            .filter(|c| c.sid == sid && c.run.as_deref() == run)
            .collect();
        mine.sort_by_key(|c| c.born);
        LiveCards {
            sid: sid.to_string(),
            run: run.map(str::to_string),
            cards: mine.into_iter().map(|c| c.now.clone()).collect(),
        }
    }

    /// 这个会话的卡不留了（会话走了 · 关了 tab）。回：哪几处变了（各自交一份空的）。
    pub fn drop_session(&mut self, sid: &str) -> Vec<Where> {
        self.take(|c| c.sid == sid)
    }

    /// 这条连接上的号作废（换了一条流 · 那台看不见了）⇒ 全撤。回：哪几处变了。
    pub fn clear(&mut self) -> Vec<Where> {
        self.take(|_| true)
    }

    fn take(&mut self, hit: impl Fn(&Card) -> bool) -> Vec<Where> {
        let mut touched: Vec<Where> = Vec::new();
        self.cards.retain(|_, c| {
            if !hit(c) {
                return true;
            }
            let at = (c.sid.clone(), c.run.clone());
            if !touched.contains(&at) {
                touched.push(at);
            }
            false
        });
        touched
    }

    /// 此刻在板上的卡数。
    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/live_tests.rs"]
mod tests;
