//! **一台的常驻流，断了自己接回去** —— 桌面与手机同一条重连循环（全仓只这一处：退避是本模块私有的，别处造不出一份）。宿主只接几根线：
//! 下面那条 SSH（[`Transport`]）· 往外交的事（[`Sink`]）· 装不装后端（[`Transport::installs`]）。
//!
//! 接一台只有一条路：先直接接常驻流（`--resident-attach`，平时一条 exec 就接上）；没接上 ⇒ `--resident-ensure`（常驻不在就起）
//! ⇒ 再接（小中继回 `absent` 时隔 [`RELAY_WAIT`] 再试）。新旧由那台在 attach 回话里给（attach 行带手上这一版）；认不得这一形 attach 行的当旧：
//! - 自己装后端的宿主（本机后端）：接不上 ⇒ 先装再 ensure；接上的不是手上这一版 ⇒ 先装、再 `--resident-ensure --replace`（只升不降由那台判）、换了就再接；
//! - 不装的（手机）：旧 ⇒ 照接、说「要更新」（认不得 attach 行的接不上 ⇒ 断着、修法「更新」）；同或新 ⇒ 接。
//!
//! 能不能用逐条命令由形状指纹判（[`InboundClient::call`]）。
//!
//! 什么时候接：
//! - 宿主交了一条 SSH（[`Feed::set_transport`]）⇒ 当场接；换了一条 ⇒ 旧流关掉、在新的上接；交 `None` ⇒ 不接、停在断开。
//! - 装得了后端的宿主（本机后端）只在接不上、或接上的不是这一版时先装（[`Transport::install`]），接上的就是这一版就什么都不多做。
//! - 流断了 ⇒ 按退避接（[`crate::backoff`]，活过 30 s 才清零）。下面那条 SSH 死没死不问宿主：再拨那一拨在拨号核心的池里自己换一条。
//! - 开链路那一步 [`DIAL_WITHIN`] 里拨号核心一步进展都没报（静默断网；慢链路每过一步重新计时，[`Progress`]）⇒ 丢下、隔下界再拨，不涨退避；拨着的时候宿主报了事（[`Feed::wake`] · [`Feed::reconnect`] ·
//!   换了一条 SSH）⇒ 丢下马上再拨。
//! - 连着的时候判死靠流自己的心跳（[`HEARTBEAT_IDLE`] 没来字节就问活、连着 [`PING_MISSES`] 问都没来字节就判死，一问的期限照最近的往返放宽），流上来着字节就算连着；
//!   宿主报「网回来了 · 网变了 · 回前台」时连着 ⇒ 当场问一句活。
//! - 走不通、要人动手的那几种不自己反复试，等 [`Feed::reconnect`] / [`Feed::wake`]（［重试］· 回前台 · 网络变了 · 机器页「起」）：
//!   那台不是 Unix（常驻脱离不了）恒是；没装 · 起不了常驻 · 不是我们的后端只在装不了后端的宿主上是（装得了的下一轮装过再试）。
//!
//! 每次接上先交 [`Event::Attached`]（那台的 hello 原样一行 ＋ 这条连接的客户端），再交 [`State::Up`]（`epoch` 加一，之前在路上的问答作废）。
//! 在看名单住这一台的名单台（[`Desk`]）：attach 行带它此刻那份（那台从第一帧起照它发），接上之后把这条连接交给它（[`Desk::up`]）。

use crate::attach::{self, AttachFail, RELAY_ABSENT, RELAY_TRIES, RELAY_WAIT};
use crate::backoff::{
    PingWindow, DIAL_WITHIN, HEARTBEAT_IDLE, MIN_HEALTHY_UPTIME, PING_CEIL, PING_MISSES,
    RECONNECT_MAX, RECONNECT_MIN,
};
use crate::client::{deliver_answer, InboundClient};
use crate::frames::Admit;
use crate::line::{read_capped_line, CappedLine, BACKEND_FRAME_LINE_CAP};
use crate::watch::Desk;
use cmd_wire::attach::Relation;
use cmd_wire::dial::OLD_BACKEND_MARKER;
use cmd_wire::machine::{Blocker, Why};
use copy_core::copy_text;
use copy_core::discard::{self, Discard};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{watch as tw, Notify};

/// 给人看的那一句 ＋ 复制详情；拨号拒了时另带原因码 · 那台出示的指纹与库里记下的那一枚（后端按它们定那一态与修法）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub said: String,
    pub detail: String,
    pub why: Option<Why>,
    pub seen_key: Option<String>,
    pub expected_key: Option<String>,
}

impl Problem {
    pub fn new(said: impl Into<String>, detail: impl Into<String>) -> Self {
        Problem {
            said: said.into(),
            detail: detail.into(),
            why: None,
            seen_key: None,
            expected_key: None,
        }
    }

    /// 拨号核心说没拨成的原因码 ＋ 那台出示的指纹与库里记下的那一枚。
    pub fn because(
        self,
        why: Option<Why>,
        (seen_key, expected_key): (Option<String>, Option<String>),
    ) -> Self {
        Problem {
            why,
            seen_key,
            expected_key,
            ..self
        }
    }
}

/// 一次性 exec 收全的那三样（`exit_status` 缺 ＝ 没退出就收了，如老后端掉进流模式、见到 hello 提前收）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exec {
    pub exit_status: Option<u32>,
    pub stdout: String,
    pub stderr: String,
}

pub type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// **开链路那一拨的进展**。拨号核心（`ssh-dial`，两端同一份）每过一步报一次 —— TCP 连上 · 收到 SSH 头 · host key 过了 ·
/// 认证过 · 经跳板开出隧道 · 通道开了（经跳板时每一台各走一遍）；宿主（桌面经本机后端的链路 · 手机在进程里）读到就原样
/// [`Progress::step`]，不自己判。循环只看它：每来一步重新计 [`DIAL_WITHIN`]，一步都没来的 [`DIAL_WITHIN`] 才丢下这一拨。
#[derive(Clone, Default)]
pub struct Progress(Arc<Notify>);

impl Progress {
    /// 过了一步。
    pub fn step(&self) {
        self.0.notify_one();
    }

    /// 等下一步（之前报过、还没被等走的那一步当场算）。
    async fn moved(&self) {
        self.0.notified().await;
    }
}

/// **开链路的期限，全仓只这一处**：等 `dial` 的同时看 `progress`，每来一步重新计 [`DIAL_WITHIN`]；一步都没来的 [`DIAL_WITHIN`] ⇒ `None`
/// （`dial` 随之被丢下）。共用循环开常驻流那一拨（[`Feed`]）与手机 SSH 那层开的每一条链路（终端 · 文件 · 握着的那条，嵌的核心在 ack 之前套它）同一个。
pub async fn dial_within<F: Future>(progress: &Progress, dial: F) -> Option<F::Output> {
    let quiet = tokio::time::sleep(DIAL_WITHIN);
    tokio::pin!(dial, quiet);
    loop {
        tokio::select! {
            got = &mut dial => return Some(got),
            // 拨号核心报了一步进展 ⇒ 重新计时：一步都没来的 DIAL_WITHIN 才丢。
            () = progress.moved() => quiet.as_mut().reset(tokio::time::Instant::now() + DIAL_WITHIN),
            () = &mut quiet => return None,
        }
    }
}

/// 下面那条 SSH：开一条长 exec（字节原样对拷）· 跑一次收全；外加宿主那两样（装不装得了后端 · 装那一步）。
/// 手机：拨号核心在这台手机上开的链路；本机常驻后端：池里那条 SSH（`control/feeds.rs`）。
pub trait Transport: Send + Sync + 'static {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;
    /// 开一条长 exec。拨号核心每报一步进展（ack 之前「过了一步」那一形阶段行 `{"stage":{"kind":"step",…}}`，`ssh-dial` 的 `step`），宿主原样交给 `progress`（[`Progress`]）。
    fn stream(
        &self,
        command: String,
        progress: Progress,
    ) -> Boxed<'_, Result<Self::Stream, Problem>>;
    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>>;
    /// 把手上这一版的后端放到那台的落点（本机后端：`deploy-install` 那一趟）。只在 [`Self::installs`] 的宿主上、
    /// 接不上或接上的不是这一版（`Relation::Same` 以外）时走，紧接着 `--resident-ensure [--replace]`。装没装成由宿主自己记（机器状态那几步）、
    /// 不阻断这一轮：手动装的后端照样接。缺省不做事。
    fn install(&self) -> Boxed<'_, ()> {
        Box::pin(async {})
    }
    /// 这一侧自己装后端（本机后端）⇒ 接不上 / 接上的不是这一版时先 [`Self::install`]、再请那台 `--replace`；不装 ⇒ `false`（手机，缺省）。
    fn installs(&self) -> bool {
        false
    }
    /// 刚开出来的那条长 exec 走的这一跳远不远：拨号核心那一跳判准的答案（`ssh-dial` 的 `compression_for`，经池里那条连接的读数交来）。
    /// 近（回环 · 往返不到门槛）⇒ attach 行不要那台压（压了只花那台的 CPU、多等几十毫秒）。答不上来 ⇒ 远（缺省：宁可压）。
    fn far(&self) -> bool {
        true
    }
}

/// 那台后端的一条子命令在远端 shell 里的写法（落点固定 `~/.cc-monitor/bin/ccm`，打头的 `--` 让它当后端用）。只收常量词。
pub fn backend_command(word: &'static str) -> String {
    format!("{} -- {word}", relay_route_core::BACKEND_LANDING_SHELL)
}

pub const ATTACH: &str = "--resident-attach";
pub const ENSURE: &str = "--resident-ensure";
/// 换装：在跑的那一个停掉、换成落点上这一版。
pub const ENSURE_REPLACE: &str = "--resident-ensure --replace";
/// 停那台的常驻（同机监督者：请它收尾 → 宽限期内等 → 到点强杀），回话读法同 [`read_one_shot`]。
pub const STOP: &str = "--resident-stop";

/// 一台此刻通不通（只有控制要的：成品由后端 `deploy::machine` 照它定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Connecting,
    /// 通着：第 `epoch` 条流；`relation` = 那台相对手上这一版（那台在 attach 回话里给的）；
    /// `differs` = 握手那一刻比出形状对不上的那几种帧（这几种的行在帧门上丢）。
    Up {
        epoch: u64,
        relation: Relation,
        differs: Vec<String>,
    },
    /// 断着：那一句；`needs_hand` ⇒ 不会自己好（要［重试］或人去装 / 起）；`blocker` ⇒ 断在那台本身的哪一处（不是 Unix · 没有后端 · 后端旧了）。
    Down {
        problem: Problem,
        needs_hand: bool,
        blocker: Option<Blocker>,
    },
}

impl State {
    /// 交给机器状态那一份（只有事实：画法字段不带）。
    pub fn round(&self) -> cmd_wire::machine::Round {
        use cmd_wire::machine::{Round, RoundProblem};
        match self {
            State::Connecting => Round::Connecting,
            State::Up {
                relation, differs, ..
            } => Round::Up {
                relation: *relation,
                differs: differs.clone(),
            },
            State::Down {
                problem, blocker, ..
            } => Round::Down {
                problem: RoundProblem {
                    said: problem.said.clone(),
                    detail: problem.detail.clone(),
                    why: problem.why,
                    seen_key: problem.seen_key.clone(),
                    expected_key: problem.expected_key.clone(),
                },
                blocker: *blocker,
            },
        }
    }
}

/// 交给宿主的事（按发生的先后、一件一件交：宿主慢了，读流就跟着慢，背压一路回到那台）。
#[derive(Clone)]
pub enum Event {
    State(State),
    /// 接上了：那台的 hello 原样一行 ＋ 这条连接的客户端。先于这条连接的 [`State::Up`] 与它的任何一帧。
    Attached {
        hello: String,
        client: Arc<InboundClient>,
    },
    /// 流上的一帧（`reply` · `cancelled` 以外的那些），原样一行。
    Frame(String),
    /// 流上有一行超过上限、整行丢了（看着的那一处原位留一个缺口）。
    Gap,
    /// **SSH 够没够着那一台**（拨通 · 认证过、开得出 exec），与常驻流通不通分开报：那台常驻后端起不来时它照样是 `true`。
    /// 手机 SSH 那层（终端 · 文件）跟它走 —— 终端是逃生口，不跟着后端卡住。变了才报。
    Ssh(bool),
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::State(s) => f.debug_tuple("State").field(s).finish(),
            Event::Attached { hello, .. } => {
                f.debug_struct("Attached").field("hello", hello).finish()
            }
            Event::Frame(l) => f.debug_tuple("Frame").field(l).finish(),
            Event::Gap => f.write_str("Gap"),
            Event::Ssh(b) => f.debug_tuple("Ssh").field(b).finish(),
        }
    }
}

/// 一台的几样定值。
#[derive(Debug, Clone)]
pub struct Config {
    /// 这台在屏上的名字（句子里称呼它）。
    pub machine: String,
    /// 手上这一版的身份（版本门的对照物；没有 ⇒ 不比）。
    pub mine: Option<String>,
    /// 看的这一台的时区（IANA 名）。
    pub tz: Option<String>,
    /// 握手每一行等多久（冷启要起常驻、读盘，给足）。
    pub handshake: Duration,
}

/// 往宿主交事的那个口：交完（宿主收下）才往下读。
pub type Sink = Arc<dyn Fn(Event) -> Boxed<'static, ()> + Send + Sync>;

/// 收下就算交完的宿主（手机：回调进 Kotlin）。
pub fn sink(f: impl Fn(Event) + Send + Sync + 'static) -> Sink {
    Arc::new(move |e| {
        f(e);
        Box::pin(async {})
    })
}

/// 走不通的一轮：那一句 ＋ 是哪一类（哪一类只循环自己看）。
pub struct Fail {
    pub problem: Problem,
    kind: FailKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailKind {
    /// 断在路上（被掐 · 小中继与常驻断开 · 拨不通 · 问不成换不换）：再试会好。
    Transient,
    /// 断在那台本身：不是 Unix ⇒ 怎样都不会好；没有后端 · 后端旧了 ⇒ 装一次会好（装得了的宿主下一轮再试，装不了的等人）。
    Blocked(Blocker),
    /// 拨出去到 [`DIAL_WITHIN`] 都没回话：没够到那一台（静默断网）⇒ 再试会好，不涨退避（没有一次登录落到它身上）。
    Unreached,
    /// 拨着的时候宿主报了事（网回来了 · 回前台 · 换了一条 SSH · ［重试］）⇒ 丢下这一拨、马上整份重来。
    Interrupted,
}

impl Fail {
    fn new(problem: Problem, kind: FailKind) -> Self {
        Fail { problem, kind }
    }
}

/// 接上之后到开读之前定下的那几样。
struct Connected<S> {
    client: Arc<InboundClient>,
    reader: crate::line::Downstream<tokio::io::ReadHalf<S>>,
    hello: attach::Hello,
    relation: Relation,
    /// 形状对不上的那几种帧（[`State::Up`] 那一格）。
    differs: Vec<String>,
    /// attach 行带过的那份名单。
    carried: Option<cmd_wire::watch::WatchAsk>,
}

/// 一台的常驻流。
pub struct Feed<T: Transport> {
    cfg: Config,
    events: Sink,
    transport: tw::Sender<Option<Arc<T>>>,
    state: tw::Sender<State>,
    /// SSH 够没够着那一台（[`Event::Ssh`]）。
    reached: tw::Sender<bool>,
    client: Mutex<Option<Arc<InboundClient>>>,
    /// 接着的那一台 hello 报的身份（没接着 ⇒ `None`）。
    theirs: Mutex<Option<String>>,
    /// ［重试］· 回前台 · 网络变了 · 「起」：每按一下 +1。接上那一刻之前按的作废（不留到下一次断线白白跳过一轮退避）。
    kick: tw::Sender<u64>,
    /// 当前这条流的收工信号（[`Feed::reconnect`] 拨它）。
    cut: Mutex<Option<Arc<Notify>>>,
    /// 连着的时候被叫醒（[`Feed::wake`]）⇒ 当场问一句活（[`Feed::pump`] 收它）。
    ask: Notify,
    /// 这一台的名单台：attach 行带它此刻那份，接上之后交它这条连接（报名单只在它那里）。
    desk: Arc<Desk>,
    /// 这一台的重连循环：[`Feed::close`] 收掉。
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl<T: Transport> Feed<T> {
    /// 起一台（在当前 tokio 运行时上起它的循环）；`transport` ＝ 起来那一刻手上的那条 SSH（还没有 ⇒ `None`，之后经 [`Feed::set_transport`] 交）；
    /// `desk` ＝ 这一台的名单台（宿主往它里交名单）。
    pub fn start(
        cfg: Config,
        transport: Option<Arc<T>>,
        desk: Arc<Desk>,
        events: Sink,
    ) -> Arc<Self> {
        let feed = Arc::new(Feed {
            cfg,
            events,
            transport: tw::channel(transport).0,
            state: tw::channel(State::Connecting).0,
            reached: tw::channel(false).0,
            client: Mutex::new(None),
            theirs: Mutex::new(None),
            kick: tw::channel(0).0,
            cut: Mutex::new(None),
            ask: Notify::new(),
            desk,
            tasks: Mutex::new(Vec::new()),
        });
        let run = tokio::spawn(Arc::clone(&feed).run());
        lock(&feed.tasks).push(run);
        feed
    }

    /// 不再看这台：循环收掉、流关掉、在等的问答一律「断了」。
    pub fn close(&self) {
        for t in lock(&self.tasks).drain(..) {
            t.abort();
        }
        if let Some(c) = lock(&self.client).take() {
            c.shutdown();
        }
    }

    /// SSH 那层此刻的连接（`None` ＝ 没通）。换了一条 ⇒ 旧流关掉、当场在新的上接。
    pub fn set_transport(&self, t: Option<Arc<T>>) {
        self.transport.send_replace(t);
        self.cut_current();
    }

    /// ［重试］：断开这一条（有的话）、马上整份重来。
    pub fn reconnect(&self) {
        self.cut_current();
        self.kick.send_modify(|k| *k += 1);
    }

    /// 回到前台 · 网络变了 · 网回来了 · ［重新连接］：通着 ⇒ 当场问一句活（与心跳同一个判法：连着 [`PING_MISSES`] 问没回话才判死重接，回了照用）；
    /// 没通 ⇒ 不等退避马上再接（正卡在一拨开不出链路的拨号里 ⇒ 丢下它马上再拨；正在做别的那几步 ⇒ 这一轮没成就不睡）。
    pub fn wake(&self) {
        if matches!(*self.state.borrow(), State::Up { .. }) {
            self.ask.notify_one();
        } else {
            self.kick.send_modify(|k| *k += 1);
        }
    }

    /// 接着的那一台 hello 报的身份（没接着 ⇒ `None`）。
    pub fn theirs(&self) -> Option<String> {
        lock(&self.theirs).clone()
    }

    /// 换上落点上那一份（手机按［装后端］装完之后）：在此刻那条 SSH 上跑一次 `--resident-ensure --replace`
    /// （在听的那一位比落点旧才停掉、起新的；只升不降那一判在那台），换了就整份重接、没换就叫醒。回换没换。
    pub async fn replace(&self) -> Result<bool, Problem> {
        let Some(t) = self.transport.borrow().clone() else {
            return Err(self.offline());
        };
        let exec = t.capture(backend_command(ENSURE_REPLACE)).await?;
        let replaced =
            read_one_shot(&exec, &self.cfg.machine).map_err(|f| f.problem)?["replaced"] == true;
        if replaced {
            self.reconnect();
        } else {
            self.wake();
        }
        Ok(replaced)
    }

    /// 这台在屏上的名字。
    pub fn machine(&self) -> &str {
        &self.cfg.machine
    }

    pub fn state(&self) -> State {
        self.state.borrow().clone()
    }

    /// SSH 此刻够没够着那一台（[`Event::Ssh`]；宿主收那件事，这里只给判据读）。
    #[cfg(test)]
    pub(crate) fn ssh_reached(&self) -> bool {
        *self.reached.borrow()
    }

    async fn set_ssh(&self, reached: bool) {
        if self.reached.send_replace(reached) != reached {
            (self.events)(Event::Ssh(reached)).await;
        }
    }

    /// 一问一答用哪条连接：流没通（在建 · 在重接 · SSH 还没通）⇒ 先等它接上（至多到 `deadline`，等的那一段也算在一问的期限里）；
    /// 断在要人动手的那一步 · 到点还没通 ⇒ `None`（问的人不用再等，宿主按「断了」说）。拿到之后经它 `call_until(.., deadline)`。
    pub async fn client_by(&self, deadline: tokio::time::Instant) -> Option<Arc<InboundClient>> {
        let mut st = self.state.subscribe();
        let ready = tokio::time::timeout_at(deadline, async {
            loop {
                let usable = match &*st.borrow_and_update() {
                    State::Up { .. } => Some(true),
                    State::Down {
                        needs_hand: true, ..
                    } => Some(false),
                    _ => None,
                };
                match usable {
                    Some(up) => return up,
                    None => {
                        if st.changed().await.is_err() {
                            return false;
                        }
                    }
                }
            }
        })
        .await;
        match ready {
            Ok(true) => lock(&self.client).clone(),
            _ => None,
        }
    }

    fn cut_current(&self) {
        if let Some(n) = lock(&self.cut).take() {
            n.notify_one();
        }
    }

    async fn set_state(&self, s: State) {
        self.state.send_replace(s.clone());
        (self.events)(Event::State(s)).await;
    }

    fn offline(&self) -> Problem {
        Problem::new(
            comms_inward::chan::said::offline(&self.cfg.machine),
            String::new(),
        )
    }

    async fn run(self: Arc<Self>) {
        let mut ssh = self.transport.subscribe();
        let mut kicks = self.kick.subscribe();
        let mut backoff = Backoff::new();
        let mut epoch = 0u64;
        loop {
            let Some(t) = ssh.borrow_and_update().clone() else {
                self.set_ssh(false).await;
                self.set_state(State::Down {
                    problem: self.offline(),
                    needs_hand: false,
                    blocker: None,
                })
                .await;
                if ssh.changed().await.is_err() {
                    return;
                }
                continue;
            };
            self.set_state(State::Connecting).await;
            let started = tokio::time::Instant::now();
            let cut = Arc::new(Notify::new());
            *lock(&self.cut) = Some(Arc::clone(&cut));
            let installs = t.installs();
            let (saw_hello, needs_hand, reached) = match self.connect(&t, &cut).await {
                Ok(Connected {
                    client,
                    reader,
                    hello,
                    relation,
                    differs,
                    carried,
                }) => {
                    epoch += 1;
                    *lock(&self.client) = Some(Arc::clone(&client));
                    *lock(&self.theirs) = Some(hello.build_id.clone());
                    (self.events)(Event::Attached {
                        hello: hello.line.clone(),
                        client: Arc::clone(&client),
                    })
                    .await;
                    self.desk.up(&client, carried.as_ref());
                    self.set_state(State::Up {
                        epoch,
                        relation,
                        differs,
                    })
                    .await;
                    // 连上那一刻：断着时按过、还没用掉的那一下作废。
                    kicks.borrow_and_update();
                    self.pump(&client, reader, &cut).await;
                    client.shutdown();
                    *lock(&self.client) = None;
                    *lock(&self.theirs) = None;
                    // 流断了：下面那条 SSH 多半也断了（心跳判死）；下一拨开得出 exec 再报够着。
                    self.set_ssh(false).await;
                    self.set_state(State::Down {
                        problem: self.offline(),
                        needs_hand: false,
                        blocker: None,
                    })
                    .await;
                    (true, false, true)
                }
                Err(Fail {
                    kind: FailKind::Interrupted,
                    ..
                }) => {
                    // 宿主报的那件事就是「马上再来」：不说断了、不睡，从下界来。
                    *lock(&self.cut) = None;
                    kicks.borrow_and_update();
                    backoff.after_wait(true, false, true);
                    continue;
                }
                Err(Fail { problem, kind }) => {
                    let blocker = match kind {
                        FailKind::Blocked(b) => Some(b),
                        FailKind::Transient | FailKind::Unreached | FailKind::Interrupted => None,
                    };
                    let needs_hand = match blocker {
                        Some(Blocker::NotUnix) => true,
                        Some(Blocker::NoBackend | Blocker::OldBackend) => !installs,
                        None => false,
                    };
                    self.set_state(State::Down {
                        problem,
                        needs_hand,
                        blocker,
                    })
                    .await;
                    (false, needs_hand, kind != FailKind::Unreached)
                }
            };
            *lock(&self.cut) = None;
            // 握上手且活过 30 s 才清零；首次失败只等下界，这一轮没站住才翻倍。
            let healthy = should_reset_backoff(saw_hello, started.elapsed());
            let wait = backoff.before_wait(healthy);
            let kicked = {
                let ssh_changed = async {
                    ssh.changed().await.discard(discard::Why::Signal);
                };
                let kicked = async {
                    kicks.changed().await.discard(discard::Why::Signal);
                };
                if needs_hand {
                    tokio::select! {
                        () = kicked => true,
                        () = ssh_changed => true,
                    }
                } else {
                    tokio::select! {
                        () = tokio::time::sleep(wait) => false,
                        () = kicked => true,
                        () = ssh_changed => true,
                    }
                }
            };
            backoff.after_wait(kicked, healthy, reached);
        }
    }

    /// 接一次：直接接 ⇒ 没接上就 ensure ⇒ 再接；每一轮自己装后端的宿主见那台不是这一版（或认不得这一形 attach 行）⇒ 请它换一次、换了再接。
    /// 链路都开不出来（拨不通 · 没人回话 · 拨着被宿主叫停）⇒ 不 ensure（ensure 走的是同一条路）。
    async fn connect(&self, t: &Arc<T>, cut: &Notify) -> Result<Connected<T::Stream>, Fail> {
        let attached = match self.attach_on(t, cut).await {
            Ok(a) => Some(a),
            Err(AttachOr::Shake(AttachFail::Old)) => None,
            Err(e @ (AttachOr::Dial(_) | AttachOr::Unanswered | AttachOr::Interrupted)) => {
                return Err(self.attach_problem(e))
            }
            Err(_) => {
                // 接不上（没人在听 · 不是我们的后端 …）：装得了的宿主先把这一版放到落点，再 ensure。
                if t.installs() {
                    t.install().await;
                }
                self.ensure(t, ENSURE).await?;
                Some(self.attach_after_ensure(t, cut).await?)
            }
        };
        // 自己装后端的宿主：那台不是手上这一版 ⇒ 先装、再请它确保在听的不旧于落点（只升不降由 `--replace` 自己判）；换了才重接。
        let attached = match attached {
            Some(a) if t.installs() && a.relation != Relation::Same => {
                t.install().await;
                if self.ensure(t, ENSURE_REPLACE).await? {
                    tracing::info!(
                        "resident feed [{}] 那台换上了落点那一份 ⇒ 重接",
                        self.cfg.machine
                    );
                    drop(a);
                    self.attach_after_ensure(t, cut).await?
                } else {
                    a
                }
            }
            Some(a) => a,
            None if t.installs() => {
                tracing::info!(
                    "resident feed [{}] 那台认不得这一形 attach 行 ⇒ 装上这一版、换掉再接",
                    self.cfg.machine
                );
                t.install().await;
                self.ensure(t, ENSURE_REPLACE).await?;
                self.attach_after_ensure(t, cut).await?
            }
            None => return Err(self.attach_problem(AttachOr::Shake(AttachFail::Old))),
        };
        let attach::Attached {
            reader,
            writer,
            hello,
            relation,
            carried,
        } = attached;
        let client = writer.into_client(hello.witness());
        client.name_machine(&self.cfg.machine);
        client.set_viewer_tz(self.cfg.tz.clone());
        let differs = client.offer().frames_differ().to_vec();
        if !differs.is_empty() {
            tracing::warn!(
                "resident feed [{}] 那台这几种帧形状跟本侧不同，不解、不交：{differs:?}{}",
                self.cfg.machine,
                if hello.frame.is_none() {
                    "（hello 全形解不开）"
                } else {
                    ""
                }
            );
        }
        Ok(Connected {
            client,
            reader,
            hello,
            relation,
            differs,
            carried,
        })
    }

    /// 开一条 `--resident-attach`、握手：attach 行带手上这一版（那台回新旧）、这一跳远 ⇒ 要那台压（回认 ⇒ 读半边边读边解；近的不要）。
    /// 那台认不得这一形（比 `deflate` 那一格早）而这一侧不自己装后端 ⇒ 不带这一格再开一条，照不压接上；自己装的宿主照旧当旧（装上这一版再接）。
    async fn attach_on(
        &self,
        t: &Arc<T>,
        cut: &Notify,
    ) -> Result<attach::Attached<T::Stream>, AttachOr> {
        match self.attach_once(t, cut, true).await {
            Err(AttachOr::Shake(AttachFail::Old)) if !t.installs() => {
                tracing::info!(
                    "resident feed [{}] 那台认不得带 deflate 的 attach 行 ⇒ 不压再接",
                    self.cfg.machine
                );
                self.attach_once(t, cut, false).await
            }
            r => r,
        }
    }

    /// 开一条 `--resident-attach`、握手一次（`deflate` 且这一跳远（[`Transport::far`]）⇒ attach 行要那台压）。
    /// 开链路那一步：[`DIAL_WITHIN`] 里拨号核心一步进展都没报（[`Progress`]）⇒ 丢下（[`AttachOr::Unanswered`]）；每报一步重新计时；拨着的时候宿主报了事 ⇒ 丢下（[`AttachOr::Interrupted`]）：
    /// [`Feed::wake`] · [`Feed::reconnect`]（按的那一下在开拨之后才算：开拨之前按的照旧只省一次等）· [`Feed::set_transport`]（拨这一轮的收工信号 `cut`）。
    async fn attach_once(
        &self,
        t: &Arc<T>,
        cut: &Notify,
        deflate: bool,
    ) -> Result<attach::Attached<T::Stream>, AttachOr> {
        let mut kicked = self.kick.subscribe();
        let progress = Progress::default();
        let dial = dial_within(
            &progress,
            t.stream(backend_command(ATTACH), progress.clone()),
        );
        let cut = cut.notified();
        tokio::pin!(dial, cut);
        let stream = tokio::select! {
            got = &mut dial => match got {
                Some(Ok(stream)) => stream,
                Some(Err(p)) => {
                    self.set_ssh(false).await;
                    return Err(AttachOr::Dial(p));
                }
                None => {
                    self.set_ssh(false).await;
                    return Err(AttachOr::Unanswered);
                }
            },
            () = &mut cut => return Err(AttachOr::Interrupted),
            _ = kicked.changed() => return Err(AttachOr::Interrupted),
        };
        // exec 开出来了 ⇒ 拨通、认证过：SSH 够着那一台（后面常驻后端在不在是另一回事）。
        self.set_ssh(true).await;
        attach::handshake(
            stream,
            self.cfg.tz.as_deref(),
            self.cfg.mine.as_deref(),
            Some(self.desk.ask_for_attach()),
            deflate && t.far(),
            self.cfg.handshake,
        )
        .await
        .map_err(AttachOr::Shake)
    }

    /// ensure 之后：小中继回 `absent`（常驻还没绑上口）隔一会儿再试。
    async fn attach_after_ensure(
        &self,
        t: &Arc<T>,
        cut: &Notify,
    ) -> Result<attach::Attached<T::Stream>, Fail> {
        for _ in 0..RELAY_TRIES {
            match self.attach_on(t, cut).await {
                Ok(a) => return Ok(a),
                Err(AttachOr::Shake(AttachFail::Refused(r))) if r == RELAY_ABSENT => {}
                Err(e) => return Err(self.attach_problem(e)),
            }
            tokio::time::sleep(RELAY_WAIT).await;
        }
        Err(Fail::new(
            Problem::new(
                copy_text(
                    "rsRemoteResident.relay.absent",
                    &[("machine", &self.cfg.machine)],
                ),
                format!("{RELAY_TRIES} × {}ms", RELAY_WAIT.as_millis()),
            ),
            FailKind::Transient,
        ))
    }

    /// 握手走不通 ⇒ 那一句。拨不通 · 断在路上的（被掐 · 小中继与常驻断开）再试会好；不是我们的后端 · 回话不对 ⇒ 装过才会好。
    fn attach_problem(&self, e: AttachOr) -> Fail {
        let m = &self.cfg.machine;
        let f = match e {
            AttachOr::Dial(p) => return Fail::new(p, FailKind::Transient),
            AttachOr::Unanswered => {
                return Fail::new(
                    Problem::new(
                        self.offline().said,
                        format!(
                            "{} · {}s",
                            copy_text("reason.link.noAnswer", &[]),
                            DIAL_WITHIN.as_secs()
                        ),
                    ),
                    FailKind::Unreached,
                )
            }
            AttachOr::Interrupted => return Fail::new(self.offline(), FailKind::Interrupted),
            AttachOr::Shake(f) => f,
        };
        match f {
            AttachFail::Refused(r) if r == RELAY_ABSENT => Fail::new(
                Problem::new(
                    copy_text("rsRemoteResident.relay.absent", &[("machine", m)]),
                    "",
                ),
                FailKind::Transient,
            ),
            AttachFail::Refused(r) if r == "unreachable" => Fail::new(
                Problem::new(
                    copy_text("rsRemoteResident.relay.unreachable", &[("machine", m)]),
                    "",
                ),
                FailKind::Transient,
            ),
            AttachFail::Refused(r) => Fail::new(
                Problem::new(
                    copy_text("rsRemoteResident.handshake.refused", &[("why", &r)]),
                    "",
                ),
                FailKind::Transient,
            ),
            AttachFail::NotOurs(line) => Fail::new(
                Problem::new(copy_text("rsRemoteResident.hello.notOurs", &[]), line),
                FailKind::Blocked(Blocker::NoBackend),
            ),
            AttachFail::Cut { said, raw, .. } => {
                Fail::new(Problem::new(said, raw), FailKind::Transient)
            }
            AttachFail::Reply(said) => Fail::new(
                Problem::new(said, ""),
                FailKind::Blocked(Blocker::NoBackend),
            ),
            AttachFail::Old => Fail::new(
                Problem::new(
                    copy_core::backend_old(m),
                    format!("refused: {}", attach::REFUSE_MALFORMED),
                ),
                FailKind::Blocked(Blocker::OldBackend),
            ),
        }
    }

    /// `--resident-ensure`（常驻不在就起）/ `--resident-ensure --replace`（在听的比落点旧才换）。回那台起没起新的一位。
    async fn ensure(&self, t: &Arc<T>, word: &'static str) -> Result<bool, Fail> {
        let exec = t
            .capture(backend_command(word))
            .await
            .map_err(|p| Fail::new(p, FailKind::Transient))?;
        read_one_shot(&exec, &self.cfg.machine).map(|v| v["replaced"] == true)
    }

    /// 读到头为止：应答交回请求方，别的帧原样交宿主。到头 · 读不动 · 被叫停（[`Feed::reconnect`]）· 心跳判死 ⇒ 回。
    ///
    /// 心跳：[`HEARTBEAT_IDLE`] 没来一个字节 ⇒ 问一句活；一问到期（[`PingWindow`]，照这条流量到的往返放宽）还是一个字节都没来 ⇒ 再问一句，
    /// 连着 [`PING_MISSES`] 问都没来 ⇒ 判死（被叫醒时当场问，同一个判法）。问出去到流上来下一行，记一个往返样本。
    /// 「来了字节」按读进来的字节算、不按整行（一行几 MiB 的记录在慢网上要读好一阵，读着就是活着）。
    /// 那台不认 `ping` ⇒ 不问（判死照旧靠 SSH 保活）。读一行不许中途取消（[`read_capped_line`] 不是 cancellation-safe）⇒ 这一行的读在心跳那几支之间一直留着。
    async fn pump(
        &self,
        client: &Arc<InboundClient>,
        reader: crate::line::Downstream<tokio::io::ReadHalf<T::Stream>>,
        cut: &Notify,
    ) {
        let mut buf = Vec::new();
        // 这条流上有几行不是合法 UTF-8（按替换字符读的）：数着，按 2 的幂次说一句。
        let mut bad_utf8 = 0u64;
        // 帧门挡下的那几种：每种说一次（之后只数）。
        let mut held: std::collections::BTreeMap<String, u64> = Default::default();
        let beats = client.accepts(PING);
        let heard = std::sync::atomic::AtomicU64::new(0);
        let mut reader = Counted {
            inner: reader,
            n: &heard,
        };
        // 上一次看到的字节数 · 那一刻；在问着的那一回（没在问 ⇒ `None`）。
        let mut seen = (0u64, tokio::time::Instant::now());
        let mut asked: Option<Asking> = None;
        let mut window = PingWindow::default();
        loop {
            let got = {
                let read = read_capped_line(&mut reader, &mut buf, BACKEND_FRAME_LINE_CAP);
                tokio::pin!(read);
                loop {
                    let due = match &asked {
                        Some(a) => a.last + window.window(),
                        None => seen.1 + HEARTBEAT_IDLE,
                    };
                    tokio::select! {
                        r = &mut read => break r,
                        () = cut.notified() => return,
                        () = tokio::time::sleep_until(due), if beats => {
                            let n = heard.load(std::sync::atomic::Ordering::Relaxed);
                            let now = tokio::time::Instant::now();
                            if n != seen.0 {
                                // 读着一行长的：来着字节就是活着。
                                if let Some(a) = asked.take() {
                                    window.sample(now - a.first);
                                }
                                seen = (n, now);
                            } else if let Some(a) = &mut asked {
                                a.unanswered += 1;
                                if a.unanswered >= PING_MISSES {
                                    tracing::warn!(
                                        "[{}] 常驻流连着问活 {} 回都没回话（{}s，一问的期限 {}s）⇒ 判这条死了、重接",
                                        self.cfg.machine,
                                        a.unanswered,
                                        (now - a.first).as_secs(),
                                        window.window().as_secs()
                                    );
                                    return;
                                }
                                a.last = ask_alive(client);
                            } else {
                                asked = Some(Asking::new(ask_alive(client)));
                            }
                        }
                        () = self.ask.notified(), if beats => {
                            if asked.is_none() {
                                asked = Some(Asking::new(ask_alive(client)));
                            }
                        }
                    }
                }
            };
            let now = tokio::time::Instant::now();
            if let Some(a) = asked.take() {
                window.sample(now - a.first);
            }
            seen = (heard.load(std::sync::atomic::Ordering::Relaxed), now);
            match got {
                Ok(CappedLine::Line) => {
                    if std::str::from_utf8(&buf).is_err() {
                        bad_utf8 += 1;
                        if bad_utf8.is_power_of_two() {
                            tracing::warn!(
                                "[{}] 常驻流上已有 {bad_utf8} 行不是合法 UTF-8（按替换字符读）",
                                self.cfg.machine
                            );
                        }
                    }
                    let text = String::from_utf8_lossy(&buf);
                    let line = text.trim_end_matches(['\n', '\r']);
                    if line.trim().is_empty() {
                        continue;
                    }
                    match client.admit(line) {
                        Admit::Pass(kind) => route(client, &kind, line, &self.events).await,
                        Admit::Differs(kind) | Admit::Unknown(kind) => {
                            let n = held.entry(kind).or_default();
                            *n += 1;
                            if *n == 1 {
                                tracing::warn!(
                                    "[{}] 常驻流上这种帧形状对不上 / 不认，丢（之后只数）：{:?}",
                                    self.cfg.machine,
                                    held.keys().collect::<Vec<_>>()
                                );
                            }
                        }
                    }
                }
                Ok(CappedLine::TooLong(n)) => {
                    tracing::warn!(
                        "[{}] 常驻流上一行 {n} 字节，超过上限 {BACKEND_FRAME_LINE_CAP}，丢了这一行",
                        self.cfg.machine
                    );
                    (self.events)(Event::Gap).await;
                }
                Ok(CappedLine::Eof) | Err(_) => return,
            }
        }
    }
}

/// 问活那一条命令（零载荷，回 `ok`）。
const PING: &str = "ping";

/// 在问着的那一回：头一问发出去的那一刻（量往返从它算）· 最近一问的那一刻 · 已到期没答的几问。
struct Asking {
    first: tokio::time::Instant,
    last: tokio::time::Instant,
    unanswered: u32,
}

impl Asking {
    fn new(at: tokio::time::Instant) -> Self {
        Asking {
            first: at,
            last: at,
            unanswered: 0,
        }
    }
}

/// 发一句问活、不等它（回话也是流上的一行，[`Feed::pump`] 按来没来字节判；这一问自己等到 [`PING_CEIL`]，晚到的回话照收、不撤）。
/// 回发出去的那一刻。
fn ask_alive(client: &Arc<InboundClient>) -> tokio::time::Instant {
    let c = Arc::clone(client);
    tokio::spawn(async move {
        if let Err(e) = c.call(PING, serde_json::Value::Null, PING_CEIL).await {
            tracing::debug!("问活没答上（{e:?}）：判不判死看流上来没来字节");
        }
    });
    tokio::time::Instant::now()
}

/// 读端套一层：数读进来（`consume` 掉）的字节，心跳按它判「来没来字节」。
struct Counted<'a, R> {
    inner: R,
    n: &'a std::sync::atomic::AtomicU64,
}

impl<R: AsyncRead + Unpin> AsyncRead for Counted<'_, R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        out: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = out.filled().len();
        let r = Pin::new(&mut self.inner).poll_read(cx, out);
        let got = (out.filled().len() - before) as u64;
        self.n.fetch_add(got, std::sync::atomic::Ordering::Relaxed);
        r
    }
}

impl<R: tokio::io::AsyncBufRead + Unpin> tokio::io::AsyncBufRead for Counted<'_, R> {
    fn poll_fill_buf(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<&[u8]>> {
        Pin::new(&mut self.get_mut().inner).poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        self.n
            .fetch_add(amt as u64, std::sync::atomic::Ordering::Relaxed);
        Pin::new(&mut self.inner).consume(amt);
    }
}

/// 开一条 attach 没成在哪一层：链路没开出来（拨不通 · 到期限没人回话 · 拨着被宿主叫停）· 握手走不通。
enum AttachOr {
    Dial(Problem),
    Unanswered,
    Interrupted,
    Shake(AttachFail),
}

/// 过了帧门的一行：`reply` · `cancelled` 按类型解后交回请求方（[`deliver_answer`]）；别的原样交宿主。
/// 应答解不开 ⇒ 两端契约对不上：当一条失败交回去，原文进详情。
async fn route(client: &InboundClient, kind: &str, line: &str, events: &Sink) {
    if kind != "reply" && kind != "cancelled" {
        events(Event::Frame(line.to_string())).await;
        return;
    }
    match cmd_wire::frame::decode::<serde::de::IgnoredAny>(line) {
        Ok(f) => {
            if deliver_answer(client, f).is_some() {
                events(Event::Frame(line.to_string())).await;
            }
        }
        Err(e) => {
            #[derive(serde::Deserialize)]
            struct IdOnly {
                #[serde(default)]
                id: String,
            }
            let id = serde_json::from_str::<IdOnly>(line)
                .map(|i| i.id)
                .unwrap_or_default();
            tracing::warn!("应答解不开（{e}）");
            client.route_reply(
                &id,
                false,
                Some("unreadable".to_string()),
                Some(copy_core::reply_unreadable(&copy_core::peer_machine())),
                Some(line.to_string()),
                None,
            );
        }
    }
}

/// shell 找不到命令 / 不能执行。
const ABSENT_CODES: [u32; 2] = [126, 127];

/// 一次性 CLI 的回话（`--resident-ensure [--replace]` · `--resident-stop`，桌面停那一趟也读它）：退出 0 ＋ stdout 一行 JSON 对象 ⇒ 成；
/// stdout 里有 hello（[`OLD_BACKEND_MARKER`]）⇒ 老后端、要更新；127 / 126 ⇒ 没装；非 0 ＋ stderr 一行失败信封（`cmd_wire::failed::Failed`）⇒ 那一句
/// （`code == "unsupported"` ⇒ 那台不是 Unix）；对不上协议 ⇒ 原样进详情。
pub fn read_one_shot(exec: &Exec, machine: &str) -> Result<serde_json::Value, Fail> {
    let out = exec.stdout.trim();
    let err = exec.stderr.trim();
    let raw = || format!("exit: {:?}\nstdout: {out}\nstderr: {err}", exec.exit_status);
    let install = |p: Problem| Err(Fail::new(p, FailKind::Blocked(Blocker::NoBackend)));
    if out.contains(OLD_BACKEND_MARKER) {
        return Err(Fail::new(
            Problem::new(copy_core::backend_old(machine), raw()),
            FailKind::Blocked(Blocker::OldBackend),
        ));
    }
    match exec.exit_status {
        Some(0) => match serde_json::from_str::<serde_json::Value>(out)
            .ok()
            .filter(serde_json::Value::is_object)
        {
            Some(v) => Ok(v),
            None => install(Problem::new(
                copy_text("rsRemoteResident.ensure.answerUnreadable", &[]),
                raw(),
            )),
        },
        Some(c) if ABSENT_CODES.contains(&c) => install(Problem::new(
            copy_text("remoteHealth.head.notDeployed", &[]),
            raw(),
        )),
        _ => {
            match serde_json::from_str::<cmd_wire::failed::Failed>(err).ok() {
                // 那台的 `said` 这一格就是它的原话（脱离不了的系统报错）：句子由这边说，原话进详情。
                Some(f) if f.code == "unsupported" => Err(Fail::new(
                    Problem::new(
                        copy_text("rsRemoteResident.ensure.unsupported", &[]),
                        if f.detail.is_empty() {
                            f.said
                        } else {
                            format!("{}\n{}", f.said, f.detail)
                        },
                    ),
                    FailKind::Blocked(Blocker::NotUnix),
                )),
                Some(f) => install(Problem::new(f.said, f.detail)),
                None => install(Problem::new(
                    copy_text("rsRemoteResident.ensure.noReason", &[]),
                    raw(),
                )),
            }
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 这一轮连接结束后，退避该不该重置回 [`RECONNECT_MIN`]。
///
/// 两个条件**都要满足**：握手成功过（`saw_hello`）**且**这条连接活过 [`MIN_HEALTHY_UPTIME`]。
/// 只看前者就是 I-1 那个自激循环；只看后者会把「连了很久但从没握手成功」也当健康。
fn should_reset_backoff(saw_hello: bool, lived: Duration) -> bool {
    saw_hello && lived >= MIN_HEALTHY_UPTIME
}

/// 把当前退避翻倍并封顶到 [`RECONNECT_MAX`]。[`Backoff::after_wait`] 在这一轮没站住时调用。
fn next_backoff(cur: Duration) -> Duration {
    (cur * 2).min(RECONNECT_MAX)
}

/// 重连循环手里的那一份退避，模块私有：别处造不出一份（第二条重连循环写不出来）。两段式：
/// 一轮结束先 [`Backoff::before_wait`]（这一轮站住了 ⇒ 回下界）按它睡，睡完 [`Backoff::after_wait`]：
/// 被叫醒（［重新连接］· 换了一条 SSH）⇒ 回下界；这一轮没站住（没握上手，或握上手却活不过 [`MIN_HEALTHY_UPTIME`]）⇒ 翻倍。
/// 「握上手却活不过」也要翻倍：对端发完 hello 就死的话，每一轮都「连上过」，只看握手退避就永远停在下界（F05 / I-1 的自激循环）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Backoff {
    cur: Duration,
}

impl Backoff {
    fn new() -> Self {
        Backoff { cur: RECONNECT_MIN }
    }

    /// 这一轮结束：`healthy` = [`should_reset_backoff`] 的结论。回这一次要睡多久。
    fn before_wait(&mut self, healthy: bool) -> Duration {
        if healthy {
            self.cur = RECONNECT_MIN;
        }
        self.cur
    }

    /// 睡完（或被叫醒）之后。`reached` = 这一轮够到了那一台（拨出去有回话）；拨到 [`DIAL_WITHIN`] 都没回话 ⇒ `false`：
    /// 没有一次登录落到那台身上，涨退避护不了谁，只会让网回来之后多等 ⇒ 下一轮照样从下界来。
    fn after_wait(&mut self, kicked: bool, healthy: bool, reached: bool) {
        if kicked || !reached {
            self.cur = RECONNECT_MIN;
        } else if !healthy {
            self.cur = next_backoff(self.cur);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/feed_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/backoff_tests.rs"]
mod backoff_tests;
