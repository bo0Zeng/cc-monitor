//! 一台的常驻流（两端同一条循环）：直接接 · 不在就 ensure 再接 · 那台旧：装不了的照接并说要更新、装得了的换一次再接 ·
//! 发完 hello 就死退避照样涨 · 问答等它接上 · 新连接整份重报在看名单 · 没装：装不了的要人动手、装得了的自己再试 ·
//! 不是 Unix 两端都等人 · ［重新连接］在拨的时候按也算数、接上那一刻作废 · 断线重连续上之后数格。全部虚拟时钟。
use super::*;
use crate::backoff::RECONNECT_MIN;
use crate::client::CallError;
use crate::gate::Verdict;
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

/// 那台握手那一行：交它认的命令与那几条的形状指纹（跟本侧编进来的同一份）。
/// `p0a-ancient` 比版本门那一问还早（不认 `resident-verdict`）；`p9b-reshaped` 认它、可那一条的形状与本侧对不上（比形状改过之前的那一版）。
fn hello(build: &str) -> String {
    let cmds: &[&str] = if build == "p0a-ancient" {
        &["ping", "cancel", "stream-watch"]
    } else {
        &["ping", "cancel", "stream-watch", "resident-verdict"]
    };
    let shapes: serde_json::Map<String, serde_json::Value> = cmds
        .iter()
        .filter_map(|c| {
            let fp = if build == "p9b-reshaped" && *c == crate::gate::VERDICT_CMD {
                "00000000"
            } else {
                cmd_wire::shapes::shape_of(c)?
            };
            Some((c.to_string(), serde_json::Value::String(fp.into())))
        })
        .collect();
    format!(
        r#"{{"kind":"hello","v":1,"build_id":"{build}","host_arch":"x86_64","claude_dir":"/c","commands":{cmds},"shapes":{shapes}}}"#,
        cmds = serde_json::to_string(cmds).unwrap(),
        shapes = serde_json::Value::Object(shapes),
    )
}

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
}

#[derive(Default)]
struct Fake {
    attaches: Mutex<VecDeque<Peer>>,
    ensures: Mutex<VecDeque<Exec>>,
    attach_at: Mutex<Vec<tokio::time::Instant>>,
    ensured: AtomicUsize,
    verdicts: AtomicUsize,
    watches: Mutex<Vec<serde_json::Value>>,
    /// 一次性 exec 的那几条命令（看换装那一条发没发）。
    captured: Mutex<Vec<String>>,
    /// 每一轮开拨之前那一步跑了几回（只有装得了后端的宿主替身 [`D`] 记）。
    rounds: AtomicUsize,
    /// 每一轮开拨之前那一步各等一个放行（有就等，没有就直接过）。
    holds: Mutex<VecDeque<Arc<tokio::sync::Notify>>>,
    /// 那台报 `stream-watch` 时答的 `from[]`（每答一次取一份；没有 ⇒ 空）。
    froms: Mutex<VecDeque<serde_json::Value>>,
    /// 拨它 ⇒ 接着的那一条当场断（断网）。
    drop_link: tokio::sync::Notify,
    /// 那台收到的 `ping`（问活）条数。
    pings: AtomicUsize,
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
            let (mine, theirs) = tokio::io::duplex(64 * 1024);
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

/// 装得了后端的宿主替身（桌面那一形）：链路同 [`T`]；每一轮之前记一回（有放行就等它）；版本门问「本机」——
/// 替身的规矩：`p9…` 比手上这一版旧 ⇒ 没换过就换、换过还旧就照接说旧。
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
    fn installer(&self) -> Option<&dyn Install> {
        Some(self)
    }
}

impl Install for D {
    fn verdict<'a>(
        &'a self,
        mine: &'a str,
        theirs: &'a str,
        replaced: bool,
    ) -> Boxed<'a, Result<Verdict, Problem>> {
        Box::pin(async move {
            assert_eq!(mine, MINE);
            let older = theirs.starts_with("p9");
            Ok(Verdict {
                replace: older && !replaced,
                older,
            })
        })
    }
}

fn start_desk(f: &Arc<Fake>) -> (Arc<Feed<D>>, Seen) {
    let seen: Seen = Arc::default();
    let s = Arc::clone(&seen);
    let feed = Feed::start(
        cfg(),
        Some(Arc::new(D(Arc::clone(f)))),
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
        Peer::Mute { build } => (build, false),
        Peer::Hang | Peer::Slow { .. } => unreachable!("没人回话 / 还在拨的那一拨开不出链路"),
    };
    let _ = r
        .get_mut()
        .write_all(format!("{}\n", hello(&build)).as_bytes())
        .await;
    let mut line = String::new();
    if r.read_line(&mut line).await.unwrap_or(0) == 0 {
        return;
    }
    let _ = r.get_mut().write_all(b"{\"attach\":\"ok\"}\n").await;
    if die_now {
        return;
    }
    let _ = r
        .get_mut()
        .write_all(b"{\"kind\":\"sessions_replayed\"}\n")
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
        // 后端判新旧（这台替身：`p9…` 比手上这一版旧，别的不旧）。
        let data = if v["cmd"] == "resident-verdict" {
            f.verdicts.fetch_add(1, Ordering::SeqCst);
            let older = v["args"]["theirs"].as_str().unwrap_or("").starts_with("p9");
            serde_json::json!({"action": if older { "replace" } else { "attach" }, "older": older})
        } else if v["cmd"] == "stream-watch" {
            let from = f.froms.lock().unwrap().pop_front();
            serde_json::json!({"from": from.unwrap_or_else(|| serde_json::json!([]))})
        } else {
            serde_json::json!({"from":[]})
        };
        let reply = serde_json::json!({"kind":"reply","id":v["id"],"ok":true,"data":data});
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
    let seen: Seen = Arc::default();
    let s = Arc::clone(&seen);
    let feed = Feed::start(cfg(), None, sink(move |e| s.lock().unwrap().push(e)));
    feed.set_transport(Some(Arc::new(T(Arc::clone(f)))));
    (feed, seen)
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
            notice: None
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

#[tokio::test(start_paused = true)]
async fn an_older_backend_is_used_and_said_to_need_an_update() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp("p9a-old"));
    let (feed, _) = start(&f);
    until("接上、说了要更新", || {
        matches!(
            feed.state(),
            State::Up {
                notice: Some(_),
                ..
            }
        )
    })
    .await;
    match feed.state() {
        State::Up {
            notice: Some(p), ..
        } => {
            assert_eq!(p.text, copy_core::backend_old("box"));
            assert!(p.detail.contains("p9a-old"), "{}", p.detail);
        }
        other => panic!("那台旧了照接、带一句要更新：{other:?}"),
    }
    assert!(
        ask(&feed, "ping", Duration::from_secs(5)).await.is_ok(),
        "旧的照用对得上的那几条"
    );
    // 那台不认这一问 ⇒ 比这一问还早 ⇒ 当旧。
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp("p0a-ancient"));
    let (feed, _) = start(&f);
    until("不认 resident-verdict ⇒ 当旧", || {
        matches!(
            feed.state(),
            State::Up {
                notice: Some(_),
                ..
            }
        )
    })
    .await;
    // 那台认这一问、可这一问的形状与本侧对不上（版本门这一条改过形之前的那一版）⇒ 问不成 ⇒ 当旧、照实说要更新（一问都不发）。
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp("p9b-reshaped"));
    let (feed, _) = start(&f);
    until("版本门那一问形状对不上 ⇒ 当旧", || {
        matches!(
            feed.state(),
            State::Up {
                notice: Some(_),
                ..
            }
        )
    })
    .await;
    match feed.state() {
        State::Up {
            notice: Some(p), ..
        } => assert_eq!(p.text, copy_core::backend_old("box")),
        other => panic!("形状对不上的那台照接、带一句要更新：{other:?}"),
    }
    assert_eq!(
        f.verdicts.load(Ordering::SeqCst),
        0,
        "形状对不上的那一问不该发出去"
    );
    for newer in ["p11a-next", MINE] {
        let f = Arc::new(Fake::default());
        f.attaches.lock().unwrap().push_back(upp(newer));
        let (feed, _) = start(&f);
        until("接上、问过新旧", || {
            up(&feed) && f.verdicts.load(Ordering::SeqCst) == 1
        })
        .await;
        tokio::task::yield_now().await;
        assert!(
            matches!(feed.state(), State::Up { notice: None, .. }),
            "{newer}：同或新 ⇒ 接、不说"
        );
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
    let (_feed, seen) = start(&f);
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
    let feed: Arc<Feed<T>> = Feed::start(cfg(), None, sink(|_| {}));
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

#[tokio::test(start_paused = true)]
async fn every_new_link_gets_the_whole_watch_list_again() {
    let f = Arc::new(Fake::default());
    f.attaches.lock().unwrap().push_back(upp(MINE));
    let (feed, _) = start(&f);
    until("流接上了", || up(&feed)).await;
    feed.watch("s1", true);
    feed.watch("s1", true);
    until("报了 s1", || {
        f.watches
            .lock()
            .unwrap()
            .iter()
            .any(|a| a["sids"] == serde_json::json!(["s1"]))
    })
    .await;
    feed.watch("s1", false);
    let before = f.watches.lock().unwrap().len();
    // 换一条 SSH（网络换了）⇒ 旧流关掉、当场在新的上接；新连接照当前名单整份重报。
    f.attaches.lock().unwrap().push_back(upp(MINE));
    feed.set_transport(Some(Arc::new(T(Arc::clone(&f)))));
    until("新连接上又报了一次", || {
        let w = f.watches.lock().unwrap();
        w.len() > before && w.last().unwrap()["sids"] == serde_json::json!(["s1"])
    })
    .await;
    assert!(matches!(feed.state(), State::Up { epoch: 2, .. }));
}

/// 装得了后端的宿主（桌面那一形）：那台旧 ⇒ 版本门问本机、`--resident-ensure --replace` 换一次再接；换上的不旧 ⇒ 接、不说。
/// 不问接上的那一台自己；每一轮开拨之前那一步先跑；接上先交 hello 与客户端，再交 `Up`。
#[tokio::test(start_paused = true)]
async fn a_host_that_installs_replaces_an_older_backend_once_then_attaches() {
    let f = Arc::new(Fake::default());
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(upp("p9a-old"));
        a.push_back(upp(MINE)); // 换上的这一版
    }
    let (feed, seen) = start_desk(&f);
    until("换过之后接上", || up(&feed)).await;
    assert_eq!(
        *f.captured.lock().unwrap(),
        vec![backend_command(ENSURE_REPLACE)],
        "旧了 ⇒ 恰好换一次（常驻在跑，不必先 ensure）"
    );
    assert_eq!(
        feed.state(),
        State::Up {
            epoch: 1,
            notice: None
        }
    );
    assert_eq!(f.rounds.load(Ordering::SeqCst), 1, "开拨之前那一步跑一回");
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        f.verdicts.load(Ordering::SeqCst),
        0,
        "装得了后端 ⇒ 版本门问本机，不问那一台"
    );
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

    // 换过还旧（落点上那份也旧）⇒ 照接、说要更新，不再换第二次。
    let f = Arc::new(Fake::default());
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(upp("p9a-old"));
        a.push_back(upp("p9b-still-old"));
    }
    let (feed, _) = start_desk(&f);
    until("换过一次之后照接", || up(&feed)).await;
    assert_eq!(f.captured.lock().unwrap().len(), 1, "只换一次");
    match feed.state() {
        State::Up {
            notice: Some(p), ..
        } => assert!(p.detail.contains("p9b-still-old"), "{}", p.detail),
        other => panic!("换过还旧 ⇒ 接上、带那一句：{other:?}"),
    }
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
                unsupported: false,
                ..
            })
        )),
        "没装那一轮 ⇒ 断着、不要人动手"
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
                    unsupported: true,
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

/// **断线 → 重连 → 续上，数格**（手机那一侧交给 Kotlin 的就是这几件）：在看 s1；那台答「s1 从第 5 行起上流」；断网；
/// 重连之后整份重报名单，那台答「从第 9 行起」。每件事恰好几格、按序，不看墙钟。
#[tokio::test(start_paused = true)]
async fn a_drop_then_reconnect_then_resume_hands_over_exactly_these_cells() {
    let f = Arc::new(Fake::default());
    {
        let mut a = f.attaches.lock().unwrap();
        a.push_back(upp(MINE));
        a.push_back(upp(MINE));
        let mut fr = f.froms.lock().unwrap();
        fr.push_back(serde_json::json!([{"sid":"s1","path":"/p/s1.jsonl","seq":5}]));
        fr.push_back(serde_json::json!([{"sid":"s1","path":"/p/s1.jsonl","seq":9}]));
    }
    let (feed, seen) = start(&f);
    feed.watch("s1", true);
    let from_cells = |seen: &Seen| {
        seen.lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, Event::WatchFrom(_)))
            .count()
    };
    until("第一条连接上报了名单、拿到续点", || {
        from_cells(&seen) == 1
    })
    .await;
    f.drop_link.notify_waiters();
    until("重连之后又报了一次、拿到新的续点", || {
        from_cells(&seen) == 2
    })
    .await;
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
            Event::WatchFrom(w) => format!("from {}@{}", w[0].sid, w[0].seq),
            Event::Ssh(b) => format!("ssh {b}"),
        })
        .collect();
    // 名单报在连上之后、那台的第一帧与它谁先到不定 ⇒ 每条连接内只比集合，跨连接的先后照比。
    let (first, second) = cells.split_at(
        cells
            .iter()
            .position(|c| c == "down hand=false")
            .expect("断网要交一格「断了」"),
    );
    let sorted = |xs: &[String]| {
        let mut v = xs.to_vec();
        v.sort();
        v
    };
    assert_eq!(
        sorted(first),
        sorted(&[
            "connecting".into(),
            "attached".into(),
            "up 1".into(),
            "frame sessions_replayed".into(),
            "from s1@5".into(),
            "ssh true".into(),
            "ssh false".into(),
        ]),
        "第一条连接：{cells:?}"
    );
    assert_eq!(
        sorted(second),
        sorted(&[
            "down hand=false".into(),
            "connecting".into(),
            "attached".into(),
            "up 2".into(),
            "frame sessions_replayed".into(),
            "from s1@9".into(),
            "ssh true".into(),
        ]),
        "断了 → 重连 → 续上：{cells:?}"
    );
    assert_eq!(
        &first[..4],
        &[
            "connecting".to_string(),
            "ssh true".into(),
            "attached".into(),
            "up 1".into()
        ],
        "先接上（hello 与客户端先于 Up），再有帧"
    );
    assert_eq!(f.watches.lock().unwrap().len(), 2, "名单每条连接整份报一次");
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
            FailKind::Install(Fix::Install)
        )
    );
    let (p, k) = fail(exec(&hello("p1a-old"), "", None));
    assert_eq!(
        (p.text, k),
        (
            copy_core::backend_old("devbox"),
            FailKind::Install(Fix::Update)
        )
    );
    let (p, k) = fail(exec("", "sh: ccm: not found", Some(127)));
    assert_eq!(
        (p.text, k),
        (
            copy_text("remoteHealth.head.notDeployed", &[]),
            FailKind::Install(Fix::Install)
        )
    );
    let (p, k) = fail(exec(
        "",
        r#"{"code":"unsupported","message":"不是 unix","detail":"命令: resident-ensure"}"#,
        Some(2),
    ));
    assert_eq!(k, FailKind::Unsupported);
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
            FailKind::Install(Fix::Install)
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
    let (feed, seen) = start(&f);
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

/// 装不了后端的宿主（手机）断在「装一次会好」那几种 ⇒ 状态里点名要哪一种：没装 ⇒ `install`；不认这一版子命令的老后端 ⇒ `update`。
/// 装得了的（桌面）下一轮自己装 ⇒ 不点名。
#[tokio::test(start_paused = true)]
async fn the_phone_is_told_whether_to_install_or_update() {
    let absent = || Exec {
        exit_status: Some(127),
        stdout: String::new(),
        stderr: "ccm: not found".into(),
    };
    let fix_of = |s: State| match s {
        State::Down { fix, .. } => fix,
        _ => None,
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
    assert_eq!(fix_of(feed.state()), Some(Fix::Install));
    let v: serde_json::Value = serde_json::to_value(feed.state()).unwrap();
    assert_eq!(v["fix"], "install", "{v}");

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
    assert_eq!(fix_of(feed.state()), Some(Fix::Update));

    let f = Arc::new(Fake::default());
    f.ensures.lock().unwrap().push_back(absent());
    f.attaches.lock().unwrap().push_back(Peer::Absent);
    let seen_fix: Arc<Mutex<Vec<Option<Fix>>>> = Arc::default();
    let s = Arc::clone(&seen_fix);
    let feed = Feed::start(
        cfg(),
        Some(Arc::new(D(Arc::clone(&f)))),
        sink(move |e| {
            if let Event::State(State::Down { fix, .. }) = e {
                s.lock().unwrap().push(fix);
            }
        }),
    );
    until("桌面断过一轮", || {
        !seen_fix.lock().unwrap().is_empty()
    })
    .await;
    assert!(
        seen_fix.lock().unwrap().iter().all(Option::is_none),
        "{:?}",
        seen_fix.lock().unwrap()
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
    feed.replace().await.expect("换装该成");
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

    let none: Arc<Feed<T>> = Feed::start(cfg(), None, sink(|_| {}));
    let e = none.replace().await.unwrap_err();
    assert_eq!(
        e.text,
        copy_core::copy_text("front.body.offline", &[("machine", "box")])
    );
    assert_eq!(none.theirs(), None);
}
