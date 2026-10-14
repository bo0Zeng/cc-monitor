//! （「后端帧命令的应答就是界面要的那个形状，前端按形状收」）：起会话两条帧命令的收发两端
//! （`control/launch_render/mod.rs`）—— 入参按线上形状严格收、渲不出来的以码 `refused` 离开后端。

use super::*;
use crate::control::launch_account::{Facts, Library};
use serde_json::Value;

/// 判号要的事实：一个号 `z`（能用）、没有谁的上次记录。
fn facts_of<T>(f: impl FnOnce(&Facts) -> T) -> T {
    let library = || {
        Library::of_product(
            &serde_json::json!({ "meta": {"enabled": true, "effectiveDefault": "z"}, "accounts": [{
                "name": "z", "configDir": "/h/z", "isDefault": true, "mode": "isolated",
                "exists": true, "authReady": true, "selectable": true,
            }]}),
        )
    };
    f(&Facts {
        has_accounts: true,
        library: &library,
        last: &|_| None,
    })
}
/// 线上那一份入参 ⇒ 本体 ⇒ 线上那一份回包 / 失败的码与那一句（解入参同 typed 分派那一步：形状不对 ⇒ `bad_args`）。
/// 线上那一份解成请求、照这台缺省的偏好填好不上线的那几格（同生产那一层的 `with_prefs`）、跑本体。
fn wired<R: serde::de::DeserializeOwned>(
    v: &Value,
    fill: impl FnOnce(R) -> R,
    f: impl FnOnce(&R, &Facts) -> Result<Rendered, LaunchRenderErr>,
) -> Result<Value, (String, String)> {
    use crate::wire::typed::Codes;
    let req: R =
        serde_json::from_value(v.clone()).map_err(|e| ("bad_args".to_string(), e.to_string()))?;
    let req = fill(req);
    facts_of(|facts| f(&req, facts))
        .map(|r| serde_json::to_value(r).unwrap())
        .map_err(|e| {
            let fail = e.into_fail();
            (fail.code, fail.message)
        })
}
fn answer_cli(v: &Value) -> Result<Value, (String, String)> {
    wired(
        v,
        |r: super::wire::CliRenderRequest| r.with_prefs(&Default::default()),
        super::answer_cli,
    )
}
fn answer_local(v: &Value) -> Result<Value, (String, String)> {
    wired(
        v,
        |r: super::local::LocalLaunchRequest| r.with_prefs(&Default::default()),
        super::answer_local,
    )
}

fn attach_req() -> Value {
    serde_json::json!({
        "agent": "claude",
        "action": {"kind": "attach", "name": "cc-foo"},
        "container": {"kind": "tmux", "name": "cc-foo", "send_into": false},
        "cwd": null, "account": {"kind": "base"}, "ccmSid": null, "model": null,
    })
}

/// `launch-render-cli` 的成品恰好一格 `cmd`；渲不出来 ⇒ `refused` ＋ 那一句；多送一格 / 旧形状 ⇒ `bad_args`。
#[test]
fn the_cli_product_is_one_cmd_and_a_refusal_is_the_refused_code() {
    let v = answer_cli(&attach_req()).unwrap();
    let entry =
        super::ccm_invocation::argv(&crate::platform::paths::installed_ccm_entry().unwrap());
    assert_eq!(
        v,
        serde_json::json!({"cmd": format!("{entry} -- --attach cc-foo"), "account": null})
    );
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
    // 不说是哪一家 ⇒ 收不下（不落默认那一家）；说了认不出的 ⇒ `refused`、列出认得的几家。
    let mut no_agent = attach_req();
    no_agent.as_object_mut().unwrap().remove("agent");
    assert_eq!(answer_cli(&no_agent).unwrap_err().0, "bad_args");
    let mut unknown = attach_req();
    unknown["agent"] = serde_json::json!("gemini");
    let (code, said) = answer_cli(&unknown).unwrap_err();
    assert_eq!(code, "refused");
    assert!(
        said.contains("gemini") && said.contains("claude / codex"),
        "{said}"
    );
}

/// ★ 进终端那一句只交意图（手机就这么交）：不交启动命令 ⇒ 照那一家的默认渲，同一行；起新会话没交启动命令也照默认、不吐 `--launcher`。
#[test]
fn an_attach_without_launchers_renders_the_same_line() {
    let mut bare = attach_req();
    for k in ["launcher", "defaultLauncher", "cwd", "ccmSid", "model"] {
        bare.as_object_mut().unwrap().remove(k);
    }
    assert_eq!(
        answer_cli(&bare).unwrap(),
        answer_cli(&attach_req()).unwrap()
    );
    let fresh = serde_json::json!({
        "agent": "claude", "action": {"kind": "new"}, "container": {"kind": "none"}, "account": {"kind": "base"},
    });
    let cmd = answer_cli(&fresh).unwrap()["cmd"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!cmd.contains("--launcher"), "{cmd}");
}

/// `launch-local` 的入参按形状收（缺必填 / 多一格 / 旧形状 ⇒ `bad_args`），成品 `{cmd, account}`。
#[test]
fn the_local_request_and_product_have_their_registered_shapes() {
    for bad in [
        serde_json::json!({"action": {"kind": "new"}}),
        serde_json::json!({"agent": "claude", "action": {"kind": "new"}, "x": 1}),
        // 不说是哪一家。
        serde_json::json!({"action": {"kind": "resume", "sid": "s-1"},}),
        serde_json::json!({"action": {"kind": "new"}, "agent": {"id": "claude-code",}, "allSessions": true}),
    ] {
        assert_eq!(answer_local(&bad).unwrap_err().0, "bad_args", "{bad}");
    }
    let v = answer_local(&serde_json::json!({
        "agent": "claude", "action": {"kind": "resume", "sid": "s-1"},
    }))
    .unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["account", "cmd"]);
    let entry = crate::platform::paths::installed_ccm_entry().unwrap();
    let head = if crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL {
        format!("& {} ", crate::platform::shell::dialect::ps_literal(&entry))
    } else {
        format!("{} ", super::ccm_invocation::argv(&entry))
    };
    assert!(v["cmd"].as_str().unwrap().starts_with(&head), "{v}");
}

/// 跟随 ⇒ 应答说出实际用的号；点名一个选不了的号 ⇒ `account_unavailable`，`data` 是那一形。
#[test]
fn the_account_used_is_in_the_product_and_an_unusable_one_is_refused_with_data() {
    let resume = |account: Value| {
        serde_json::json!({
            "agent": "claude", "action": {"kind": "resume", "sid": "s-1"},
            "account": account,
        })
    };
    let v = answer_local(&resume(serde_json::json!({"kind": "follow"}))).unwrap();
    assert_eq!(
        v["account"],
        serde_json::json!({"name": "z", "configDir": "/h/z", "model": null})
    );
    assert!(v["cmd"].as_str().unwrap().contains("--account z"), "{v}");
    let fail = {
        use crate::wire::typed::Codes;
        let req: super::local::LocalLaunchRequest =
            serde_json::from_value(resume(serde_json::json!({"kind": "named", "name": "gone"})))
                .unwrap();
        let req = req.with_prefs(&Default::default());
        facts_of(|f| super::answer_local(&req, f))
            .unwrap_err()
            .into_fail()
    };
    assert_eq!(fail.code, "account_unavailable");
    assert_eq!(
        fail.data,
        Some(
            serde_json::json!({"requested": "gone", "pinned": false, "listKnown": true, "alternative": "z"})
        )
    );
}
