//! **一台的活卡板**：那台后端拼好的活卡（`live_card` 帧：开 · 加块 · 换尾巴 · 改相 · 撤）折成「某会话某运行此刻有哪几张、各是什么样」的整份 ——
//! 桌面壳（`event_replay`，每台一块）与手机核心（`mobcore::resident`，每台一块）都只调这里，界面 / Kotlin 照折好的画、不认 `op`。
//!
//! 对账 · 缺口 · 截头 · 挤谁全在后端（`src/backend/observe/live.rs`）。唯一一条规矩：一张卡的 `n` 不接上一帧 ⇒ 这张先不画，
//! 等那台下一张整卡（`open`）或撤卡（`close`）—— 那台在发件箱满过时自己补整卡，这边从不自己补缺口。
//!
//! 每折一帧回 [`LiveChange`]：一段字只换了一张卡正文的尾巴 ⇒ [`LiveChange::Tail`]（前 `keep` 块不变 ＋ 新的尾巴，多半一两块）；
//! 别的（开卡 · 加块 · 改相 · 撤卡 · 缺口）⇒ [`LiveChange::Whole`]，那一处整份。

use std::collections::BTreeMap;

use cmd_wire::live::{BlockKind, LiveBlock, LiveCard, LiveOp, LivePhase};
/// 折出来的成品形住契约包（线上的形），这里转出：折法用它、手机核心照旧从这里取。
pub use cmd_wire::live::{LiveCardNow, LiveCards, LiveChange, LiveTail, Where};
use cmd_wire::md::{Inl, MdBlock};
use copy_core::copy_text;

/// 工具块那一句（那台没报工具名 ⇒ `?`）。
fn tool_use(tool: Option<&str>) -> String {
    copy_text("liveCard.block.toolUse", &[("tool", tool.unwrap_or("?"))])
}

/// 一段只有一句字的块。
fn line(s: String) -> MdBlock {
    MdBlock::P { k: vec![Inl::S(s)] }
}

/// 卡里的一块：种类 · 工具名 · 它在正文里占几块。
#[derive(Debug, Clone)]
struct Part {
    kind: BlockKind,
    tool: Option<String>,
    n: usize,
}

#[derive(Debug, Clone)]
struct Card {
    sid: String,
    run: Option<String>,
    /// 下一帧该是第几号。
    next: u64,
    parts: Vec<Part>,
    now: LiveCardNow,
    born: u64,
}

impl Card {
    /// 照块写出正文（整卡那一下）。
    fn fill(&mut self, blocks: &[LiveBlock]) {
        self.parts.clear();
        self.now.body.clear();
        if self.now.clipped {
            self.now
                .body
                .push(line(copy_text("liveCard.body.clipped", &[])));
        }
        for b in blocks {
            let body: Vec<MdBlock> = match b.kind {
                BlockKind::Text => b.md.clone().unwrap_or_default(),
                BlockKind::Thinking => vec![line(copy_text("runs.last.think", &[]))],
                BlockKind::Tool => vec![line(tool_use(b.tool.as_deref()))],
                BlockKind::Other => Vec::new(),
            };
            self.parts.push(Part {
                kind: b.kind,
                tool: b.tool.clone(),
                n: body.len(),
            });
            self.now.body.extend(body);
        }
    }

    /// 第 `i` 块在正文里从第几块起。
    fn offset(&self, i: usize) -> usize {
        usize::from(self.now.clipped) + self.parts[..i].iter().map(|p| p.n).sum::<usize>()
    }

    /// 照此刻的相与最后一块写 [`LiveCardNow::head`] · [`LiveCardNow::last`]。
    fn words(&mut self) {
        self.now.head = match self.now.phase {
            LivePhase::Streaming => copy_text("liveCard.state.streaming", &[]),
            LivePhase::Awaiting => copy_text("liveCard.state.awaitingRecord", &[]),
        };
        self.now.last = match (self.now.phase, self.parts.last()) {
            (LivePhase::Streaming, Some(p)) => match p.kind {
                BlockKind::Text => Some(copy_text("runs.last.say", &[])),
                BlockKind::Thinking => Some(copy_text("runs.last.think", &[])),
                BlockKind::Tool => Some(tool_use(p.tool.as_deref())),
                BlockKind::Other => None,
            },
            _ => None,
        };
    }
}

/// 一台的活卡板（见模块头注）。
#[derive(Debug, Default)]
pub struct Board {
    cards: BTreeMap<u64, Card>,
    born: u64,
}

impl Board {
    /// **一帧一步**（全仓唯一的折法）。变了 ⇒ 哪里怎么变了（[`LiveChange`]）；没变 ⇒ `None`。
    pub fn apply(&mut self, f: &LiveCard) -> Option<LiveChange> {
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
            let mut card = Card {
                sid: f.sid.clone(),
                run: f.run.clone(),
                next: f.n + 1,
                parts: Vec::new(),
                now: LiveCardNow {
                    card: f.card,
                    rid: rid.clone(),
                    phase: *phase,
                    clipped: *clipped,
                    head: String::new(),
                    body: Vec::new(),
                    last: None,
                },
                born,
            };
            card.fill(blocks);
            card.words();
            let at = (card.sid.clone(), card.run.clone());
            self.cards.insert(f.card, card);
            return Some(self.whole(at));
        }
        let c = self.cards.get_mut(&f.card)?;
        let at = (c.sid.clone(), c.run.clone());
        if f.n != c.next || matches!(f.op, LiveOp::Close { .. }) {
            self.cards.remove(&f.card);
            return Some(self.whole(at));
        }
        c.next += 1;
        match &f.op {
            LiveOp::Block { i, block, tool } => {
                let i = *i as usize;
                let mut blocks: Vec<LiveBlock> = Vec::new();
                for (k, p) in c.parts.iter().enumerate() {
                    let from = c.offset(k);
                    blocks.push(LiveBlock {
                        kind: p.kind,
                        md: (p.kind == BlockKind::Text)
                            .then(|| c.now.body[from..from + p.n].to_vec()),
                        tool: p.tool.clone(),
                    });
                }
                while blocks.len() <= i {
                    blocks.push(LiveBlock {
                        kind: BlockKind::Other,
                        md: None,
                        tool: None,
                    });
                }
                blocks[i] = LiveBlock {
                    kind: *block,
                    md: None,
                    tool: tool.clone(),
                };
                c.fill(&blocks);
            }
            LiveOp::Text { i, keep, tail } => {
                let Some(i) = usize::try_from(*i).ok().filter(|i| *i < c.parts.len()) else {
                    return None;
                };
                let keep = (*keep as usize).min(c.parts[i].n);
                let from = c.offset(i) + keep;
                let rest = c.now.body.split_off(from + (c.parts[i].n - keep));
                c.now.body.truncate(from);
                c.now.body.extend(tail.iter().cloned());
                c.now.body.extend(rest);
                c.parts[i].n = keep + tail.len();
                c.words();
                return Some(LiveChange::Tail {
                    tail: LiveTail {
                        sid: at.0,
                        run: at.1,
                        card: f.card,
                        keep: from as u64,
                        blocks: c.now.body[from..].to_vec(),
                        last: c.now.last.clone(),
                    },
                });
            }
            LiveOp::Awaiting => c.now.phase = LivePhase::Awaiting,
            LiveOp::Open { .. } | LiveOp::Close { .. } => {}
        }
        c.words();
        Some(self.whole(at))
    }

    fn whole(&self, at: Where) -> LiveChange {
        LiveChange::Whole {
            whole: self.cards_of(&at.0, at.1.as_deref()),
        }
    }

    /// **折一帧，回要推给只收成品那一头的那一格**（变了的那一处 ＋ 那一处此刻整份；没变 ⇒ `None`）：本机后端替各台
    /// （`control/feeds.rs`）、替本机那条要成品的连接（`stream/tap.rs`）都只调它。
    pub fn step(&mut self, f: &LiveCard) -> Option<cmd_wire::frame::LiveNow> {
        let change = self.apply(f)?;
        let at = change.at();
        let whole = self.cards_of(&at.0, at.1.as_deref());
        Some(cmd_wire::frame::LiveNow { change, whole })
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
#[path = "../../../../tests/common/face-core/live_tests.rs"]
mod tests;
