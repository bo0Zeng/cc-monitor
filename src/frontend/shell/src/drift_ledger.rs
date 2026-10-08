//! monitor 自己看得见的那一面漂移：那台后端 `hello` 里我们不认识的能力 token（保守缺省：那条能力不用）。
//!
//! 记录与进程状态那几面（未知记录类型 · 已知类型解析失败 · 认不出的会话 kind / status）在那台后端记（帧命令 `drift-report`，
//! 界面经通道按机器问）。这里只记**见过哪些** token：每台一份、有上限，不改行为、不发 warn。
//!
//! 写入口 [`record`] 必须说是哪台（没有缺省：缺省记在本机名下，就是把远端的记录悄悄记成本机的）；
//! 读口 [`drift_ledger_report`] 只答所问那一台。

use crate::detail::Said;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

/// 每台最多记多少个不同的 token。超出的并进 [`OVERFLOW_KEY`]。
pub const MAX_TOKENS: usize = 64;
/// 超限之后的归并键。
pub const OVERFLOW_KEY: &str = "<overflow>";

/// origin 线上串 → 那台见过的 token。
type Book = BTreeMap<String, BTreeSet<String>>;

/// 读口的回包：带回它答的是哪台（界面按回声判）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct DriftLedgerReport {
    /// 这份账是哪台机器的（本机 `"<local>"`、远端那台的名字）。
    pub origin: crate::origin::Origin,
    /// 那台 `hello` 里我们不认识的 token（升序）；没见过 ⇒ 空。
    pub unknown_tokens: Vec<String>,
}

fn lock() -> std::sync::MutexGuard<'static, Book> {
    static L: std::sync::OnceLock<Mutex<Book>> = std::sync::OnceLock::new();
    // 毒化了也继续用：这是诊断面。
    L.get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// 记一个不认识的 token，记在 `origin` 那台名下。**这是本模块唯一的写入口。**
pub fn record(origin: &crate::origin::Origin, token: &str) {
    record_in_book(&mut lock(), origin, token);
}

/// [`record`] 的纯形式（显式传账本：全局账本会被任何走过喂账点的测试写，单测在局部账本上跑）。
fn record_in_book(book: &mut Book, origin: &crate::origin::Origin, token: &str) {
    let seen = book.entry(origin.as_wire_str().to_string()).or_default();
    if seen.len() >= MAX_TOKENS && !seen.contains(token) {
        seen.insert(OVERFLOW_KEY.to_string());
    } else {
        seen.insert(token.to_string());
    }
}

/// [`drift_ledger_report`] 的纯形式：只取那一台；没记过 ⇒ 空。
fn snapshot_in_book(book: &Book, origin: &crate::origin::Origin) -> Vec<String> {
    book.get(origin.as_wire_str())
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default()
}

/// 诊断面读口。只读、按需，不轮询。收 `origin`，只答那一台；空白名（「没说」）拒收，不许被当成某一台。
/// **monitor 自己的命令，不经后端**。
#[tauri::command]
pub async fn drift_ledger_report(origin: crate::origin::Origin) -> Result<DriftLedgerReport, Said> {
    match origin.route("drift_ledger_report")? {
        crate::origin::Route::Local | crate::origin::Route::Remote(_) => {}
    }
    Ok(DriftLedgerReport {
        unknown_tokens: snapshot_in_book(&lock(), &origin),
        origin,
    })
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/drift_ledger_tests.rs"]
mod tests;
