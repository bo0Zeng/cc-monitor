//! U-CC1：**数据面漂移记账** —— 把「Claude Code 变了」从不可观测变成看一眼就知道。
//!
//! # 为什么需要它（这条是实测，不是预防性设计）
//!
//! `src/doc/INVARIANTS.md §18.1`（F63，2026-07-16）记下当时的实测：**7 个未知 `type` /
//! 8,774 条 / 157,385 行**。2026-08-02 本机重新全量扫一遍（只读）：
//!
//! ```text
//! 1,904 个 jsonl · 472,115 条记录 · 非法 JSON 1 条
//! 20 种 type，monitor 认识 11 种（含自造的 cc-monitor-unrecognized）
//! 未知 10 种 / 27,747 条 / 5.88%
//!   mode 20526 · file-history-delta 2564 · agent-name 2312 · pr-link 2198 ·
//!   relocated 108 · worktree-state 19 · started 6 · result 6 · fork-context-ref 5 · frame-link 3
//! ```
//!
//! 也就是说 **CC 在 17 天里新增了 3 个记录类型**（`started` / `result` / `fork-context-ref`），
//! **而仓里没有任何东西知道这件事** —— 要靠人手工扫语料才发现。
//!
//! # 「宽容降级」与「排他白名单」有一个共同的、此前缺失的配套义务
//!
//! **宽容 ≠ 无声；排他 ≠ 无声。**
//!
//! - `parser.rs` 对未知 `type` **刻意不 warn**（那个决定是对的：20,526 条 `mode` 会刷屏）；
//! - `session_map.rs` 对未登记的 pidfile `kind` 一声不吭（那个排他也是对的，`kind` 是**授权型**判据）。
//!
//! 两者都是**正确的行为**，但都不该是**无声的**：否则「CC 变了」这件事本身不可观测。
//! 本模块只做一件事 —— **记账**。它**不改变任何行为**，不发 warn，不影响渲染。
//!
//! # 有界
//!
//! 键数上限 [`MAX_KEYS`]；再多的一律并进 `<overflow>`。样例只留首见的一条并截断到
//! [`MAX_SAMPLE_BYTES`]。**这是诊断面，不是数据管道** —— 内存必须有硬上界。
//!
//! # 为什么只有四个面，没有「未登记的 `status`」
//!
//! 设计稿 B 列了四个降级点，其中「未登记的 `status`」在**前端**（`src/session-status.ts`
//! 的 `activityLightClass`），Rust 这侧只是原样透传（`session_map.rs` 对 `status` 没有任何
//! 白名单分支，逐行核过）。**在这里加一个没有落点的面，就是「登记了但不产生信号」** ——
//! 比不加更糟。TS 侧那个面另记，不在本轮。
//!
//! # 计数的量纲不一样，别横向比
//!
//! - `UnknownRecordType` / `KnownTypeParseFailed`：**每条记录一次**（解析热路径）。
//! - `UnknownSessionKind`：**每次扫描一次**（`scan_dir` 随文件事件重扫）⇒ 数字反映的是
//!   「观测了多少次」，不是「有多少个这样的会话」。**要看的是键的集合，不是数字。**
//! - `UnknownBackendToken`：每次收到 `hello` 一次（每条连接一次 + 重连）。
//!
//! # 〔ST3 · 第四波〕按机器分
//!
//! 账本的第一层键是 **origin**（`"<local>"` 或那台远端的名字），第二层才是面。
//! 远端的记录本来就在 monitor 里解析（`parse_line` 跑在本进程），看不懂的那一刻 monitor 就在场 ——
//! 缺的只是「这一行是从哪台来的」没带到写点。⇒ 写入口 [`record`] **必须**说是哪台（没有缺省：
//! 缺省记在本机名下，就是把远端的记录悄悄记成本机的，`origin·rs` 头注整篇治的那一形）。
//! 读口 [`drift_ledger_report`] 按 origin 答，**只答那一台**。设计与逐写点读数在
//! `调研/第四波记录/ST3.md`。

use crate::copy_table::copy_text;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// 每个面最多记多少个不同的键。超出的并进 [`OVERFLOW_KEY`]。
pub const MAX_KEYS: usize = 64;
/// 首见样例的截断长度（字节，按字符边界安全截断）。
pub const MAX_SAMPLE_BYTES: usize = 400;
/// 键数超限之后的归并键。
pub const OVERFLOW_KEY: &str = "<overflow>";

/// 四个「降级点」。每一个都对应一处**刻意的**宽容或排他。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum DriftFace {
    /// jsonl 里我们不认识的记录 `type`（`parser.rs` 抢救成 `Unrecognized`，刻意不 warn）。
    UnknownRecordType,
    /// 已知 `type` 但字段解析失败（**这一类值得警惕**：多半是 CC 改了已知类型的形状）。
    KnownTypeParseFailed,
    /// `sessions/<PID>.json` 里未登记的 `kind`（排他白名单：非 `interactive` 一律当 bg）。
    UnknownSessionKind,
    /// 远端 backend `hello` 里我们不认识的能力 token（`capabilities` / `emits` / `commands`）。
    UnknownBackendToken,
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
            DriftFace::UnknownBackendToken => {
                copy_text("rsDriftLedger.consequence.unknownBackendToken", &[])
            }
        }
    }
}

/// 一个键的记账。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct DriftEntry {
    /// 看不懂的那个值（记录 type / kind / status / token）。
    pub key: String,
    /// 见过多少次。**本进程内**的计数，不落盘。
    ///
    /// C03：`u64` 必须显式声明 TS 侧类型，否则 ts-rs 默认吐 `bigint`，
    /// 与 JSON IPC 的运行时（`number`）不一致。
    #[cfg_attr(test, ts(type = "number"))]
    pub count: u64,
    /// **首见**的一条样例（截断）。后来的不再覆盖 —— 第一条最接近「它刚出现时长什么样」。
    pub first_sample: Option<String>,
}

/// 一个面的快照。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct DriftFaceReport {
    pub face: DriftFace,
    /// 这个面「看不懂时会发生什么」（`DriftFace::consequence`）。
    pub consequence: String,
    /// 按 `count` 降序、同数按键名升序。
    pub entries: Vec<DriftEntry>,
    /// 该面是否已经溢出（键数触顶）。溢出后新键并进 `<overflow>`。
    pub overflowed: bool,
}

/// 〔ST3〕读口的回包：**带回它答的是哪台**（界面按回声判，同足迹那一格）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct DriftLedgerReport {
    /// 这份账是哪台机器的（本机 `"<local>"`、远端那台的名字）。
    pub origin: crate::origin::Origin,
    /// 这台机器名下的各面快照（没记过 ⇒ 空）。
    pub faces: Vec<DriftFaceReport>,
}

/// 一台机器的账：面 → 键 → 记账。
type Ledger = BTreeMap<DriftFace, BTreeMap<String, DriftEntry>>;
/// 〔ST3〕整本账：origin 线上串 → 那台的账。键用线上串（`Origin` 不带 `Ord`，也不该为这里加）。
type Book = BTreeMap<String, Ledger>;

fn ledger() -> &'static Mutex<Book> {
    static L: std::sync::OnceLock<Mutex<Book>> = std::sync::OnceLock::new();
    L.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn lock() -> std::sync::MutexGuard<'static, Book> {
    // 毒化了也继续用：这是诊断面，绝不能因为它 panic 而拖垮解析热路径。
    ledger().lock().unwrap_or_else(|e| e.into_inner())
}

/// 按字符边界把样例截到 [`MAX_SAMPLE_BYTES`] 以内。
fn truncate_sample(s: &str) -> String {
    if s.len() <= MAX_SAMPLE_BYTES {
        return s.to_string();
    }
    let mut end = MAX_SAMPLE_BYTES;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// 记一次。**这是本模块唯一的写入口。**
///
/// `origin` = 这条看不懂的东西是从哪台机器来的（〔ST3〕没有缺省，见头注「按机器分」）。
/// `key` 为空时用 `"<empty>"`（空串当键会让诊断面显示成空行，看不出是哪一类）。
pub fn record(origin: &crate::origin::Origin, face: DriftFace, key: &str, sample: Option<&str>) {
    record_in_book(&mut lock(), origin, face, key, sample);
}

/// 〔ST3〕[`record`] 的纯形式（显式传整本账，理由同 [`record_into`]）：先按 origin 找那台的账。
fn record_in_book(
    book: &mut Book,
    origin: &crate::origin::Origin,
    face: DriftFace,
    key: &str,
    sample: Option<&str>,
) {
    let led = book.entry(origin.as_wire_str().to_string()).or_default();
    record_into(led, face, key, sample);
}

/// [`record`] 的纯形式：**显式传账本**。
///
/// # 为什么要拆出来（实测踩过，不是洁癖）
///
/// 账本是进程内全局的，而**任何跑过 `parse_line` 的测试都会往里写**
/// （`history` / `search` / `lib` 的批处理测试都会）。于是「reset + 断言整表形状」这种
/// 单测**必然 flaky**：实测 6 次全量跑里红 4 次。加一把跨模块串行闸也救不了 ——
/// 那些测试根本不知道有这把闸。
///
/// ⇒ 单测一律在**局部账本**上跑（完全隔离、不需要闸）；只有接缝测试碰全局，
/// 而它必须写成**容忍污染**的形状（断言「我那条在」，不断言「表里只有我那条」）。
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

/// 只读快照：**只答 `origin` 那一台**。按需调用，不轮询。
pub fn snapshot(origin: &crate::origin::Origin) -> Vec<DriftFaceReport> {
    snapshot_in_book(&lock(), origin)
}

/// 〔ST3〕[`snapshot`] 的纯形式：**只取那一台**；没记过的那台 ⇒ 空。
fn snapshot_in_book(book: &Book, origin: &crate::origin::Origin) -> Vec<DriftFaceReport> {
    book.get(origin.as_wire_str())
        .map(snapshot_of)
        .unwrap_or_default()
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

/// U-CC1：诊断面读口。**只读、按需，不新增任何轮询。**
///
/// 〔ST3〕收 `origin`，只答那一台；回包带回 `origin`（界面按回声判）。
/// **monitor 自己的命令，不经后端**：远端的记录本来就在本进程里解析、在本进程里记账。
/// 两臂读的是同一本账，`route` 在这里只做一件事 —— 空白名（「没说」）拒收，不许被当成某一台。
#[tauri::command]
pub async fn drift_ledger_report(
    origin: crate::origin::Origin,
) -> Result<DriftLedgerReport, String> {
    match origin.route("drift_ledger_report")? {
        crate::origin::Route::Local | crate::origin::Route::Remote(_) => {}
    }
    Ok(DriftLedgerReport {
        faces: snapshot(&origin),
        origin,
    })
}

#[cfg(test)]
#[path = "../../../tests/bridge/drift_ledger_tests.rs"]
mod tests;
