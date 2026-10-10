//! 测试连接：界面把设置页表单里那台（可能还没保存的）配置交过来，本机常驻后端组拨号请求（[`super::machine`]）、
//! 拨一次（短命探活，不进连接池）、回结局。三步，每步的结论都进回包（部分成功照样回，不当错误）：
//!
//! 1. 拨号 ＋ 鉴权 ＋ exec 那台的后端（流模式显式词）：阶段行逐条推成 `stage` 格（与界面 `ConnectStage` 同形）；ack 不成 ⇒ `sshOk: false`；
//! 2. 读那台后端的首行：是 `hello` ⇒ `backendOk: true` ＋ 人读摘要；超时 / 关了 / 不是 hello ⇒ 「SSH 通了、后端没响应」；
//! 3. 那台声明认 `ping` ⇒ 同一条流上发一次、等应答（控制通道往返）；不认 ⇒ 「后端太旧」；不回 ⇒ 「后端连上了、不能起会话」。
//!
//! 边拨边推：每走一段往本连接的应答通道推一帧 `probe {ticket, cell}`（`stage` 握手那几行 → `reached: ssh` → `reached: hello` →
//! `reached: control`），结局是最后一格（`end`）；monitor 把它们交进界面订的 `probe-progress/<ticket>`，到点没等到结局时最后一格说得出停在哪一段。
//! 期限归发起方：本 crate 零定时器；界面那一问的预算约 15 s（`src/frontend/ui/remote-probe.ts`），到点由宿主那侧 `cancel` 打断。
//! 往返毫秒数照旧报：量一次墙钟差不是定时器。

use copy_core::copy_text;
use serde_json::{json, Value};
use tokio::io::{AsyncWriteExt, BufReader};

use crate::dial::remote_ask::{self, AbortOnDrop};
use crate::stream::wire::Frame;

/// 链路上一行的上限（阶段行 · ack · hello · 应答；hello 行是后端出方向单行，同一个量级）。
const LINE_CAP: u64 = 1024 * 1024;
/// 探针发的那一问的 id（这条链路只有这一问）。
const PING_ID: &str = "probe-ping";

/// 票的上限与字符集（界面给的是一个 UUID；本后端只当不透明的串回填，但它会进帧 ⇒ 有界、可打印）。
const TICKET_MAX: usize = 64;

/// 结局（界面 `ConnTestResult` 的形状）。
fn outcome(
    ssh_ok: bool,
    fingerprint: Option<String>,
    endpoint: Option<String>,
    hello: Option<String>,
    gaps: Vec<GapCount>,
    message: String,
    detail: Option<String>,
) -> Value {
    json!({
        "sshOk": ssh_ok,
        "fingerprint": fingerprint,
        "endpoint": endpoint,
        "backendOk": hello.is_some(),
        "backendHello": hello,
        "backendGaps": gaps,
        "message": message,
        "detail": detail,
    })
}

/// 没过的那几形的复制详情：时刻 · 机器 · 命令 · 下层原话（那一句里不接原话，原话在这里）。
fn failed_detail(raw: Option<&str>) -> Option<String> {
    Some(crate::stream::detail::of_run("remote-probe", raw))
}

/// 测试连接那一行给人看的几格要的事实：后端版本（BUILD_ID）· 这台说做不到几项；
/// 做不到的按码分类、每类几项交出去（那句人话归 monitor：`control-said.ts::unavailableReason`，与置灰那一句同一个家 ——
/// `wire::Unavailable` 头注「那句人话今天归 monitor」）。键值对那一形（`wire::hello_summary`）只进日志。
struct HelloFacts {
    build: String,
    gaps: usize,
    by_code: Vec<GapCount>,
}

/// `hello.unavailable` 的一项（线上形状同 `wire::Unavailable`；那边只要写，这边只要读）。
#[derive(serde::Deserialize)]
struct Gap {
    command: String,
    code: String,
}

/// `backendGaps` 的一项：这台说做不到的一类（码）· 这一类几项。
#[derive(serde::Serialize)]
struct GapCount {
    code: String,
    count: usize,
}

fn hello_facts(hello: &Value) -> HelloFacts {
    use std::collections::{BTreeMap, BTreeSet};
    // 逐项收，坏项丢掉、不连累整张。
    let mut by_code: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in hello
        .get("unavailable")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Ok(g) = serde_json::from_value::<Gap>(e.clone()) {
            by_code.entry(g.code).or_default().insert(g.command);
        }
    }
    let cant: BTreeSet<&String> = by_code.values().flatten().collect();
    HelloFacts {
        build: hello
            .get("build_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| copy_text("beProbe.hello.noBuild", &[])),
        gaps: cant.len(),
        by_code: by_code
            .iter()
            .map(|(code, cmds)| GapCount {
                code: code.clone(),
                count: cmds.len(),
            })
            .collect(),
    }
}

/// 进度格的出口：本连接的应答通道（不丢、与应答同序）。
pub(crate) struct Cells<'a> {
    ticket: String,
    out: &'a tokio::sync::mpsc::Sender<Frame>,
}

impl Cells<'_> {
    async fn push(&self, cell: Value) -> Result<(), (&'static str, String)> {
        self.out
            .send(Frame::Probe {
                ticket: self.ticket.clone(),
                cell,
            })
            .await
            .map_err(|_| ("failed", copy_text("beProbe.client.gone", &[])))
    }
}

/// 读票（`ticket`）：非空、有界、只许字母数字与 `-`。
fn ticket_of(args: &Value) -> Result<String, (&'static str, String)> {
    let t = args
        .get("ticket")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if t.is_empty()
        || t.len() > TICKET_MAX
        || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`ticket` must be 1..=64 chars of [A-Za-z0-9-]"),
        ));
    }
    Ok(t.to_string())
}

/// `remote-probe` 的本体：`serve` 是链路那一侧（生产 = `dial::uses::run`；判据用替身，拿改好的请求 · 上行读端 · 下行写端）；
/// `out` 是进度格的出口（生产 = 本连接的应答通道）。结局推成最后一格（`end`），应答本身不带体。
pub(crate) async fn probe_with<F, Fut>(
    args: &Value,
    serve: F,
    out: &tokio::sync::mpsc::Sender<Frame>,
) -> Result<(), (&'static str, String)>
where
    F: FnOnce(crate::dial::DialRequest, tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let cells = Cells {
        ticket: ticket_of(args)?,
        out,
    };
    let end = run(args, serve, &cells).await?;
    cells.push(json!({ "end": end })).await
}

async fn run<F, Fut>(
    args: &Value,
    serve: F,
    cells: &Cells<'_>,
) -> Result<Value, (&'static str, String)>
where
    F: FnOnce(crate::dial::DialRequest, tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (machine, _, _) = super::machine::from_args(args)?;
    let command = format!(
        "{} -- {}",
        relay_route_core::BACKEND_LANDING_SHELL,
        crate::STREAM_FLAG_EXPLICIT
    );
    let req = crate::dial::parse_request_value(&json!({
        "machine": args.get("machine"),
        "saved": args.get("saved"),
        "jump": args.get("jump"),
        "use": "stream",
        "command": command,
        "stages": true,
        "probe": true,
    }))?;
    let (mut up_w, up_r) = tokio::io::duplex(64 * 1024);
    let (down_w, down_r) = tokio::io::duplex(256 * 1024);
    // 探完即丢 ⇒ 链路那一侧一起收（后端收掉那条链路；探针不进连接池 ⇒ 那条 SSH 连接随之关）。
    let _link = AbortOnDrop(tokio::spawn(serve(req, up_r, down_w)));
    let mut rd = BufReader::new(down_r);

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
                Vec::new(),
                copy_text(
                    "beProbe.test.sshFailed",
                    &[("why", &copy_text("beProbe.link.droppedBeforeAck", &[]))],
                ),
                failed_detail(None),
            ));
        };
        let v: Value = serde_json::from_str(&line).map_err(|e| {
            (
                "failed",
                crate::common::contract::malformed(&format!("unreadable link line: {e}")),
            )
        })?;
        match v.get("stage") {
            Some(st) => cells.push(json!({ "stage": st })).await?,
            None => break v,
        }
    };
    if ack.get("ok").and_then(Value::as_bool) != Some(true) {
        // 握手失败（含 host key 不匹配被拒）：看到过的指纹**刻意不回**（失败时给一个指纹，界面那条「固化指纹」的路
        // 就可能把一把失配的 key 固化进去）。
        // 那一句（原因词，拨号那一侧写好的）进句子；那一侧的复制详情原样带上（原话在那里）。
        let why = ack
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let detail = ack
            .get("detail")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| failed_detail(None));
        return Ok(outcome(
            false,
            None,
            None,
            None,
            Vec::new(),
            copy_text("beProbe.test.sshFailed", &[("why", &why)]),
            detail,
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
    cells.push(json!({ "reached": "ssh" })).await?;

    // ② 首行 hello。
    let first = match remote_ask::capped_line(&mut rd, LINE_CAP).await {
        Ok(None) => None,
        Err(e) => {
            return Ok(outcome(
                true,
                fingerprint,
                endpoint,
                None,
                Vec::new(),
                copy_text("beProbe.test.probeFailed", &[]),
                failed_detail(Some(&e)),
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
            Vec::new(),
            copy_text("beProbe.test.noHello", &[]),
            failed_detail(None),
        ));
    };
    // 键值对那一形只进日志（`wire::hello_summary`）；界面那一行由下面三格拼（文案表）。
    tracing::info!("测试连接：{}", crate::stream::wire::hello_summary(&hello));
    let facts = hello_facts(&hello);
    let (build, gaps) = (facts.build.clone(), facts.gaps.to_string());
    cells.push(json!({ "reached": "hello" })).await?;

    // ③ 控制通道往返。
    let accepts_ping = hello
        .get("commands")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|c| c.as_str() == Some("ping")));
    let (line, message, detail) = if !accepts_ping {
        // 那台的后端不认控制命令：全产品同一句（按名字取）。
        let said = copy_core::backend_old(machine.name());
        (
            copy_text(
                "beProbe.hello.tooOld",
                &[("build", &build), ("said", &said)],
            ),
            said,
            failed_detail(None),
        )
    } else {
        // 量一次往返经过的墙钟（不是节拍：只读两次钟、算个差）。
        let t0 = crate::common::time::now();
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
        if answered.is_ok() {
            cells.push(json!({ "reached": "control" })).await?;
        }
        match answered {
            Ok(()) => {
                let ms = t0.elapsed().map(|d| d.as_millis()).unwrap_or(0).to_string();
                let line = if facts.gaps == 0 {
                    copy_text("beProbe.hello.ok", &[("build", &build), ("ms", &ms)])
                } else {
                    copy_text(
                        "beProbe.hello.okGaps",
                        &[("build", &build), ("gaps", &gaps), ("ms", &ms)],
                    )
                };
                (line, copy_text("beProbe.test.ok", &[]), None)
            }
            Err(e) => (
                copy_text("beProbe.hello.noAnswer", &[("build", &build)]),
                copy_text("beProbe.test.controlDown", &[]),
                failed_detail(Some(&e)),
            ),
        }
    };
    let _ = up_w.shutdown().await;
    Ok(outcome(
        true,
        fingerprint,
        endpoint,
        Some(line),
        facts.by_code,
        message,
        detail,
    ))
}

/// 生产的 `remote-probe`：真拨号；进度格进本连接的应答通道。
pub async fn answer_probe(
    args: &Value,
    replies: &tokio::sync::mpsc::Sender<Frame>,
) -> Result<(), (&'static str, String)> {
    probe_with(
        args,
        |req, up_r, mut down_w| async move {
            let stages = crate::dial::StageSink::new(true);
            crate::dial::uses::run(&req, &stages, up_r, &mut down_w).await;
        },
        replies,
    )
    .await
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_probe_tests.rs"]
mod tests;
