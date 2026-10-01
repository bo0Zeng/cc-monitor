//! （「后端帧命令的应答就是界面要的那个形状，前端按形状收」）：起会话两条帧命令的收发两端
//! （`control/launch_render/mod.rs`）—— 入参按线上形状严格收、渲不出来的以码 `refused` 离开后端。

use super::*;

fn attach_req() -> Value {
    serde_json::json!({
        "action": {"kind": "attach", "name": "cc-foo"},
        "container": {"kind": "tmux", "name": "cc-foo", "send_into": false},
        "cwd": null, "account": {"kind": "base"}, "ccmSid": null, "model": null,
        "launcher": "claude", "defaultLauncher": "claude", "rbindToken": null,
    })
}

/// `launch-render-cli` 的成品恰好一格 `cmd`；渲不出来 ⇒ `refused` ＋ 那一句；多送一格 / 旧形状 ⇒ `bad_args`。
#[test]
fn the_cli_product_is_one_cmd_and_a_refusal_is_the_refused_code() {
    let v = answer_cli(&attach_req()).unwrap();
    assert_eq!(v, serde_json::json!({"cmd": "ccm -- --attach cc-foo"}));
    let mut bad = attach_req();
    bad["action"] = serde_json::json!({"kind": "resume", "sid": "-x"});
    bad["container"] = serde_json::json!({"kind": "none"});
    let (code, said) = answer_cli(&bad).unwrap_err();
    assert_eq!(code, "refused");
    assert!(said.contains("-x"), "那一句没说是哪个值：{said}");
    let mut stale = attach_req();
    stale["isSsh"] = serde_json::json!(true);
    assert_eq!(
        answer_cli(&stale).unwrap_err().0,
        "bad_args",
        "旧形状（带 `isSsh`）该被拒，不静默吞"
    );
}

/// `launch-local` 的入参按形状收（缺必填 / 多一格 / 旧形状 ⇒ `bad_args`），成品 `{cmd, launchId}`。
#[test]
fn the_local_request_and_product_have_their_registered_shapes() {
    for bad in [
        serde_json::json!({"action": {"kind": "new"}}),
        serde_json::json!({"action": {"kind": "new"}, "defaultLauncher": "claude", "x": 1}),
        serde_json::json!({"action": {"kind": "new"}, "agent": {"id": "claude-code", "defaultLauncher": "claude"}, "allSessions": true}),
    ] {
        assert_eq!(answer_local(&bad).unwrap_err().0, "bad_args", "{bad}");
    }
    let v = answer_local(&serde_json::json!({
        "action": {"kind": "resume", "sid": "s-1"}, "defaultLauncher": "claude",
    }))
    .unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["cmd", "launchId"]);
    assert!(v["cmd"].as_str().unwrap().starts_with("ccm "), "{v}");
}
