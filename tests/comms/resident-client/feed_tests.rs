//! 一台的常驻流（两端同一条循环）：直接接 · 不在就 ensure 再接 · 那台旧（回话说旧 · 认不得新一形 attach 行）：装不了的照接 / 断着并说要更新、装得了的换一次再接 ·
//! 那台新不点名「更新」· 开出链路到通着一个来回 ·
//! 发完 hello 就死退避照样涨 · 问答等它接上 · 新连接的 attach 行带在看名单（握手中改了才补报） · 没装：装不了的要人动手、装得了的自己再试 ·
//! 不是 Unix 两端都等人 · ［重新连接］在拨的时候按也算数、接上那一刻作废 · 断线重连之后数格。全部虚拟时钟。
use super::*;
use crate::backoff::RECONNECT_MIN;
use crate::client::CallError;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

/// 宿主那一侧的一问一答（手机核心照这样用）：等它接上，再在同一个期限里问。
async fn ask(
    feed: &Feed<T>,
    cmd: &str,
    within: Duration,
) -> Result<Option<serde_json::Value>, CallError> {
    let deadline = tokio::time::Instant::now() + within;
    match feed.client_by(deadline).await {
        Some(c) => c.call_until(cmd, serde_json::json!({}), deadline).await,
        None => Err(CallError::Disconnected),
    }
}

const MINE: &str = "p10g-mine";

/// 那台握手那一行：交它认的命令与那几条的形状指纹 ＋ 帧表（跟本侧编进来的同一份）。
/// [`ANCIENT`] 比 attach 行带 `mine` 那一形还早（见了就拒）；[`SKEWED`] 的 `session_status` 帧形状不同；[`HEAD_ONLY`] 的 hello 只解得出头。
fn hello(build: &str) -> String {
    if build == HEAD_ONLY {
        return format!(r#"{{"kind":"hello","build_id":"{build}"}}"#);
    }
    let cmds: &[&str] = &["ping", "cancel", "stream-watch"];
    let shapes: serde_json::Map<String, serde_json::Value> = cmds
        .iter()
        .filter_map(|c| {
            let fp = cmd_wire::shapes::shape_of(c)?;
            Some((c.to_string(), serde_json::Value::String(fp.into())))
        })
        .collect();
    let frames: serde_json::Map<String, serde_json::Value> = cmd_wire::frame_shapes::FRAMES
        .iter()
        .map(|(k, h)| {
            let h = if build == SKEWED && *k == "session_status" {
                "00000000"
            } else {
                h
            };
            (k.to_string(), serde_json::Value::String(h.into()))
        })
        .collect();
    format!(
        r#"{{"kind":"hello","build_id":"{build}","host_arch":"x86_64","commands":{cmds},"shapes":{shapes},"frames":{frames}}}"#,
        cmds = serde_json::to_string(cmds).unwrap(),
        shapes = serde_json::Value::Object(shapes),
        frames = serde_json::Value::Object(frames),
    )
}

const SKEWED: &str = "p10h-skewed";
const ANCIENT: &str = "p0a-ancient";
const HEAD_ONLY: &str = "p10h-head-only";

/// 对端那一台怎么回一条 `--resident-attach`。
#[derive(Clone)]
enum Peer {
    /// 小中继说没人在听。
    Absent,
    /// 握手成，之后照答；`die_now` ⇒ 回完 attach 就断。
    Up { build: String, die_now: bool },
    /// 拨出去没人回话（路断成黑洞：SYN 发出去石沉大海）：开链路那一步一直不回。
    Hang,
    /// 握手成、交完头一帧之后**路断成黑洞**：问什么都不回、也不断（没有 EOF）。
    Mute { build: String },
    /// 慢链路：开链路那一步隔 `gaps`（秒）报一步进展（拨号核心那几行 `progress`），报完照 `then` 回。
    Slow { gaps: Vec<u64>, then: Box<Peer> },
    /// 链路通了、每个方向的字节各晚 `lag` 到（握手成，之后照答）。
    Laggy { build: String, lag: Duration },
    /// 拨号核心说没拨成（那一句 ＋ 原因码 ＋ 那台出示的指纹）。
    Refused(Problem),
}

#[derive(Default)]
struct Fake {
    attaches: Mutex<VecDeque<Peer>>,
    ensures: Mutex<VecDeque<Exec>>,
    attach_at: Mutex<Vec<tokio::time::Instant>>,
    ensured: AtomicUsize,
    /// 那台收到的 attach 行（原样）。
    attach_lines: Mutex<Vec<String>>,
    watches: Mutex<Vec<serde_json::Value>>,
    /// 一次性 exec 的那几条命令（看换装那一条发没发）。
    captured: Mutex<Vec<String>>,
    /// 每一轮开拨之前那一步跑了几回（只有装得了后端的宿主替身 [`D`] 记）。
    rounds: AtomicUsize,
    /// 每一轮开拨之前那一步各等一个放行（有就等，没有就直接过）。
    holds: Mutex<VecDeque<Arc<tokio::sync::Notify>>>,
    /// 拨它 ⇒ 接着的那一条当场断（断网）。
    drop_link: tokio::sync::Notify,
    /// 那台收到的 `ping`（问活）条数。
    pings: AtomicUsize,
    /// 置位 ⇒ 应答缺 `ok` 那一格（两端契约对不上）。
    reply_without_ok: std::sync::atomic::AtomicBool,
    /// 有 ⇒ 下一条 attach 行收到之后、回话之前等它放行一次（握手中改名单）。
    reply_hold: Mutex<Option<Arc<tokio::sync::Notify>>>,
}

struct T(Arc<Fake>);

impl Transport for T {
    type Stream = tokio::io::DuplexStream;
    fn stream(
        &self,
        command: String,
        progress: Progress,
    ) -> Boxed<'_, Result<Self::Stream, Problem>> {
        let f = Arc::clone(&self.0);
        Box::pin(async move {
            assert!(command.ends_with("-- --resident-attach"), "{command}");
            f.attach_at
                .lock()
                .unwrap()
                .push(tokio::time::Instant::now());
            let mut peer = f
                .attaches
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Peer::Absent);
            if let Peer::Slow { gaps, then } = peer {
                for g in gaps {
                    tokio::time::sleep(Duration::from_secs(g)).await;
                    progress.step();
                }
                peer = *then;
            }
            if matches!(peer, Peer::Hang) {
                return std::future::pending().await;
            }
            if let Peer::Refused(p) = peer {
                return Err(p);
            }
            let (mine, theirs) = match &peer {
                Peer::Laggy { lag, .. } => laggy(*lag),
                _ => tokio::io::duplex(64 * 1024),
            };
            tokio::spawn(serve(peer, theirs, Arc::clone(&f)));
            Ok(mine)
        })
    }
    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        let f = Arc::clone(&self.0);
        Box::pin(async move {
            assert!(command.contains("-- --resident-ensure"), "{command}");
            f.ensured.fetch_add(1, Ordering::SeqCst);
            f.captured.lock().unwrap().push(command.clone());
            Ok(f.ensures.lock().unwrap().pop_front().unwrap_or(Exec {
                exit_status: Some(0),
                stdout: "{}".into(),
                stderr: String::new(),
            }))
        })
    }
}

/// 每一轮自己装后端的宿主替身（桌面那一形）：链路同 [`T`]；每一轮之前记一回（有放行就等它）。
struct D(Arc<Fake>);

impl Transport for D {
    type Stream = tokio::io::DuplexStream;
    fn stream(
        &self,
        command: String,
        progress: Progress,
    ) -> Boxed<'_, Result<Self::Stream, Problem>> {
        let t = T(Arc::clone(&self.0));
        Box::pin(async move { t.stream(command, progress).await })
    }
    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        let t = T(Arc::clone(&self.0));
        Box::pin(async move { t.capture(command).await })
    }
    fn before_round(&self) -> Boxed<'_, ()> {
        let f = Arc::clone(&self.0);
        Box::pin(async move {
            f.rounds.fetch_add(1, Ordering::SeqCst);
            let hold = f.holds.lock().unwrap().pop_front();
            if let Some(h) = hold {
                h.notified().await;
            }
        })
    }
    fn installs(&self) -> bool {
        true
    }
}

/// `--resident-ensure --replace` 的答：那台换没换（只升不降在那台判）。
fn swapped(replaced: bool) -> Exec {
    Exec {
        exit_status: Some(0),
        stdout: format!(r#"{{"pid":7,"replaced":{replaced}}}"#),
        stderr: String::new(),
    }
}

fn start_desk(f: &Arc<Fake>) -> (Arc<Feed<D>>, Seen) {
    let seen: Seen = Arc::default();
    let s = Arc::clone(&seen);
    let feed = Feed::start(
        cfg(),
        Some(Arc::new(D(Arc::clone(f)))),
        desk(),
        sink(move |e| s.lock().unwrap().push(e)),
    );
    (feed, seen)
}

async fn serve(peer: Peer, s: tokio::io::DuplexStream, f: Arc<Fake>) {
    let mut r = tokio::io::BufReader::new(s);
    let mute = matches!(peer, Peer::Mute { .. });
    let (build, die_now) = match peer {
        Peer::Absent => {
            let _ = r
                .get_mut()
                .write_all(b"{\"attach\":\"refused\",\"reason\":\"absent\"}\n")
                .await;
            return;
        }
        Peer::Up { build, die_now } => (build, die_now),
        Peer::Mute { build } | Peer::Laggy { build, .. } => (build, false),
        Peer::Hang | Peer::Slow { .. } | Peer::Refused(_) => {
            unreachable!("没人回话 / 还在拨 / 拨号拒了的那一拨开不出链路")
        }
    };
    let _ = r
        .get_mut()
        .write_all(format!("{}\n", hello(&build)).as_bytes())
        .await;
    let mut line = String::new();
    if r.read_line(&mut line).await.unwrap_or(0) == 0 {
        return;
    }
    f.attach_lines
        .lock()
        .unwrap()
        .push(line.trim_end().to_string());
    let hold = f.reply_hold.lock().unwrap().take();
    if let Some(h) = hold {
        h.notified().await;
    }
    // 新旧照后端同一个函数算；比带 `mine` 那一形还早的那台认不得这一格 ⇒ 拒（同它当年的 `attach_flags`）。
    let attach: serde_json::Value = serde_json::from_str(&line).unwrap();
    let reply = match attach["mine"].as_str() {
        Some(_) if build == ANCIENT => {
            let _ = r
                .get_mut()
                .write_all(b"{\"attach\":\"refused\",\"reason\":\"malformed-attach\"}\n")
                .await;
            return;
        }
        Some(mine) => serde_json::json!({
            "attach": "ok",
            "relation": deploy_contract::build_relation(&build, mine),
        }),
        None => serde_json::json!({"attach": "ok"}),
    };
    let _ = r.get_mut().write_all(format!("{reply}\n").as_bytes()).await;
    if die_now {
        return;
    }
    let _ = r
        .get_mut()
        .write_all(b"{\"kind\":\"sessions_replayed\"}\n")
        .await;
    // 帧门那两条：会话状态 ＋ 一行（别的替身只发上面那一帧）。
    let gated = build == SKEWED || build == HEAD_ONLY;
    let _ = r
        .get_mut()
        .write_all(if !gated { &b""[..] } else {
            concat!(
                r#"{"kind":"session_status","sid":"s","activity":"idle","said":"闲着","tone":"neutral","name":"n"}"#,
                "\n",
                r#"{"kind":"line","session_id":"s","path":"/p","seq":0,"byte_offset":3}"#,
                "\n"
            )
            .as_bytes()
        })
        .await;
    loop {
        line.clear();
        let got = tokio::select! {
            n = r.read_line(&mut line) => n.unwrap_or(0),
            () = f.drop_link.notified() => 0,
        };
        if got == 0 {
            return;
        }
        if mute {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        if v["cmd"] == "ping" {
            f.pings.fetch_add(1, Ordering::SeqCst);
        }
        if v["cmd"] == "stream-watch" {
            f.watches.lock().unwrap().push(v["args"].clone());
        }
        let data = serde_json::json!({"from":[]});
        let mut reply = serde_json::json!({"kind":"reply","id":v["id"],"ok":true,"data":data});
        if f.reply_without_ok.load(Ordering::SeqCst) {
            reply.as_object_mut().unwrap().remove("ok");
        }
        let _ = r.get_mut().write_all(format!("{reply}\n").as_bytes()).await;
    }
}

fn cfg() -> Config {
    Config {
        machine: "box".into(),
        mine: Some(MINE.into()),
        tz: Some("Asia/Shanghai".into()),
        handshake: Duration::from_secs(20),
    }
}

type Seen = Arc<Mutex<Vec<Event>>>;

fn start(f: &Arc<Fake>) -> (Arc<Feed<T>>, Seen) {
    start_with(f, desk())
}

/// 同 [`start`]，名单台由判据给（看 attach 行带什么 · 握手中改名单）。
fn start_with(f: &Arc<Fake>, desk: Arc<Desk>) -> (Arc<Feed<T>>, Seen) {
    let seen: Seen = Arc::default();
    let s = Arc::clone(&seen);
    let feed = Feed::start(cfg(), None, desk, sink(move |e| s.lock().unwrap().push(e)));
    feed.set_transport(Some(Arc::new(T(Arc::clone(f)))));
    (feed, seen)
}

/// 不读清单、不收 `from[]` 的名单台（手机那一形）；名单从空开始。
fn desk() -> Arc<Desk> {
    Desk::new(|_| Vec::new(), |_| {})
}

/// 那台收到的第 `i` 条 attach 行里的名单。
fn carried(f: &Fake, i: usize) -> serde_json::Value {
    let l = f.attach_lines.lock().unwrap()[i].clone();
    serde_json::from_str::<serde_json::Value>(&l).unwrap()["watch"]["sids"].clone()
}

/// 虚拟时钟下等一个条件（封顶那个兜底的数：本组全在 `start_paused` 下跑，墙钟上不费时）。
async fn until(what: &str, mut f: impl FnMut() -> bool) {
    let waited = tokio::time::timeout(guard_core::wall::CEILING, async {
        while !f() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "等不到：{what}");
}

fn up<X: Transport>(f: &Feed<X>) -> bool {
    matches!(f.state(), State::Up { .. })
}

fn upp(build: &str) -> Peer {
    Peer::Up {
        build: build.into(),
        die_now: false,
    }
}

#[tokio::test(start_paused = true)]
async fn a_running_resident_is_attached_with_one_exec() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp(MINE));
    let (feed, seen) = start(&f);
    until("流接上了", || up(&feed)).await;
    assert_eq!(
        f.ensured.load(Ordering::SeqCst),
        0,
        "常驻在跑 ⇒ 不该 ensure"
    );
    assert_eq!(
        feed.state(),
        State::Up {
            epoch: 1,
            relation: Relation::Same,
            differs: vec![],
        }
    );
    assert_eq!(
        ask(&feed, "ping", Duration::from_secs(5)).await.unwrap(),
        Some(serde_json::json!({"from":[]}))
    );
    until("会话帧原样交出去（hello 不交）", || {
        seen.lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::Frame(l) if l.contains("sessions_replayed")))
    })
    .await;
    assert!(
        !seen
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::Frame(l) if l.contains("\"hello\""))),
        "hello 是握手那一行，不当帧交"
    );
}

#[tokio::test(start_paused = true)]
async fn a_resident_that_is_not_running_is_ensured_then_attached() {
    let f = Arc::new(Fake::default());
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(Peer::Absent);
        a.push_back(Peer::Absent); // ensure 之后头一回还没绑上口
        a.push_back(upp(MINE));
    }
    let (feed, _) = start(&f);
    until("流接上了", || up(&feed)).await;
    assert_eq!(f.ensured.load(Ordering::SeqCst), 1);
}

/// 新旧是那台在 attach 回话里给的（attach 行带手上这一版）：装不了后端的（手机）旧的 · 新的都照接，新旧原样记在 `Up` 里
/// （说什么、给不给［更新］由后端照它定）。不另问一趟。
#[tokio::test(start_paused = true)]
async fn the_relation_comes_back_in_the_attach_reply_and_the_phone_attaches_either_way() {
    for (theirs, relation) in [
        ("p9a-old", Relation::Older),
        ("p11a-next", Relation::Newer),
        (MINE, Relation::Same),
    ] {
        let f = Arc::new(Fake::default());
        f.attaches.lock().unwrap().push_back(upp(theirs));
        let (feed, _) = start(&f);
        until("流接上了", || up(&feed)).await;
        let State::Up { relation: r, .. } = feed.state() else {
            unreachable!()
        };
        assert_eq!(r, relation, "{theirs}");
        assert_eq!(
            *f.attach_lines.lock().unwrap(),
            [format!(
                r#"{{"attach":true,"tz":"Asia/Shanghai","mine":"{MINE}","watch":{{"sids":[],"branch":[]}}}}"#
            )],
            "attach 行带手上这一版（名单从空开始）"
        );
        assert!(
            ask(&feed, "ping", Duration::from_secs(5)).await.is_ok(),
            "{theirs}：照用对得上的那几条"
        );
    }
}

/// 手上没带这一版的身份 ⇒ attach 行不带 `mine`、新旧记不可比（不看回话）。
#[tokio::test(start_paused = true)]
async fn without_its_own_build_the_relation_is_incomparable() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp(MINE));
    let seen: Seen = Arc::default();
    let s = Arc::clone(&seen);
    let feed = Feed::start(
        Config {
            mine: None,
            ..cfg()
        },
        None,
        desk(),
        sink(move |e| s.lock().unwrap().push(e)),
    );
    feed.set_transport(Some(Arc::new(T(Arc::clone(&f)))));
    until("流接上了", || up(&feed)).await;
    assert!(matches!(
        feed.state(),
        State::Up {
            relation: Relation::Incomparable,
            ..
        }
    ));
    assert_eq!(
        *f.attach_lines.lock().unwrap(),
        [r#"{"attach":true,"tz":"Asia/Shanghai","watch":{"sids":[],"branch":[]}}"#]
    );
}

/// 那台认不得带 `mine` 的 attach 行（比这一形早）⇒ 当旧：手机接不上、断着、记「后端旧了」，不自己反复试。
#[tokio::test(start_paused = true)]
async fn a_backend_that_refuses_the_new_attach_line_is_old() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp(ANCIENT));
    let (feed, _) = start(&f);
    until("断在要人动手的那一步", || {
        matches!(
            feed.state(),
            State::Down {
                needs_hand: true,
                ..
            }
        )
    })
    .await;
    match feed.state() {
        State::Down {
            problem, blocker, ..
        } => {
            assert_eq!(blocker, Some(Blocker::OldBackend));
            assert_eq!(problem.text, copy_core::backend_old("box"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        f.ensured.load(Ordering::SeqCst),
        0,
        "旧的不是没起来：不 ensure"
    );
}

/// 往返数：每个方向的字节各晚 1 s 到（暂停时钟），从开出链路到 `Up` 恰一个来回 —— attach 行不等 hello 就发、带着名单；
/// 桌面也一样（不另问本机后端）。接上之后不再为名单多一个来回（那台从第一帧起照名单发）。
#[tokio::test(start_paused = true)]
async fn from_link_open_to_up_takes_one_round_trip() {
    for desk in [false, true] {
        let lag = Duration::from_secs(1);
        let f = Arc::new(Fake::default());
        f.attaches.lock().unwrap().push_back(Peer::Laggy {
            build: MINE.into(),
            lag,
        });
        let up_at: Arc<Mutex<Option<tokio::time::Instant>>> = Arc::default();
        let u = Arc::clone(&up_at);
        let events = sink(move |e| {
            if let Event::State(State::Up { .. }) = e {
                u.lock().unwrap().get_or_insert(tokio::time::Instant::now());
            }
        });
        let d = self::desk();
        d.set(["s1".to_string()].into(), Default::default());
        if desk {
            Feed::start(cfg(), Some(Arc::new(D(Arc::clone(&f)))), d, events);
        } else {
            Feed::start(cfg(), Some(Arc::new(T(Arc::clone(&f)))), d, events);
        }
        until("流接上了", || up_at.lock().unwrap().is_some()).await;
        let opened = f.attach_at.lock().unwrap()[0];
        assert_eq!(
            up_at.lock().unwrap().unwrap() - opened,
            2 * lag,
            "桌面 {desk}：开出链路到通着该是一个来回"
        );
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(carried(&f, 0), serde_json::json!(["s1"]));
        assert!(
            f.watches.lock().unwrap().is_empty(),
            "桌面 {desk}：接上之后又为名单问了一次"
        );
    }
}

/// 每个方向各晚 `lag` 的一对双工流（客户端那头 · 替身后端那头）：一边读进来的每一块，`lag` 之后才在另一边读得到，先后不变；一边关了另一边读到头。
fn laggy(lag: Duration) -> (tokio::io::DuplexStream, tokio::io::DuplexStream) {
    let (client, a) = tokio::io::duplex(64 * 1024);
    let (b, server) = tokio::io::duplex(64 * 1024);
    let (ar, aw) = tokio::io::split(a);
    let (br, bw) = tokio::io::split(b);
    tokio::spawn(lag_pipe(ar, bw, lag));
    tokio::spawn(lag_pipe(br, aw, lag));
    (client, server)
}

async fn lag_pipe<R, W>(mut r: R, mut w: W, lag: Duration)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use tokio::io::AsyncReadExt;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(tokio::time::Instant, Vec<u8>)>();
    tokio::spawn(async move {
        while let Some((at, bytes)) = rx.recv().await {
            tokio::time::sleep_until(at).await;
            if w.write_all(&bytes).await.is_err() {
                return;
            }
        }
    });
    let mut buf = [0u8; 4096];
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                let _ = tx.send((tokio::time::Instant::now() + lag, buf[..n].to_vec()));
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_backend_that_dies_right_after_hello_backs_off_and_does_not_hammer() {
    let f = Arc::new(Fake::default());
    for _ in 0..6 {
        f.attaches.lock().unwrap().push_back(Peer::Up {
            build: MINE.into(),
            die_now: true,
        });
    }
    let (_feed, _seen) = start(&f);
    until("接了 6 回", || f.attach_at.lock().unwrap().len() >= 6).await;
    let at = f.attach_at.lock().unwrap().clone();
    let gaps: Vec<u64> = at.windows(2).map(|w| (w[1] - w[0]).as_secs()).collect();
    assert_eq!(
        gaps,
        vec![2, 4, 8, 16, 30],
        "发完 hello 就死：退避照样涨（每轮都握上手也不清零）"
    );
}

#[tokio::test(start_paused = true)]
async fn a_missing_backend_needs_a_hand_and_does_not_retry_by_itself() {
    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(Exec {
        exit_status: Some(127),
        stdout: String::new(),
        stderr: "not found".into(),
    });
    let (feed, seen) = start(&f);
    until("断在要人动手的那一步", || {
        matches!(
            feed.state(),
            State::Down {
                needs_hand: true,
                ..
            }
        )
    })
    .await;
    match feed.state() {
        State::Down { problem, .. } => assert_eq!(
            problem.text,
            copy_core::copy_text("remoteHealth.head.notDeployed", &[])
        ),
        _ => unreachable!(),
    }
    // 后端起不来，SSH 本身通着：这一条循环照样报「SSH 够着那一台」（终端 · 文件跟它走，不跟着后端卡住）。
    assert!(feed.ssh_reached(), "常驻后端起不来、SSH 通 ⇒ SSH 够着");
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::Ssh(true))),
        "SSH 够着那一刻要往外报"
    );
    let tries = f.attach_at.lock().unwrap().len();
    tokio::time::sleep(Duration::from_secs(600)).await;
    assert_eq!(
        f.attach_at.lock().unwrap().len(),
        tries,
        "要人动手 ⇒ 不自己反复试"
    );
    assert_eq!(
        ask(&feed, "ping", Duration::from_secs(5)).await.err(),
        Some(CallError::Disconnected),
        "问的人不用等"
    );
    f.attaches.lock().unwrap().push_back(upp(MINE));
    feed.reconnect();
    until("［重试］之后接上", || up(&feed)).await;
}

#[tokio::test(start_paused = true)]
async fn a_call_made_before_the_link_is_up_waits_for_it() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp(MINE));
    let feed: Arc<Feed<T>> = Feed::start(cfg(), None, desk(), sink(|_| {}));
    let fe = Arc::clone(&feed);
    let call = tokio::spawn(async move { ask(&fe, "ping", Duration::from_secs(30)).await });
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        matches!(
            feed.state(),
            State::Down {
                needs_hand: false,
                ..
            }
        ),
        "SSH 没通 ⇒ 断着、不是要人动手"
    );
    feed.set_transport(Some(Arc::new(T(Arc::clone(&f)))));
    assert!(call.await.unwrap().is_ok(), "等它接上再问");
}

/// 名单并进 attach 行：每条新连接的 attach 行带此刻那份，那台从第一帧起照它发 ⇒ 接上之后不再问 `stream-watch`；
/// 握手中改了（attach 行写出之后、接上之前）⇒ 接上后补报一次、是最新那份。
#[tokio::test(start_paused = true)]
async fn every_new_link_carries_the_watch_list_on_its_attach_line() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp(MINE));
    let d = desk();
    d.set(["s1".to_string()].into(), Default::default());
    let (feed, _) = start_with(&f, Arc::clone(&d));
    until("流接上了", || up(&feed)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(carried(&f, 0), serde_json::json!(["s1"]));
    assert!(f.watches.lock().unwrap().is_empty(), "attach 行带过了还问");
    // 换一条 SSH（网络换了）⇒ 新连接的 attach 行照当前名单整份带上。
    f.attaches.lock().unwrap().push_back(upp(MINE));
    feed.set_transport(Some(Arc::new(T(Arc::clone(&f)))));
    until("第二条连接接上", || {
        matches!(feed.state(), State::Up { epoch: 2, .. })
    })
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(carried(&f, 1), serde_json::json!(["s1"]));
    assert!(f.watches.lock().unwrap().is_empty());
    // 握手中名单变了：第三条连接的 attach 行写出之后、回话之前改 ⇒ 接上后补报一次最新那份。
    let gate = Arc::new(tokio::sync::Notify::new());
    *f.reply_hold.lock().unwrap() = Some(Arc::clone(&gate));
    f.attaches.lock().unwrap().push_back(upp(MINE));
    feed.set_transport(Some(Arc::new(T(Arc::clone(&f)))));
    until("第三条 attach 行到了", || {
        f.attach_lines.lock().unwrap().len() == 3
    })
    .await;
    d.set(
        ["s1".to_string(), "s2".to_string()].into(),
        Default::default(),
    );
    gate.notify_one();
    until("补报了最新那份", || {
        f.watches
            .lock()
            .unwrap()
            .iter()
            .any(|a| a["sids"] == serde_json::json!(["s1", "s2"]))
    })
    .await;
    assert_eq!(carried(&f, 2), serde_json::json!(["s1"]));
    assert_eq!(f.watches.lock().unwrap().len(), 1, "只补那一次");
}

/// 每一轮自己装后端的宿主（桌面那一形）：接上的不是这一版（回话说旧 · 认不得新一形 attach 行）⇒ `--resident-ensure --replace` 一次；
/// 那台换了 ⇒ 再接；换上的是这一版 ⇒ 不说。每一轮开拨之前那一步先跑；接上先交 hello 与客户端，再交 `Up`；桌面从不点名「更新」（自己装）。
#[tokio::test(start_paused = true)]
async fn a_host_that_installs_replaces_an_older_backend_once_then_attaches() {
    for old in ["p9a-old", ANCIENT] {
        let f = Arc::new(Fake::default());
        f.ensures.lock().unwrap().push_back(swapped(true));
        {
            let mut a = f.attaches.lock().unwrap();
            a.push_back(upp(old));
            a.push_back(upp(MINE)); // 换上的这一版
        }
        let (feed, seen) = start_desk(&f);
        until("换过之后接上", || up(&feed)).await;
        assert_eq!(
            *f.captured.lock().unwrap(),
            vec![backend_command(ENSURE_REPLACE)],
            "{old}：旧了 ⇒ 恰好换一次（常驻在跑，不必先 ensure）"
        );
        assert_eq!(
            feed.state(),
            State::Up {
                epoch: 1,
                relation: Relation::Same,
                differs: vec![],
            }
        );
        assert_eq!(f.rounds.load(Ordering::SeqCst), 1, "开拨之前那一步跑一回");
        let seen = seen.lock().unwrap();
        let at = seen
            .iter()
            .position(|e| matches!(e, Event::Attached { hello, .. } if hello.contains(MINE)))
            .expect("接上要交 hello 与客户端");
        let up_at = seen
            .iter()
            .position(|e| matches!(e, Event::State(State::Up { .. })))
            .unwrap();
        assert!(at < up_at, "hello 先于 Up：{seen:?}");
    }

    // 换过还旧（落点上那份也旧）⇒ 照接、记那台旧，不再换第二次。
    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(swapped(true));
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(upp("p9a-old"));
        a.push_back(upp("p9b-still-old"));
    }
    let (feed, _) = start_desk(&f);
    until("换过一次之后照接", || up(&feed)).await;
    assert_eq!(f.captured.lock().unwrap().len(), 1, "只换一次");
    assert!(
        matches!(
            feed.state(),
            State::Up {
                relation: Relation::Older,
                ..
            }
        ),
        "换过还旧 ⇒ 接上、记那台旧：{:?}",
        feed.state()
    );
    assert_eq!(feed.theirs().as_deref(), Some("p9b-still-old"));

    // 那台说没换（在听的不比落点旧）⇒ 不重接，接着的就是第一条。
    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(swapped(false));
    f.attaches.lock().unwrap().push_back(upp("p9q-newer"));
    let (feed, _) = start_desk(&f);
    until("没换照接", || up(&feed)).await;
    assert_eq!(f.captured.lock().unwrap().len(), 1);
    assert_eq!(feed.theirs().as_deref(), Some("p9q-newer"));
    assert!(f.attaches.lock().unwrap().is_empty());
}

/// 没装 · 起不了常驻：装不了后端的要人动手（上面那条）；装得了的不等人、下一轮（先部署）再试。
#[tokio::test(start_paused = true)]
async fn a_host_that_installs_retries_a_missing_backend_by_itself() {
    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(Exec {
        exit_status: Some(127),
        stdout: String::new(),
        stderr: "not found".into(),
    });
    f.attaches.lock().unwrap().push_back(Peer::Absent);
    f.attaches.lock().unwrap().push_back(upp(MINE));
    let (feed, seen) = start_desk(&f);
    until("下一轮自己接上", || up(&feed)).await;
    assert_eq!(f.rounds.load(Ordering::SeqCst), 2, "每一轮先部署一回");
    assert!(
        seen.lock().unwrap().iter().any(|e| matches!(
            e,
            Event::State(State::Down {
                needs_hand: false,
                blocker: Some(Blocker::NoBackend),
                ..
            })
        )),
        "没装那一轮 ⇒ 断着、记没有后端、不要人动手"
    );
}

/// 那台不是 Unix（常驻脱离不了）：两端都停在要人动手、不按退避空转，原话进详情；叫醒（机器页「起」· ［重试］）才再试一次。
#[tokio::test(start_paused = true)]
async fn a_remote_that_is_not_unix_waits_for_a_hand() {
    let unsupported = || Exec {
        exit_status: Some(2),
        stdout: String::new(),
        stderr: r#"{"code":"unsupported","message":"setsid: not supported","detail":""}"#.into(),
    };
    async fn waits<X: Transport>(feed: &Feed<X>, f: &Fake) {
        until("停在要人动手", || {
            matches!(
                feed.state(),
                State::Down {
                    needs_hand: true,
                    blocker: Some(Blocker::NotUnix),
                    ..
                }
            )
        })
        .await;
        match feed.state() {
            State::Down { problem, .. } => {
                assert_eq!(
                    problem.text,
                    copy_core::copy_text("rsRemoteResident.ensure.unsupported", &[])
                );
                assert!(problem.detail.contains("setsid"), "{}", problem.detail);
            }
            _ => unreachable!(),
        }
        let tries = f.attach_at.lock().unwrap().len();
        tokio::time::sleep(Duration::from_secs(600)).await;
        assert_eq!(f.attach_at.lock().unwrap().len(), tries, "不按退避空转");
        f.attaches.lock().unwrap().push_back(upp(MINE));
        feed.wake();
        until("叫醒之后再试、接上", || up(feed)).await;
    }
    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(unsupported());
    let (feed, _) = start(&f);
    waits(&feed, &f).await;
    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(unsupported());
    let (feed, _) = start_desk(&f);
    waits(&feed, &f).await;
}

/// 拨号拒了：原因码与那台出示的指纹随断开那一句走到 `Down`（后端按它定那一态与修法）。
#[tokio::test(start_paused = true)]
async fn a_refused_dial_carries_its_reason_and_the_seen_key_into_down() {
    let f = Arc::new(Fake::default());
    let refused = Problem::new("密钥被拒", "命令：dial").because(
        Some(cmd_wire::machine::Why::Auth),
        Some("SHA256:seen".into()),
    );
    f.attaches.lock().unwrap().push_back(Peer::Refused(refused));
    let (feed, _) = start(&f);
    until("断开", || matches!(feed.state(), State::Down { .. })).await;
    let state = feed.state();
    let State::Down { problem, .. } = &state else {
        unreachable!()
    };
    assert_eq!(
        (problem.why, problem.seen_key.as_deref()),
        (Some(cmd_wire::machine::Why::Auth), Some("SHA256:seen"))
    );
}

/// ［重新连接］在拨的时候按也算数：这一轮没成就不睡；接上那一刻之前按的作废（下一次断线照样按退避等）。
#[tokio::test(start_paused = true)]
async fn a_wake_while_dialing_skips_one_wait_and_is_void_once_up() {
    let f = Arc::new(Fake::default());
    let (h1, h2) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(tokio::sync::Notify::new()),
    );
    f.holds
        .lock()
        .unwrap()
        .extend([Arc::clone(&h1), Arc::clone(&h2)]);
    // 第一轮：没在跑、ensure 也没起来 ⇒ 没成；第二轮：接上、发完 hello 就断；第三轮：接上。
    f.ensures.lock().unwrap().push_back(Exec {
        exit_status: Some(1),
        stdout: String::new(),
        stderr: r#"{"code":"spawn_failed","message":"起不来","detail":""}"#.into(),
    });
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(Peer::Absent);
        a.push_back(Peer::Up {
            build: MINE.into(),
            die_now: true,
        });
        a.push_back(upp(MINE));
    }
    let (feed, _) = start_desk(&f);
    until("第一轮在拨", || f.rounds.load(Ordering::SeqCst) == 1).await;
    feed.wake();
    h1.notify_one();
    until("第二轮在拨", || f.rounds.load(Ordering::SeqCst) == 2).await;
    feed.wake();
    h2.notify_one();
    until("第三轮接上", || {
        f.attach_at.lock().unwrap().len() >= 3 && up(&feed)
    })
    .await;
    let at = f.attach_at.lock().unwrap().clone();
    let gaps: Vec<u64> = at.windows(2).map(|w| (w[1] - w[0]).as_secs()).collect();
    assert_eq!(
        gaps,
        vec![0, 2],
        "在拨时按的那一下：第一轮没成不睡（0）；第二轮接上了 ⇒ 那一下作废，断了照退避等（2）"
    );
}

/// **断线 → 重连，数格**（手机那一侧交给 Kotlin 的就是这几件）：在看 s1；断网；重连。每条连接的 attach 行都带着 s1、
/// 接上之后不再问名单（续点由那台的宣告给，`from[]` 只答连接中途刚进名单的）。每件事恰好几格、按序，不看墙钟。
#[tokio::test(start_paused = true)]
async fn a_drop_then_reconnect_hands_over_exactly_these_cells() {
    let f = Arc::new(Fake::default());
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(upp(MINE));
        a.push_back(upp(MINE));
    }
    let d = desk();
    d.set(["s1".to_string()].into(), Default::default());
    let (feed, seen) = start_with(&f, d);
    let ups = |seen: &Seen| {
        seen.lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, Event::State(State::Up { .. })))
            .count()
    };
    until("第一条连接接上", || ups(&seen) == 1).await;
    f.drop_link.notify_waiters();
    until("重连接上", || ups(&seen) == 2).await;
    tokio::time::sleep(Duration::from_secs(60)).await;
    let cells: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|e| match e {
            Event::State(State::Connecting) => "connecting".to_string(),
            Event::State(State::Up { epoch, .. }) => format!("up {epoch}"),
            Event::State(State::Down { needs_hand, .. }) => format!("down hand={needs_hand}"),
            Event::Attached { .. } => "attached".to_string(),
            Event::Frame(l) => format!(
                "frame {}",
                serde_json::from_str::<serde_json::Value>(l).unwrap()["kind"]
                    .as_str()
                    .unwrap()
            ),
            Event::Gap => "gap".to_string(),
            Event::Ssh(b) => format!("ssh {b}"),
        })
        .collect();
    assert_eq!(
        cells,
        [
            "connecting",
            "ssh true",
            "attached",
            "up 1",
            "frame sessions_replayed",
            "ssh false",
            "down hand=false",
            "connecting",
            "ssh true",
            "attached",
            "up 2",
            "frame sessions_replayed",
        ]
        .map(String::from),
        "断了 → 重连：{cells:?}"
    );
    assert_eq!(carried(&f, 0), serde_json::json!(["s1"]));
    assert_eq!(carried(&f, 1), serde_json::json!(["s1"]));
    assert!(
        f.watches.lock().unwrap().is_empty(),
        "attach 行带过的不再问"
    );
    feed.close();
}

/// `--resident-ensure [--replace]` 的答（两端同一个读法）：成了 ⇒ 那个对象（pid 可能是 null）；不是对象 ⇒ 认不得；
/// 老后端掉进流模式发 hello ⇒「要更新」；127 ⇒ 没装；那台脱离不了（非 unix）⇒ 句子由这边说「远端只支持 Unix」、那台原话 ＋ 详情进复制详情、
/// 归成「不是 Unix」；别的失败 ⇒ 那台那一句、它写的详情原样。除了「成了」都是失败，没有回落。
#[test]
fn the_ensure_answer_is_read_the_same_on_both_ends() {
    let exec = |stdout: &str, stderr: &str, code: Option<u32>| Exec {
        exit_status: code,
        stdout: stdout.into(),
        stderr: stderr.into(),
    };
    let fail = |e: Exec| match read_one_shot(&e, "devbox") {
        Ok(v) => panic!("该是失败：{v}"),
        Err(Fail { problem, kind }) => (problem, kind),
    };
    assert_eq!(
        read_one_shot(&exec(r#"{"pid":null}"#, "", Some(0)), "devbox").ok(),
        Some(serde_json::json!({"pid": null}))
    );
    let (p, k) = fail(exec("7", "", Some(0)));
    assert_eq!(
        (p.text, k),
        (
            copy_text("rsRemoteResident.ensure.answerUnreadable", &[]),
            FailKind::Blocked(Blocker::NoBackend)
        )
    );
    let (p, k) = fail(exec(&hello("p1a-old"), "", None));
    assert_eq!(
        (p.text, k),
        (
            copy_core::backend_old("devbox"),
            FailKind::Blocked(Blocker::OldBackend)
        )
    );
    let (p, k) = fail(exec("", "sh: ccm: not found", Some(127)));
    assert_eq!(
        (p.text, k),
        (
            copy_text("remoteHealth.head.notDeployed", &[]),
            FailKind::Blocked(Blocker::NoBackend)
        )
    );
    let (p, k) = fail(exec(
        "",
        r#"{"code":"unsupported","message":"不是 unix","detail":"命令: resident-ensure"}"#,
        Some(2),
    ));
    assert_eq!(k, FailKind::Blocked(Blocker::NotUnix));
    assert_eq!(
        p.text,
        copy_text("rsRemoteResident.ensure.unsupported", &[])
    );
    assert!(
        p.detail.contains("不是 unix") && p.detail.contains("resident-ensure"),
        "{}",
        p.detail
    );
    let (p, k) = fail(exec(
        "",
        r#"{"code":"spawn_failed","message":"x","detail":"Permission denied (os error 13)"}"#,
        Some(2),
    ));
    assert_eq!(
        (p.text.as_str(), p.detail.as_str(), k),
        (
            "x",
            "Permission denied (os error 13)",
            FailKind::Blocked(Blocker::NoBackend)
        )
    );
    let (p, _) = fail(exec("", "garbage", Some(1)));
    assert_eq!(p.text, copy_text("rsRemoteResident.ensure.noReason", &[]));
}

/// 中继一直说「没人在听」（ensure 说起了、却一直没绑上口）：至多试 [`RELAY_TRIES`] 回（隔 [`RELAY_WAIT`]），然后说「后端没在运行」、照退避再来；
/// 中继说别的理由 ⇒ 不再等，当场说那一句。
#[tokio::test(start_paused = true)]
async fn a_relay_that_stays_absent_is_tried_a_capped_number_of_times() {
    let f = Arc::new(Fake::default());
    let (feed, _) = start(&f);
    until("断在「后端没在运行」", || {
        matches!(feed.state(), State::Down { ref problem, needs_hand: false, .. }
            if problem.text == copy_text("rsRemoteResident.relay.absent", &[("machine", "box")]))
    })
    .await;
    assert_eq!(
        f.attach_at.lock().unwrap().len(),
        1 + RELAY_TRIES as usize,
        "先直接接一回，ensure 之后至多再接 RELAY_TRIES 回"
    );
    assert_eq!(f.ensured.load(Ordering::SeqCst), 1);
}

/// 隔一段一段的时刻（秒，虚拟时钟）。
fn gaps_of(f: &Fake) -> Vec<u64> {
    let at = f.attach_at.lock().unwrap().clone();
    at.windows(2).map(|w| (w[1] - w[0]).as_secs()).collect()
}

/// **拨出去没人回话**（静默断网：网卡还在、包全丢，SYN 石沉大海）：这一拨到 [`DIAL_WITHIN`] 就放弃、隔下界再拨，
/// 一直没人回话也不往上涨（够不着那台，没有一次登录落到它身上，涨退避护不了谁）——网回来之后至多
/// 「拨号期限 ＋ 下界」就有一拨赶上。拨不通 ⇒ 不去 ensure（ensure 走的是同一条拨不通的路）。
#[tokio::test(start_paused = true)]
async fn a_dial_nobody_answers_is_dropped_in_time_and_retried_without_climbing() {
    let f = Arc::new(Fake::default());
    {
        let mut a = f.attaches.lock().unwrap();
        for _ in 0..8 {
            a.push_back(Peer::Hang);
        }
        a.push_back(upp(MINE));
    }
    let (feed, seen) = start(&f);
    tokio::time::sleep(DIAL_WITHIN * 3).await;
    assert!(!feed.ssh_reached(), "一拨都没拨通 ⇒ SSH 没够着");
    assert!(
        !seen
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::Ssh(true))),
        "没拨通不许报够着"
    );
    until("网回来那一拨接上了", || up(&feed)).await;
    assert!(feed.ssh_reached());
    let every = (DIAL_WITHIN + RECONNECT_MIN).as_secs();
    assert_eq!(
        gaps_of(&f),
        vec![every; 8],
        "没人回话的每一拨：到期限放弃、隔下界再拨，不涨"
    );
    assert_eq!(f.ensured.load(Ordering::SeqCst), 0, "拨不通 ⇒ 不 ensure");
    // 回过话的失败照旧涨：发完 hello 就死的那一形见 `a_backend_that_dies_right_after_hello_backs_off_and_does_not_hammer`。
}

/// **慢链路**（往返一秒以上、再隔跳板）：登录 ＋ 开 exec 合计远超 [`DIAL_WITHIN`]，但拨号核心一步一步在报进展 ——
/// 每一步都重新计时，这一拨不丢、一次接上。
#[tokio::test(start_paused = true)]
async fn a_slow_dial_that_keeps_moving_is_not_dropped() {
    let f = Arc::new(Fake::default());
    let step = DIAL_WITHIN.as_secs() - 2;
    f.attaches.lock().unwrap().push_back(Peer::Slow {
        gaps: vec![step; 5],
        then: Box::new(upp(MINE)),
    });
    let t0 = tokio::time::Instant::now();
    let (feed, _) = start(&f);
    until("慢链路接上了", || up(&feed)).await;
    assert_eq!(
        f.attach_at.lock().unwrap().len(),
        1,
        "一步一步在动的那一拨不该被丢下重拨"
    );
    assert!(t0.elapsed() >= Duration::from_secs(5 * step));
}

/// 动过几步、然后**一步都不动了**（拨到一半网断了）：从最后一步起 [`DIAL_WITHIN`] 丢下、隔下界再拨，同样不涨退避。
#[tokio::test(start_paused = true)]
async fn a_dial_that_stops_moving_is_dropped_its_window_after_the_last_step() {
    let f = Arc::new(Fake::default());
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(Peer::Slow {
            gaps: vec![6, 6],
            then: Box::new(Peer::Hang),
        });
        a.push_back(upp(MINE));
    }
    let (feed, _) = start(&f);
    until("再拨接上了", || up(&feed)).await;
    assert_eq!(
        gaps_of(&f),
        vec![12 + (DIAL_WITHIN + RECONNECT_MIN).as_secs()],
        "最后一步之后 {DIAL_WITHIN:?} 才丢、隔下界再拨"
    );
}

/// **宿主报「网回来了 / 网变了 / 回前台」**（`wake`）· 换了一条 SSH（`set_transport`）· ［重试］（`reconnect`）：
/// 正卡在一拨没人回话的拨号里 ⇒ 当场丢下它、马上再拨，不等拨号期限，也不等退避。
#[tokio::test(start_paused = true)]
async fn a_host_event_drops_a_hanging_dial_and_dials_again_at_once() {
    for how in ["wake", "set_transport", "reconnect"] {
        let f = Arc::new(Fake::default());
        {
            let mut a = f.attaches.lock().unwrap();
            a.push_back(Peer::Hang);
            a.push_back(upp(MINE));
        }
        let (feed, _) = start(&f);
        until("第一拨在拨", || f.attach_at.lock().unwrap().len() == 1).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        match how {
            "wake" => feed.wake(),
            "set_transport" => feed.set_transport(Some(Arc::new(T(Arc::clone(&f))))),
            _ => feed.reconnect(),
        }
        until("再拨接上了", || up(&feed)).await;
        assert_eq!(
            gaps_of(&f),
            vec![1],
            "{how}：卡着的那一拨当场丢下、马上再拨"
        );
    }
}

/// **连接自己的心跳**：流上 [`HEARTBEAT_IDLE`] 没来一个字节 ⇒ 问一句活（`ping`）；问出去 [`PING_WITHIN`] 还是一个字节都没来 ⇒
/// 判这条死了、收掉重接（不等 SSH 保活：它要最后收包之后 20 秒才判死，判死之前网回来了，补发的包还要等 TCP 的退避）。
/// 健康的空闲连接照答、不断；来着字节的连接（哪怕一行很长、还没读完）不问。
#[tokio::test(start_paused = true)]
async fn a_link_gone_silent_is_judged_dead_by_its_own_heartbeat() {
    let f = Arc::new(Fake::default());
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(Peer::Mute { build: MINE.into() });
        a.push_back(upp(MINE));
    }
    let (feed, _seen) = start(&f);
    until("流接上了", || up(&feed)).await;
    until("判死、重接上了", || {
        f.attach_at.lock().unwrap().len() == 2 && up(&feed)
    })
    .await;
    let gap = gaps_of(&f)[0];
    assert_eq!(
        gap,
        (HEARTBEAT_IDLE + PING_WITHIN + RECONNECT_MIN).as_secs(),
        "静了 {HEARTBEAT_IDLE:?} 问活、{PING_WITHIN:?} 没回话判死、隔下界重接"
    );
    // 第二条照答：空闲十分钟不断，问活照问。
    tokio::time::sleep(Duration::from_secs(600)).await;
    assert_eq!(
        f.attach_at.lock().unwrap().len(),
        2,
        "健康的空闲连接不该被心跳判死"
    );
    let asked = f.pings.load(Ordering::SeqCst) as u64;
    assert!(
        asked >= 600 / (HEARTBEAT_IDLE.as_secs() + 1),
        "空闲时每 {HEARTBEAT_IDLE:?} 问一次活：只问了 {asked} 次"
    );
}

/// 宿主报「网回来了 / 网变了 / 回前台」时**连着**（[`Feed::wake`]）⇒ 当场问一句活，不等下一次心跳：
/// 没回话 ⇒ [`PING_WITHIN`] 判死重接；回了 ⇒ 照用。
#[tokio::test(start_paused = true)]
async fn a_wake_while_up_asks_at_once() {
    let f = Arc::new(Fake::default());
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(Peer::Mute { build: MINE.into() });
        a.push_back(upp(MINE));
    }
    let (feed, _) = start(&f);
    until("流接上了", || up(&feed)).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    feed.wake();
    until("判死、重接上了", || {
        f.attach_at.lock().unwrap().len() == 2 && up(&feed)
    })
    .await;
    assert_eq!(
        gaps_of(&f)[0],
        (Duration::from_secs(1) + PING_WITHIN + RECONNECT_MIN).as_secs(),
        "叫醒那一刻就问活"
    );
    tokio::time::sleep(Duration::from_secs(1)).await;
    let before = f.pings.load(Ordering::SeqCst);
    feed.wake();
    until("健康的那一条答了这一问", || {
        f.pings.load(Ordering::SeqCst) == before + 1
    })
    .await;
    tokio::time::sleep(PING_WITHIN * 2).await;
    assert_eq!(
        f.attach_at.lock().unwrap().len(),
        2,
        "回了话 ⇒ 照用、不重接"
    );
}

/// 断在那台本身的哪一处两端照实记（`blocker`：没装 ⇒ 没有后端；不认这一版子命令的老后端 ⇒ 后端旧了）；
/// 等不等人按只看这一端装不装：装不了的（手机）停下等人，装得了的（桌面）下一轮自己装。
#[tokio::test(start_paused = true)]
async fn both_ends_record_where_the_round_was_blocked_and_only_the_phone_waits() {
    let absent = || Exec {
        exit_status: Some(127),
        stdout: String::new(),
        stderr: "ccm: not found".into(),
    };
    let stuck = |s: State| match s {
        State::Down {
            blocker,
            needs_hand,
            ..
        } => (blocker, needs_hand),
        other => panic!("{other:?}"),
    };
    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(absent());
    let (feed, _) = start(&f);
    until("没装 ⇒ 要人动手", || {
        matches!(
            feed.state(),
            State::Down {
                needs_hand: true,
                ..
            }
        )
    })
    .await;
    assert_eq!(stuck(feed.state()), (Some(Blocker::NoBackend), true));

    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(Exec {
        exit_status: None,
        stdout: hello("p0a-ancient"),
        stderr: String::new(),
    });
    let (feed, _) = start(&f);
    until("老后端 ⇒ 要人动手", || {
        matches!(
            feed.state(),
            State::Down {
                needs_hand: true,
                ..
            }
        )
    })
    .await;
    assert_eq!(stuck(feed.state()), (Some(Blocker::OldBackend), true));

    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(absent());
    f.attaches.lock().unwrap().push_back(Peer::Absent);
    let seen: Arc<Mutex<Vec<(Option<Blocker>, bool)>>> = Arc::default();
    let s = Arc::clone(&seen);
    let feed = Feed::start(
        cfg(),
        Some(Arc::new(D(Arc::clone(&f)))),
        desk(),
        sink(move |e| {
            if let Event::State(st @ State::Down { .. }) = e {
                s.lock().unwrap().push(stuck(st));
            }
        }),
    );
    until("桌面断在没有后端那一轮", || {
        seen.lock()
            .unwrap()
            .contains(&(Some(Blocker::NoBackend), false))
    })
    .await;
    assert!(
        seen.lock().unwrap().iter().all(|(_, hand)| !hand),
        "桌面不等人：{:?}",
        seen.lock().unwrap()
    );
    feed.close();
}

/// 手机装完之后换常驻：在此刻那条 SSH 上跑一次 `--resident-ensure --replace`、成了就整份重接（接上的是新的那一位）；
/// 接着时报那台 hello 的身份，断着时不报。没有 SSH ⇒ 不跑、说离线。
#[tokio::test(start_paused = true)]
async fn replacing_the_resident_runs_ensure_replace_then_reconnects() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp("p9z-old"));
    let (feed, _) = start(&f);
    until("接上旧的", || up(&feed)).await;
    assert_eq!(feed.theirs().as_deref(), Some("p9z-old"));
    f.attaches.lock().unwrap().push_back(upp(MINE));
    f.ensures.lock().unwrap().push_back(swapped(true));
    assert!(feed.replace().await.expect("换装该成"), "那台说换了");
    assert!(
        f.captured
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.ends_with("-- --resident-ensure --replace")),
        "{:?}",
        f.captured.lock().unwrap()
    );
    until("接上新的", || feed.theirs().as_deref() == Some(MINE)).await;

    let none: Arc<Feed<T>> = Feed::start(cfg(), None, desk(), sink(|_| {}));
    let e = none.replace().await.unwrap_err();
    assert_eq!(
        e.text,
        copy_core::copy_text("front.body.offline", &[("machine", "box")])
    );
    assert_eq!(none.theirs(), None);
}

fn frames_seen(seen: &Seen) -> Vec<String> {
    seen.lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            Event::Frame(l) => Some(cmd_wire::frame::kind_of(l).unwrap()),
            _ => None,
        })
        .collect()
}

/// 那台有一种帧形状不同：握手那一刻就点名（`differs`），那一种的行一条不交，别的照交。
#[tokio::test(start_paused = true)]
async fn a_frame_kind_whose_shape_differs_is_named_at_hello_and_never_handed_over() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp(SKEWED));
    let (feed, seen) = start(&f);
    until("流接上了", || up(&feed)).await;
    match feed.state() {
        State::Up { differs, .. } => assert_eq!(differs, ["session_status"]),
        other => panic!("形状不同的那一种要在接上那一刻点名：{other:?}"),
    }
    until("line 照交", || {
        frames_seen(&seen).iter().any(|k| k == "line")
    })
    .await;
    assert_eq!(frames_seen(&seen), ["sessions_replayed", "line"]);
}

/// hello 只解得出头：照接（认得出是我们的后端），帧种全部当对不上、一帧不交，一条命令不发。
#[tokio::test(start_paused = true)]
async fn a_hello_whose_body_does_not_parse_attaches_and_hands_over_nothing() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp(HEAD_ONLY));
    let (feed, seen) = start(&f);
    until("流接上了", || up(&feed)).await;
    let State::Up { differs, .. } = feed.state() else {
        unreachable!()
    };
    assert_eq!(differs.len(), cmd_wire::frame_shapes::FRAMES.len());
    // 那台照发了三行；读完它们（再等一会儿）也一行不交。
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(frames_seen(&seen).is_empty(), "{:?}", frames_seen(&seen));
    assert_eq!(f.ensured.load(Ordering::SeqCst), 0);
}

/// 应答解不开（缺 `ok`）：当一条失败交回请求方，码是「认不出」、原文进详情。
#[tokio::test(start_paused = true)]
async fn an_answer_that_does_not_parse_comes_back_as_unreadable() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp(MINE));
    f.reply_without_ok.store(true, Ordering::SeqCst);
    let (feed, _seen) = start(&f);
    match ask(&feed, "ping", Duration::from_secs(5)).await {
        Err(CallError::Remote { code, detail, .. }) => {
            assert_eq!(code, "unreadable");
            assert!(detail.contains("\"reply\""), "{detail}");
        }
        other => panic!("缺 `ok` 的应答该交回「认不出」：{other:?}"),
    }
}
