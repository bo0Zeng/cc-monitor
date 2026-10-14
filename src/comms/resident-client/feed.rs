//! **一台的常驻流，断了自己接回去** —— 桌面与手机同一条重连循环（全仓只这一处，`one_loop_guard`）。宿主只接几根线：
//! 下面那条 SSH（[`Transport`]）· 往外交的事（[`Sink`]）· 装不装得了后端（[`Transport::installer`]）。
//!
//! 接一台只有一条路：先直接接常驻流（`--resident-attach`，平时一条 exec 就接上）；没接上 ⇒ `--resident-ensure`（常驻不在就起）
//! ⇒ 再接（小中继回 `absent` 时隔 [`RELAY_WAIT`] 再试）。接上之后照版本门（[`crate::gate`]）：
//! - 装得了后端的宿主（桌面）：问本机常驻后端（[`Install::verdict`]）；那台旧 ⇒ `--resident-ensure --replace` 换一次再接；同或新 ⇒ 接。
//! - 装不了的（手机）：问接上的那一台自己；旧 ⇒ 照接、说「要更新」；同或新 ⇒ 接。
//!
//! 能不能用逐条命令由形状指纹判（[`InboundClient::call`]）。
//!
//! 什么时候接：
//! - 宿主交了一条 SSH（[`Feed::set_transport`]）⇒ 当场接；换了一条 ⇒ 旧流关掉、在新的上接；交 `None` ⇒ 不接、停在断开。
//! - 每一轮开拨之前先做宿主那一步（[`Transport::before_round`]，桌面：把这一版的后端放到那台）。
//! - 流断了、SSH 还在 ⇒ 交一个 [`Event::Lost`]（让宿主去探 SSH：多半是它先死了），再按退避接（[`crate::backoff`]，活过 30 s 才清零）。
//! - 走不通、要人动手的那几种不自己反复试，等 [`Feed::reconnect`] / [`Feed::wake`]（［重试］· 回前台 · 网络变了 · 机器页「起」）：
//!   那台不是 Unix（常驻脱离不了）恒是；没装 · 起不了常驻 · 不是我们的后端只在装不了后端的宿主上是（装得了的下一轮装过再试）。
//!
//! 每次接上先交 [`Event::Attached`]（那台的 hello 原样一行 ＋ 这条连接的客户端），再交 [`State::Up`]（`epoch` 加一，之前在路上的问答作废）。

use crate::attach::{self, AttachFail, RELAY_ABSENT, RELAY_TRIES, RELAY_WAIT};
use crate::backoff::{should_reset_backoff, Backoff};
use crate::client::{BackendHello, InboundClient};
use crate::gate::Verdict;
use crate::line::{read_capped_line, CappedLine, BACKEND_FRAME_LINE_CAP};
use crate::watch::{self, WatchFrom, Watched};
use copy_core::copy_text;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{watch as tw, Notify};

/// 给人看的那一句 ＋ 复制详情。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub text: String,
    pub detail: String,
}

impl Problem {
    pub fn new(text: impl Into<String>, detail: impl Into<String>) -> Self {
        Problem {
            text: text.into(),
            detail: detail.into(),
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

/// 下面那条 SSH：开一条长 exec（字节原样对拷）· 跑一次收全；外加宿主那两样（每一轮之前做的那一步 · 装不装得了后端）。
/// 手机：拨号核心在这台手机上开的链路；桌面：本机常驻后端给的链路（monitor `stream_source/run.rs`）。
pub trait Transport: Send + Sync + 'static {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;
    fn stream(&self, command: String) -> Boxed<'_, Result<Self::Stream, Problem>>;
    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>>;
    /// 每一轮开拨之前（桌面：部署预检，把这一版的后端放到那台的落点）。缺省不做事。
    fn before_round(&self) -> Boxed<'_, ()> {
        Box::pin(async {})
    }
    /// 这一侧装得了后端 ⇒ 版本门问它、旧了换装（桌面）；装不了 ⇒ `None`（手机，缺省）。
    fn installer(&self) -> Option<&dyn Install> {
        None
    }
}

/// 装得了后端的宿主：「那台报 `theirs`，换还是接」由它问本机常驻后端（判定只在后端）。
pub trait Install: Send + Sync {
    fn verdict<'a>(
        &'a self,
        mine: &'a str,
        theirs: &'a str,
        replaced: bool,
    ) -> Boxed<'a, Result<Verdict, Problem>>;
}

/// 那台后端的一条子命令在远端 shell 里的写法（落点固定 `~/.cc-monitor/bin/ccm`，打头的 `--` 让它当后端用）。只收常量词。
pub fn backend_command(word: &'static str) -> String {
    format!("{} -- {word}", relay_route_core::BACKEND_LANDING_SHELL)
}

pub const ATTACH: &str = "--resident-attach";
pub const ENSURE: &str = "--resident-ensure";
/// 换装：在跑的那一个停掉、换成落点上这一版。
pub const ENSURE_REPLACE: &str = "--resident-ensure --replace";

/// 一台此刻通不通。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum State {
    Connecting,
    /// 通着：第 `epoch` 条流；`notice` = 通着但要人动手的那一句（那台后端旧了）。
    Up {
        epoch: u64,
        notice: Option<Problem>,
    },
    /// 断着：那一句；`needs_hand` ⇒ 不会自己好（要［重试］或人去装 / 起）；`unsupported` ⇒ 那台不是 Unix（常驻脱离不了）。
    Down {
        problem: Problem,
        needs_hand: bool,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        unsupported: bool,
    },
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
    /// `stream-watch` 应答里的 `from[]`。
    WatchFrom(Vec<WatchFrom>),
    /// 流断了、SSH 还在：请宿主探一次 SSH。
    Lost,
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
            Event::WatchFrom(w) => f.debug_tuple("WatchFrom").field(w).finish(),
            Event::Lost => f.write_str("Lost"),
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

/// 走不通的一轮：那一句 ＋ 是哪一类。
struct Fail {
    problem: Problem,
    kind: FailKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailKind {
    /// 断在路上（被掐 · 小中继与常驻断开 · 拨不通 · 问不成换不换）：再试会好。
    Transient,
    /// 没装 · 起不了常驻 · 不是我们的后端：装一次会好 ⇒ 装得了的宿主下一轮再试，装不了的等人。
    Install,
    /// 那台不是 Unix：怎样都不会好。
    Unsupported,
}

impl Fail {
    fn new(problem: Problem, kind: FailKind) -> Self {
        Fail { problem, kind }
    }
}

/// 接上之后到开读之前定下的那几样。
struct Connected<S> {
    client: Arc<InboundClient>,
    reader: tokio::io::BufReader<tokio::io::ReadHalf<S>>,
    hello: attach::Hello,
    /// 装得了后端的宿主：本机常驻后端判那台比手上这一版旧（换过一次还旧）⇒ 那一句。
    notice: Option<Problem>,
}

/// 一台的常驻流。
pub struct Feed<T: Transport> {
    cfg: Config,
    events: Sink,
    transport: tw::Sender<Option<Arc<T>>>,
    state: tw::Sender<State>,
    client: Mutex<Option<Arc<InboundClient>>>,
    /// ［重试］· 回前台 · 网络变了 · 「起」：每按一下 +1。接上那一刻之前按的作废（不留到下一次断线白白跳过一轮退避）。
    kick: tw::Sender<u64>,
    /// 当前这条流的收工信号（[`Feed::reconnect`] 拨它）。
    cut: Mutex<Option<Arc<Notify>>>,
    /// 在看名单：`None` ＝ 宿主从没报过（不发 `stream-watch`，那台照旧全发）。
    watched: Mutex<Option<Watched>>,
    watch_tick: tw::Sender<u64>,
    /// 这一台那两个循环（重连 · 报名单）：[`Feed::close`] 收掉。
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl<T: Transport> Feed<T> {
    /// 起一台（在当前 tokio 运行时上起它的循环）；`transport` ＝ 起来那一刻手上的那条 SSH（还没有 ⇒ `None`，之后经 [`Feed::set_transport`] 交）。
    pub fn start(cfg: Config, transport: Option<Arc<T>>, events: Sink) -> Arc<Self> {
        let feed = Arc::new(Feed {
            cfg,
            events,
            transport: tw::channel(transport).0,
            state: tw::channel(State::Connecting).0,
            client: Mutex::new(None),
            kick: tw::channel(0).0,
            cut: Mutex::new(None),
            watched: Mutex::new(None),
            watch_tick: tw::channel(0).0,
            tasks: Mutex::new(Vec::new()),
        });
        let run = tokio::spawn(Arc::clone(&feed).run());
        let report = tokio::spawn(Arc::clone(&feed).report_watch());
        lock(&feed.tasks).extend([run, report]);
        feed
    }

    /// 不再看这台：两个循环收掉、流关掉、在等的问答一律「断了」。
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

    /// 回到前台 · 网络变了 · ［重新连接］：通着不动；没通 ⇒ 不等退避马上再接（此刻正在拨 ⇒ 这一轮没成就不睡）。
    pub fn wake(&self) {
        if !matches!(*self.state.borrow(), State::Up { .. }) {
            self.kick.send_modify(|k| *k += 1);
        }
    }

    /// 这台在屏上的名字。
    pub fn machine(&self) -> &str {
        &self.cfg.machine
    }

    pub fn state(&self) -> State {
        self.state.borrow().clone()
    }

    /// 名单从空开始报（宿主起步时一次：一条会话都还没人看 ⇒ 那台一条都不发）。
    pub fn watch_none(&self) {
        let fresh = {
            let mut w = lock(&self.watched);
            let fresh = w.is_none();
            w.get_or_insert_with(Watched::default);
            fresh
        };
        if fresh {
            self.watch_tick.send_modify(|t| *t += 1);
        }
    }

    /// 多一处 / 少一处在看这条会话。名单变了 ⇒ 在当前连接上整份重报。
    pub fn watch(&self, sid: &str, holding: bool) {
        let changed = {
            let mut g = lock(&self.watched);
            let fresh = g.is_none();
            let w = g.get_or_insert_with(Watched::default);
            let changed = if holding { w.hold(sid) } else { w.release(sid) };
            changed || fresh
        };
        if changed {
            self.watch_tick.send_modify(|t| *t += 1);
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
            copy_text("front.body.offline", &[("machine", &self.cfg.machine)]),
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
                self.set_state(State::Down {
                    problem: self.offline(),
                    needs_hand: false,
                    unsupported: false,
                })
                .await;
                if ssh.changed().await.is_err() {
                    return;
                }
                continue;
            };
            self.set_state(State::Connecting).await;
            let started = tokio::time::Instant::now();
            t.before_round().await;
            let cut = Arc::new(Notify::new());
            *lock(&self.cut) = Some(Arc::clone(&cut));
            let installs = t.installer().is_some();
            let (saw_hello, needs_hand) = match self.connect(&t).await {
                Ok(Connected {
                    client,
                    reader,
                    hello,
                    notice,
                }) => {
                    epoch += 1;
                    *lock(&self.client) = Some(Arc::clone(&client));
                    (self.events)(Event::Attached {
                        hello: hello.line.clone(),
                        client: Arc::clone(&client),
                    })
                    .await;
                    self.set_state(State::Up { epoch, notice }).await;
                    // 连上那一刻：断着时按过、还没用掉的那一下作废。
                    kicks.borrow_and_update();
                    self.watch_tick.send_modify(|t| *t += 1);
                    // 装不了后端的宿主：版本门那一问走这条流本身，应答要等下面开始读才回得来 ⇒ 与读并行；问完了还是这一条才补上那一句。
                    let asking = (!installs).then(|| {
                        let me = Arc::clone(&self);
                        let c = Arc::clone(&client);
                        let theirs = hello.build_id.clone();
                        tokio::spawn(async move {
                            if let Some(notice) = me.version_notice(&c, &theirs).await {
                                let same = matches!(*me.state.borrow(), State::Up { epoch: e, .. } if e == epoch);
                                if same {
                                    me.set_state(State::Up {
                                        epoch,
                                        notice: Some(notice),
                                    })
                                    .await;
                                }
                            }
                        })
                    });
                    self.pump(&client, reader, &cut).await;
                    if let Some(a) = asking {
                        a.abort();
                    }
                    client.shutdown();
                    *lock(&self.client) = None;
                    self.set_state(State::Down {
                        problem: self.offline(),
                        needs_hand: false,
                        unsupported: false,
                    })
                    .await;
                    (self.events)(Event::Lost).await;
                    (true, false)
                }
                Err(Fail { problem, kind }) => {
                    let needs_hand = match kind {
                        FailKind::Transient => false,
                        FailKind::Install => !installs,
                        FailKind::Unsupported => true,
                    };
                    self.set_state(State::Down {
                        problem,
                        needs_hand,
                        unsupported: kind == FailKind::Unsupported,
                    })
                    .await;
                    (false, needs_hand)
                }
            };
            *lock(&self.cut) = None;
            // 握上手且活过 30 s 才清零；首次失败只等下界，这一轮没站住才翻倍。
            let healthy = should_reset_backoff(saw_hello, started.elapsed());
            let wait = backoff.before_wait(healthy);
            let kicked = {
                let ssh_changed = async {
                    let _ = ssh.changed().await;
                };
                let kicked = async {
                    let _ = kicks.changed().await;
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
            backoff.after_wait(kicked, healthy);
        }
    }

    /// 接一次：直接接 ⇒ 没接上就 ensure ⇒ 再接；装得了后端的宿主再过一道版本门（旧 ⇒ 换一次再接）。
    async fn connect(&self, t: &Arc<T>) -> Result<Connected<T::Stream>, Fail> {
        let mut attached = match self.attach_once(t).await {
            Ok(a) => a,
            Err(_) => {
                self.ensure(t, ENSURE).await?;
                self.attach_after_ensure(t).await?
            }
        };
        let mut replaced = false;
        let notice = loop {
            let Some(installer) = t.installer() else {
                break None;
            };
            let theirs = attached.hello.build_id.clone();
            let verdict = crate::gate::verdict_for(self.cfg.mine.as_deref(), |mine| {
                installer.verdict(mine, &theirs, replaced)
            })
            .await
            .map_err(|p| Fail::new(p, FailKind::Transient))?;
            if verdict.replace {
                tracing::info!(
                    "resident feed [{}] 本机后端判那台常驻后端比手上这一版旧 ⇒ 换掉再接",
                    self.cfg.machine
                );
                drop(attached);
                replaced = true;
                self.ensure(t, ENSURE_REPLACE).await?;
                attached = self.attach_after_ensure(t).await?;
                continue;
            }
            break verdict.older.then(|| self.old_notice(&theirs));
        };
        let attach::Attached {
            reader,
            writer,
            hello,
        } = attached;
        let witness = BackendHello::from_hello_frame(&hello).expect("Hello 恒交得出能力事实");
        let client = writer.into_client(witness);
        client.name_machine(&self.cfg.machine);
        client.set_viewer_tz(self.cfg.tz.clone());
        Ok(Connected {
            client,
            reader,
            hello,
            notice,
        })
    }

    fn old_notice(&self, theirs: &str) -> Problem {
        Problem::new(
            copy_core::backend_old(&self.cfg.machine),
            format!(
                "theirs: {theirs}\nmine: {}",
                self.cfg.mine.as_deref().unwrap_or_default()
            ),
        )
    }

    /// 版本门（装不了后端的宿主，[`crate::gate`]）：问接上的那一台它比手上这一版旧不旧（判定在后端）。旧 ⇒ 照接、带一句「要更新」；
    /// 同或新 ⇒ 不说。那台不认这一问 · 认、可这一问的形状与本侧对不上 ⇒ 比这一问（这一形）还早 ⇒ 当旧。问不成（超时 · 断了）⇒ 不说（下次接上再问）。
    async fn version_notice(&self, client: &InboundClient, theirs: &str) -> Option<Problem> {
        let asked = crate::gate::verdict_for(self.cfg.mine.as_deref(), |mine| async move {
            // 不认这一问（比版本门还早）· 认、可这一问的形状与本侧对不上（版本门这一条改过形之前的那一版）⇒ 问不成，那台就是旧的：
            //   照接、照实说要更新（两端同一版之前别的命令本来就不发，逐条由形状指纹拦）。
            if !client.accepts(crate::gate::VERDICT_CMD)
                || client.shape_differs(crate::gate::VERDICT_CMD)
            {
                return Ok(Verdict {
                    replace: true,
                    older: true,
                });
            }
            let asked = client
                .call(
                    crate::gate::VERDICT_CMD,
                    crate::gate::verdict_args(mine, theirs, false),
                    crate::gate::VERDICT_BUDGET,
                )
                .await;
            let data = match asked {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("resident-verdict 没问成：{e}");
                    return Err(());
                }
            };
            match crate::gate::decode_verdict(&data.unwrap_or_default()) {
                Ok(v) => Ok(v),
                Err(what) => {
                    tracing::warn!("resident-verdict 答的形状不对：{what}");
                    Err(())
                }
            }
        })
        .await;
        match asked {
            Ok(v) if v.older => Some(self.old_notice(theirs)),
            _ => None,
        }
    }

    /// 开一条 `--resident-attach`、握手（attach 行带这条流的出口声明：会话帧里不要本机才用得上的 `pid`）。
    async fn attach_on(&self, t: &Arc<T>) -> Result<attach::Attached<T::Stream>, AttachOr> {
        let stream = t
            .stream(backend_command(ATTACH))
            .await
            .map_err(AttachOr::Dial)?;
        attach::handshake(
            stream,
            self.cfg.tz.as_deref(),
            Some(&attach::remote_stream_view()),
            self.cfg.handshake,
        )
        .await
        .map_err(AttachOr::Shake)
    }

    async fn attach_once(&self, t: &Arc<T>) -> Result<attach::Attached<T::Stream>, Fail> {
        self.attach_on(t).await.map_err(|e| self.attach_problem(e))
    }

    /// ensure 之后：小中继回 `absent`（常驻还没绑上口）隔一会儿再试。
    async fn attach_after_ensure(&self, t: &Arc<T>) -> Result<attach::Attached<T::Stream>, Fail> {
        for _ in 0..RELAY_TRIES {
            match self.attach_on(t).await {
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
                FailKind::Install,
            ),
            AttachFail::Cut { said, raw, .. } => {
                Fail::new(Problem::new(said, raw), FailKind::Transient)
            }
            AttachFail::Reply(said) => Fail::new(Problem::new(said, ""), FailKind::Install),
        }
    }

    /// `--resident-ensure`（常驻不在就起）/ `--resident-ensure --replace`（换成落点上这一版）。
    async fn ensure(&self, t: &Arc<T>, word: &'static str) -> Result<(), Fail> {
        let exec = t
            .capture(backend_command(word))
            .await
            .map_err(|p| Fail::new(p, FailKind::Transient))?;
        read_one_shot(&exec, &self.cfg.machine).map(|_| ())
    }

    /// 读到头为止：应答交回请求方，别的帧原样交宿主。到头 · 读不动 · 被叫停（[`Feed::reconnect`]）⇒ 回。
    async fn pump(
        &self,
        client: &Arc<InboundClient>,
        mut reader: tokio::io::BufReader<tokio::io::ReadHalf<T::Stream>>,
        cut: &Notify,
    ) {
        let mut buf = Vec::new();
        // 这条流上有几行不是合法 UTF-8（按替换字符读的）：数着，按 2 的幂次说一句。
        let mut bad_utf8 = 0u64;
        loop {
            let got = tokio::select! {
                r = read_capped_line(&mut reader, &mut buf, BACKEND_FRAME_LINE_CAP) => r,
                () = cut.notified() => return,
            };
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
                    route(client, text.trim_end_matches(['\n', '\r']), &self.events).await;
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

    /// 名单变了 · 换了一条新连接 ⇒ 在当前连接上整份重报；`from[]` 交宿主。报没成不重试：下一次接上照样整份重报。
    /// 宿主从没报过名单 ⇒ 不报（那台照旧全发）。经这里报名单的宿主（手机）不读主线外清单 ⇒ `branch` 交空的（一帧清单都不要）；
    /// 读它的宿主（桌面壳 `stream_watch.rs`）自己报，照手上那份交回。
    async fn report_watch(self: Arc<Self>) {
        let mut tick = self.watch_tick.subscribe();
        while tick.changed().await.is_ok() {
            let Some(client) = lock(&self.client).clone() else {
                continue;
            };
            let Some(sids) = lock(&self.watched).as_ref().map(Watched::sids) else {
                continue;
            };
            match client
                .call(
                    watch::WATCH_CMD,
                    watch::args(&sids, &[]),
                    watch::WATCH_BUDGET,
                )
                .await
            {
                Ok(data) => {
                    let from = watch::from_of(data.as_ref());
                    if !from.is_empty() {
                        (self.events)(Event::WatchFrom(from)).await;
                    }
                }
                Err(e) => tracing::warn!("stream_watch 报名单没报上（这条连接照旧全看）：{e:?}"),
            }
        }
    }
}

/// 开一条 attach 没成在哪一层：链路没开出来 · 握手走不通。
enum AttachOr {
    Dial(Problem),
    Shake(AttachFail),
}

/// 只有含这两个词的行才可能是应答（别的帧不必整行解一遍 JSON 才交出去）。
fn maybe_reply(line: &str) -> bool {
    line.contains("\"reply\"") || line.contains("\"cancelled\"")
}

/// 一行帧：`reply` · `cancelled` 交回请求方；别的原样交宿主。空行跳过。
async fn route(client: &InboundClient, line: &str, events: &Sink) {
    if line.trim().is_empty() {
        return;
    }
    let v = if maybe_reply(line) {
        serde_json::from_str::<serde_json::Value>(line).ok()
    } else {
        None
    };
    let Some(v) = v else {
        events(Event::Frame(line.to_string())).await;
        return;
    };
    let id = v
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    match v.get("kind").and_then(serde_json::Value::as_str) {
        Some("reply") => {
            let s = |k: &str| {
                v.get(k)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            };
            match v.get("ok").and_then(serde_json::Value::as_bool) {
                Some(ok) => {
                    client.route_reply(
                        id,
                        ok,
                        s("code"),
                        s("message"),
                        s("detail"),
                        v.get("data").cloned(),
                    );
                }
                // 应答缺 `ok`：两端契约对不上 ⇒ 当一条失败交回去，原文进详情。
                None => {
                    client.route_reply(
                        id,
                        false,
                        Some("unreadable".to_string()),
                        Some(copy_core::reply_unreadable(&copy_core::peer_machine())),
                        Some(line.to_string()),
                        None,
                    );
                }
            }
        }
        Some("cancelled") => {
            client.route_cancelled(id);
        }
        _ => events(Event::Frame(line.to_string())).await,
    }
}

/// shell 找不到命令 / 不能执行。
const ABSENT_CODES: [u32; 2] = [126, 127];

/// 不认 `--resident-ensure` 的老后端会把它当未知旗标、直接进流模式发 hello ⇒ 见到它就当「太旧」（桌面收全那一趟见到它就提前收）。
pub const OLD_BACKEND_MARKER: &str = "\"kind\":\"hello\"";

/// 一次性 CLI 的回话（`--resident-ensure [--replace]`）：退出 0 ＋ stdout 一行 JSON 对象 ⇒ 成；stdout 里有 hello ⇒ 老后端、要更新；
/// 127 / 126 ⇒ 没装；非 0 ＋ stderr 一行 `{code, message, detail}` ⇒ 那一句（`code == "unsupported"` ⇒ 那台不是 Unix）；
/// 对不上协议 ⇒ 原样进详情。
fn read_one_shot(exec: &Exec, machine: &str) -> Result<serde_json::Value, Fail> {
    let out = exec.stdout.trim();
    let err = exec.stderr.trim();
    let raw = || format!("exit: {:?}\nstdout: {out}\nstderr: {err}", exec.exit_status);
    let install = |p: Problem| Err(Fail::new(p, FailKind::Install));
    if out.contains(OLD_BACKEND_MARKER) {
        return install(Problem::new(copy_core::backend_old(machine), raw()));
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
            let env: serde_json::Value = serde_json::from_str(err).unwrap_or_default();
            let detail = env["detail"].as_str().unwrap_or_default();
            match (env["code"].as_str(), env["message"].as_str()) {
                // 那台的 `message` 这一格就是它的原话（脱离不了的系统报错）：句子由这边说，原话进详情。
                (Some("unsupported"), Some(m)) => Err(Fail::new(
                    Problem::new(
                        copy_text("rsRemoteResident.ensure.unsupported", &[]),
                        if detail.is_empty() {
                            m.to_string()
                        } else {
                            format!("{m}\n{detail}")
                        },
                    ),
                    FailKind::Unsupported,
                )),
                (Some(_), Some(m)) => install(Problem::new(m, detail)),
                _ => install(Problem::new(
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

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/feed_tests.rs"]
mod tests;
