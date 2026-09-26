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
//! # 〔W5-VIS · `设计/15 §3.4 ②`〕截断 / 改写检测：续传之前先核一行
//!
//! `15 §3.4 ②` 逐字：「**截断检测**（远端 jsonl 在断连期间被截断/分叉，`(sid,seq)` 会指向不同的行而没有东西会发现）—— **仍开**（W5-VIS）」。
//! 上面那条「文件比锚短 ⇒ 整份」只接住了**变短**；断线期间被**整份改写而且变长**（编辑器存盘整份覆盖、agent 按路径重建）的文件，
//! 续传照接，前端已有的那几行与盘上不再是同一内容。
//! ⇒ 续点旁边多记一格**见证**（[`Witness`]）：快照做完那一刻文件最后一个可计行的字节区间与内容摘要。续传之前先读回那一行
//! （一次 `history-read`，一行大小）比一下（[`witness_holds`]）；对不上 ⇒ 当被改写：续点作废、**整份重读**，并交那个会话一格
//! 「记录文件被改过、已从头重读」（与 FW1 后端那一形同一句话、同一条前端路：`FileChange::Rewritten`）。
//! 与 FW1「游标旁记末尾若干字节，续读前核」（主会话 09-25 认可）同形；它管后端实时读，本处管 monitor 的断线续传。
//!
//! # 买不到
//!
//! - 见证只钉**锚那一行**：锚之后、续点之前那几条实时行被单独改掉（前缀原样、只改中段）看不见 —— 实时行没带字节偏移。
//! - 见证那一行本身含非 UTF-8 字节（后端有损解码过、字节位对不上）⇒ 那一次不记见证，续传照今天的样子不核。
//! - 改写之后整份重读出来的行，行号与旧行同号（远端 `seq` 是行号空间，`INVARIANTS §25a`）—— 前端怎么把两份收成一份不在本处。
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
    /// 〔W5-VIS〕锚那一行的见证（续传之前先核它）；`None` = 这一次没记下（那一行含非 UTF-8 / 一行可计行都没有）。
    pub(crate) witness: Option<Witness>,
}

/// 〔W5-VIS〕见证：一行在文件里的**原始字节区间** `[start, end)`（含它的换行）与它的内容摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Witness {
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) hash: u64,
}

/// 一行正文（去掉 `\r`）的摘要 —— 只在本进程里比（`DefaultHasher::new()` 的钥是固定的），不落盘、不过线。
pub(crate) fn line_hash(line: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    line.hash(&mut h);
    h.finish()
}

/// 〔W5-VIS〕读回 `[w.start, w.end)` 那一段（`history-read` 的 `text`）之后：还是不是那一行。
/// 去掉末尾的 `\n` 与 `\r` 再比摘要；中间多出 / 少了换行（那一段已不是恰好一行）⇒ 不是。
pub(crate) fn witness_holds(w: &Witness, text: &str) -> bool {
    let Some(body) = text.strip_suffix('\n') else {
        return false;
    };
    let body = body.strip_suffix('\r').unwrap_or(body);
    !body.contains('\n') && line_hash(body) == w.hash
}

/// 〔W5-VIS〕一页正文逐行切开，每行带上它在文件里的原始字节区间 `[start, end)`（含换行）。
///
/// 页是后端**有损解码**过的（`frame_query::Page`）：一行里出现 U+FFFD ⇒ 那一行起本页余下各行的字节位都说不准了 ⇒
/// 区间给 `None`（下一页从 `page.next` 那个原始偏移重新对齐）。页末那一截没有换行的残尾（torn）同样 `None`。
/// 回 `(去掉 \r 的正文, 区间)`。
pub(crate) fn page_lines(offset: u64, text: &str) -> Vec<(&str, Option<(u64, u64)>)> {
    let mut out = Vec::new();
    let mut pos = Some(offset);
    let mut rest = text;
    while !rest.is_empty() {
        let (raw, tail, had_nl) = match rest.find('\n') {
            Some(i) => (&rest[..i], &rest[i + 1..], true),
            None => (rest, "", false),
        };
        if raw.contains('\u{FFFD}') {
            pos = None;
        }
        let span = match (pos, had_nl) {
            (Some(p), true) => Some((p, p + raw.len() as u64 + 1)),
            _ => None,
        };
        out.push((raw.strip_suffix('\r').unwrap_or(raw), span));
        pos = span.map(|(_, e)| e);
        rest = tail;
    }
    out
}

/// 〔W5-VIS〕走读时挑见证：**区间末端是 `plan.end` 的那一段**里、最后一个可计行（整份读时是尾段，续传时是唯一那一段）。
/// 那一行的区间说不准 ⇒ 这一次不记。
#[derive(Debug, Default)]
pub(crate) struct WitnessPick {
    last: Option<Option<Witness>>,
}

impl WitnessPick {
    /// 一个可计行：它所在那一段的末端 `seg_upto`、这张图的末端 `plan_end`、正文与区间。
    pub(crate) fn see(
        &mut self,
        seg_upto: u64,
        plan_end: u64,
        line: &str,
        span: Option<(u64, u64)>,
    ) {
        if seg_upto == plan_end {
            self.last = Some(span.map(|(start, end)| Witness {
                start,
                end,
                hash: line_hash(line),
            }));
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
    let same = g.get(&key).filter(|c| c.path == path);
    let next = same.map_or(plan.total, |c| c.next.max(plan.total));
    // 〔W5-VIS〕见证先照旧带过来（这一趟没读到新的可计行时它仍是锚那一行）；[`note_witness`] 随后按这一趟的走读改。
    let witness = same.and_then(|c| c.witness.clone());
    g.insert(
        key,
        Cursor {
            path: path.to_string(),
            anchor_total: plan.total,
            anchor_end: plan.end,
            next,
            witness,
        },
    );
}

/// 〔W5-VIS〕立锚之后按这一趟的走读改见证（[`WitnessPick::done`] 的三形：`None` 照留 · `Some(None)` 清掉 · `Some(Some)` 换新）。
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
