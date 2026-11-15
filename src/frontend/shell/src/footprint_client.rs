//! 「足迹」里 **monitor 自己那台的那几行**（`HostScope::Client`）与「文件与数据」本机那几行要的、**只有 monitor 知道**的事实 —— 只读。
//!
//! 申报表与判定都进了后端（`src/backend/footprint/`）。本机后端与 monitor 同一台、同一用户 ⇒ stat 由本机后端自己做；
//! monitor 只交它自己进程的那几条（家目录 · `PATH` · 网页引擎的数据目录；agent 家由本机后端自己解析），
//! 界面原样带给本机后端 `data-report {client}`，一问出整份。

use crate::detail::Shell as _;
use copy_core::said::Said;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// 界面要 monitor 这台自己进程的事实：`{home, path, webview}`（`path` · `webview` 取不到 ⇒ `null`）。
#[tauri::command]
pub fn footprint_client_facts(handle: AppHandle) -> Result<Value, cmd_wire::failed::Failed> {
    let r: Result<Value, Said> = (move || -> Result<Value, Said> {
        let webview = handle
            .path()
            .app_local_data_dir()
            .ok()
            .map(|d| webview_dir_in(&d));
        facts(
            crate::data_paths::home(),
            std::env::var("PATH").ok(),
            webview,
        )
    })();
    r.map_err(|s| s.named("footprint_client_facts"))
}

/// 网页引擎的数据目录：Tauri 2 默认放在本机应用数据目录（`app_local_data_dir`）底下 `EBWebView/`。
/// 是约定、不是保证 —— 约定变了，那一行照实说不在盘上。
pub(crate) fn webview_dir_in(app_local_data: &Path) -> PathBuf {
    app_local_data.join("EBWebView")
}

/// [`footprint_client_facts`] 的本体（环境是参数，判据拿夹具喂）。
pub(crate) fn facts(
    home: Option<PathBuf>,
    path: Option<String>,
    webview: Option<PathBuf>,
) -> Result<Value, Said> {
    let home = home.ok_or_else(|| Said::of("rsFootprintClient.env.noHome", &[]))?;
    Ok(json!({
        "home": home.display().to_string(),
        "path": path,
        "webview": webview.map(|w| w.display().to_string()),
    }))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/footprint_client_tests.rs"]
mod tests;
