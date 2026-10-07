//! 要求：一个子运行至多一个窗口（窗口名按「会话 ＋ 运行」定，已开着就前置），窗口与查看窗同一套权限。

use super::{agent_window_label, AGENT_WINDOW_PREFIX};

#[test]
fn an_agent_window_is_named_by_session_and_run() {
    let a = agent_window_label("5e550001-0000-4000-8000-000000000001", "a1b2");
    assert_eq!(a, "viewer-agent-5e550001-0000-4000-8000-000000000001-a1b2");
    assert_eq!(
        a,
        agent_window_label("5e550001-0000-4000-8000-000000000001", "a1b2"),
        "同一个子运行 ⇒ 同一个窗口名"
    );
    assert_ne!(
        a,
        agent_window_label("5e550001-0000-4000-8000-000000000001", "a1b3"),
        "别的子运行 ⇒ 别的窗口"
    );
    assert_ne!(
        a,
        agent_window_label("5e550001-0000-4000-8000-000000000002", "a1b2"),
        "别的会话 ⇒ 别的窗口"
    );
}

#[test]
fn an_agent_window_name_stays_inside_the_viewer_permission_set() {
    // 权限文件里查看窗那一组是 `viewer-*`：agent 窗口落在里面才拿得到同一套权限。
    let caps = include_str!("../../../src/frontend/shell/capabilities/default.json");
    assert!(
        caps.contains("\"viewer-*\""),
        "权限文件里没有查看窗那一组了"
    );
    assert!(AGENT_WINDOW_PREFIX.starts_with("viewer-"));
    for run in ["a/b c", "ä", "x_y", "-"] {
        let l = agent_window_label("s", run);
        assert!(l.starts_with(AGENT_WINDOW_PREFIX), "{l}");
        assert!(
            l.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
            "窗口名只许字母数字与 - _：{l}"
        );
    }
    assert_ne!(
        agent_window_label("s", "a_2f"),
        agent_window_label("s", "a/"),
        "写成 _xx 的字节不许与原样的撞名"
    );
    assert_ne!(
        agent_window_label("s-x", "y"),
        agent_window_label("s", "x-y"),
        "两段之间的分隔不许认错"
    );
}
