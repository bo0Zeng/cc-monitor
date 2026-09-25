//! 〔C2 · U3 第 3 件〕**远端流断线重连之后，旁路快照从续点接着拉，不再从第 0 行整份重拉。**
//!
//! # 病
//!
//! 远端长连接每断一次（弱网上一小时可以断几次），重连后后端重新宣告每个会话，
//! `ssh_source` 的快照分发器就对每个会话**从第 0 行起**把整份历史再拉一遍、再灌一遍前端 ——
//! 前端按 `(sid, seq)` 去重把它们全扔掉。一份 270 MB 的会话 = 每次重连 270 MB 白跑。
//!
//! # 形状
//!
//! 本进程按 `(origin, sid)` 记一个**续点**（[`Cursor`]）：
//! - **锚**：某次快照做完时那张图的 `(total, end)` —— 「第 `total` 行从字节 `end` 起」这件事**确知**；
//! - **已有到哪**（`next`）：锚之后实时行**连续**到达时往前推；不连续就不推（宁可少记，不许多记 ——
//!   多记 = 续传时跳过一段真没拿到的行，而那一段从此只能靠骨架按需取）。
//!
//! 重连后快照照旧先问 `history-tail`（它本来就要问）。[`plan_read`] 从两个已知锚里
//! （续点那份 `(total, end)` · 这一次的 `(tail_from, split_at)`）挑**行号不超过 `next`** 的最近一个，
//! 只读 `[锚的字节, end)`，锚到 `next` 之间的行本地数掉、不发。续点对不上（换了路径 · 文件变短）⇒ 整份。
//!
//! ⚠ **不用骨架索引**（当时的 `read_session_index`〔散文墓碑〕）：它那时不在帧面（`frame_query::STILL_DIALED` 那一形）⇒
//! 用它续传等于每次重连多拨一条 SSH，与 `C1`「去掉逐次拨号」方向相反。帧面的 `history-tail` ＋
//! `history-read` 已经是「按偏移」—— 续传要的两样（行号 ↔ 字节的锚 · 按字节区间读）都在。
//! 设计住 `设计/05 §13.6 ①`。
//!
//! # 买不到
//!
//! - 锚到 `next` 之间那一截**照样过线**（只是不发给前端）：锚只有「快照做完那一刻」与「这次的尾段起点」两个，
//!   实时行没带字节偏移。最坏多读「上次快照之后的实时行」那么多字节 —— 与整份重拉比小几个数量级，但不是零。
//! - 进程重启续点全丢（本来就是进程内软状态）。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::backend::control::frame_query::TailPlan;
use crate::origin::Origin;

/// 某台某会话的续点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cursor {
    /// 会话文件路径（换了路径 ⇒ 续点作废）。
    pub(crate) path: String,
    /// 锚：快照做完时的可计行数 …
    pub(crate) anchor_total: u64,
    /// … 与那一刻最后一个完整行的末字节（= 第 `anchor_total` 行的起点）。
    pub(crate) anchor_end: u64,
    /// 已有到哪：`[0, next)` 这些行号前端**确实**拿到过（发出去过）。
    pub(crate) next: u64,
}

/// 这一次快照怎么读。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Read {
    /// 整份：尾段 `[split_at, end)` 先到、头段 `[0, split_at)` 回填（今天的形状）。
    Full,
    /// 续传：只读 `[from_byte, upto)`；那一段第一行的行号是 `first_seq`，行号 `< skip_below` 的数掉不发。
    Resume {
        from_byte: u64,
        upto: u64,
        first_seq: u64,
        skip_below: u64,
    },
}

/// **纯函数**：续点 × 这一次的尾段图 ⇒ 怎么读。
///
/// 续点缺席 / 路径不同 / 文件比锚短（被截断重写过）/ `next` 超过这次的总行数 ⇒ [`Read::Full`]。
pub(crate) fn plan_read(cursor: Option<&Cursor>, path: &str, plan: &TailPlan) -> Read {
    let Some(c) = cursor.filter(|c| c.path == path) else {
        return Read::Full;
    };
    if c.anchor_total > plan.total || c.anchor_end > plan.end || c.next > plan.total {
        return Read::Full;
    }
    // 两个确知的「行号 → 字节」锚，挑行号不超过 `next` 的最近一个。
    let mine = (c.anchor_total, c.anchor_end);
    let theirs = (plan.tail_from, plan.split_at);
    let pick = [mine, theirs]
        .into_iter()
        .filter(|(seq, _)| *seq <= c.next)
        .max_by_key(|(seq, _)| *seq);
    let Some((first_seq, from_byte)) = pick else {
        return Read::Full;
    };
    Read::Resume {
        from_byte,
        upto: plan.end,
        first_seq,
        skip_below: c.next,
    }
}

/// 一次快照的**走法**：读哪几段、第 k 个到达的可计行是几号、发不发。`fetch_snapshot` 与判据共用这一份。
#[derive(Debug, Clone)]
pub(crate) struct Walk {
    segments: Vec<(u64, u64)>,
    /// `None` = 整份（行号走 `tail_seq` 那条两段映射）；`Some(b)` = 续传（行号 = b + 到达序）。
    first_seq: Option<u64>,
    skip_below: u64,
    total: u64,
    tail_from: u64,
    arrived: u64,
}

impl Walk {
    pub(crate) fn new(how: &Read, plan: &TailPlan) -> Self {
        let (segments, first_seq, skip_below) = match *how {
            // 尾段先到（最新 N 行先就位），头段回填。
            Read::Full => (vec![(plan.split_at, plan.end), (0, plan.split_at)], None, 0),
            Read::Resume {
                from_byte,
                upto,
                first_seq,
                skip_below,
            } => (vec![(from_byte, upto)], Some(first_seq), skip_below),
        };
        Walk {
            segments,
            first_seq,
            skip_below,
            total: plan.total,
            tail_from: plan.tail_from,
            arrived: 0,
        }
    }

    /// 按顺序要读的字节区间 `[from, upto)`。
    pub(crate) fn segments(&self) -> &[(u64, u64)] {
        &self.segments
    }

    /// 读到下一个**可计行**：回它的行号 —— `None` 表示这一行前端已有、数掉不发。
    pub(crate) fn step(&mut self) -> Option<u64> {
        let k = self.arrived;
        self.arrived += 1;
        let seq = match self.first_seq {
            None => crate::ssh_source::tail_seq(k, self.total, self.tail_from),
            Some(base) => base + k,
        };
        (seq >= self.skip_below).then_some(seq)
    }

    /// 到此为止数过的可计行（发的 ＋ 数掉的）。
    pub(crate) fn arrived(&self) -> u64 {
        self.arrived
    }

    /// 这次应数到的可计行数：整份 = `total`；续传 = 锚之后那一截。
    pub(crate) fn want(&self) -> u64 {
        self.total - self.first_seq.unwrap_or(0)
    }
}

fn registry() -> &'static Mutex<HashMap<(String, String), Cursor>> {
    static R: OnceLock<Mutex<HashMap<(String, String), Cursor>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// 取某台某会话的续点（快照开拉之前问）。
pub(crate) fn cursor_of(origin: &Origin, sid: &str) -> Option<Cursor> {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(origin.as_wire_str().to_string(), sid.to_string()))
        .cloned()
}

/// 一次快照**完整**做完：`[0, plan.total)` 全到了 ⇒ 立锚。`next` 取「原有的」与 `total` 的大者 ——
/// 原有的只会是连续推上去的，不会越过真没拿到的行（见 [`note_flushed`]）。
pub(crate) fn note_snapshot_done(origin: &Origin, sid: &str, path: &str, plan: &TailPlan) {
    let mut g = registry().lock().unwrap_or_else(|e| e.into_inner());
    let key = (origin.as_wire_str().to_string(), sid.to_string());
    let next = g
        .get(&key)
        .filter(|c| c.path == path)
        .map_or(plan.total, |c| c.next.max(plan.total));
    g.insert(
        key,
        Cursor {
            path: path.to_string(),
            anchor_total: plan.total,
            anchor_end: plan.end,
            next,
        },
    );
}

/// 一批行发给前端之后：有续点的会话，行号**恰好接上** `next` 才往前推（连续才推）。
pub(crate) fn note_flushed<'a>(origin: &Origin, flushed: impl IntoIterator<Item = (&'a str, u64)>) {
    let mut g = registry().lock().unwrap_or_else(|e| e.into_inner());
    for (sid, seq) in flushed {
        if let Some(c) = g.get_mut(&(origin.as_wire_str().to_string(), sid.to_string())) {
            if seq == c.next {
                c.next += 1;
            }
        }
    }
}

/// 会话真结束（后端报 `session_removed`）⇒ 续点作废：再宣告时整份拉（与今天同）。
pub(crate) fn forget(origin: &Origin, sid: &str) {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&(origin.as_wire_str().to_string(), sid.to_string()));
}

#[cfg(test)]
#[path = "../../../tests/bridge/snapshot_resume_tests.rs"]
mod tests;
