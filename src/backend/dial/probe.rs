//! 〔MIG-1 续 · `设计/99 §2.1 ⑬` · 主会话裁「后端持有全部 SSH」〕**测试连接**：界面把设置页表单里那台（可能还没保存的）配置交过来，
//! 本机常驻后端组拨号请求（[`super::machine`]）、拨一次（短命探活，不进连接池）、回结局 —— monitor 那条 Tauri 命令
//! `test_remote_connection` 与它手里那份探针退役（原住 `ssh_source.rs`）。
//!
//! 三步，每步的结论都进回包（部分成功照样回，不当错误）：
//!
//! 1. 拨号 ＋ 鉴权 ＋ exec 那台的后端（流模式显式词）：阶段行逐条收进 `stages`（与界面 `ConnectStage` 同形）；ack 不成 ⇒ `sshOk: false`；
//! 2. 读那台后端的首行：是 `hello` ⇒ `backendOk: true` ＋ 人读摘要；超时 / 关了 / 不是 hello ⇒ 「SSH 通了、后端没响应」；
//! 3. 那台声明认 `ping` ⇒ 同一条流上发一次、等应答（控制通道往返）；不认 ⇒ 「后端太旧」；不回 ⇒ 「后端连上了、不能起会话」。
//!
//! ⚠ 阶段行不再逐条流到界面（原先经 Tauri `Channel` 边拨边推）：结局里一并交回，界面在结局到时一次画出来。
//! ⚠ **期限归发起方**（主会话裁 · DL1「值归发起方」）：本 crate 零定时器（`no_timer_guard` 按调用形态禁 `timeout(`），原先 monitor 那两段
//!   等待（hello 8 s · ping 5 s）不在这里；界面那一问的预算按那个量级给（约 15 s，`src/remote-probe.ts`），到点由宿主那侧 `cancel` 打断。
//!   往返毫秒数照旧报：量一次经过时间不是定时器（取的是墙钟 `SystemTime` 的差，不让任何东西自己醒来）。

use copy_core::copy_text;
use serde_json::{json, Value};
use tokio::io::{AsyncWriteExt, BufReader};

use crate::remote_ask::{self, AbortOnDrop};

/// 链路上一行的上限（阶段行 · ack · hello · 应答；hello 行是后端出方向单行，同一个量级）。
const LINE_CAP: u64 = 1024 * 1024;
/// 探针发的那一问的 id（这条链路只有这一问）。
const PING_ID: &str = "probe-ping";

/// 结局（界面 `ConnTestResult` 的形状 ＋ `stages`）。
fn outcome(
    ssh_ok: bool,
    fingerprint: Option<String>,
    endpoint: Option<String>,
    hello: Option<String>,
    message: String,
    stages: Vec<Value>,
) -> Value {
    json!({
        "sshOk": ssh_ok,
        "fingerprint": fingerprint,
        "endpoint": endpoint,
        "backendOk": hello.is_some(),
        "backendHello": hello,
        "message": message,
        "stages": stages,
    })
}

/// `remote-probe` 的本体：`serve` 是链路那一侧（生产 = `dial::uses::run`；判据用替身，拿改好的请求 · 上行读端 · 下行写端）。
pub(crate) async fn probe_with<F, Fut>(
    args: &Value,
    serve: F,
) -> Result<Value, (&'static str, String)>
where
    F: FnOnce(crate::dial::DialRequest, tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (machine, saved, jump) = super::machine::from_args(args)?;
    let command = format!(
        "{} -- {}",
        relay_route_core::BACKEND_LANDING_SHELL,
        crate::STREAM_FLAG_EXPLICIT
    );
    let req = super::machine::request(
        &machine,
        saved.as_ref(),
        jump.as_ref(),
        "stream",
        json!({ "command": command, "stages": true, "probe": true }),
    )?;
    let req = crate::dial::parse_request_value(&req).map_err(|e| {
        (
            "invalid_args",
            crate::common::contract::malformed(&format!("dial request unreadable: {e}")),
        )
    })?;
    let (mut up_w, up_r) = tokio::io::duplex(64 * 1024);
    let (down_w, down_r) = tokio::io::duplex(256 * 1024);
    // 探完即丢 ⇒ 链路那一侧一起收（后端收掉那条链路；探针不进连接池 ⇒ 那条 SSH 连接随之关）。
    let _link = AbortOnDrop(tokio::spawn(serve(req, up_r, down_w)));
    let mut rd = BufReader::new(down_r);
    let mut stages = Vec::new();

    // ① 阶段行 → ack。
    let ack = loop {
        let line = remote_ask::capped_line(&mut rd, LINE_CAP)
            .await
            .map_err(|e| ("failed", e))?;
        let Some(line) = line else {
            return Ok(outcome(
                false,
                None,
                None,
                None,
                copy_text(
                    "beProbe.test.sshFailed",
                    &[("e", &copy_text("beProbe.link.droppedBeforeAck", &[]))],
                ),
                stages,
            ));
        };
        let v: Value = serde_json::from_str(&line).map_err(|e| {
            (
                "failed",
                crate::common::contract::malformed(&format!("unreadable link line: {e}")),
            )
        })?;
        match v.get("stage") {
            Some(st) => stages.push(st.clone()),
            None => break v,
        }
    };
    if ack.get("ok").and_then(Value::as_bool) != Some(true) {
        // 握手失败（含 host key 不匹配被拒）：看到过的指纹**刻意不回**（失败时给一个指纹，界面那条「固化指纹」的路
        // 就可能把一把失配的 key 固化进去）。
        let why = ack
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        return Ok(outcome(
            false,
            None,
            None,
            None,
            copy_text("beProbe.test.sshFailed", &[("e", &why)]),
            stages,
        ));
    }
    let fingerprint = ack
        .get("fingerprint")
        .and_then(Value::as_str)
        .map(str::to_string);
    let endpoint = ack
        .get("endpoint")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| machine.endpoints().first().map(|(h, p)| format!("{h}:{p}")));

    // ② 首行 hello。
    let first = match remote_ask::capped_line(&mut rd, LINE_CAP).await {
        Ok(None) => None,
        Err(e) => {
            return Ok(outcome(
                true,
                fingerprint,
                endpoint,
                None,
                copy_text("beProbe.test.probeFailed", &[("e", &e)]),
                stages,
            ))
        }
        Ok(Some(l)) => serde_json::from_str::<Value>(&l)
            .ok()
            .filter(|v| v.get("kind").and_then(Value::as_str) == Some("hello")),
    };
    let Some(hello) = first else {
        return Ok(outcome(
            true,
            fingerprint,
            endpoint,
            None,
            copy_text("beProbe.test.noHello", &[]),
            stages,
        ));
    };
    let head = crate::wire::hello_summary(&hello);

    // ③ 控制通道往返。
    let accepts_ping = hello
        .get("commands")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|c| c.as_str() == Some("ping")));
    let (control, message) = if !accepts_ping {
        (
            copy_text("beProbe.control.unsupported", &[]),
            copy_text("beProbe.test.noControl", &[]),
        )
    } else {
        // 量一次往返经过的墙钟（不是节拍：只读两次钟、算个差）。
        let t0 = std::time::SystemTime::now();
        let asked = up_w
            .write_all(
                format!(
                    "{}\n",
                    json!({ "id": PING_ID, "cmd": "ping", "args": null })
                )
                .as_bytes(),
            )
            .await;
        let answered = match asked {
            Err(e) => Err(e.to_string()),
            Ok(()) => loop {
                match remote_ask::capped_line(&mut rd, LINE_CAP).await {
                    Ok(Some(l)) => {
                        let v: Value = serde_json::from_str(&l).unwrap_or(Value::Null);
                        if v.get("kind").and_then(Value::as_str) == Some("reply")
                            && v.get("id").and_then(Value::as_str) == Some(PING_ID)
                        {
                            break if v.get("ok").and_then(Value::as_bool) == Some(true) {
                                Ok(())
                            } else {
                                Err(v
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string())
                            };
                        }
                    }
                    Ok(None) => break Err(copy_text("beProbe.link.closed", &[])),
                    Err(e) => break Err(e),
                }
            },
        };
        match answered {
            Ok(()) => (
                format!(
                    "control=ok({}ms)",
                    t0.elapsed().map(|d| d.as_millis()).unwrap_or(0)
                ),
                copy_text("beProbe.test.ok", &[]),
            ),
            Err(e) => (
                format!("control=failed({e})"),
                copy_text("beProbe.test.controlDown", &[]),
            ),
        }
    };
    let _ = up_w.shutdown().await;
    Ok(outcome(
        true,
        fingerprint,
        endpoint,
        Some(format!("{head} {control}")),
        message,
        stages,
    ))
}

/// 生产的 `remote-probe`：真拨号。
pub async fn answer_probe(args: &Value) -> Result<Value, (&'static str, String)> {
    probe_with(args, |req, up_r, mut down_w| async move {
        let stages = crate::dial::StageSink::new(true);
        crate::dial::uses::run(&req, &stages, up_r, &mut down_w).await;
    })
    .await
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_probe_tests.rs"]
mod tests;
