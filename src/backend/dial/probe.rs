//! 测试连接：界面把设置页表单里那台（可能还没保存的）配置交过来，本机常驻后端组拨号请求（[`super::machine`]）、
//! 拨一次（短命探活，不进连接池）、回结局。三步，每步的结论都进结局（部分成功照样回，不当错误）：
//!
//! 1. 拨号 ＋ 鉴权 ＋ exec 那台的后端（流模式显式词）：阶段行逐条写成一格；ack 不成 ⇒ 停在这里；
//! 2. 读那台后端的首行：是 `hello` ⇒ 人读摘要；超时 / 关了 / 不是 hello ⇒ 「SSH 通了、后端没响应」；
//! 3. 那台声明认 `ping` ⇒ 同一条流上发一次、等应答（控制通道往返）；不认 ⇒ 「后端太旧」；不回 ⇒ 「后端连上了、不能起会话」。
//!
//! 边拨边推：每走一步往本连接的应答通道推一帧 `probe {ticket, cell}`，每格是写好的那一句 ＋ 语气 ＋ 四段（[`ProbeCell`]），
//! 最后一格带结局；monitor 把它们交进界面订的 `probe-progress/<ticket>`，界面照画、不按码取字。
//! 一趟的期限在这里（[`PROBE_BUDGET`]）：到点没等到结局 ⇒ 收掉链路，结局那一格说等了多久、最后一步是哪一句。

use copy_core::copy_text;
use serde_json::{json, Value};
use tokio::io::{AsyncWriteExt, BufReader};

use crate::common::cells::{Tone, Words};
use crate::dial::remote_ask::{self, AbortOnDrop};
use crate::wire::Frame;

/// 链路上一行的上限（阶段行 · ack · hello · 应答；hello 行是后端出方向单行，同一个量级）。
const LINE_CAP: u64 = 1024 * 1024;
/// 探针发的那一问的 id（这条链路只有这一问）。
const PING_ID: &str = "probe-ping";

/// 票的上限与字符集（界面给的是一个 UUID；本后端只当不透明的串回填，但它会进帧 ⇒ 有界、可打印）。
const TICKET_MAX: usize = 64;

/// 一趟测试连接最多等多久（握手 ＋ hello ＋ 一次往返，秒级；给足余量）。
#[allow(
    clippy::disallowed_methods,
    reason = "测试连接一趟的期限（`probe::PROBE_BUDGET`）：只交给 `probe_with` 那一次有界等待，到点收掉链路、结局说停在哪一步；不驱动循环"
)]
pub(crate) const PROBE_BUDGET: std::time::Duration = std::time::Duration::from_millis(15_000);

/// 测试连接那一行的一段到哪了：还没到 · 过了 · 没过。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum SegMark {
    #[default]
    Wait,
    Ok,
    Fail,
}

/// 测试连接那一行的四段：解析 · 连接 · 认证 · 就绪。
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Segs {
    resolve: SegMark,
    connect: SegMark,
    auth: SegMark,
    ready: SegMark,
}

impl Segs {
    fn all(&mut self) -> [&mut SegMark; 4] {
        [
            &mut self.resolve,
            &mut self.connect,
            &mut self.auth,
            &mut self.ready,
        ]
    }

    /// 前 `n` 段过了。
    fn reached(mut self, n: usize) -> Self {
        self.all()
            .into_iter()
            .take(n)
            .for_each(|m| *m = SegMark::Ok);
        self
    }

    /// 没过：第一处还没过的那一段打 ✗（已有 ✗ 的不再多打）。
    fn failed(mut self) -> Self {
        let all = self.all();
        if !all.iter().any(|m| **m == SegMark::Fail) {
            if let Some(m) = all.into_iter().find(|m| **m != SegMark::Ok) {
                *m = SegMark::Fail;
            }
        }
        self
    }
}

/// 进度流的一格：写好的那一句 · 语气 · 四段；最后一格带结局。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ProbeCell {
    /// 握手那一行 · 走到哪一步 · 结局那一句（结局不接下层原话，原话在复制详情）。
    said: Words,
    tone: Tone,
    segs: Segs,
    end: Option<ConnTestResult>,
}

/// 测试连接的结局（进度流最后那一格的 `end`）。
#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnTestResult {
    /// 那台后端的三格（版本 · 能用几项 / 做不到几项 · 往返毫秒），按文案表拼好的一句；没回 hello ⇒ `null`。
    backend_hello: Option<String>,
    /// 这台说做不到的那几类（空 = 没有 / 没回 hello）。
    backend_gaps: Vec<GapCount>,
    /// 没过时的复制详情；都过了 ⇒ `null`。
    detail: Option<String>,
}

/// 没走到 hello 就停了：结局那一格（那一句 ＋ 复制详情；四段照推出去的最后那一格、第一处没过的打 ✗）。
fn stopped(cells: &Cells<'_>, said: String, detail: Option<String>) -> ProbeCell {
    ProbeCell {
        said: said.into(),
        tone: Tone::Fail,
        segs: cells.last().1.failed(),
        end: Some(ConnTestResult {
            detail,
            ..ConnTestResult::default()
        }),
    }
}

/// 没过的那几形的复制详情：时刻 · 机器 · 命令 · 下层原话（那一句里不接原话，原话在这里）。
fn failed_detail(raw: Option<&str>) -> Option<String> {
    Some(crate::wire::detail::of_run("remote-probe", raw))
}

/// 测试连接那一行给人看的几格要的事实：后端版本（BUILD_ID）· 这台说做不到几项；
/// 做不到的按码分类、每类几项 ＋ 那一句（与置灰那一句同一个家 `copy_core::unavailable`）。键值对那一形（`wire::hello_summary`）只进日志。
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

/// `backendGaps` 的一项：这台说做不到的一类（码）· 这一类几项 · 那一句。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
struct GapCount {
    code: String,
    count: usize,
    said: String,
}

/// `machine`：那一句里怎么称呼那台（表单里的名字）。
fn hello_facts(hello: &Value, machine: &str) -> HelloFacts {
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
                said: copy_core::unavailable(code, machine),
                code: code.clone(),
                count: cmds.len(),
            })
            .collect(),
    }
}

/// 进度格的出口：本连接的应答通道（不丢、与应答同序）。记着推出去的最后那一格（到点时结局照它说）。
pub(crate) struct Cells<'a> {
    ticket: String,
    out: &'a tokio::sync::mpsc::Sender<Frame>,
    last: std::sync::Mutex<(String, Segs)>,
}

impl Cells<'_> {
    fn last(&self) -> (String, Segs) {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    async fn send(&self, cell: ProbeCell) -> Result<(), (&'static str, String)> {
        let (said, segs) = (cell.said.0.clone(), cell.segs);
        self.out
            .send(Frame::Probe(crate::wire::Probe {
                ticket: self.ticket.clone(),
                cell: serde_json::to_value(&cell).unwrap_or(Value::Null),
            }))
            .await
            .map_err(|_| ("failed", copy_text("beProbe.client.gone", &[])))?;
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = (said, segs);
        Ok(())
    }
}

/// 走了一步的那一格：那一句 ＋ 语气 ＋ 四段。
fn step(said: String, tone: Tone, segs: Segs) -> ProbeCell {
    ProbeCell {
        said: said.into(),
        tone,
        segs,
        end: None,
    }
}

/// 握手那一行 ⇒ 那一格（四段在 `segs` 上往前走）。
fn stage_line(st: &ssh_dial::Stage, segs: Segs) -> ProbeCell {
    use ssh_dial::Stage;
    match st {
        Stage::Dialing { endpoint } => step(
            copy_text("beProbe.stage.dial", &[("endpoint", endpoint)]),
            Tone::Now,
            segs,
        ),
        Stage::HostKey {
            endpoint,
            fingerprint,
        } => step(
            copy_text(
                "beProbe.stage.fingerprint",
                &[("endpoint", endpoint), ("fingerprint", fingerprint)],
            ),
            Tone::Now,
            segs.reached(2),
        ),
        Stage::Failed { endpoint, reason } => step(
            copy_text(
                "beProbe.stage.failed",
                &[("endpoint", endpoint), ("reason", reason)],
            ),
            Tone::Warn,
            segs,
        ),
        Stage::Won { endpoint } => step(
            copy_text("beProbe.stage.won", &[("endpoint", endpoint)]),
            Tone::Now,
            segs.reached(2),
        ),
        Stage::Auth { ok: true, .. } => step(
            copy_text("beProbe.stage.authOk", &[]),
            Tone::Now,
            segs.reached(3),
        ),
        Stage::Auth { ok: false, detail } => {
            let said = detail
                .clone()
                .unwrap_or_else(|| copy_text("reason.io.unknown", &[]));
            step(
                copy_text("beProbe.stage.authFailed", &[("said", &said)]),
                Tone::Fail,
                Segs {
                    auth: SegMark::Fail,
                    ..segs.reached(2)
                },
            )
        }
        Stage::Established => step(copy_text("beProbe.stage.ready", &[]), Tone::Now, segs),
    }
}

/// `remote-probe` 的入参：界面交来的票 ＋ 那台的配置（可能还没保存）· 已保存的那一份 · 跳板那一台。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ProbeReq {
    /// 1..=64 个 `[A-Za-z0-9-]`；进度帧 `probe` 原样回填。
    pub(crate) ticket: String,
    /// 那台的配置。
    pub(crate) machine: super::pick::MachineConfig,
    /// 跳板那一台的配置（可缺）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) jump: Option<super::pick::MachineConfig>,
}

crate::wire::typed::codes! {
    /// `remote-probe` 的码。
    pub(crate) enum ProbeErr {
        /// 跳板那一台的配置不对。
        BadJump = "bad_jump",
        /// 进度帧发不出去（本连接没了）· 读链路那一侧出错。
        Failed = "failed",
    }
}

/// `remote-probe`（帧面硬臂：进度格走本连接的应答通道；本体真异步，`cancel` 能在 await 点打断）。结局推成最后一格，应答本身不带体。
pub(crate) struct RemoteProbe;

impl crate::wire::typed::Command for RemoteProbe {
    const NAME: &'static str = "remote-probe";
    type Req = ProbeReq;
    type Reply = ();
    type Err = ProbeErr;
}

/// 票：非空、有界、只许字母数字与 `-`。
fn ticket_ok(t: &str) -> Result<String, (&'static str, String)> {
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
    req: &ProbeReq,
    serve: F,
    out: &tokio::sync::mpsc::Sender<Frame>,
) -> Result<(), (&'static str, String)>
where
    F: FnOnce(crate::dial::DialRequest, tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let cells = Cells {
        ticket: ticket_ok(&req.ticket)?,
        out,
        last: std::sync::Mutex::new((copy_text("beProbe.stage.notStarted", &[]), Segs::default())),
    };
    let pick = super::pick::Pick {
        machine: &req.machine,
        jump: req.jump.as_ref(),
        prefer: None,
    };
    #[allow(
        clippy::disallowed_methods,
        reason = "测试连接一趟的有界等待（`PROBE_BUDGET`）：到点丢掉那一趟（链路随之收），结局说停在哪一步"
    )]
    let end = match tokio::time::timeout(PROBE_BUDGET, run(&pick, serve, &cells)).await {
        Ok(end) => end?,
        Err(_) => {
            let dur = copy_core::format_elapsed(PROBE_BUDGET);
            let said = copy_text(
                "beProbe.test.stalled",
                &[("dur", &dur), ("last", &cells.last().0)],
            );
            stopped(&cells, said, None)
        }
    };
    cells.send(end).await
}

async fn run<F, Fut>(
    pick: &super::pick::Pick<'_>,
    serve: F,
    cells: &Cells<'_>,
) -> Result<ProbeCell, (&'static str, String)>
where
    F: FnOnce(crate::dial::DialRequest, tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (machine, _) = pick.machines()?;
    let command = format!(
        "{} -- {}",
        relay_route_core::BACKEND_LANDING_SHELL,
        crate::wire::cli_words::STREAM_FLAG_EXPLICIT
    );
    let req = pick.request(
        "stream",
        json!({ "command": command, "stages": true, "probe": true }),
    )?;
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
            return Ok(stopped(
                cells,
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
            Some(st) => {
                let st: ssh_dial::Stage = serde_json::from_value(st.clone()).map_err(|e| {
                    (
                        "failed",
                        crate::common::contract::malformed(&format!("unreadable stage line: {e}")),
                    )
                })?;
                cells.send(stage_line(&st, cells.last().1)).await?;
            }
            None => break v,
        }
    };
    if ack.get("ok").and_then(Value::as_bool) != Some(true) {
        // 握手失败（含 host key 不匹配被拒）：看到过的指纹不回（信哪一把只在拨号核心那份库里判，比对走机器状态那一格）。
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
        return Ok(stopped(
            cells,
            copy_text("beProbe.test.sshFailed", &[("why", &why)]),
            detail,
        ));
    }
    let ssh_up = Segs::default().reached(3);
    cells
        .send(step(
            copy_text("beProbe.reached.ssh", &[]),
            Tone::Now,
            ssh_up,
        ))
        .await?;

    // ② 首行 hello。
    let first = match remote_ask::capped_line(&mut rd, LINE_CAP).await {
        Ok(None) => None,
        Err(e) => {
            return Ok(stopped(
                cells,
                copy_text("beProbe.test.probeFailed", &[]),
                failed_detail(Some(&e)),
            ))
        }
        Ok(Some(l)) => serde_json::from_str::<Value>(&l)
            .ok()
            .filter(|v| v.get("kind").and_then(Value::as_str) == Some("hello")),
    };
    let Some(hello) = first else {
        return Ok(stopped(
            cells,
            copy_text("beProbe.test.noHello", &[]),
            failed_detail(None),
        ));
    };
    // 键值对那一形只进日志（`wire::hello_summary`）；界面那一行由下面三格拼（文案表）。
    tracing::info!("测试连接：{}", crate::wire::hello_summary(&hello));
    let facts = hello_facts(&hello, machine.name());
    let (build, gaps) = (facts.build.clone(), facts.gaps.to_string());
    cells
        .send(step(
            copy_text("beProbe.reached.hello", &[]),
            Tone::Now,
            ssh_up,
        ))
        .await?;

    // ③ 控制通道往返。
    let accepts_ping = hello
        .get("commands")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|c| c.as_str() == Some("ping")));
    let (line, said, tone, detail) = if !accepts_ping {
        // 那台的后端不认控制命令：全产品同一句（按名字取）。
        let said = copy_core::backend_old(machine.name());
        (
            copy_text(
                "beProbe.hello.tooOld",
                &[("build", &build), ("said", &said)],
            ),
            said,
            Tone::Warn,
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
        match answered {
            Ok(()) => {
                let dur = t0
                    .elapsed()
                    .map(copy_core::format_elapsed)
                    .unwrap_or_else(|_| copy_core::format_duration(0));
                let line = if facts.gaps == 0 {
                    copy_text("beProbe.hello.ok", &[("build", &build), ("dur", &dur)])
                } else {
                    copy_text(
                        "beProbe.hello.okGaps",
                        &[("build", &build), ("gaps", &gaps), ("dur", &dur)],
                    )
                };
                (line, copy_text("beProbe.test.ok", &[]), Tone::Plain, None)
            }
            Err(e) => (
                copy_text("beProbe.hello.noAnswer", &[("build", &build)]),
                copy_text("beProbe.test.controlDown", &[]),
                Tone::Warn,
                failed_detail(Some(&e)),
            ),
        }
    };
    let _ = up_w.shutdown().await;
    Ok(ProbeCell {
        said: said.into(),
        tone,
        segs: Segs::default().reached(4),
        end: Some(ConnTestResult {
            backend_hello: Some(line),
            backend_gaps: facts.by_code,
            detail,
        }),
    })
}

/// 生产的 `remote-probe`：真拨号；进度格进本连接的应答通道。
pub(crate) async fn answer_probe(
    req: &ProbeReq,
    replies: &tokio::sync::mpsc::Sender<Frame>,
) -> Result<(), (&'static str, String)> {
    probe_with(
        req,
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
