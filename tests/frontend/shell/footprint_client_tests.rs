//! 要求：「`HostScope::Client` 那几行照旧由 monitor 答」＋「四拍收成两拍：monitor 只交它独有的那几条事实」。
//!
//! monitor 这一侧只交环境三格（原样），不 stat、不判定（判定与 stat 在本机后端 `footprint-report`，`tests/backend/footprint/`）。

use super::*;

/// ★ 三格原样交出（`path` 取不到 ⇒ `null`，查不动不是空）；没有家目录 ⇒ 说清，不猜。
#[test]
fn facts_are_the_three_monitor_own_values_verbatim() {
    let got = facts(
        Some(PathBuf::from("/m")),
        Some(PathBuf::from("/m/.claude")),
        Some("/usr/bin".into()),
    )
    .unwrap();
    assert_eq!(
        got,
        json!({ "home": "/m", "agentHome": "/m/.claude", "path": "/usr/bin" })
    );
    let bare = facts(
        Some(PathBuf::from("/m")),
        Some(PathBuf::from("/m/.claude")),
        None,
    )
    .unwrap();
    assert!(bare["path"].is_null(), "PATH 取不到 ⇒ null");
    assert!(
        facts(None, None, None).is_err(),
        "没有家目录就解不了 `~/…`，不猜"
    );
}
