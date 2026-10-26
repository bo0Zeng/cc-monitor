//! **各台的常驻流归本机后端**：机器表里要连的那几台各起一条（`resident_client::feed`，手机同一条循环），
//! 循环交的那一态直接进机器状态（`control/machine_state.rs`，推 `changed {machine}`）—— 没有谁再从外面交「这一轮怎么了」。
//!
//! - 怎么够到一台只看这一端的机器表（按名字组请求，[`crate::dial::dial_named`]），链路是池里那条 SSH（[`Pool`]）。
//! - 装后端只在 connect 那一支里（[`Transport::install`]）：接不上或接上的不是这一版 ⇒ 先 `deploy-install` 那一趟
//!   （`control/deploy_plan.rs::answer_install`，每一步记进机器状态），再 `--resident-ensure [--replace]`。
//!   带着哪几格后端字节是 monitor 落在盘上的那一份清单（[`deploy_contract::SLOTS_MANIFEST`]）。
//! - 机器表一改（`machines::ring`）就对齐（[`align`]）：新来的起、没了的收；机器页「停」那一台停着（[`hold`]），「起」/［重试］叫醒（[`resume`]）。
//! - 版本号：与这一版同一份字节的那几台填这份二进制的版本号，别的空着（hello 带版本号那一格到 ③ 换形时补）。
//!
//! 这一步壳自己那条循环还在（收帧 · 问那台，不装）；在看名单不在这条上（名单台空着：这条连接那台不往上发会话）。

use resident_client::feed::{self, Boxed, Config, Event, Exec, Feed, Problem, Progress, Transport};
use resident_client::watch::Desk;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::dial::probe::Dial as _;
pub(crate) use crate::dial::probe::Pipe;

/// 握手每一行等多久（同测试连接 · 手机：冷启要起常驻、读盘，给足）。
#[allow(
    clippy::disallowed_methods,
    reason = "不是本后端的定时器：交给连常驻那一层（客户端那一侧）当握手每一行的期限"
)]
const HANDSHAKE: Duration = Duration::from_secs(60);

/// 下面那条 SSH 与装后端那一步（生产 = [`Pool`]；判据用替身）。
pub(crate) trait Ssh: Send + Sync + 'static {
    /// 开一条长 exec（拨号核心每过一步报 `progress`）。
    fn open(
        &self,
        machine: &str,
        command: String,
        progress: Progress,
    ) -> Boxed<'_, Result<Pipe, Problem>>;
    /// 跑一次收全。
    fn capture(&self, machine: &str, command: String) -> Boxed<'_, Result<Exec, Problem>>;
    /// 把这一版的后端放到那台的落点（成没成记进机器状态，不阻断这一轮）。
    fn install(&self, machine: &str) -> Boxed<'_, ()>;
}

/// 一台的链路：名字 ＋ 下面那条 SSH。本机后端自己装后端（`installs`）。
struct Link {
    machine: String,
    ssh: Arc<dyn Ssh>,
}

impl Transport for Link {
    type Stream = Pipe;

    fn stream(&self, command: String, progress: Progress) -> Boxed<'_, Result<Pipe, Problem>> {
        self.ssh.open(&self.machine, command, progress)
    }

    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        self.ssh.capture(&self.machine, command)
    }

    fn install(&self) -> Boxed<'_, ()> {
        self.ssh.install(&self.machine)
    }

    fn installs(&self) -> bool {
        true
    }
}

/// 此刻在跑的那几条（按名字）。
fn table() -> &'static Mutex<HashMap<String, Arc<Feed<Link>>>> {
    static T: std::sync::OnceLock<Mutex<HashMap<String, Arc<Feed<Link>>>>> =
        std::sync::OnceLock::new();
    T.get_or_init(Default::default)
}

/// 机器页「停」过、还没「起」的那几台（对齐机器表时不起）。
fn stopped() -> &'static Mutex<BTreeSet<String>> {
    static S: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    &S
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 照 `wanted`（要连的那几台的名字）对齐：没了的 · 停着的收掉，新来的起；其余不碰（在跑的不断）。
pub(crate) fn align_in(wanted: &[String], ssh: &Arc<dyn Ssh>) {
    let held = lock(stopped()).clone();
    let mut t = lock(table());
    t.retain(|name, f| {
        let keep = wanted.contains(name) && !held.contains(name);
        if !keep {
            f.close();
        }
        keep
    });
    for name in wanted {
        if held.contains(name) || t.contains_key(name) {
            continue;
        }
        t.insert(name.clone(), start_one(name, ssh));
    }
}

/// 这个进程是常驻那一位、各台的常驻流由它持有（[`start`] 之后）。
static HOSTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 常驻那一位起来时调一次：之后持有各台的常驻流，机器表一改就对齐。
/// 带 `shots` 特性编的后端（截图台架）不起：那几台的状态由台架交（`machine_state::shots_rounds`），截图照它定格。
pub fn start() {
    if cfg!(feature = "shots") {
        return;
    }
    HOSTED.store(true, std::sync::atomic::Ordering::SeqCst);
    align();
}

/// [`start`] 过没有（一次性进程里改机器表不起流）。
pub(crate) fn hosted() -> bool {
    HOSTED.load(std::sync::atomic::Ordering::SeqCst)
}

/// 照这一端的机器表此刻那一张对齐（常驻起来时一次 · 机器表每改一次）。
pub(crate) fn align() {
    let pool: Arc<dyn Ssh> = Arc::new(Pool);
    align_in(&wanted_now(), &pool);
}

/// 机器表里要连的那几台。
fn wanted_now() -> Vec<String> {
    crate::dial::machines_now()
        .iter()
        .filter(|h| h.connect)
        .map(|h| h.name().to_string())
        .collect()
}

/// 那一台此刻有没有在跑的一条。
pub(crate) fn running(machine: &str) -> bool {
    lock(table()).contains_key(machine)
}

/// 「停」：那一条收掉、记成停着（对齐机器表时不再起）。
pub(crate) fn hold(machine: &str) {
    lock(stopped()).insert(machine.to_string());
    if let Some(f) = lock(table()).remove(machine) {
        f.close();
    }
}

/// 「起」/［重试］（[`resume_in`] 的生产那一份）。
pub(crate) fn resume(machine: &str) {
    let pool: Arc<dyn Ssh> = Arc::new(Pool);
    resume_in(machine, &wanted_now(), &pool);
}

/// 不再停着；在跑 ⇒ 叫醒（断着时不等退避马上再接）；不在跑而机器表要连 ⇒ 起。
pub(crate) fn resume_in(machine: &str, wanted: &[String], ssh: &Arc<dyn Ssh>) {
    lock(stopped()).remove(machine);
    let running = lock(table()).get(machine).cloned();
    match running {
        Some(f) => f.wake(),
        None => align_in(wanted, ssh),
    }
}

/// 起一台的那一条：循环交的那一态原样进机器状态；接着的时候记「那台后端此刻在」（[`crate::dial::streams`]）。
fn start_one(machine: &str, ssh: &Arc<dyn Ssh>) -> Arc<Feed<Link>> {
    let name = machine.to_string();
    let up: Mutex<Option<crate::dial::streams::Held>> = Mutex::new(None);
    Feed::start(
        Config {
            machine: name.clone(),
            mine: Some(crate::BUILD_ID.to_string()),
            tz: None,
            handshake: HANDSHAKE,
        },
        Some(Arc::new(Link {
            machine: name.clone(),
            ssh: Arc::clone(ssh),
        })),
        Desk::new(|_| Vec::new(), |_| {}),
        feed::sink(move |e| {
            let Event::State(s) = e else {
                return;
            };
            *lock(&up) =
                matches!(s, feed::State::Up { .. }).then(|| crate::dial::streams::hold(&name));
            crate::control::machine_state::note(
                &name,
                deploy::machine::Fact::Round {
                    round: s.round(),
                    version: Some(env!("CC_MONITOR_VERSION").to_string()),
                },
            );
        }),
    )
}

/// 生产那一侧：池里那条 SSH（照这一端的机器表按名字组请求），装后端走 `deploy-install` 那一趟。
struct Pool;

impl Pool {
    fn request(
        machine: &str,
        use_: &str,
        extra: serde_json::Value,
    ) -> Result<crate::dial::DialRequest, Problem> {
        crate::dial::host();
        crate::dial::dial_named(&crate::dial::machines_now(), machine, use_, extra)
            .map_err(|(_, said)| Problem::new(said, ""))
    }
}

impl Ssh for Pool {
    fn open(
        &self,
        machine: &str,
        command: String,
        progress: Progress,
    ) -> Boxed<'_, Result<Pipe, Problem>> {
        let req = Self::request(machine, "stream", serde_json::json!({}));
        Box::pin(async move {
            let mut req = req?;
            req.command = command;
            // 这一条不报阶段（只有测试连接要）：收的那一头随手丢。
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            crate::dial::probe::real()
                .open(req, tx, progress)
                .await
                .map_err(crate::dial::probe::problem_of)
        })
    }

    fn capture(&self, machine: &str, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        // 一次性 exec 只经问远端那一跳（`remote_ask::capture_full`：上限 · 老后端见到 hello 就收都在那里）。
        crate::dial::host();
        let dial = crate::dial::request_named(
            &crate::dial::machines_now(),
            machine,
            "capture",
            serde_json::json!({}),
        )
        .map_err(|(_, said)| Problem::new(said, ""));
        Box::pin(async move {
            let got = crate::dial::remote_ask::capture_full(&dial?, command)
                .await
                .map_err(|said| Problem::new(said, ""))?;
            Ok(Exec {
                exit_status: got.exit_status,
                stdout: got.stdout,
                stderr: got.stderr,
            })
        })
    }

    fn install(&self, machine: &str) -> Boxed<'_, ()> {
        let machine = machine.to_string();
        Box::pin(async move {
            let slots = match crate::control::deploy_plan::carried_slots() {
                Ok(s) => s,
                Err(why) => {
                    tracing::warn!(
                        "[feeds] {machine}：读不到带着的那几格后端字节，这一轮不装：{why}"
                    );
                    return;
                }
            };
            let req = crate::control::deploy_plan::DeployInstallReq {
                slots,
                machine: machine.clone(),
            };
            if let Err(e) = crate::control::deploy_plan::answer_install(req).await {
                tracing::warn!("[feeds] {machine} 没装上（照样接已有的后端）：{e:?}");
            }
        })
    }
}

/// `machine-wake` 的入参与 `machine-stop` 的入参：那台的名字。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct MachineName {
    pub(crate) machine: String,
}

fn named(req: &MachineName) -> Result<&str, String> {
    let m = req.machine.trim();
    if m.is_empty() {
        return Err(crate::common::contract::malformed(
            "missing `machine` (non-empty string)",
        ));
    }
    Ok(m)
}

crate::wire::typed::codes! {
    /// `machine-wake` 没有别的码。
    pub(crate) enum MachineWakeErr {}
}

/// `machine-wake`：机器页「起」/［重试］—— 那台不再停着；断着 ⇒ 不等退避马上再接。
pub(crate) struct MachineWake;

impl crate::wire::typed::Command for MachineWake {
    const NAME: &'static str = "machine-wake";
    type Req = MachineName;
    type Reply = ();
    type Err = MachineWakeErr;
}

impl crate::wire::typed::Handle for MachineWake {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MachineName,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<(), MachineWakeErr>> + Send + 'static {
        std::future::ready(named(&req).map(resume).map_err(MachineWakeErr::BadArgs))
    }
}

/// `machine-stop` 的回包：那台常驻怎么停的（`graceful` · `killed` · `not_running`）· 被停的 pid。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct MachineStopped {
    pub(crate) stopped: String,
    pub(crate) pid: Option<u32>,
}

crate::wire::typed::codes! {
    /// `machine-stop` 的码：在那台上停没成（够不到 · 那台说的那一句）。
    pub(crate) enum MachineStopErr {
        StopFailed = "stop_failed",
    }
}

/// `machine-stop`：机器页「停」—— 先停这一端那一条（不然转眼又 ensure 起来），再在那台跑一次 `--resident-stop`、回它的结局。
pub(crate) struct MachineStop;

impl crate::wire::typed::Command for MachineStop {
    const NAME: &'static str = "machine-stop";
    type Req = MachineName;
    type Reply = MachineStopped;
    type Err = MachineStopErr;
}

impl crate::wire::typed::Handle for MachineStop {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MachineName,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachineStopped, MachineStopErr>> + Send + 'static
    {
        async move {
            let machine = named(&req).map_err(MachineStopErr::BadArgs)?.to_string();
            hold(&machine);
            stop_with(&Pool, &machine)
                .await
                .map_err(|f| crate::wire::typed::of_fail(f, MachineStopErr::StopFailed))
        }
    }
}

/// 在那台上跑一次 `--resident-stop`，读它那一行结局（读法同接常驻那一层的一次性回话，[`feed::read_one_shot`]）。
pub(crate) async fn stop_with(
    ssh: &dyn Ssh,
    machine: &str,
) -> Result<MachineStopped, crate::wire::spec::Fail> {
    let fail = |p: Problem| {
        crate::wire::spec::Fail::new("stop_failed", p.text).with_raw(Some(p.detail.as_str()))
    };
    let exec = ssh
        .capture(machine, feed::backend_command(feed::STOP))
        .await
        .map_err(fail)?;
    let v = feed::read_one_shot(&exec, machine).map_err(|f| fail(f.problem))?;
    Ok(MachineStopped {
        stopped: v["stopped"].as_str().unwrap_or_default().to_string(),
        pid: v["pid"].as_u64().and_then(|p| u32::try_from(p).ok()),
    })
}

/// 那一跳断开（那台没有在跑的一条 · 等到期限还没接上 · 接着的时候断了）：与问那台的别处同一个码。
pub(crate) const UNREACHABLE: &str = "unreachable";

/// 发起方没带期限时，转过去那一问等多久（帧面的调用方都带 `within_ms`；这只是兜底）。
#[allow(
    clippy::disallowed_methods,
    reason = "不是本后端的定时器：交给连常驻那一层（客户端那一侧）当这一问的期限；帧面的调用方都带自己的期限"
)]
const FORWARD_WITHOUT_DEADLINE: Duration = Duration::from_secs(60);

/// **问那台的一问经它的常驻流转过去**（请求信封带 `machine` 的那一形，分派在 `stream/inbound`）：命令 · 入参 · 出口声明原样交那台，
/// 那台的回包原样回来；那台拒了 ⇒ 码 · 那一句 · 原话 · 按码定形的 `data` 原样交回。能不能发（那台认不认 · 形状对不对得上）
/// 由这一端连那台的客户端判（`InboundClient::call`），判下来的那几种照码说。期限是发起方那一份（`within_ms`）。
pub(crate) async fn forward(
    machine: String,
    req: crate::wire::Request,
) -> crate::wire::spec::Outcome {
    use crate::wire::spec::Fail;
    use resident_client::client::CallError;
    let left = req.until.map_or(FORWARD_WITHOUT_DEADLINE, |u| {
        Duration::from_millis(u.left_ms())
    });
    let deadline = tokio::time::Instant::now() + left;
    let feed = lock(table()).get(&machine).cloned();
    let unreachable = || {
        Fail::new(
            UNREACHABLE,
            copy_core::copy_text("front.body.offline", &[("machine", &machine)]),
        )
    };
    let Some(feed) = feed else {
        return Err(unreachable());
    };
    let Some(client) = feed.client_by(deadline).await else {
        return Err(unreachable());
    };
    let got = if req.view.is_null() {
        client.call_until(&req.cmd, req.args, deadline).await
    } else {
        client
            .call_until_viewed(&req.cmd, req.args, &req.view, deadline)
            .await
    };
    got.map_err(|e| match e {
        CallError::Remote {
            code,
            message,
            detail,
            data,
        } => Fail {
            code,
            message,
            data: data.and_then(|d| serde_json::from_str(&d).ok()),
            raw: (!detail.is_empty()).then_some(detail),
        },
        CallError::Disconnected => unreachable(),
        CallError::Unsupported { .. } => Fail::new("unknown_command", e.to_string()),
        CallError::Unavailable { ref code, .. } => Fail::new(code, e.to_string()),
        CallError::Differs { ref said, .. } => Fail::new("differs", said.clone()),
        CallError::Timeout { .. } => Fail::new("timeout", e.to_string()),
        CallError::TooManyPending => Fail::new("busy", e.to_string()),
        CallError::Cancelled => Fail::new("cancelled", e.to_string()),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/feeds_tests.rs"]
mod tests;
