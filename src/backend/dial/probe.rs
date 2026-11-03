//! 测试连接：界面把设置页表单里那台（可能还没保存的）配置交过来，本机常驻后端组拨号请求（[`super::machine`]）、
//! 跑一趟**真连接那一串**、回结局：连常驻那一层（`resident_client::feed`，桌面 · 手机同一份）照它自己的走法
//! attach → 没接上 ensure → 再 attach（拨号都是短命探活，不进连接池；测试连接不换装）。走到第一个通 / 断为止：
//!
//! 1. 拨号核心报的阶段逐条写成一格；SSH 够着了 ⇒ 「已连上」那一格；
//! 2. 断了 ⇒ 结局就是真连接会说的那一句（原话在复制详情）；
//! 3. 通了 ⇒ hello 拼那一行；那台认 `ping` ⇒ 同一条流上问一次（控制通道往返）；不认 ⇒ 「后端太旧」；不回 ⇒ 「后端连上了、不能起会话」。
//!
//! 边拨边推：每走一步往本连接的应答通道推一帧 `probe {ticket, cell}`，每格是写好的那一句 ＋ 语气 ＋ 四段（[`ProbeCell`]），
//! 最后一格带结局；monitor 把它们交进界面订的 `probe-progress/<ticket>`，界面照画、不按码取字。
//! 本文件不起定时器：总期限的值归发起方（请求信封 `within_ms`；没带 ⇒ 登记的上限 [`PROBE_CAP`]），执行在期限原语那一处（`platform::child::bounded`）：
//! 到点那一趟丢掉、那一台的常驻流随之收，结局那一格照分派的那个数说等了多久、最后一步是哪一句。拨号 · 握手 · 一问的期限是连常驻那一层自己的（客户端那一侧）。

use copy_core::copy_text;
use copy_core::{Discard, Why};
use resident_client::client::CallError;
use resident_client::feed::{
    self, Boxed, Config, Event, Exec, Feed, Problem, Progress, State, Transport,
};
use resident_client::watch::Desk;
use serde_json::{json, Value};
use ssh_dial::Captured;
use std::sync::Arc;
use std::time::Duration;

use crate::common::cells::{Tone, Words};
use crate::common::said::Said;
use crate::platform::child::{bounded, Deadline, Until};
use crate::wire::Frame;

/// 票的上限与字符集（界面给的是一个 UUID；本后端只当不透明的串回填，但它会进帧 ⇒ 有界、可打印）。
const TICKET_MAX: usize = 64;

/// 测试连接的总期限上限（握手 ＋ hello ＋ 一次往返，秒级）：发起方没带期限时按它；带了更早的截止时刻 ⇒ 照那个。
pub(crate) const PROBE_CAP: Deadline = Deadline::secs(15);

/// 测试连接那一行的一段到哪了：还没到 · 过了 · 没过。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub(crate) enum SegMark {
    #[default]
    Wait,
    Ok,
    Fail,
}

/// 测试连接那一行的四段：解析 · 连接 · 认证 · 就绪。
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct ProbeCell {
    /// 握手那一行 · 走到哪一步 · 结局那一句（结局不接下层原话，原话在复制详情）。
    said: Words,
    tone: Tone,
    segs: Segs,
    end: Option<ConnTestResult>,
}

/// 测试连接的结局（进度流最后那一格的 `end`）。
#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
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

    async fn send(&self, cell: ProbeCell) -> Result<(), Said> {
        let (said, segs) = (cell.said.0.clone(), cell.segs);
        self.out
            .send(Frame::Probe(crate::wire::Probe {
                ticket: self.ticket.clone(),
                cell: serde_json::to_value(&cell).unwrap_or(Value::Null),
            }))
            .await
            .map_err(|_| Said::new("failed", copy_text("beProbe.client.gone", &[])))?;
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

/// 握手那一行 ⇒ 那一格：字是拨号核心写好的那一句，这里只按支定语气、让四段往前走。「过了一步」不成格 ⇒ `None`。
fn stage_line(st: &ssh_dial::Stage, segs: Segs) -> Option<ProbeCell> {
    use ssh_dial::Stage;
    let (said, tone, segs) = match st {
        Stage::Step { .. } => return None,
        Stage::Dialing { said, .. } => (said, Tone::Now, segs),
        Stage::HostKey { said, .. } | Stage::Won { said, .. } => (said, Tone::Now, segs.reached(2)),
        Stage::Failed { said, .. } => (said, Tone::Warn, segs),
        Stage::Auth { ok: true, said } => (said, Tone::Now, segs.reached(3)),
        Stage::Auth { ok: false, said } => (
            said,
            Tone::Fail,
            Segs {
                auth: SegMark::Fail,
                ..segs.reached(2)
            },
        ),
        Stage::Established { said } => (said, Tone::Now, segs),
    };
    Some(step(said.clone(), tone, segs))
}

/// `remote-probe` 的入参：界面交来的票 ＋ 那台的配置（可能还没保存；它的跳板照这一端的机器表按名字找）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct ProbeReq {
    /// 1..=64 个 `[A-Za-z0-9-]`；进度帧 `probe` 原样回填。
    pub(crate) ticket: String,
    /// 那台的配置。
    #[serde(deserialize_with = "ssh_dial::machine::loose_field")]
    pub(crate) machine: super::Host,
}

crate::wire::typed::codes! {
    /// `remote-probe` 的码。
    pub enum ProbeErr {
        /// 跳板那一台的配置不对。
        BadJump = "bad_jump",
        /// 进度帧发不出去（本连接没了）· 读链路那一侧出错。
        Failed = "failed",
    }
}

/// `remote-probe`（帧面硬臂：进度格走本连接的应答通道；本体真异步，`cancel` 能在 await 点打断）。结局推成最后一格，应答本身不带体。
pub struct RemoteProbe;

impl crate::wire::typed::Command for RemoteProbe {
    const NAME: &'static str = "remote-probe";
    type Req = ProbeReq;
    type Reply = ();
    type Err = ProbeErr;
}

/// 票：非空、有界、只许字母数字与 `-`。
fn ticket_ok(t: &str) -> Result<String, Said> {
    if t.is_empty()
        || t.len() > TICKET_MAX
        || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(Said::new(
            "bad_args",
            crate::common::contract::malformed("`ticket` must be 1..=64 chars of [A-Za-z0-9-]"),
        ));
    }
    Ok(t.to_string())
}

/// 拨号那一侧报阶段的口：报一条就推一格（[`stage_line`]）。
pub(crate) type StageTx = tokio::sync::mpsc::UnboundedSender<crate::dial::Stage>;

/// 开成的一条长 exec：读那台后端的输出 · 往它写（两头随丢随收）。
pub type Pipe = tokio::io::Join<tokio::io::DuplexStream, tokio::io::DuplexStream>;

/// 拨号那一侧：开一条长 exec（拨号核心报的阶段照序交 `stages`，每过一步 `progress.step()`）· 跑一次收全。
/// 生产 = [`Real`]（拨号核心）；判据用替身。
pub trait Dial: Send + Sync + 'static {
    fn open(
        &self,
        req: crate::dial::DialRequest,
        stages: StageTx,
        progress: Progress,
    ) -> Boxed<'_, Result<Pipe, ssh_dial::DialFail>>;
    fn capture(
        &self,
        req: crate::dial::DialRequest,
    ) -> Boxed<'_, Result<Captured, ssh_dial::DialFail>>;
}

/// 生产那一侧：拨号核心开一条（`ssh_dial::uses::open`），每过一步把攒下的阶段交出去、报一步进展；
/// 开成之后那条通道接在一对内存管子上对拷（`Opened::pump`，两头的读写端收了它就收）。收全 = `ssh_dial::uses::capture`。
/// 各台的常驻流（`feeds.rs`）开链路也用它（[`real`]）。
pub struct Real;

/// 生产那一侧的拨号（[`Real`]）。
pub fn real() -> Real {
    Real
}

impl Dial for Real {
    fn open(
        &self,
        req: crate::dial::DialRequest,
        stages: StageTx,
        progress: Progress,
    ) -> Boxed<'_, Result<Pipe, ssh_dial::DialFail>> {
        Box::pin(async move {
            let sink = crate::dial::StageSink::new(true);
            // 收的那一头没了 ⇒ 这一趟已经收场，余下的不必再交。
            let pass = |sink: &crate::dial::StageSink| {
                for st in sink.drain() {
                    if stages.send(st).is_err() {
                        break;
                    }
                }
            };
            let opened = ssh_dial::uses::open(&req, &sink, |_| {
                pass(&sink);
                progress.step();
            })
            .await;
            pass(&sink);
            let opened = opened?;
            let (up_w, up_r) = tokio::io::duplex(64 * 1024);
            let (mut down_w, down_r) = tokio::io::duplex(256 * 1024);
            // 对拷完（任一头收了）这一侧的写端随任务一起丢 ⇒ 读的那一头读到 EOF。
            tokio::spawn(async move {
                opened
                    .pump(up_r, &mut down_w, ssh_dial::uses::Control::none())
                    .await;
            });
            Ok(tokio::io::join(down_r, up_w))
        })
    }

    fn capture(
        &self,
        req: crate::dial::DialRequest,
    ) -> Boxed<'_, Result<Captured, ssh_dial::DialFail>> {
        Box::pin(async move { ssh_dial::uses::capture(&req).await })
    }
}

/// 握手每一行等多久（同手机：冷启要起常驻、读盘，给足）。比总期限长 ⇒ 卡在握手上的那一趟由总期限收，照最后一步说。
#[expect(
    clippy::disallowed_methods,
    reason = "不是本后端的定时器：交给连常驻那一层（客户端那一侧）当握手每一行的期限；比总期限长，实际由 PROBE_CAP 先收"
)]
const HANDSHAKE: Duration = Duration::from_secs(20);

/// 这一趟里发生的事，按发生的先后进一条通道：拨号核心报的阶段 · 常驻流交给宿主的事。
enum Ev {
    Stage(crate::dial::Stage),
    Feed(Event),
}

/// 表单那一台的连接：每一趟在组好的拨号请求上填命令（长 exec 一份 · 收全一份）。测试连接不换装（`installs` 取缺省）。
struct ProbeLink {
    dial: Arc<dyn Dial>,
    stream: crate::dial::DialRequest,
    capture: crate::dial::DialRequest,
    events: tokio::sync::mpsc::UnboundedSender<Ev>,
}

impl Transport for ProbeLink {
    type Stream = Pipe;

    fn stream(&self, command: String, progress: Progress) -> Boxed<'_, Result<Pipe, Problem>> {
        let mut req = self.stream.clone();
        req.command = command;
        Box::pin(async move {
            // 报来的阶段原样进同一条通道，先于开成之后的那几件事（开完把还没交的补完）。收的那一头没了 ⇒ 这一趟已经收场，不必再交。
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let opening = self.dial.open(req, tx, progress);
            tokio::pin!(opening);
            let mut heard = true;
            let opened = loop {
                tokio::select! {
                    biased;
                    Some(st) = rx.recv(), if heard => heard = self.events.send(Ev::Stage(st)).is_ok(),
                    r = &mut opening => break r,
                }
            };
            while let (true, Ok(st)) = (heard, rx.try_recv()) {
                heard = self.events.send(Ev::Stage(st)).is_ok();
            }
            opened.map_err(problem_of)
        })
    }

    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        let mut req = self.capture.clone();
        req.command = command;
        Box::pin(async move {
            let got = self.dial.capture(req).await.map_err(problem_of)?;
            Ok(Exec {
                exit_status: got.exit_status,
                stdout: got.stdout,
                stderr: got.stderr,
            })
        })
    }
}

/// 拨号核心没开成的那一份 ⇒ 那一句（空 ⇒ 链路断了那一句）＋ 复制详情 ＋ 原因码与指纹（同手机那一侧）。
pub fn problem_of(f: ssh_dial::DialFail) -> Problem {
    Problem::new(
        if f.said.is_empty() {
            copy_text("reason.link.lost", &[])
        } else {
            f.said
        },
        f.detail.unwrap_or_default(),
    )
    .because(f.reason, (f.fingerprint, f.expected_fingerprint))
}

/// 这一趟起的那一台常驻流：随这一趟收（结局出了 · 到点被丢）。
struct Closing(Arc<Feed<ProbeLink>>);

impl Drop for Closing {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// `remote-probe` 的本体：`dial` 是拨号那一侧（生产 = [`Real`]；判据用替身）；`out` 是进度格的出口（生产 = 本连接的应答通道）；
/// `until` 是分派那一层交来的截止时刻。结局推成最后一格（`end`），应答本身不带体。
pub(crate) async fn probe_with(
    req: &ProbeReq,
    until: Option<Until>,
    dial: Arc<dyn Dial>,
    out: &tokio::sync::mpsc::Sender<Frame>,
) -> Result<(), Said> {
    let cells = Cells {
        ticket: ticket_ok(&req.ticket)?,
        out,
        last: std::sync::Mutex::new((copy_text("beProbe.stage.notStarted", &[]), Segs::default())),
    };
    let end = match bounded(Deadline::within(PROBE_CAP, until), run(req, dial, &cells)).await {
        Ok(end) => end?,
        Err(total) => {
            let dur = copy_core::format_duration(total.shown_secs().saturating_mul(1000));
            let said = copy_text(
                "beProbe.test.stalled",
                &[("dur", &dur), ("last", &cells.last().0)],
            );
            stopped(&cells, said, None)
        }
    };
    cells.send(end).await
}

/// 跑一趟真连接那一串（`resident_client::feed`：attach → 没接上 ensure → 再 attach），等到第一个通 / 断。
/// 通了 ⇒ 问一次 `ping` 量往返；断了 ⇒ 那一句就是真连接会说的那一句。
async fn run(req: &ProbeReq, dial: Arc<dyn Dial>, cells: &Cells<'_>) -> Result<ProbeCell, Said> {
    let name = req.machine.name().to_string();
    let hosts = super::machines_now();
    let request = |use_: &str, extra| {
        super::request_draft(&hosts, &req.machine, use_, extra)
            .and_then(|v| super::read_request(&v))
    };
    // 探完即丢 ⇒ 每一条都是短命探活（不进连接池：表单那份可能还没保存）。
    let stream = request("stream", json!({ "stages": true, "probe": true }))?;
    let capture = request(
        "capture",
        // 上限同别的远端一次性收全（`--resident-ensure` 只回一行）；老后端认不得它、掉进流模式 ⇒ 见到 hello 就收（判在连常驻那一层）。
        json!({ "probe": true, "capture": {
            "max_bytes": super::remote_ask::PULL_MAX_BYTES.bytes,
            "abort_marker": cmd_wire::dial::OLD_BACKEND_MARKER,
        } }),
    )?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let link = ProbeLink {
        dial,
        stream,
        capture,
        events: tx.clone(),
    };
    let _feed = Closing(Feed::start(
        Config {
            machine: name.clone(),
            mine: Some(crate::common::id::BUILD_ID.to_string()),
            tz: None,
            handshake: HANDSHAKE,
        },
        Some(Arc::new(link)),
        Desk::new(|_| Vec::new(), |_| {}),
        feed::sink(move |e| {
            tx.send(Ev::Feed(e)).discard(Why::PeerGone); // 这一趟已收场（结局出了 · 到点被丢）⇒ 没人要这一下
        }),
    ));
    let ssh_up = Segs::default().reached(3);
    let mut ssh = false;
    let mut attached = None;
    loop {
        let Some(ev) = rx.recv().await else {
            return Ok(stopped(
                cells,
                copy_text("beProbe.test.probeFailed", &[]),
                failed_detail(None),
            ));
        };
        match ev {
            Ev::Stage(st) => {
                if let Some(cell) = stage_line(&st, cells.last().1) {
                    cells.send(cell).await?
                }
            }
            Ev::Feed(Event::Ssh(true)) if !ssh => {
                ssh = true;
                cells
                    .send(step(
                        copy_text("beProbe.reached.ssh", &[]),
                        Tone::Now,
                        ssh_up,
                    ))
                    .await?;
            }
            Ev::Feed(Event::Attached { hello, client }) => attached = Some((hello, client)),
            Ev::Feed(Event::State(State::Up { .. })) => break,
            Ev::Feed(Event::State(State::Down { problem, .. })) => {
                // SSH 都没够着 ⇒ 那一句说握手没成（原因是真连接那一句）；够着了 ⇒ 就是真连接那一句。原话在复制详情。
                let said = if ssh {
                    problem.said
                } else {
                    copy_text("beProbe.test.sshFailed", &[("why", &problem.said)])
                };
                let detail = Some(problem.detail).filter(|d| !d.is_empty());
                return Ok(stopped(cells, said, detail.or_else(|| failed_detail(None))));
            }
            Ev::Feed(_) => {}
        }
    }
    let Some((hello, client)) = attached else {
        return Ok(stopped(
            cells,
            copy_text("beProbe.test.noHello", &[]),
            failed_detail(None),
        ));
    };
    let hello: Value = serde_json::from_str(&hello).unwrap_or(Value::Null);
    // 键值对那一形只进日志（`wire::hello_summary`）；界面那一行由下面三格拼（文案表）。
    tracing::info!("测试连接：{}", crate::wire::hello_summary(&hello));
    let facts = hello_facts(&hello, &name);
    let (build, gaps) = (facts.build.clone(), facts.gaps.to_string());
    cells
        .send(step(
            copy_text("beProbe.reached.hello", &[]),
            Tone::Now,
            ssh_up,
        ))
        .await?;

    // 控制通道往返。
    let (line, said, tone, detail) = if !client.accepts("ping") {
        // 那台的后端不认控制命令：全产品同一句（按名字取）。
        let said = copy_core::backend_old(&name);
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
        match client
            .call("ping", Value::Null, resident_client::backoff::PING_WITHIN)
            .await
        {
            Ok(_) => {
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
            Err(e) => {
                let raw = match &e {
                    CallError::Remote(r) => r.said.clone(),
                    other => other.to_string(),
                };
                (
                    copy_text("beProbe.hello.noAnswer", &[("build", &build)]),
                    copy_text("beProbe.test.controlDown", &[]),
                    Tone::Warn,
                    failed_detail(Some(&raw)),
                )
            }
        }
    };
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
pub async fn answer_probe(
    req: &ProbeReq,
    until: Option<Until>,
    replies: &tokio::sync::mpsc::Sender<Frame>,
) -> Result<(), Said> {
    probe_with(req, until, Arc::new(Real), replies).await
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_probe_tests.rs"]
mod tests;
