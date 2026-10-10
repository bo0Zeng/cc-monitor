//! **远端流断线重连之后，旁路快照从续点接着拉，不再从第 0 行整份重拉。**
//!
//! # 病
//!
//! 远端长连接每断一次（弱网上一小时可以断几次），重连后后端重新宣告每个会话，
//! `stream_source` 的快照分发器就对每个会话**从第 0 行起**把整份历史再拉一遍、再灌一遍前端 ——
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
//! 只读 `[锚的字节, end)`，锚到 `next` 之间的行本地数掉、不发。续点对不上（换了路径 · 文件变短）⇒ 只读尾段（[`Read::Tail`]，与冷连上同一形）。
//!
//! 冷连上 / monitor 重启（续点全丢）也只读尾段：头段不预拉，界面往上翻时按行号 / 按偏移取回 ——
//! 原先整份拉回来，壳留存只留每会话最后 `REPLAY_TAIL_KEEP` 条、界面账本只留 2000 条，其余当场丢、往上翻时再要一遍（同一段走两趟）。
//!
//! ⚠ **不用骨架索引**（当时的 `read_session_index`〔散文墓碑〕）：它那时不在帧面（`frame_query::STILL_DIALED` 那一形）⇒
//! 用它续传等于每次重连多拨一条 SSH，与 `C1`「去掉逐次拨号」方向相反。帧面的 `history-tail` ＋
//! `history-read` 已经是「按偏移」—— 续传要的两样（行号 ↔ 字节的锚 · 按字节区间读）都在。
//! 设计住。
//!
//! # 截断 / 改写检测：续传之前先核一行
//!
//! 「**截断检测**（远端 jsonl 在断连期间被截断/分叉，`(sid,seq)` 会指向不同的行而没有东西会发现）—— **仍开**（W5-VIS）」。
//! 上面那条「文件比锚短 ⇒ 从尾段重来」只接住了**变短**；断线期间被**整份改写而且变长**（编辑器存盘整份覆盖、agent 按路径重建）的文件，
//! 续传照接，前端已有的那几行与盘上不再是同一内容。
//! ⇒ 续点旁边多记一格**见证**（[`Witness`]）：快照做完那一刻文件最后一个可计行的字节区间与内容摘要。续传之前先读回那一行
//! （一次 `history-read`，一行大小）比一下（[`witness_holds`]）；对不上 ⇒ 当被改写：续点作废、**从尾段重读**，并交那个会话一格
//! 「记录文件被改过、已从头重读」（与 FW1 后端那一形同一句话、同一条前端路：`FileChange::Rewritten`）。
//! 与 FW1「游标旁记末尾若干字节，续读前核」同形；它管后端实时读，本处管 monitor 的断线续传。
//!
//! # 买不到
//!
//! - 见证只钉**锚那一行**：锚之后、续点之前那几条实时行被单独改掉（前缀原样、只改中段）看不见 —— 实时行没带字节偏移。
//! - 见证那一行的字节位与摘要都由后端按原始字节给（原先 monitor 拿有损解码过的正文自己切、自己算，
//!   碰到非 UTF-8 字节那一次就记不了见证）。
//! - 改写之后重读出来的行，行号与旧行同号（远端 `seq` 是行号空间，`INVARIANTS §25a`）—— 前端怎么把两份收成一份不在本处。
//! - 实时行带着后端的 `byte_offset`（它的末端）⇒ 推续点时记下第 `next` 行的起点（`Cursor::next_byte`），
//!   续传就从那一行读起、一行都不数掉。只剩「推的那一行说不准末端」（快照那一行是没收尾的残尾）
//!   才退回挑锚、锚到续点那一截照样过线。
//! - 进程重启续点全丢（本来就是进程内软状态）⇒ 回到冷连上那一形：只读尾段。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::frame_query::TailPlan;
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
    /// 已有到哪：`[tail_from, next)` 这些行号前端**确实**拿到过（发出去过）；尾段之下的由前端按需取回，不算缺。
    pub(crate) next: u64,
    /// 第 `next` 行从哪个字节起（推 `next` 的那一行带着它的末端）；说不准 ⇒ `None`（退回挑锚）。
    pub(crate) next_byte: Option<u64>,
    /// 锚那一行的见证（续传之前先核它）；`None` = 这一次没记下（那一行是残尾 / 一行可计行都没有）。
    pub(crate) witness: Option<Witness>,
}

/// 见证：一行在文件里的**原始字节区间** `[start, end)`（含它的换行）与它的内容摘要（后端算的）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Witness {
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) hash: u64,
}

/// 读回 `[w.start, w.end)` 那一段（`history-read` 的逐行成品）之后：还是不是那一行。
/// 恰好一行、末端对得上、摘要对得上（摘要由后端按原始字节算、跨进程稳定；原先 monitor 按本进程的哈希算）。
pub(crate) fn witness_holds(w: &Witness, rows: &[crate::frame_query::Row]) -> bool {
    matches!(rows, [r] if r.end == Some(w.end) && r.hash == w.hash)
}

/// 一页的逐行成品 ⇒ 每行在文件里的原始字节区间 `[start, end)`（含换行）。
///
/// 末端由后端按原始字节给（永远说得准）；起点 ＝ 上一个可计行的末端（页的第一行 ＝ `offset`）——
/// 中间夹着的空白行（不是可计行，后端不交）算进这一行的区间：续传前读回这一段时空白行照样不成行，见证照样比得上。
/// 残尾（末端 `None`）那一行及其后说不准 ⇒ `None`。
pub(crate) fn row_spans(offset: u64, rows: &[crate::frame_query::Row]) -> Vec<Option<(u64, u64)>> {
    let mut start = Some(offset);
    rows.iter()
        .map(|r| {
            let span = start.zip(r.end);
            start = r.end;
            span
        })
        .collect()
}

/// 走读时挑见证：**区间末端是 `plan.end` 的那一段**里、最后一个可计行（只读尾段时是尾段，续传时是续读那一段）。
/// 那一行的区间说不准 ⇒ 这一次不记。
#[derive(Debug, Default)]
pub(crate) struct WitnessPick {
    last: Option<Option<Witness>>,
}

impl WitnessPick {
    /// 一个可计行：它所在那一段的末端 `seg_upto`、这张图的末端 `plan_end`、正文摘要（后端给的）与区间。
    pub(crate) fn see(
        &mut self,
        seg_upto: u64,
        plan_end: u64,
        hash: u64,
        span: Option<(u64, u64)>,
    ) {
        if seg_upto == plan_end {
            self.last = Some(span.map(|(start, end)| Witness { start, end, hash }));
        }
    }

    /// 这一趟的结论：`None` = 末端那一段里一个可计行都没读到（锚那一行没变 ⇒ 旧见证照留）；
    /// `Some(None)` = 读到了但区间说不准（这一次不记，续传时不核）；`Some(Some(w))` = 新见证。
    pub(crate) fn done(self) -> Option<Option<Witness>> {
        self.last
    }
}

/// 这一次快照怎么读。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Read {
    /// 只读尾段 `[split_at, end)`（第 `tail_from` 行起）：连上、monitor 重启、续点作废都是这一形。
    /// 头段 `[0, split_at)` 不预拉 —— 界面往上翻时按行号 / 按偏移取回（`live-window.ts` 的 `BelowState`）。
    Tail,
    /// 续传：只读 `[from_byte, upto)`；那一段第一行的行号是 `first_seq`，行号 `< skip_below` 的数掉不发。
    Resume {
        from_byte: u64,
        upto: u64,
        first_seq: u64,
        skip_below: u64,
    },
}

/// 同一份文件的续点比这一次的图还长（行数 / 字节 / 已有到哪 任一越过）⇒ 文件在断线期间变短了。
pub(crate) fn shrank(cursor: Option<&Cursor>, path: &str, plan: &TailPlan) -> bool {
    cursor.is_some_and(|c| {
        c.path == path
            && (c.anchor_total > plan.total || c.anchor_end > plan.end || c.next > plan.total)
    })
}

/// **纯函数**：续点 × 这一次的尾段图 ⇒ 怎么读。
///
/// 续点缺席 / 路径不同 / 文件比锚短（被截断重写过）/ `next` 超过这次的总行数 ⇒ [`Read::Tail`]（只读尾段）。
pub(crate) fn plan_read(cursor: Option<&Cursor>, path: &str, plan: &TailPlan) -> Read {
    let Some(c) = cursor.filter(|c| c.path == path) else {
        return Read::Tail;
    };
    if shrank(Some(c), path, plan) {
        return Read::Tail;
    }
    // 确知第 `next` 行的起点 ⇒ 就从那里读：锚到续点之间那一截不再过线。
    if let Some(b) = c.next_byte.filter(|b| *b <= plan.end) {
        return Read::Resume {
            from_byte: b,
            upto: plan.end,
            first_seq: c.next,
            skip_below: c.next,
        };
    }
    // 两个确知的「行号 → 字节」锚，挑行号不超过 `next` 的最近一个。
    let mine = (c.anchor_total, c.anchor_end);
    let theirs = (plan.tail_from, plan.split_at);
    let pick = [mine, theirs]
        .into_iter()
        .filter(|(seq, _)| *seq <= c.next)
        .max_by_key(|(seq, _)| *seq);
    let Some((first_seq, from_byte)) = pick else {
        return Read::Tail;
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
    /// 那一段第一行的行号（行号 = 它 ＋ 到达序）：只读尾段时是 `tail_from`，续传时是挑中的锚。
    first_seq: u64,
    skip_below: u64,
    total: u64,
    arrived: u64,
}

impl Walk {
    pub(crate) fn new(how: &Read, plan: &TailPlan) -> Self {
        let (segments, first_seq, skip_below) = match *how {
            Read::Tail => (
                vec![(plan.split_at, plan.end)],
                plan.tail_from,
                plan.tail_from,
            ),
            Read::Resume {
                from_byte,
                upto,
                first_seq,
                skip_below,
            } => (vec![(from_byte, upto)], first_seq, skip_below),
        };
        Walk {
            segments,
            first_seq,
            skip_below,
            total: plan.total,
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
        let seq = self.first_seq + k;
        (seq >= self.skip_below).then_some(seq)
    }

    /// 到此为止数过的可计行（发的 ＋ 数掉的）。
    pub(crate) fn arrived(&self) -> u64 {
        self.arrived
    }

    /// 这次应数到的可计行数：那一段第一行到 `total`（只读尾段 = 尾段几行；续传 = 锚之后那一截）。
    pub(crate) fn want(&self) -> u64 {
        self.total - self.first_seq
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

/// 一次快照**完整**做完：`[tail_from, plan.total)` 全到了（尾段之下的前端按需取回，不算缺）⇒ 立锚。`next` 取「原有的」与 `total` 的大者 ——
/// 原有的只会是连续推上去的，不会越过真没拿到的行（见 [`note_flushed`]）。
pub(crate) fn note_snapshot_done(origin: &Origin, sid: &str, path: &str, plan: &TailPlan) {
    let mut g = registry().lock().unwrap_or_else(|e| e.into_inner());
    let key = (origin.as_wire_str().to_string(), sid.to_string());
    let same = g.get(&key).filter(|c| c.path == path);
    let next = same.map_or(plan.total, |c| c.next.max(plan.total));
    // `next` 落在锚上 ⇒ 起点就是锚的末字节；推得比锚远 ⇒ 沿用推的那一行带来的。
    let next_byte = match same {
        Some(c) if c.next > plan.total => c.next_byte,
        _ => Some(plan.end),
    };
    // 见证先照旧带过来（这一趟没读到新的可计行时它仍是锚那一行）；[`note_witness`] 随后按这一趟的走读改。
    let witness = same.and_then(|c| c.witness.clone());
    g.insert(
        key,
        Cursor {
            path: path.to_string(),
            anchor_total: plan.total,
            anchor_end: plan.end,
            next,
            next_byte,
            witness,
        },
    );
}

/// 立锚之后按这一趟的走读改见证（[`WitnessPick::done`] 的三形：`None` 照留 · `Some(None)` 清掉 · `Some(Some)` 换新）。
pub(crate) fn note_witness(origin: &Origin, sid: &str, picked: Option<Option<Witness>>) {
    let Some(witness) = picked else {
        return;
    };
    if let Some(c) = registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(&(origin.as_wire_str().to_string(), sid.to_string()))
    {
        c.witness = witness;
    }
}

/// 一批行发给前端之后：有续点的会话，行号**恰好接上** `next` 才往前推（连续才推）；
/// 推的那一行带着它的末端 ⇒ 记成新 `next` 的起点（不带 ⇒ 说不准）。
pub(crate) fn note_flushed<'a>(
    origin: &Origin,
    flushed: impl IntoIterator<Item = (&'a str, u64, Option<u64>)>,
) {
    let mut g = registry().lock().unwrap_or_else(|e| e.into_inner());
    for (sid, seq, end) in flushed {
        if let Some(c) = g.get_mut(&(origin.as_wire_str().to_string(), sid.to_string())) {
            if seq == c.next {
                c.next += 1;
                c.next_byte = end;
            }
        }
    }
}

/// 会话真结束（后端报 `session_removed`）⇒ 续点作废：再宣告时只读尾段（与冷连上同）。
pub(crate) fn forget(origin: &Origin, sid: &str) {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&(origin.as_wire_str().to_string(), sid.to_string()));
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/snapshot_resume_tests.rs"]
mod tests;
