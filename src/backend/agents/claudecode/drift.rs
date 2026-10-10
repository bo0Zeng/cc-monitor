//! 数据面漂移记账：把「Claude Code 变了」从不可观测变成看一眼就知道。
//!
//! CC 的记录类型会自己长（`src/doc/INVARIANTS.md §18.1` 记着语料里未知 `type` 的读数），而仓里没有别的东西知道这件事。
//! `parse.rs` 对未知 `type` 刻意不 warn（几万条 `mode` 会刷屏）、未登记的 pidfile `kind` 一声不吭（`kind` 是授权型判据）——
//! 两者都对，但都不该是无声的。本模块只做一件事：记账。不改变任何行为，不发 warn，不影响渲染。
//!
//! # 有界
//!
//! 键数上限 [`MAX_KEYS`]；再多的一律并进 `<overflow>`。样例只留首见的一条并截断到 [`EXCERPT_BYTES`](crate::agents::record::EXCERPT_BYTES)。这是诊断面，不是数据管道。
//!
//! # 按机器分
//!
//! 记录在那台机器自己的后端里解析 ⇒ 账天然按机器分；界面按机器经通道问那台后端（帧命令 `drift-report`）。
//! 计数是每条记录一次（解析热路径）；要看的是键的集合，不是数字。

use copy_core::copy_text;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// 每个面最多记多少个不同的键。超出的并进 [`OVERFLOW_KEY`]。
pub const MAX_KEYS: usize = 64;
/// 键数超限之后的归并键。
pub const OVERFLOW_KEY: &str = "<overflow>";

/// 记录那两个「降级点」。每一个都对应一处**刻意的**宽容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftFace {
    /// jsonl 里我们不认识的记录 `type`（`parse.rs` 抢救成 `Unrecognized`，刻意不 warn）。
    UnknownRecordType,
    /// 已知 `type` 但字段解析失败（**这一类值得警惕**：多半是 CC 改了已知类型的形状）。
    KnownTypeParseFailed,
    /// 进程状态文件里认不出的 `kind`（当后台会话，不成 tab）。
    UnknownSessionKind,
    /// 进程状态文件里认不出的 `status`（活动灯说不清）。
    UnknownSessionStatus,
    /// 进程状态文件里认不出的 `waitingFor`（在等什么说不清 ⇒ 「需手动」判不出种类）。
    UnknownWaitingFor,
    /// 记录里「延后加载的工具变了」附件里认不出的 MCP 状态表（`…McpServers`）：那一种状态的服务器不进会话的 MCP 那一格。
    UnknownMcpList,
}

impl DriftFace {
    /// 给人看的一句话：**这个面看不懂东西时会发生什么**。诊断面直接显示它。
    pub fn consequence(self) -> String {
        match self {
            DriftFace::UnknownRecordType => {
                copy_text("rsDriftLedger.consequence.unknownRecord", &[])
            }
            DriftFace::KnownTypeParseFailed => {
                copy_text("rsDriftLedger.consequence.knownTypeParse", &[])
            }
            DriftFace::UnknownSessionKind => {
                copy_text("rsDriftLedger.consequence.unknownSessionKind", &[])
            }
            DriftFace::UnknownSessionStatus => {
                copy_text("rsDriftLedger.consequence.unknownSessionStatus", &[])
            }
            DriftFace::UnknownWaitingFor => {
                copy_text("rsDriftLedger.consequence.unknownWaitingFor", &[])
            }
            DriftFace::UnknownMcpList => copy_text("rsDriftLedger.consequence.unknownMcpList", &[]),
        }
    }
}

/// 一个键的记账。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DriftEntry {
    /// 看不懂的那个值（记录 type / kind / status / token）。
    pub key: String,
    /// 见过多少次。本进程内的计数，不落盘。`u64` 必须显式声明 TS 侧类型，否则 ts-rs 默认吐 `bigint`，与 JSON IPC 的运行时（`number`）不一致。
    pub count: u64,
    /// **首见**的一条样例（截断）。后来的不再覆盖 —— 第一条最接近「它刚出现时长什么样」。
    pub first_sample: Option<String>,
}

/// 一个面的快照。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DriftFaceReport {
    pub face: DriftFace,
    /// 这个面「看不懂时会发生什么」（`DriftFace::consequence`）。
    pub consequence: String,
    /// 按 `count` 降序、同数按键名升序。
    pub entries: Vec<DriftEntry>,
    /// 该面是否已经溢出（键数触顶）。溢出后新键并进 `<overflow>`。
    pub overflowed: bool,
}

/// 这台机器的账：面 → 键 → 记账。
type Ledger = BTreeMap<DriftFace, BTreeMap<String, DriftEntry>>;

fn ledger() -> &'static Mutex<Ledger> {
    static L: std::sync::OnceLock<Mutex<Ledger>> = std::sync::OnceLock::new();
    L.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn lock() -> std::sync::MutexGuard<'static, Ledger> {
    // 毒化了也继续用：这是诊断面，绝不能因为它 panic 而拖垮解析热路径。
    ledger().lock().unwrap_or_else(|e| e.into_inner())
}

/// 按字符边界把样例截到 [`EXCERPT_BYTES`](crate::agents::record::EXCERPT_BYTES) 以内（与记录里认不出那一条的摘录同一个截断：[`crate::agents::record::excerpt_of`]），截了 ⇒ 末尾一个 `…`。
fn truncate_sample(s: &str) -> String {
    let cut = crate::agents::record::excerpt_of(s);
    if cut.len() == s.len() {
        cut
    } else {
        format!("{cut}…")
    }
}

/// 记一次。**这是本模块唯一的写入口。**
///
/// `key` 为空时用 `"<empty>"`（空串当键会让诊断面显示成空行，看不出是哪一类）。
pub fn record(face: DriftFace, key: &str, sample: Option<&str>) {
    record_into(&mut lock(), face, key, sample);
}

/// [`record`] 的纯形式：显式传账本。账本是进程内全局的，任何跑过 `parse_line` 的测试都会往里写 ⇒ 「reset + 断言整表形状」必然 flaky。
/// 单测一律在局部账本上跑；只有接缝测试碰全局，而它写成容忍污染的形状（断言「我那条在」，不断言「表里只有我那条」）。
fn record_into(led: &mut Ledger, face: DriftFace, key: &str, sample: Option<&str>) {
    let key = if key.is_empty() { "<empty>" } else { key };
    let per_face = led.entry(face).or_default();
    // 有界：键数触顶且是新键 ⇒ 并进 `<overflow>`。
    let effective = if per_face.len() >= MAX_KEYS && !per_face.contains_key(key) {
        OVERFLOW_KEY
    } else {
        key
    };
    let e = per_face
        .entry(effective.to_string())
        .or_insert_with(|| DriftEntry {
            key: effective.to_string(),
            count: 0,
            first_sample: None,
        });
    e.count += 1;
    if e.first_sample.is_none() {
        if let Some(s) = sample {
            e.first_sample = Some(truncate_sample(s));
        }
    }
}

/// 只读快照。按需调用，不轮询。
pub fn snapshot() -> Vec<DriftFaceReport> {
    snapshot_of(&lock())
}

/// [`snapshot`] 的纯形式：**显式传账本**（见 [`record_into`] 的头注）。
fn snapshot_of(led: &Ledger) -> Vec<DriftFaceReport> {
    let mut out = Vec::new();
    for (face, per_face) in led.iter() {
        let mut entries: Vec<DriftEntry> = per_face.values().cloned().collect();
        // count 降序、同数按键名升序 —— 稳定顺序，诊断面不许每次刷新都跳。
        entries.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
        out.push(DriftFaceReport {
            face: *face,
            consequence: face.consequence(),
            overflowed: per_face.contains_key(OVERFLOW_KEY),
            entries,
        });
    }
    out
}

/// 诊断面读口的成品（帧命令 `drift-report`，经注册表 `RecordFace.drift` 够到）：`{faces: [...]}`。只读、按需，不新增任何轮询。
pub(crate) fn report() -> serde_json::Value {
    serde_json::json!({ "faces": snapshot() })
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/drift_tests.rs"]
mod tests;
