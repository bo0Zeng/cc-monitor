//! 「足迹」里 **monitor 自己那台的那几行**（`HostScope::Client`）要的、**只有 monitor 知道**的事实 —— 只读。
//!
//! 申报表与判定都进了后端（`src/backend/footprint/`）。本机后端与 monitor 同一台、同一用户 ⇒ stat 由本机后端自己做；
//! monitor 只交它自己进程的那几条（家目录 · `PATH`，「足迹里 monitor 自己那几行」；agent 家由本机后端自己解析），
//! 界面原样带给本机后端 `footprint-report {client}`，一问出整份报告。

use serde_json::{json, Value};
use std::path::PathBuf;

/// 界面要 monitor 这台自己进程的足迹事实：`{home, path}`（`path` 取不到 ⇒ `null`）。
#[tauri::command]
pub fn footprint_client_facts() -> Result<Value, String> {
    facts(creds_core::store::home_dir(), std::env::var("PATH").ok())
}

/// [`footprint_client_facts`] 的本体（环境是参数，判据拿夹具喂）。
pub(crate) fn facts(home: Option<PathBuf>, path: Option<String>) -> Result<Value, String> {
    let home =
        home.ok_or_else(|| crate::copy_table::copy_text("rsFootprintClient.env.noHome", &[]))?;
    Ok(json!({
        "home": home.display().to_string(),
        "path": path,
    }))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/footprint_client_tests.rs"]
mod tests;
