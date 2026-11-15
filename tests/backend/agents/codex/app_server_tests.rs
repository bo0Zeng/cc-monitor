//! Codex 一次性 app-server 那几行与读法：握手逐字 · 请求那一行 · 从输出里认回包。
//! 形照 codex-rs `rust-v0.167.0` 的 app-server 协议（一行一条 JSON、不带 `jsonrpc` 那一格）手写，不起真 codex。

use super::*;
use serde_json::json;

#[test]
fn the_handshake_lines_are_initialize_then_initialized() {
    assert_eq!(
        INITIALIZE,
        r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"cc-monitor","version":"0"}}}"#
    );
    assert_eq!(INITIALIZED, r#"{"method":"initialized"}"#);
    let v: Value = serde_json::from_str(INITIALIZE).expect("是一行 JSON");
    assert!(v.get("jsonrpc").is_none());
}

#[test]
fn the_request_line_carries_the_method_and_params_as_given() {
    let line = request(
        "thread/fork",
        &json!({"threadId": "s1", "lastTurnId": "t2"}),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({"id": 2, "method": "thread/fork", "params": {"threadId": "s1", "lastTurnId": "t2"}})
    );
    assert!(!line.contains('\n'));
}

/// 先 `"id":1`（握手的回包）再 `"id":2` ⇒ 取 `"id":2` 那一行的 `result`；它是 `error` ⇒ 照实报错；没有那一行 ⇒ 报错。
#[test]
fn the_reply_is_the_id_2_line() {
    let out = format!(
        "{}\n{}\n",
        json!({"id": 1, "result": {"userAgent": "x"}}),
        json!({"id": 2, "result": {"ok": true}})
    );
    assert!(!is_reply(out.lines().next().unwrap()));
    assert!(is_reply(out.lines().nth(1).unwrap()));
    assert!(!is_reply("not json"));
    assert_eq!(reply_in(&out), Ok(json!({"ok": true})));
    let err = format!(
        "{}\n",
        json!({"id": 2, "error": {"code": -32600, "message": "turn is in progress"}})
    );
    assert!(reply_in(&err).unwrap_err().contains("turn is in progress"));
    assert!(reply_in(&format!("{}\n", json!({"id": 1, "result": {}}))).is_err());
    assert!(reply_in("").is_err());
}

/// 起进程那一手（替身进程，不起真 codex）：读完握手两行与那一条请求、回 `"id":2` ⇒ 交回它的 `result`；
/// 不回不退 ⇒ 到期收掉、照实说没等到回包（期限在判据里注入成很短）。
#[cfg(unix)]
#[test]
fn the_server_is_fed_the_three_lines_and_killed_when_it_never_answers() {
    let home = test_home::scratch("codex-app-server");
    let sh = |script: &str| {
        (
            std::ffi::OsString::from("sh"),
            vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from(script),
            ],
        )
    };
    let (p, lead) = sh(r#"read a; read b; read c; echo '{"id":2,"result":{"ok":true}}'"#);
    assert_eq!(
        ask_in(p, lead, &home, "x/y", &json!({}), Deadline::secs(10)),
        Ok(json!({"ok": true}))
    );
    let (p, lead) = sh("sleep 30");
    let err = ask_in(p, lead, &home, "x/y", &json!({}), Deadline::millis(300)).unwrap_err();
    assert!(err.contains("no reply within"), "到期收掉那一句：{err}");
    let _ = std::fs::remove_dir_all(&home);
}
