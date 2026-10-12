//! **一台的常驻流，断了自己接回去**（手机那一侧整条循环；桌面那一侧的循环住 monitor `stream_source/run.rs`，退避与判法同用本 crate）。
//!
//! 接一台只有一条路：先直接接常驻流（`--resident-attach`，平时一条 exec 就接上）；没接上 ⇒ `--resident-ensure`（常驻不在就起）
//! ⇒ 再接（小中继回 `absent` 时隔 [`RELAY_WAIT`] 再试，同桌面）。接上之后照版本门（[`crate::gate`]）：那台旧 ⇒ 照接、说「要更新」
//! （手机还装不了后端；装得了以后换成直接换装）；同或新 ⇒ 接。能不能用逐条命令由形状指纹判（[`InboundClient::call`]）。
//!
//! 什么时候接：
//! - 宿主交了一条 SSH（[`Feed::set_transport`]）⇒ 当场接；换了一条 ⇒ 旧流关掉、在新的上接；交 `None` ⇒ 不接、停在断开。
//! - 流断了、SSH 还在 ⇒ 交一个 [`Event::Lost`]（让宿主去探 SSH：多半是它先死了），再按退避接（[`crate::backoff`]，活过 30 s 才清零）。
//! - 走不通、要人动手的那几种（没装 · 起不了常驻 · 不是我们的后端）不自己反复试：等 [`Feed::reconnect`]（［重试］· 回前台 · 网络变了）。
//!
//! 每次接上，那台整份重报会话；宿主收到的 [`State::Up`] 里 `epoch` 加一，之前在路上的问答作废。

use crate::attach::{self, AttachFail, RELAY_ABSENT, RELAY_TRIES, RELAY_WAIT};
use crate::backoff::{should_reset_backoff, Backoff};
use crate::client::{BackendHello, InboundClient};
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

/// 一次性 exec 收全的那三样（同桌面 `RemoteExec`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exec {
    pub exit_status: Option<u32>,
    pub stdout: String,
    pub stderr: String,
}

pub type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 下面那条 SSH：开一条长 exec（字节原样对拷）· 跑一次收全。宿主装（手机：拨号核心开的链路）。
pub trait Transport: Send + Sync + 'static {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;
    fn stream(&self, command: String) -> Boxed<'_, Result<Self::Stream, Problem>>;
    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>>;
}

/// 那台后端的一条子命令在远端 shell 里的写法（落点固定 `~/.cc-monitor/bin/ccm`，打头的 `--` 让它当后端用）。只收常量词。
pub fn backend_command(word: &'static str) -> String {
    format!("{} -- {word}", relay_route_core::BACKEND_LANDING_SHELL)
}

pub const ATTACH: &str = "--resident-attach";
pub const ENSURE: &str = "--resident-ensure";

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
    /// 断着：那一句；`needs_hand` ⇒ 不会自己好（要［重试］或人去装 / 起）。
    Down {
        problem: Problem,
        needs_hand: bool,
    },
}

/// 交给宿主的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    State(State),
    /// 流上的一帧（hello · `reply` · `cancelled` 以外的那些），原样一行。
    Frame(String),
    /// `stream-watch` 应答里的 `from[]`。
    WatchFrom(Vec<WatchFrom>),
    /// 流断了、SSH 还在：请宿主探一次 SSH。
    Lost,
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

pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

/// 一台的常驻流。
pub struct Feed<T: Transport> {
    cfg: Config,
    events: Sink,
    transport: tw::Sender<Option<Arc<T>>>,
    state: tw::Sender<State>,
    client: Mutex<Option<Arc<InboundClient>>>,
    kick: Notify,
    /// 当前这条流的收工信号（[`Feed::reconnect`] 拨它）。
    cut: Mutex<Option<Arc<Notify>>>,
    watched: Mutex<Watched>,
    watch_tick: tw::Sender<u64>,
    /// 这一台那两个循环（重连 · 报名单）：[`Feed::close`] 收掉。
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl<T: Transport> Feed<T> {
    /// 起一台（在当前 tokio 运行时上起它的循环）。
    pub fn start(cfg: Config, events: Sink) -> Arc<Self> {
        let feed = Arc::new(Feed {
            cfg,
            events,
            transport: tw::channel(None).0,
            state: tw::channel(State::Connecting).0,
            client: Mutex::new(None),
            kick: Notify::new(),
            cut: Mutex::new(None),
            watched: Mutex::new(Watched::default()),
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
        self.kick.notify_one();
    }

    /// 回到前台 · 网络变了：通着不动；断着马上再接。
    pub fn wake(&self) {
        if matches!(*self.state.borrow(), State::Down { .. }) {
            self.kick.notify_one();
        }
    }

    /// 这台在屏上的名字。
    pub fn machine(&self) -> &str {
        &self.cfg.machine
    }

    pub fn state(&self) -> State {
        self.state.borrow().clone()
    }

    /// 多一处 / 少一处在看这条会话。名单变了 ⇒ 在当前连接上整份重报。
    pub fn watch(&self, sid: &str, holding: bool) {
        let changed = {
            let mut w = lock(&self.watched);
            if holding {
                w.hold(sid)
            } else {
                w.release(sid)
            }
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

    fn set_state(&self, s: State) {
        self.state.send_replace(s.clone());
        (self.events)(Event::State(s));
    }

    fn offline(&self, detail: String) -> Problem {
        Problem::new(
            copy_text("front.body.offline", &[("machine", &self.cfg.machine)]),
            detail,
        )
    }

    async fn run(self: Arc<Self>) {
        let mut ssh = self.transport.subscribe();
        let mut backoff = Backoff::new();
        let mut epoch = 0u64;
        loop {
            let Some(t) = ssh.borrow_and_update().clone() else {
                self.set_state(State::Down {
                    problem: self.offline(String::new()),
                    needs_hand: false,
                });
                if ssh.changed().await.is_err() {
                    return;
                }
                continue;
            };
            self.set_state(State::Connecting);
            let started = tokio::time::Instant::now();
            let cut = Arc::new(Notify::new());
            *lock(&self.cut) = Some(Arc::clone(&cut));
            let (saw_hello, needs_hand) = match self.connect(&t).await {
                Ok((client, reader, theirs)) => {
                    epoch += 1;
                    *lock(&self.client) = Some(Arc::clone(&client));
                    self.set_state(State::Up {
                        epoch,
                        notice: None,
                    });
                    self.watch_tick.send_modify(|t| *t += 1);
                    // 版本门那一问走这条流本身，应答要等下面开始读才回得来 ⇒ 与读并行；问完了还是这一条才补上那一句。
                    let asking = {
                        let me = Arc::clone(&self);
                        let c = Arc::clone(&client);
                        tokio::spawn(async move {
                            if let Some(notice) = me.version_notice(&c, &theirs).await {
                                if matches!(*me.state.borrow(), State::Up { epoch: e, .. } if e == epoch)
                                {
                                    me.set_state(State::Up {
                                        epoch,
                                        notice: Some(notice),
                                    });
                                }
                            }
                        })
                    };
                    self.pump(&client, reader, &cut).await;
                    asking.abort();
                    client.shutdown();
                    *lock(&self.client) = None;
                    self.set_state(State::Down {
                        problem: self.offline(String::new()),
                        needs_hand: false,
                    });
                    (self.events)(Event::Lost);
                    (true, false)
                }
                Err((problem, needs_hand)) => {
                    self.set_state(State::Down {
                        problem,
                        needs_hand,
                    });
                    (false, needs_hand)
                }
            };
            *lock(&self.cut) = None;
            // 退避同桌面那一份：握上手且活过 30 s 才清零；首次失败只等下界，这一轮没站住才翻倍。
            let healthy = should_reset_backoff(saw_hello, started.elapsed());
            let wait = backoff.before_wait(healthy);
            let ssh_changed = async {
                let _ = ssh.changed().await;
            };
            let kicked = if needs_hand {
                tokio::select! {
                    () = self.kick.notified() => true,
                    () = ssh_changed => true,
                }
            } else {
                tokio::select! {
                    () = tokio::time::sleep(wait) => false,
                    () = self.kick.notified() => true,
                    () = ssh_changed => true,
                }
            };
            backoff.after_wait(kicked, healthy);
        }
    }

    /// 接一次：直接接 ⇒ 没接上就 ensure ⇒ 再接。`Err((那一句, 要不要人动手))`。
    #[allow(clippy::type_complexity)]
    async fn connect(
        &self,
        t: &Arc<T>,
    ) -> Result<
        (
            Arc<InboundClient>,
            tokio::io::BufReader<tokio::io::ReadHalf<T::Stream>>,
            String,
        ),
        (Problem, bool),
    > {
        let attached = match self.attach_once(t).await {
            Ok(a) => a,
            Err(_) => {
                self.ensure(t).await?;
                self.attach_after_ensure(t).await?
            }
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
        Ok((client, reader, hello.build_id))
    }

    /// 版本门（[`crate::gate`]）：问接上的那一台它比手上这一版旧不旧（判定在后端）。旧 ⇒ 照接、带一句「要更新」（手机还装不了后端）；
    /// 同或新 ⇒ 不说。那台不认这一问 ⇒ 比这一问还早 ⇒ 当旧。问不成（超时 · 断了）⇒ 不说（下次接上再问）。
    async fn version_notice(&self, client: &InboundClient, theirs: &str) -> Option<Problem> {
        let asked = crate::gate::verdict_for(self.cfg.mine.as_deref(), |mine| async move {
            if !client.accepts(crate::gate::VERDICT_CMD) {
                return Ok(crate::gate::Verdict {
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
            Ok(v) if v.older => Some(Problem::new(
                copy_core::backend_old(&self.cfg.machine),
                format!(
                    "theirs: {theirs}\nmine: {}",
                    self.cfg.mine.as_deref().unwrap_or_default()
                ),
            )),
            _ => None,
        }
    }

    async fn attach_once(
        &self,
        t: &Arc<T>,
    ) -> Result<attach::Attached<T::Stream>, (Problem, bool)> {
        let stream = t
            .stream(backend_command(ATTACH))
            .await
            .map_err(|p| (p, false))?;
        attach::handshake(stream, self.cfg.tz.as_deref(), None, self.cfg.handshake)
            .await
            .map_err(|f| self.attach_problem(f))
    }

    /// ensure 之后：小中继回 `absent`（常驻还没绑上口）隔一会儿再试，同桌面。
    async fn attach_after_ensure(
        &self,
        t: &Arc<T>,
    ) -> Result<attach::Attached<T::Stream>, (Problem, bool)> {
        for _ in 0..RELAY_TRIES {
            let stream = t
                .stream(backend_command(ATTACH))
                .await
                .map_err(|p| (p, false))?;
            match attach::handshake(stream, self.cfg.tz.as_deref(), None, self.cfg.handshake).await
            {
                Ok(a) => return Ok(a),
                Err(AttachFail::Refused(r)) if r == RELAY_ABSENT => {}
                Err(f) => return Err(self.attach_problem(f)),
            }
            tokio::time::sleep(RELAY_WAIT).await;
        }
        Err((
            Problem::new(
                copy_text(
                    "rsRemoteResident.relay.absent",
                    &[("machine", &self.cfg.machine)],
                ),
                format!("{RELAY_TRIES} × {}ms", RELAY_WAIT.as_millis()),
            ),
            false,
        ))
    }

    /// 握手走不通 ⇒ 那一句。断在路上的（被掐 · 小中继与常驻断开）再试会好；不是我们的后端 · 回话不对 ⇒ 要人动手。
    fn attach_problem(&self, f: AttachFail) -> (Problem, bool) {
        let m = &self.cfg.machine;
        match f {
            AttachFail::Refused(r) if r == RELAY_ABSENT => (
                Problem::new(
                    copy_text("rsRemoteResident.relay.absent", &[("machine", m)]),
                    "",
                ),
                false,
            ),
            AttachFail::Refused(r) if r == "unreachable" => (
                Problem::new(
                    copy_text("rsRemoteResident.relay.unreachable", &[("machine", m)]),
                    "",
                ),
                false,
            ),
            AttachFail::Refused(r) => (
                Problem::new(
                    copy_text("rsRemoteResident.handshake.refused", &[("why", &r)]),
                    "",
                ),
                false,
            ),
            AttachFail::NotOurs(line) => (
                Problem::new(copy_text("rsRemoteResident.hello.notOurs", &[]), line),
                true,
            ),
            AttachFail::Cut { said, raw, .. } => (Problem::new(said, raw), false),
            AttachFail::Reply(said) => (Problem::new(said, ""), true),
        }
    }

    /// `--resident-ensure`（常驻不在就起）。
    async fn ensure(&self, t: &Arc<T>) -> Result<(), (Problem, bool)> {
        let exec = t
            .capture(backend_command(ENSURE))
            .await
            .map_err(|p| (p, false))?;
        read_one_shot(&exec).map(|_| ())
    }

    /// 读到头为止：应答交回请求方，别的帧原样交宿主。到头 · 读不动 · 被叫停（[`Feed::reconnect`]）⇒ 回。
    async fn pump(
        &self,
        client: &Arc<InboundClient>,
        mut reader: tokio::io::BufReader<tokio::io::ReadHalf<T::Stream>>,
        cut: &Notify,
    ) {
        let mut buf = Vec::new();
        loop {
            let got = tokio::select! {
                r = read_capped_line(&mut reader, &mut buf, BACKEND_FRAME_LINE_CAP) => r,
                () = cut.notified() => return,
            };
            match got {
                Ok(CappedLine::Line) => {
                    let line = String::from_utf8_lossy(&buf).into_owned();
                    route(client, &line, &self.events);
                }
                Ok(CappedLine::TooLong(n)) => {
                    tracing::warn!("常驻流上一行 {n} 字节，超过上限，丢了这一行");
                }
                Ok(CappedLine::Eof) | Err(_) => return,
            }
        }
    }

    /// 名单变了 · 换了一条新连接 ⇒ 在当前连接上整份重报；`from[]` 交宿主。报没成不重试：下一次接上照样整份重报。
    async fn report_watch(self: Arc<Self>) {
        let mut tick = self.watch_tick.subscribe();
        while tick.changed().await.is_ok() {
            let Some(client) = lock(&self.client).clone() else {
                continue;
            };
            let sids = lock(&self.watched).sids();
            match client
                .call(watch::WATCH_CMD, watch::args(&sids), watch::WATCH_BUDGET)
                .await
            {
                Ok(data) => {
                    let from = watch::from_of(data.as_ref());
                    if !from.is_empty() {
                        (self.events)(Event::WatchFrom(from));
                    }
                }
                Err(e) => tracing::warn!("stream_watch 报名单没报上（这条连接照旧全看）：{e:?}"),
            }
        }
    }
}

/// 一行帧：`reply` · `cancelled` 交回请求方；别的原样交宿主。不是 JSON 对象 ⇒ 说一句、不断流。
fn route(client: &InboundClient, line: &str, events: &Sink) {
    if line.trim().is_empty() {
        return;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        tracing::warn!(
            "常驻流上一行不是 JSON：{}",
            line.chars().take(200).collect::<String>()
        );
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
        _ => events(Event::Frame(line.to_string())),
    }
}

/// shell 找不到命令 / 不能执行。
const ABSENT_CODES: [u32; 2] = [126, 127];

/// 一次性 CLI 的回话（`--resident-ensure`）：退出 0 ＋ stdout 一行 JSON 对象 ⇒ 成；127 / 126 ⇒ 没装；
/// 非 0 ＋ stderr 一行 `{code, message, detail}` ⇒ 那一句；对不上协议 ⇒ 原样进详情。失败一律要人动手。
pub fn read_one_shot(exec: &Exec) -> Result<serde_json::Value, (Problem, bool)> {
    let out = exec.stdout.trim();
    let err = exec.stderr.trim();
    let raw = || format!("exit: {:?}\nstdout: {out}\nstderr: {err}", exec.exit_status);
    match exec.exit_status {
        Some(0) => serde_json::from_str::<serde_json::Value>(out)
            .ok()
            .filter(serde_json::Value::is_object)
            .ok_or_else(|| {
                (
                    Problem::new(
                        copy_text("rsRemoteResident.ensure.answerUnreadable", &[]),
                        raw(),
                    ),
                    true,
                )
            }),
        Some(c) if ABSENT_CODES.contains(&c) => Err((
            Problem::new(copy_text("remoteHealth.head.notDeployed", &[]), raw()),
            true,
        )),
        _ => {
            let env: serde_json::Value = serde_json::from_str(err).unwrap_or_default();
            match (env["code"].as_str(), env["message"].as_str()) {
                (Some(_), Some(m)) => Err((
                    Problem::new(m, env["detail"].as_str().unwrap_or_default()),
                    true,
                )),
                _ => Err((
                    Problem::new(copy_text("rsRemoteResident.ensure.noReason", &[]), raw()),
                    true,
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
