//! 一台的常驻流（手机那一侧整条循环）：直接接 · 不在就 ensure 再接 · 那台旧照接并说要更新 · 发完 hello 就死退避照样涨 ·
//! 问答等它接上 · 新连接整份重报在看名单 · 没装要人动手 · 换一条 SSH 当场重接。全部虚拟时钟。
use super::*;
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

fn hello(build: &str) -> String {
    format!(
        r#"{{"kind":"hello","v":1,"build_id":"{build}","host_arch":"x86_64","claude_dir":"/c","commands":{cmds}}}"#,
        cmds = if build == "p0a-ancient" {
            r#"["ping","cancel","stream-watch"]"#
        } else {
            r#"["ping","cancel","stream-watch","resident-verdict"]"#
        }
    )
}

/// 对端那一台怎么回一条 `--resident-attach`。
#[derive(Clone)]
enum Peer {
    /// 小中继说没人在听。
    Absent,
    /// 握手成，之后照答；`die_now` ⇒ 回完 attach 就断。
    Up { build: String, die_now: bool },
}

#[derive(Default)]
struct Fake {
    attaches: Mutex<VecDeque<Peer>>,
    ensures: Mutex<VecDeque<Exec>>,
    attach_at: Mutex<Vec<tokio::time::Instant>>,
    ensured: AtomicUsize,
    verdicts: AtomicUsize,
    watches: Mutex<Vec<serde_json::Value>>,
}

struct T(Arc<Fake>);

impl Transport for T {
    type Stream = tokio::io::DuplexStream;
    fn stream(&self, command: String) -> Boxed<'_, Result<Self::Stream, Problem>> {
        let f = Arc::clone(&self.0);
        Box::pin(async move {
            assert!(command.ends_with("-- --resident-attach"), "{command}");
            f.attach_at
                .lock()
                .unwrap()
                .push(tokio::time::Instant::now());
            let peer = f
                .attaches
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Peer::Absent);
            let (mine, theirs) = tokio::io::duplex(64 * 1024);
            tokio::spawn(serve(peer, theirs, Arc::clone(&f)));
            Ok(mine)
        })
    }
    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        let f = Arc::clone(&self.0);
        Box::pin(async move {
            assert!(command.ends_with("-- --resident-ensure"), "{command}");
            f.ensured.fetch_add(1, Ordering::SeqCst);
            Ok(f.ensures.lock().unwrap().pop_front().unwrap_or(Exec {
                exit_status: Some(0),
                stdout: "{}".into(),
                stderr: String::new(),
            }))
        })
    }
}

async fn serve(peer: Peer, s: tokio::io::DuplexStream, f: Arc<Fake>) {
    let mut r = tokio::io::BufReader::new(s);
    let (build, die_now) = match peer {
        Peer::Absent => {
            let _ = r
                .get_mut()
                .write_all(b"{\"attach\":\"refused\",\"reason\":\"absent\"}\n")
                .await;
            return;
        }
        Peer::Up { build, die_now } => (build, die_now),
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
        if r.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        if v["cmd"] == "stream-watch" {
            f.watches.lock().unwrap().push(v["args"].clone());
        }
        // 后端判新旧（这台替身：`p9…` 比手上这一版旧，别的不旧）。
        let data = if v["cmd"] == "resident-verdict" {
            f.verdicts.fetch_add(1, Ordering::SeqCst);
            let older = v["args"]["theirs"].as_str().unwrap_or("").starts_with("p9");
            serde_json::json!({"action": if older { "replace" } else { "attach" }, "older": older})
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
    let feed = Feed::start(cfg(), Arc::new(move |e| s.lock().unwrap().push(e)));
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

fn up(f: &Feed<T>) -> bool {
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
    until("接上", || up(&feed)).await;
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
    until("hello 与会话帧交出去", || {
        seen.lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, Event::Frame(_)))
            .count()
            >= 2
    })
    .await;
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
    until("接上", || up(&feed)).await;
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
    assert!(
        seen.lock().unwrap().iter().any(|e| *e == Event::Lost),
        "接上过的流断了 ⇒ 请宿主探 SSH"
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
        State::Down { problem, .. } => assert_eq!(
            problem.text,
            copy_core::copy_text("remoteHealth.head.notDeployed", &[])
        ),
        _ => unreachable!(),
    }
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
    let feed: Arc<Feed<T>> = Feed::start(cfg(), Arc::new(|_| {}));
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
    until("接上", || up(&feed)).await;
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
