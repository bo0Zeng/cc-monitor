//! **各台的常驻流归本机后端**：机器表里要连的那几台各起一条（`resident_client::feed`，手机同一条循环），
//! 循环交的那一态直接进机器状态（`control/machine_state.rs`，推 `changed {machine}`）—— 没有谁再从外面交「这一轮怎么了」。
//!
//! - 怎么够到一台只看这一端的机器表（按名字组请求，[`crate::dial::dial_named`]），链路是池里那条 SSH（[`Pool`]）。
//! - 装后端只在 connect 那一支里（[`Transport::install`]）：接不上或接上的不是这一版 ⇒ 先 `deploy-install` 那一趟
//!   （`control/deploy_plan.rs::answer_install`，每一步记进机器状态），再 `--resident-ensure [--replace]`。
//!   带着哪几格后端字节是 monitor 落在盘上的那一份清单（[`deploy_contract::SLOTS_MANIFEST`]）。
//! - 机器表一改（`machines::ring`）就对齐（[`align`]）：新来的起、没了的收；机器页「停」那一台停着（[`hold`]），「起」/［重试］叫醒（[`resume`]）。
//! - 版本号：那台 hello 自报的那一格（`version`；没带 ⇒ 空）。
//! - 那台流上的事原样转给连着本机后端的每条流连接（[`Relay`]，生产 ＝ 每一份 watcher 的出方向，由 `main.rs` 接上）：
//!   每条连接先它的 hello、再流上的帧、丢了一行一格 `gap`、断了一帧 `ended`（`relayed` 帧，`cmd_wire::frame::Relayed`）。
//!   断的那一刻先给那台活的 · 可重连的每一条转一帧 `session_left`（说不清），这一端记着那台说过的会话（端核心的会话簿）。
//!   新连上来的流先收此刻接着的各台的 hello（[`attached_hellos`]），不然它收不到那台这一条连接的开头。
//! - 在看名单：每台一张名单台（[`Desk`]，随这个进程一直留着，那台换连接重报）；连着本机后端的各条流报那台的名单
//!   （`stream-watch` 信封带 `machine`，[`watch_at`]），并起来交那张台；那条流走了它那一份摘掉（[`Watching`]）。

use crate::wire::Frame;
use crate::wire::{SessionFace, SessionFaceExt as _, SessionLeft};
use cmd_wire::frame::Relayed;
use cmd_wire::machine::{MachineState, MachineStopReq, MachineStopped, MachineWakeReq};
use cmd_wire::watch::{BranchHeld, WatchAsk, WatchFrom};
use resident_client::book::{Seat, SessionBook};
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
    /// 到那台的这一跳远不远（池里那条连接的读数，`ssh_dial::pool::far_of`；常驻流照它定要不要那台压）。
    fn far(&self, machine: &str) -> bool;
}

/// 那台流上的事转到哪（生产 ＝ 每一份 watcher 的出方向，`main.rs` 起常驻时交进来，[`start`]；判据收下来看）。
pub type Relay = Arc<dyn Fn(Frame) + Send + Sync>;

/// 生产那一份转出口（[`start`] 交进来的；还没起 ⇒ 不转）。
fn relay_out() -> Relay {
    RELAY.get().cloned().unwrap_or_else(|| Arc::new(|_| {}))
}

static RELAY: std::sync::OnceLock<Relay> = std::sync::OnceLock::new();

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

    fn far(&self) -> bool {
        self.ssh.far(&self.machine)
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
pub(crate) fn align_in(wanted: &[String], ssh: &Arc<dyn Ssh>, relay: &Relay) {
    let held = lock(stopped()).clone();
    let mut t = lock(table());
    t.retain(|name, f| {
        let keep = wanted.contains(name) && !held.contains(name);
        if !keep {
            close(name, f);
        }
        keep
    });
    for name in wanted {
        if held.contains(name) || t.contains_key(name) {
            continue;
        }
        t.insert(name.clone(), start_one(name, ssh, relay));
    }
}

/// 这个进程是常驻那一位、各台的常驻流由它持有（[`start`] 之后）。
static HOSTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 常驻那一位起来时调一次：之后持有各台的常驻流，机器表一改就对齐；那台流上的事交 `relay`（[`Relay`]）。
pub fn start(relay: Relay) {
    if RELAY.set(relay).is_err() {
        tracing::warn!("[feeds] start 被调了第二次：转出口照第一次那一份");
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
    align_in(&wanted_now(), &pool, &relay_out());
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
        close(machine, &f);
    }
}

/// 收掉一台那一条：新连上的流不再先收它那一条的 hello。
fn close(machine: &str, f: &Feed<Link>) {
    f.close();
    lock(hellos()).remove(machine);
}

/// 「起」/［重试］（[`resume_in`] 的生产那一份）。
pub(crate) fn resume(machine: &str) {
    let pool: Arc<dyn Ssh> = Arc::new(Pool);
    resume_in(machine, &wanted_now(), &pool, &relay_out());
}

/// 不再停着；在跑 ⇒ 叫醒（断着时不等退避马上再接）；不在跑而机器表要连 ⇒ 起。
pub(crate) fn resume_in(machine: &str, wanted: &[String], ssh: &Arc<dyn Ssh>, relay: &Relay) {
    lock(stopped()).remove(machine);
    let running = lock(table()).get(machine).cloned();
    match running {
        Some(f) => f.wake(),
        None => align_in(wanted, ssh, relay),
    }
}

/// 一台那条流上此刻的那几样（循环交事那一口记）。
#[derive(Default)]
struct Seen {
    /// 接着的时候记「那台后端此刻在」（[`crate::dial::streams`]）。
    up: Option<crate::dial::streams::Held>,
    /// 这一条连接的 hello 转出去了、还没断（断了要转一帧 `ended`）。
    attached: bool,
    /// 那台 hello 自报的版本号（最近一次接上的那一份）。
    version: Option<String>,
    /// 那台说过的会话（端核心的会话簿，只折 `session_*` 那几种）：随这一台的 feed 活着，换连接不丢 ——
    /// 断的那一刻照它说哪几条说不清（[`unclear_now`]），重连之后那台重报的帧把它们一条条换掉。
    book: SessionBook,
}

/// 流上这一行是会话帧 ⇒ 折进那台的簿（别的帧不解）；是「活会话清单报完了」那一帧 ⇒ 回 `true`（调用方记进机器状态：首屏那一句照它）。
fn fold_line(book: &mut SessionBook, line: &str) -> bool {
    let Ok(kind) = cmd_wire::frame::kind_of(line) else {
        return false;
    };
    if !kind.starts_with("session") {
        return false;
    }
    if let Ok(f) = cmd_wire::frame::decode::<serde::de::IgnoredAny>(line) {
        book.fold(&f);
        return matches!(f, cmd_wire::frame::Frame::SessionsReplayed(_));
    }
    false
}

/// 那台这一条连接断的那一刻：簿里活的 · 可重连的每一条落成说不清（看不见那一张脸，`face.life = unclear`），
/// 自己先折进簿，交回要转出去的那几行（按 sid 排）。已结束 · 记录已不在的不说。
fn unclear_now(book: &mut SessionBook, machine: &str) -> Vec<String> {
    let open: Vec<String> = book
        .seats()
        .filter(|(_, p)| match p {
            Seat::Live(_) => true,
            Seat::Left(s) => s.face.life == cmd_wire::frame::Life::Reconnectable,
        })
        .map(|(sid, _)| sid.clone())
        .collect();
    let face = SessionFace::unseen(cmd_wire::cells::Words(comms_inward::chan::said::offline(
        machine,
    )));
    open.into_iter()
        .filter_map(|sid| {
            let f: cmd_wire::frame::Frame<serde_json::Value> =
                cmd_wire::frame::Frame::SessionLeft(SessionLeft {
                    sid,
                    face: face.clone(),
                });
            book.fold(&f);
            serde_json::to_string(&f).ok()
        })
        .collect()
}

/// 起一台的那一条：循环交的那一态原样进机器状态；那台流上的事原样转出去（[`Relay`]）。
fn start_one(machine: &str, ssh: &Arc<dyn Ssh>, relay: &Relay) -> Arc<Feed<Link>> {
    let name = machine.to_string();
    let seen: Mutex<Seen> = Mutex::new(Seen::default());
    let relay = Arc::clone(relay);
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
        desk_of(&name),
        feed::sink(move |e| {
            let machine = name.clone();
            match e {
                Event::Attached { hello, .. } => {
                    let mut s = lock(&seen);
                    s.version = serde_json::from_str::<cmd_wire::frame::Hello>(&hello)
                        .ok()
                        .and_then(|h| h.version);
                    s.attached = true;
                    lock(hellos()).insert(machine.clone(), hello.clone());
                    relay(Frame::Relayed(Relayed::Line {
                        machine,
                        line: hello,
                    }));
                }
                Event::Frame(line) => {
                    let listed = fold_line(&mut lock(&seen).book, &line);
                    if listed {
                        crate::control::machine_state::note(&name, deploy::machine::Fact::Listed);
                    }
                    relay(Frame::Relayed(Relayed::Line { machine, line }));
                }
                Event::Gap => relay(Frame::Relayed(Relayed::Gap { machine })),
                Event::State(st) => {
                    let up = matches!(st, feed::State::Up { .. });
                    let (version, ended, unclear) = {
                        let mut s = lock(&seen);
                        s.up = up.then(|| crate::dial::streams::hold(&name));
                        let ended = !up && std::mem::take(&mut s.attached);
                        let unclear = if ended {
                            unclear_now(&mut s.book, &name)
                        } else {
                            Vec::new()
                        };
                        (s.version.clone(), ended, unclear)
                    };
                    if ended {
                        lock(hellos()).remove(&machine);
                        // 断了 ≠ 会话死了：那台活的 · 可重连的由这一端说说不清（排在 `ended` 之前），壳不合成。
                        for line in unclear {
                            relay(Frame::Relayed(Relayed::Line {
                                machine: machine.clone(),
                                line,
                            }));
                        }
                        relay(Frame::Relayed(Relayed::Ended { machine }));
                    }
                    crate::control::machine_state::note(
                        &name,
                        deploy::machine::Fact::Round {
                            round: st.round(),
                            version,
                        },
                    );
                }
                Event::Ssh(_) => {}
            }
        }),
    )
}

/// 此刻接着的各台那一条连接的 hello（那台换连接就换，断了就摘）。
fn hellos() -> &'static Mutex<HashMap<String, String>> {
    static H: std::sync::OnceLock<Mutex<HashMap<String, String>>> = std::sync::OnceLock::new();
    H.get_or_init(Default::default)
}

/// 新连上来的一条流要先收的那几帧：此刻接着的各台那一条连接的 hello（按名字排）。
/// 之后那台流上的帧照常转来（新流在 watcher 上接好之后才取这一份 ⇒ 不会漏开头；同一台的 hello 至多重一次，收的那一头当换了一条）。
pub fn attached_hellos() -> Vec<Frame> {
    let mut all: Vec<(String, String)> = lock(hellos())
        .iter()
        .map(|(m, h)| (m.clone(), h.clone()))
        .collect();
    all.sort();
    all.into_iter()
        .map(|(machine, line)| Frame::Relayed(Relayed::Line { machine, line }))
        .collect()
}

/// 各台的名单台（随这个进程一直留着：那台的常驻流断了又起，名单照旧）。
fn desks() -> &'static Mutex<HashMap<String, Arc<Desk>>> {
    static D: std::sync::OnceLock<Mutex<HashMap<String, Arc<Desk>>>> = std::sync::OnceLock::new();
    D.get_or_init(Default::default)
}

/// 连着本机后端的各条流报来的各台名单：那台 → 连接号 → 那条流报的那一份。
fn asked() -> &'static Mutex<HashMap<String, HashMap<u64, WatchAsk>>> {
    static A: std::sync::OnceLock<Mutex<HashMap<String, HashMap<u64, WatchAsk>>>> =
        std::sync::OnceLock::new();
    A.get_or_init(Default::default)
}

/// 那台的名单台（没有就起一张）：手上各份主线外清单照各条流报的交（同一个会话先报的那一份）。
fn desk_of(machine: &str) -> Arc<Desk> {
    let mut t = lock(desks());
    if let Some(d) = t.get(machine) {
        return Arc::clone(d);
    }
    let at = machine.to_string();
    let d = Desk::new(move |sids| held_of(&at, sids), |_| {});
    t.insert(machine.to_string(), Arc::clone(&d));
    d
}

/// 各条流报来的那台的主线外清单里，名单里那几条的那一份（同一个会话先报的那一份）。
fn held_of(machine: &str, sids: &BTreeSet<String>) -> Vec<BranchHeld> {
    let all = lock(asked());
    let mut out: Vec<BranchHeld> = Vec::new();
    let mut conns: Vec<_> = all.get(machine).into_iter().flatten().collect();
    conns.sort_by_key(|(c, _)| **c);
    for (_, ask) in conns {
        for b in &ask.branch {
            if sids.contains(&b.sid) && !out.iter().any(|o| o.sid == b.sid) {
                out.push(b.clone());
            }
        }
    }
    out
}

/// 那台此刻各条流并起来的那一份（在看的 · 手上留着的）。
fn union_of(machine: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let all = lock(asked());
    let mut sids = BTreeSet::new();
    let mut kept = BTreeSet::new();
    for ask in all.get(machine).into_iter().flat_map(|m| m.values()) {
        sids.extend(ask.sids.iter().cloned());
        kept.extend(ask.kept.iter().cloned());
    }
    (sids, kept)
}

/// **一条流报那台的在看名单**（`stream-watch` 信封带 `machine`，分派在 `stream/inbound`）：记在这条流名下、并起来交那台的名单台；
/// 回那一趟报上去之后那台说的 `from[]` 里这条流名单里的那几条（那台此刻没接着 ⇒ 空的，下一条连接的 attach 行照样带上）。
pub(crate) fn watch_at(
    machine: &str,
    conn: u64,
    ask: WatchAsk,
) -> impl std::future::Future<Output = Vec<WatchFrom>> + Send + 'static {
    let mine: BTreeSet<String> = ask.sids.iter().cloned().collect();
    lock(asked())
        .entry(machine.to_string())
        .or_default()
        .insert(conn, ask);
    let (sids, kept) = union_of(machine);
    let heard = desk_of(machine).set_and_hear(sids, kept);
    async move {
        heard
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|f| mine.contains(&f.sid))
            .collect()
    }
}

/// 一条流在各台报过的名单：随它的读循环活着，读循环收了就从各台摘掉、各台的名单台照剩下的那几份重报。
pub(crate) struct Watching(pub(crate) u64);

impl Drop for Watching {
    fn drop(&mut self) {
        let touched: Vec<String> = {
            let mut all = lock(asked());
            let touched = all
                .iter_mut()
                .filter_map(|(m, by)| by.remove(&self.0).map(|_| m.clone()))
                .collect();
            all.retain(|_, by| !by.is_empty());
            touched
        };
        for m in touched {
            let (sids, kept) = union_of(&m);
            desk_of(&m).set(sids, kept);
        }
    }
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

    /// 照这台的拨号请求问池里那几条连接的读数；请求组不成 ⇒ 远（宁可压）。
    fn far(&self, machine: &str) -> bool {
        Self::request(machine, "stream", serde_json::json!({})).map_or(true, |req| {
            ssh_dial::pool::far_of(&ssh_dial::pool::ssh().links(), &req)
        })
    }
}

/// `machine-wake` · `machine-stop` 入参里那台的名字（空白 ⇒ `bad_args`）。
fn named(machine: &str) -> Result<&str, String> {
    let m = machine.trim();
    if m.is_empty() {
        return Err(crate::common::contract::malformed(
            "missing `machine` (non-empty string)",
        ));
    }
    Ok(m)
}

crate::wire::typed::codes! {
    /// `machine-wake` 的码：机器表里没有那台 · 那台停用着（「连接这台」关着）· 正在连 / 装 / 更新（带那一刻的成品）。
    pub(crate) enum MachineWakeErr {
        UnknownMachine = "unknown_machine",
        Disabled = "disabled",
        Busy = "busy" => MachineState,
    }
}

/// `machine-wake`：重新连接一台（机器条 · 新建会话框 · 会话头 · 机器页「起」/［重试］都是这一条）——
/// 那台不再停着；断着 ⇒ 不等退避马上再接。回那台此刻的成品（多半之后照常推 `changed {machine}`）。
pub(crate) struct MachineWake;

impl crate::wire::typed::Command for MachineWake {
    crate::wire::typed::by_call!(MachineWakeReq);
    type Err = MachineWakeErr;
}

impl crate::wire::typed::Handle for MachineWake {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MachineWakeReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachineState, MachineWakeErr>> + Send + 'static
    {
        std::future::ready(
            named(&req.machine)
                .map_err(MachineWakeErr::BadArgs)
                .and_then(|m| wake_in(m, &crate::control::machine_state::read(), resume)),
        )
    }
}

/// [`MachineWake`] 的本体（各台此刻的成品与「叫醒」由调用方给）：先照那台的成品判拒不拒，不拒才叫醒。
pub(crate) fn wake_in(
    machine: &str,
    states: &std::collections::BTreeMap<String, MachineState>,
    wake: impl FnOnce(&str),
) -> Result<MachineState, MachineWakeErr> {
    use cmd_wire::machine::MachineStateKind as K;
    let Some(m) = states
        .get(machine)
        .filter(|_| machine != cmd_wire::machine::LOCAL)
    else {
        return Err(MachineWakeErr::UnknownMachine(copy_core::copy_text(
            "beMachineWake.refuse.unknown",
            &[("machine", machine)],
        )));
    };
    match m.state {
        K::Disabled => Err(MachineWakeErr::Disabled(m.said.clone())),
        K::Connecting | K::Installing | K::Updating => {
            Err(MachineWakeErr::Busy(m.said.clone(), m.clone()))
        }
        _ => {
            wake(machine);
            Ok(m.clone())
        }
    }
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
    crate::wire::typed::by_call!(MachineStopReq);
    type Err = MachineStopErr;
}

impl crate::wire::typed::Handle for MachineStop {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MachineStopReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachineStopped, MachineStopErr>> + Send + 'static
    {
        async move {
            let machine = named(&req.machine)
                .map_err(MachineStopErr::BadArgs)?
                .to_string();
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
#[allow(
    clippy::disallowed_methods,
    reason = "不是本后端的定时器：发起方那一份期限（`within_ms` 换成的截止时刻）原样换成连那台的客户端要的那一刻，交它去等"
)]
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
    let unreachable = || Fail::new(UNREACHABLE, comms_inward::chan::said::offline(&machine));
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
        // 连那台的客户端事前就拒的那几种：那一句是它自己的（`CallError` 的 `Display` 照文案表写），码照它是哪一种。
        other => {
            let code = match &other {
                CallError::Unsupported { .. } => "unknown_command".to_string(),
                CallError::Unavailable { code, .. } => code.clone(),
                CallError::Differs { .. } => "differs".to_string(),
                CallError::Timeout { .. } => "timeout".to_string(),
                CallError::TooManyPending => "busy".to_string(),
                _ => "cancelled".to_string(),
            };
            let said = other.to_string();
            Fail::new(&code, said)
        }
    })
}

/// **本机后端自己问那台一条有类型的命令**（并好再答的那几条用它：界面只问本机后端一次）：与 [`forward`] 同一条路
/// （经那台的常驻流），入参 · 回包照那条命令的类型；期限是发起方那一份（`until`）。那台的回包对不上类型 ⇒ 说读不懂那台的回包。
pub(crate) async fn call<C>(
    machine: &str,
    req: &C::Req,
    until: Option<crate::platform::child::Until>,
) -> Result<C::Reply, crate::wire::spec::Fail>
where
    C: crate::wire::typed::Command,
    C::Req: serde::Serialize,
    C::Reply: serde::de::DeserializeOwned,
{
    let got = call_passing::<C>(machine, req, until).await?;
    serde_json::from_value(got).map_err(|_| unreadable(machine))
}

/// [`call`] 那一问，回包不收成类型、原样交回：这一侧只转交、不读的那几条用（`ext-list` 带各台那一格 `mcp-read`）。
pub(crate) async fn call_passing<C>(
    machine: &str,
    req: &C::Req,
    until: Option<crate::platform::child::Until>,
) -> Result<serde_json::Value, crate::wire::spec::Fail>
where
    C: crate::wire::typed::Command,
    C::Req: serde::Serialize,
{
    let line = serde_json::json!({
        "id": "",
        "cmd": C::NAME,
        "args": serde_json::to_value(req).map_err(|_| unreadable(machine))?,
        "within_ms": until.map(|u| u.left_ms()),
    });
    let mut asked: crate::wire::Request =
        serde_json::from_value(line).map_err(|_| unreadable(machine))?;
    asked.until = until;
    let got = forward(machine.to_string(), asked).await?;
    Ok(got.unwrap_or(serde_json::Value::Null))
}

/// 读不懂那台的回包那一句。
fn unreadable(machine: &str) -> crate::wire::spec::Fail {
    crate::wire::spec::Fail::new("failed", copy_core::reply_unreadable(machine))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/feeds_tests.rs"]
mod tests;
