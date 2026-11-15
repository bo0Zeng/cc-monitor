//! 要求：「`HostScope::Client` 那几行照旧由 monitor 答」＋「四拍收成两拍：monitor 只交它独有的那几条事实」。
//!
//! monitor 这一侧只交它进程独有的三格（原样），不 stat、不判定（判定与 stat 在本机后端 `data-report`，`tests/backend/footprint/`）；
//! agent 家不交 —— Claude 目录只由那台后端解析。

use super::*;

/// ★ 三格原样交出（`path` · `webview` 取不到 ⇒ `null`，查不动不是空）；没有家目录 ⇒ 说清，不猜。
#[test]
fn facts_are_the_monitor_own_values_verbatim() {
    let got = facts(
        Some(PathBuf::from("/m")),
        Some("/usr/bin".into()),
        Some(PathBuf::from("/m/web/EBWebView")),
    )
    .unwrap();
    assert_eq!(
        got,
        json!({ "home": "/m", "path": "/usr/bin", "webview": "/m/web/EBWebView" })
    );
    let bare = facts(Some(PathBuf::from("/m")), None, None).unwrap();
    assert!(bare["path"].is_null(), "PATH 取不到 ⇒ null");
    assert!(bare["webview"].is_null(), "网页引擎目录推不出 ⇒ null");
    assert!(
        facts(None, None, None).is_err(),
        "没有家目录就解不了 `~/…`，不猜"
    );
}

/// ★ 网页引擎的数据目录 ＝ 运行时给的本机应用数据目录底下 `EBWebView`（Tauri 2 的默认约定）。
#[test]
fn the_webview_dir_sits_under_the_app_local_data_dir() {
    assert_eq!(
        webview_dir_in(Path::new("/m/.local/share/app")),
        PathBuf::from("/m/.local/share/app/EBWebView")
    );
}
