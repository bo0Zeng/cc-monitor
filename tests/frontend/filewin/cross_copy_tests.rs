//! `filewin/cross_copy.rs` 的判据 —— **复制到另一台机器**那一问。做的那一半住本机后端（`transfer-copy`，判据
//! `tests/backend/control/transfer_copy_tests.rs`）；窗口发一条的那一层（带票 · 冲突框 · 断了再问 · 撤）判据在 `batch_tests.rs`。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`the_local_origin_is_the_app_one`] | 窗口那份 `<local>` == app 侧 `inbound_client::LOCAL_ORIGIN` | 读两侧源码 |
//! | [`the_local_machine_is_shown_as_a_word_and_sent_as_its_address`] | 本机在下拉里写成字、发出去是地址 | 期望手写 |

use super::*;

#[test]
fn the_local_origin_is_the_app_one() {
    // app 侧那一份住 `resident-client`（monitor `inbound_client.rs` 转用）。
    let app =
        guard_core::production_code(include_str!("../../../src/comms/resident-client/client.rs"));
    let needle = format!("pub const LOCAL_ORIGIN: &str = \"{LOCAL_ORIGIN}\";");
    assert_eq!(
        app.matches(needle.as_str()).count(),
        1,
        "窗口那份 <local> 与 app 侧不是同一个值"
    );
}

#[test]
fn the_local_machine_is_shown_as_a_word_and_sent_as_its_address() {
    let shown = shown_machine(LOCAL_ORIGIN);
    assert_eq!(
        shown,
        copy_text("rsFilewinCrossCopy.prompt.localMachine", &[])
    );
    assert!(!shown.contains('<'), "下拉里出现了字面的本机地址");
    assert_eq!(origin_of(&shown), LOCAL_ORIGIN);
    assert_eq!(origin_of(&format!(" {shown} ")), LOCAL_ORIGIN);
    assert_eq!(
        (shown_machine("gpu-01"), origin_of(" gpu-01 ")),
        ("gpu-01".to_string(), "gpu-01".to_string())
    );
}
