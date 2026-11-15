//! 一次性的 `codex app-server`（stdio，一行一条 JSON）：握手两行 · 一条请求 · 从输出里认回包。报用量与按轮分叉共用。
//!
//! 依据（codex-rs，核到 tag `rust-v0.167.0`，只读源码、没起真 codex）：
//! - 一行一条 JSON、不带 `jsonrpc` 那一格；`initialize` 必填只有 `clientInfo.name` · `clientInfo.version`，
//!   `initialized` 服务端不等，照官方客户端也发；之后那一条请求的 `id` 是 2。
//! - 回包是带同一个 `id` 的那一行：`result` 或 `error{code, message}`。
//!
//! 起法：`codex app-server`，`CODEX_HOME` 指那个号的目录；读到回包就收掉进程，30 秒没等到也收掉、照实报错。

use crate::platform::child::{Child, Deadline, Heard, Until};
use serde_json::{json, Value};
use std::path::Path;

/// 起哪个程序 · 带什么参数。
pub(crate) const PROGRAM: &str = "codex";
pub(crate) const ARGS: &[&str] = &["app-server"];
/// 握手头一行（`clientInfo.version` 只进它自己的客户端标识，这里交一个固定值）。
pub(crate) const INITIALIZE: &str =
    r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"cc-monitor","version":"0"}}}"#;
/// 握手第二行。
pub(crate) const INITIALIZED: &str = r#"{"method":"initialized"}"#;
/// 那一条请求的 `id`。
const REQUEST_ID: u64 = 2;
/// 等回包最多多久。
const WITHIN: Deadline = Deadline::secs(30);

/// 那一条请求：`{"id":2,"method":<方法>,"params":<参数>}`（一行）。
pub(crate) fn request(method: &str, params: &Value) -> String {
    json!({"id": REQUEST_ID, "method": method, "params": params}).to_string()
}

/// 这一行是不是那一条请求的回包（`id` 是 2）。
pub(crate) fn is_reply(line: &str) -> bool {
    serde_json::from_str::<Value>(line.trim())
        .ok()
        .and_then(|v| v.get("id").and_then(Value::as_u64))
        == Some(REQUEST_ID)
}

/// 输出里那一条请求的回包 ⇒ 它的 `result`；是 `error` ⇒ 它说的那一句；没有那一行 ⇒ 照实说没等到。
pub(crate) fn reply_in(stdout: &str) -> Result<Value, String> {
    let v = stdout
        .lines()
        .filter(|l| is_reply(l))
        .find_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .ok_or_else(|| "codex app-server: no reply to the request".to_string())?;
    if let Some(e) = v.get("error") {
        let said = e.get("message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("codex app-server: {said}"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| "codex app-server: reply without result".to_string())
}

/// 起一次 app-server（`CODEX_HOME` ＝ `home`）、握手、发 `method` 那一条、读到回包就收掉 ⇒ 回包的 `result`。
pub(crate) fn ask(home: &Path, method: &str, params: &Value) -> Result<Value, String> {
    let at = crate::platform::shell::agent_launch(PROGRAM, None)?;
    ask_in(at.program, at.lead, home, method, params, WITHIN)
}

/// [`ask`] 的本体：起哪个程序（＋ 前导参数）与期限是参数（判据交替身进程、把期限注入成很短）。
/// 期限到了还没等到回包 ⇒ 收掉它、照实说没等到。
pub(crate) fn ask_in(
    program: std::ffi::OsString,
    lead: Vec<std::ffi::OsString>,
    home: &Path,
    method: &str,
    params: &Value,
    within: Deadline,
) -> Result<Value, String> {
    #[expect(
        clippy::disallowed_methods,
        reason = "起那个号自己的 codex app-server 叉一条会话到某一轮为止（适配层登记的那一形，argv 不过 shell、stdin 只喂握手与那一条请求、期限 30 秒）：新会话由 Codex 自己写进那个号的目录，后端不写；不许加第二种用途"
    )]
    let mut child = Child::new(program)
        .args(lead)
        .args(ARGS)
        .current_dir(home)
        .env(super::parse::HOME_ENV, home);
    for k in super::sign_in::FACE.env_remove {
        child = child.env_remove(k);
    }
    let mut conv = child.converse().map_err(|e| e.to_string())?;
    let heard = conv
        .take_heard()
        .ok_or_else(|| "codex app-server: no output".to_string())?;
    for line in [INITIALIZE, INITIALIZED, &request(method, params)] {
        conv.say(line).map_err(|e| e.to_string())?;
    }
    let until = Until::after(within);
    let mut out = String::new();
    loop {
        match next_heard(&heard, until, within) {
            Ok(Heard::Out(l)) => {
                let done = is_reply(&l);
                out.push_str(&l);
                out.push('\n');
                if done {
                    break;
                }
            }
            Ok(Heard::Err(_)) => {}
            Ok(Heard::Exited(_)) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // 放手 `conv` ⇒ 收掉它（不回不退的那一种）。
                drop(conv);
                return Err(format!(
                    "codex app-server: no reply within {}s",
                    within.shown_secs()
                ));
            }
        }
    }
    // 放手 `conv` ⇒ 收掉它（读到回包时它还没退）。
    drop(conv);
    reply_in(&out)
}

/// 它说的下一件，至多等到 `until`（`within` 从起它那一刻算；钟面装不下 ⇒ 按整段期限等）。
#[expect(
    clippy::disallowed_methods,
    reason = "一次性 app-server 读到回包就收的那一种：等它回那一条请求的有界等待（期限即交进来的 `within`，生产是 `WITHIN`，从起它那一刻算）；它卡住不回时没有它这一下就一直挂着"
)]
fn next_heard(
    heard: &std::sync::mpsc::Receiver<Heard>,
    until: Option<Until>,
    within: Deadline,
) -> Result<Heard, std::sync::mpsc::RecvTimeoutError> {
    let left = until.map_or_else(|| within.as_ms(), Until::left_ms);
    heard.recv_timeout(std::time::Duration::from_millis(left))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/app_server_tests.rs"]
mod tests;
