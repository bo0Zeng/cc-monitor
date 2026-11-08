//! 首次运行「开始用」那份成品：两步各自照事实打勾，每步的名字 · 那一句 · 按钮的字与去处都由这里写好。

use super::*;
use copy_core::copy_text;
use serde_json::json;

/// 成品序列化回 JSON 比（同帧面那一跳）。
fn product(terminal: bool, remotes: u64, skipped: bool) -> serde_json::Value {
    serde_json::to_value(super::product(terminal, remotes, skipped)).unwrap()
}
fn args(a: &serde_json::Value) -> Result<FirstRunReq, crate::common::said::Said> {
    crate::wire::typed::take::<FirstRunReq>(a)
}

#[test]
fn 两步照事实打勾_字与去处由这里写好() {
    assert_eq!(
        product(false, 0, false),
        json!({"steps": [
            {"id": "terminal", "done": false,
             "name": copy_text("beFirstRun.terminal.name", &[]),
             "hint": copy_text("beFirstRun.terminal.hint", &[]),
             "act": {"label": copy_text("beFirstRun.terminal.act", &[]), "go": "aliases"}},
            {"id": "remote", "done": false,
             "name": copy_text("beFirstRun.remote.name", &[]),
             "hint": copy_text("beFirstRun.remote.hint", &[]),
             "act": {"label": copy_text("mobHosts.fab.aria", &[]), "go": "add_machine"}},
        ], "skipped": false})
    );
    let done = product(true, 1, false);
    assert_eq!(done["steps"][0]["done"], json!(true));
    assert_eq!(
        done["steps"][1]["done"],
        json!(true),
        "机器表里有一台 ⇒ 那一步打勾"
    );
    assert_eq!(
        product(false, 0, true)["skipped"],
        json!(true),
        "点过「跳过」原样带回"
    );
}

#[test]
fn 没有给登录命名那一步() {
    let p = product(false, 0, false);
    let ids: Vec<&str> = p["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["terminal", "remote"]);
}

#[test]
fn 入参是空的_台数不由问的那一方带() {
    assert!(args(&json!({})).is_ok());
    assert_eq!(
        args(&json!({"remotes": 3})).unwrap_err().code.as_deref(),
        Some("bad_args")
    );
}
