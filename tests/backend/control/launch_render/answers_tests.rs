//! 设计/05 §14.3（「后端帧命令的应答就是界面要的那个形状，前端按形状收」）· `99 §2.1 ⑬`：起会话三条帧命令的收发两端
//! （`control/launch_render/mod.rs`）—— 入参按线上形状严格收、渲不出来的「拒」以码 `refused` 离开后端（不再靠 `REFUSE:` 串标）。

use super::*;

/// 坏输入 ⇒ 码 `refused`，那一句原样带出、串标摘掉；多送一格 ⇒ `bad_args`（`deny_unknown_fields`）。
#[test]
fn a_refusal_leaves_the_backend_as_the_refused_code() {
    let bad = serde_json::json!({
        "env": [{"kind": "export-config-dir", "value": "rel/dir"}],
        "cwd": null, "launcher": "claude", "args": [], "nestedEnv": [], "wrap": [],
    });
    let (code, said) = answer_payload(&bad).unwrap_err();
    assert_eq!(code, "refused");
    assert!(!said.starts_with(payload::REFUSE_TAG), "串标没摘：{said}");
    assert!(said.contains("rel/dir"), "那一句没说是哪个值：{said}");
    let mut extra = bad.clone();
    extra["whatever"] = serde_json::json!(1);
    assert_eq!(answer_payload(&extra).unwrap_err().0, "bad_args");
    let ok = serde_json::json!({
        "env": [], "cwd": "/w", "launcher": "claude", "args": [], "nestedEnv": [], "wrap": [],
    });
    assert_eq!(answer_payload(&ok).unwrap(), serde_json::json!({"cmd": "cd '/w' && claude"}));
}

/// `launch-render-cli` 的成品恰好三格；`launch-local` 的入参按形状收（缺必填 / 多一格 ⇒ `bad_args`）。
#[test]
fn the_cli_product_and_the_local_request_have_their_registered_shapes() {
    let req = serde_json::json!({
        "isSsh": true, "action": {"kind": "attach", "name": "cc-foo"},
        "container": {"kind": "tmux", "name": "cc-foo", "send_into": false},
        "cwd": null, "account": {"kind": "base"}, "ccmSid": null, "model": null,
        "launcher": "claude", "defaultLauncher": "claude",
    });
    let v = answer_cli(&req).unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["cmd", "ok", "reason"]);
    assert_eq!(v["ok"], true, "{v}");
    let mut stale = req.clone();
    stale["ccm"] = serde_json::json!({"state": "installed", "caps": []});
    assert_eq!(answer_cli(&stale).unwrap_err().0, "bad_args", "旧形状（带 `ccm`）该被拒，不静默吞");
    for bad in [
        serde_json::json!({"action": {"kind": "new"}}),
        serde_json::json!({"action": {"kind": "new"}, "agent": {"id": "claude-code", "defaultLauncher": "claude", "resumeFlag": "--resume"}, "allSessions": true, "x": 1}),
    ] {
        assert_eq!(answer_local(&bad).unwrap_err().0, "bad_args", "{bad}");
    }
}
