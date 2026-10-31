//! 各台的常驻流归本机后端：机器表里要连的那几台各起一条（`control/feeds.rs`），循环交的那一态直接进机器状态（推 `changed {machine}`），
//! 没有谁再交 `machine-round`。拨号那一侧用替身（[`Fake`]：开长 exec 照排好的回 · 一次性 exec 只认 ensure · 装那一步记次数）。
use super::*;
use crate::control::machine_state;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, DuplexStream};

/// 那台认的几条命令（转过去问的那一条在里面）。
const COMMANDS: &[&str] = &["ping", "history-tail", "machine-interrupts"];

/// 那台握手那一行（命令表 · 形状指纹 · 帧表照本侧编进来的那一份，`build` 是那台自报的身份）。
fn hello(build: &str) -> String {
    let frames: serde_json::Map<String, serde_json::Value> = cmd_wire::frame_shapes::FRAMES
        .iter()
        .map(|(k, h)| (k.to_string(), serde_json::json!(h)))
        .collect();
    let shapes: serde_json::Map<String, serde_json::Value> = COMMANDS
        .iter()
        .filter_map(|c| {
            Some((
                c.to_string(),
                serde_json::json!(cmd_wire::shapes::shape_of(c)?),
            ))
        })
        .collect();
    serde_json::json!({
        "kind": "hello", "build_id": build, "host_arch": "x86_64", "version": THEIR_VERSION,
        "commands": COMMANDS, "unavailable": [], "shapes": shapes, "frames": frames,
    })
    .to_string()
}

/// 那台 hello 自报的版本号（与这一份不同：机器页那台的版本号得是它）。
const THEIR_VERSION: &str = "0.0.1-their";

/// 转出去的那几帧都记在这里（判据读）。
fn caught() -> (Relay, Arc<Mutex<Vec<Frame>>>) {
    let got = Arc::new(Mutex::new(Vec::new()));
    let g = Arc::clone(&got);
    (Arc::new(move |f| g.lock().unwrap().push(f)), got)
}

/// 转出去的不看。
fn quiet() -> Relay {
    Arc::new(|_| {})
}

/// 开成之后那台怎么回 attach。
enum Peer {
    /// 回 hello、照 attach 行带的那一版比新旧。
    Serve(String),
    /// 同 `Serve`，接上之后发这几行就断开。
    Talk(String, Vec<String>),
    /// 小中继说常驻还没绑上口。
    Absent,
}

#[derive(Default)]
struct Fake {
    opens: Mutex<VecDeque<Peer>>,
    installs: AtomicUsize,
    words: Mutex<Vec<String>>,
}

impl Ssh for Fake {
    fn open(
        &self,
        _machine: &str,
        command: String,
        _progress: Progress,
    ) -> Boxed<'_, Result<Pipe, Problem>> {
        assert!(command.ends_with("-- --resident-attach"), "{command}");
        self.words.lock().unwrap().push("attach".into());
        let peer = self
            .opens
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Peer::Absent);
        Box::pin(async move {
            let (up_w, up_r) = tokio::io::duplex(64 * 1024);
            let (down_w, down_r) = tokio::io::duplex(256 * 1024);
            tokio::spawn(serve(peer, up_r, down_w));
            Ok(tokio::io::join(down_r, up_w))
        })
    }

    fn capture(&self, _machine: &str, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        assert!(command.contains("-- --resident-ensure"), "{command}");
        self.words.lock().unwrap().push(command);
        Box::pin(async {
            Ok(Exec {
                exit_status: Some(0),
                stdout: r#"{"pid":7,"replaced":false}"#.into(),
                stderr: String::new(),
            })
        })
    }

    fn install(&self, _machine: &str) -> Boxed<'_, ()> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        self.words.lock().unwrap().push("install".into());
        Box::pin(async {})
    }

    fn far(&self, _machine: &str) -> bool {
        true
    }
}

async fn serve(peer: Peer, up: DuplexStream, mut down: DuplexStream) {
    let mut lines = tokio::io::BufReader::new(up).lines();
    let (build, talk) = match peer {
        Peer::Absent => {
            let _ = down
                .write_all(b"{\"attach\":\"refused\",\"reason\":\"absent\"}\n")
                .await;
            return;
        }
        Peer::Serve(b) => (b, None),
        Peer::Talk(b, lines) => (b, Some(lines)),
    };
    let _ = down
        .write_all(format!("{}\n", hello(&build)).as_bytes())
        .await;
    let Ok(Some(attach)) = lines.next_line().await else {
        return;
    };
    let attach: serde_json::Value = serde_json::from_str(&attach).unwrap();
    let mine = attach["mine"].as_str().unwrap_or_default();
    let reply = serde_json::json!({
        "attach": "ok",
        "relation": deploy_contract::build_relation(&build, mine),
    });
    let _ = down.write_all(format!("{reply}\n").as_bytes()).await;
    if let Some(talk) = talk {
        for l in talk {
            let _ = down.write_all(format!("{l}\n").as_bytes()).await;
        }
        return;
    }
    // 之后照答：问活答「活着」；别的命令把收到的那一问原样装进回包（`args.fail` ⇒ 照那台的口吻拒）。连接一直留着。
    while let Ok(Some(l)) = lines.next_line().await {
        let asked: serde_json::Value = serde_json::from_str(&l).unwrap();
        let reply = if asked["cmd"] == "ping" {
            serde_json::json!({"kind": "reply", "id": asked["id"], "ok": true})
        } else if asked["cmd"] == "machine-interrupts" {
            // 有类型的那一问（`call`）：那台照入参答它自己那一份（带 appExit ⇒ 7 个活着的，不带 ⇒ 3 个）。
            let live = if asked["args"]["appExit"] == true {
                7
            } else {
                3
            };
            serde_json::json!({"kind": "reply", "id": asked["id"], "ok": true,
                "data": {"relayedSessions": 1, "relayedMaybe": 0, "liveStreams": live, "forwards": 0}})
        } else if asked["args"]["fail"] == true {
            serde_json::json!({"kind": "reply", "id": asked["id"], "ok": false,
                "code": "no_such_file", "message": "那台说：没有这个文件", "detail": "那台的原话"})
        } else {
            serde_json::json!({"kind": "reply", "id": asked["id"], "ok": true,
                "data": {"cmd": asked["cmd"], "args": asked["args"], "view": asked["view"]}})
        };
        let _ = down.write_all(format!("{reply}\n").as_bytes()).await;
    }
}

/// 在跑的那张表是进程里的一张：对齐一次就按那一份收别的 ⇒ 这几条一条一条跑。
async fn one_at_a_time() -> tokio::sync::MutexGuard<'static, ()> {
    static L: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    L.lock().await
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    for _ in guard_core::wall::rounds() {
        if f() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(f(), "等不到：{what}");
}

fn state_of(machine: &str) -> String {
    let all = machine_state::read_in(&[(machine.to_string(), true)], &Default::default());
    serde_json::to_value(&all[machine]).unwrap()["state"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// ★ 先红的那一条：本机后端自己连上那台（替身那台跑的就是这一版）⇒ 那台的成品是 `up`、推了 `changed {machine}`；
/// 一条 `machine-round` 都没有（壳没参与），接上的就是这一版 ⇒ 不装、不跑一次性 exec。
#[tokio::test]
async fn the_backend_attaches_a_machine_itself_and_its_product_is_up() {
    let machine = "feeds-devbox";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::BUILD_ID.to_string()));
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet());
    until("那台的成品是 up", || state_of(machine) == "up").await;
    let mut pushed = false;
    while let Ok((m, p)) = changes.try_recv() {
        if m == machine && serde_json::to_value(&p).unwrap()["state"] == "up" {
            pushed = true;
        }
    }
    assert!(pushed, "接上那一刻要推一帧 `changed {{machine}}`");
    assert_eq!(fake.installs.load(Ordering::SeqCst), 0, "这一版 ⇒ 不装");
    assert_eq!(*fake.words.lock().unwrap(), vec!["attach".to_string()]);
    align_in(&[], &ssh, &quiet());
    assert!(!running(machine), "表里没了 ⇒ 那一条收掉");
}

/// 接不上（常驻不在）⇒ 先装（本机后端那一趟 `deploy-install`）、再 ensure、再接；接上的是这一版 ⇒ `up`。
#[tokio::test]
async fn a_machine_without_a_resident_is_installed_then_ensured_then_attached() {
    let machine = "feeds-fresh";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    {
        let mut o = fake.opens.lock().unwrap();
        o.push_back(Peer::Absent);
        o.push_back(Peer::Serve(crate::BUILD_ID.to_string()));
    }
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet());
    until("那台的成品是 up", || state_of(machine) == "up").await;
    let words = fake.words.lock().unwrap().clone();
    assert_eq!(
        words[..2],
        ["attach".to_string(), "install".to_string()],
        "{words:?}"
    );
    assert!(words[2].ends_with("-- --resident-ensure"), "{words:?}");
    align_in(&[], &ssh, &quiet());
}

/// 机器页「停」：那一条停下、不再自己接（不然转眼又 ensure 起来）；「起」/［重试］叫醒它。
#[tokio::test]
async fn a_stopped_machine_stays_stopped_until_it_is_woken() {
    let machine = "feeds-stopped";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::BUILD_ID.to_string()));
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet());
    until("先接上", || state_of(machine) == "up").await;
    hold(machine);
    assert!(!running(machine), "停了 ⇒ 那一条收掉");
    align_in(&[machine.to_string()], &ssh, &quiet());
    assert!(!running(machine), "停着的那一台对齐机器表时也不起");
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::BUILD_ID.to_string()));
    resume_in(machine, &[machine.to_string()], &ssh, &quiet());
    assert!(running(machine), "起 ⇒ 又有那一条");
    align_in(&[], &ssh, &quiet());
}

/// 一问请求（信封照帧面那一形，`within_ms` 给足）。
fn asked(cmd: &str, args: serde_json::Value, view: serde_json::Value) -> crate::wire::Request {
    let line = serde_json::json!({"id": "a1", "cmd": cmd, "args": args, "view": view, "within_ms": 30_000});
    let mut req: crate::wire::Request = serde_json::from_value(line).unwrap();
    req.until =
        crate::platform::child::Until::after(crate::platform::child::Deadline::millis(30_000));
    req
}

/// ★ ② 先红的那一条：信封带 `machine` 的一问经那台的常驻流转过去 —— 命令 · 入参 · 出口声明原样到那台，那台的回包原样回来；
/// 那台拒了 ⇒ 码 · 那一句 · 原话原样交回（不压成一个）。
#[tokio::test]
async fn a_call_for_a_machine_goes_through_its_resident_stream() {
    let _one = one_at_a_time().await;
    let machine = "feeds-forward";
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::BUILD_ID.to_string()));
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet());
    until("那台的成品是 up", || state_of(machine) == "up").await;

    let view = serde_json::json!({"omit": {"record": ["blocks[type=tool_use].input"]}});
    let got = forward(
        machine.to_string(),
        asked(
            "history-tail",
            serde_json::json!({"path": "/p/s.jsonl", "n": 2}),
            view.clone(),
        ),
    )
    .await
    .expect("转过去该成")
    .expect("那台回了体");
    assert_eq!(got["cmd"], "history-tail");
    assert_eq!(got["args"]["path"], "/p/s.jsonl");
    assert_eq!(got["view"], view, "出口声明原样带到那台");

    let fail = forward(
        machine.to_string(),
        asked(
            "history-tail",
            serde_json::json!({"fail": true}),
            serde_json::Value::Null,
        ),
    )
    .await
    .expect_err("那台拒了");
    assert_eq!(fail.code.as_deref(), Some("no_such_file"));
    assert_eq!(fail.said, "那台说：没有这个文件");
    assert_eq!(fail.raw.as_deref(), Some("那台的原话"));
    align_in(&[], &ssh, &quiet());
}

/// 没有那一台的常驻流（机器表里没有 · 停着）⇒ 不发，说「那一跳断开」，码是协议里那一个。
#[tokio::test]
async fn a_call_for_a_machine_without_a_stream_is_not_sent() {
    let _one = one_at_a_time().await;
    let fail = forward(
        "feeds-nowhere".to_string(),
        asked(
            "history-tail",
            serde_json::json!({}),
            serde_json::Value::Null,
        ),
    )
    .await
    .expect_err("没有那一条就不发");
    assert_eq!(fail.code.as_deref(), Some(UNREACHABLE));
}

/// 本机后端自己问那台一条有类型的命令（`call`，并好再答的那几条用）：经同一条常驻流，入参照类型带过去、回包照类型收；
/// 没有那一条 ⇒ 同 `forward` 那个码。
#[tokio::test]
async fn a_typed_call_from_this_backend_goes_through_the_same_stream() {
    use crate::faces::read_face::{MachineInterrupts, MachineInterruptsReq};
    let _one = one_at_a_time().await;
    let machine = "feeds-call";
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::BUILD_ID.to_string()));
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet());
    until("那台的成品是 up", || state_of(machine) == "up").await;
    let within =
        crate::platform::child::Until::after(crate::platform::child::Deadline::millis(30_000));
    let exit = MachineInterruptsReq {
        app_exit: Some(true),
        ..Default::default()
    };
    let got = call::<MachineInterrupts>(machine, &exit, within)
        .await
        .expect("问到了");
    assert_eq!(got.live_streams, 7, "入参照类型带到那台");
    assert_eq!(got.relayed_sessions, Some(1));
    let plain = call::<MachineInterrupts>(machine, &MachineInterruptsReq::default(), within)
        .await
        .expect("问到了");
    assert_eq!(plain.live_streams, 3);
    align_in(&[], &ssh, &quiet());
    let fail = call::<MachineInterrupts>("feeds-call-nowhere", &exit, within)
        .await
        .expect_err("没有那一条就不发");
    assert_eq!(fail.code.as_deref(), Some(UNREACHABLE));
}

/// ★ ④a 先红的那一条：那台那条常驻流上的事原样转给连着本机后端的壳 —— 先它的 hello（一行），再流上的帧（一行一帧），
/// 断了一帧 `ended`；机器页那台的版本号是它 hello 里自报的那一格（不是这一份的）。
#[tokio::test]
async fn a_machines_stream_is_relayed_as_is_and_its_version_comes_from_its_hello() {
    use cmd_wire::frame::Relayed;
    let machine = "feeds-relay";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    let said = r#"{"kind":"sessions_replayed"}"#.to_string();
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Talk(crate::BUILD_ID.to_string(), vec![said.clone()]));
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    align_in(&[machine.to_string()], &ssh, &relay);
    let ended = |g: &[Frame]| {
        g.iter()
            .any(|f| matches!(f, Frame::Relayed(Relayed::Ended { machine: m }) if m == machine))
    };
    until("那台这一条断了 ⇒ 转一帧 ended", || {
        ended(&got.lock().unwrap())
    })
    .await;
    align_in(&[], &ssh, &quiet());
    let seen: Vec<Relayed> = got
        .lock()
        .unwrap()
        .iter()
        .map(|f| match f {
            Frame::Relayed(r) => r.clone(),
            other => panic!("只转 relayed：{other:?}"),
        })
        .collect();
    let first_end = seen
        .iter()
        .position(|r| matches!(r, Relayed::Ended { .. }))
        .unwrap();
    assert_eq!(
        seen[..=first_end],
        [
            Relayed::Line {
                machine: machine.into(),
                line: hello(crate::BUILD_ID),
            },
            Relayed::Line {
                machine: machine.into(),
                line: said,
            },
            Relayed::Ended {
                machine: machine.into(),
            },
        ],
        "{seen:?}"
    );
    let mut up_version = None;
    while let Ok((m, p)) = changes.try_recv() {
        if m == machine && p.state == cmd_wire::machine::MachineStateKind::Up {
            up_version = Some(p.version);
        }
    }
    assert_eq!(
        up_version,
        Some(Some(THEIR_VERSION.to_string())),
        "接上那一刻那台的版本号是它 hello 里那一格"
    );
}

/// 新连上来的流先收的那几帧：此刻接着的那台那一条连接的 hello；那一条收了 ⇒ 不再给。
#[tokio::test]
async fn a_new_stream_first_gets_the_hello_of_each_machine_attached_now() {
    use cmd_wire::frame::Relayed;
    let machine = "feeds-hello-now";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::BUILD_ID.to_string()));
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    let hello_now = || {
        attached_hellos().into_iter().any(|f| {
            matches!(&f, Frame::Relayed(Relayed::Line { machine: m, line }) if m == machine && *line == hello(crate::BUILD_ID))
        })
    };
    align_in(&[machine.to_string()], &ssh, &relay);
    until("那台接上了、它的 hello 转出去了", || {
        got.lock()
            .unwrap()
            .iter()
            .any(|f| matches!(f, Frame::Relayed(Relayed::Line { machine: m, .. }) if m == machine))
    })
    .await;
    assert!(hello_now(), "那台接着的时候新流拿不到它的 hello");
    align_in(&[], &ssh, &quiet());
    assert!(!hello_now(), "那台那一条收了，新流还先收它的 hello");
}

fn ask(sids: &[&str], kept: &[&str]) -> WatchAsk {
    WatchAsk {
        sids: sids.iter().map(|s| s.to_string()).collect(),
        branch: Vec::new(),
        kept: kept.iter().map(|s| s.to_string()).collect(),
    }
}

/// 各条流报那台的在看名单 ⇒ 那台的名单台是它们的并集；一条流走了 ⇒ 只剩别的那几条的；那台没接着 ⇒ 回空的 `from[]`。
#[tokio::test]
async fn each_streams_watch_list_for_a_machine_unions_into_that_machines_desk() {
    let machine = "feeds-watch-union";
    let set = |v: &[&str]| {
        v.iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<String>>()
    };
    let a = Watching(9_001);
    let b = Watching(9_002);
    let from = watch_at(machine, a.0, ask(&["s1", "s2"], &["k1"])).await;
    assert!(from.is_empty(), "那台没接着，却回了从第几行起：{from:?}");
    let _ = watch_at(machine, b.0, ask(&["s2", "s3"], &[])).await;
    assert_eq!(desk_of(machine).sids(), set(&["s1", "s2", "s3"]));
    assert_eq!(union_of(machine).1, set(&["k1"]));
    drop(a);
    assert_eq!(
        desk_of(machine).sids(),
        set(&["s2", "s3"]),
        "那条流走了，它报的那几条还在那台的名单里"
    );
    drop(b);
    assert!(desk_of(machine).sids().is_empty());
}

/// 那台说的一帧会话帧（一行）：宣告 / 离开，脸照后端的构造口造。
fn told(kind: &str, sid: &str, life: crate::wire::Life) -> String {
    use crate::wire::SessionFaceExt as _;
    let face = crate::wire::SessionFace::of(life);
    let mut v = serde_json::json!({"kind": kind, "sid": sid, "face": face});
    if kind == "session_added" {
        v["agent_kind"] = serde_json::json!("claude");
        v["pid"] = serde_json::json!(7);
        v["label"] = serde_json::json!(sid);
    }
    v.to_string()
}

/// ★ A1-3-3 先红的那一条：那台这一条连接断的那一刻，本机后端给它簿里活的 · 可重连的每一条转一帧 `session_left`（脸 ＝ 看不见那一张，
/// `face.life = unclear`），排在 `ended` 之前；已结束的不说。壳一条都不用合成。
#[tokio::test]
async fn a_lost_machine_has_each_of_its_open_sessions_said_unclear_by_this_backend() {
    use crate::wire::Life;
    use cmd_wire::frame::Relayed;
    let machine = "feeds-unclear";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    let talk = vec![
        told("session_added", "s-live", Life::live(None, None)),
        told("session_added", "s-back", Life::live(None, None)),
        told("session_left", "s-back", Life::Reconnectable),
        told("session_added", "s-done", Life::live(None, None)),
        told("session_left", "s-done", Life::Ended),
    ];
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Talk(crate::BUILD_ID.to_string(), talk.clone()));
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    align_in(&[machine.to_string()], &ssh, &relay);
    until("那台这一条断了 ⇒ 转一帧 ended", || {
        got.lock()
            .unwrap()
            .iter()
            .any(|f| matches!(f, Frame::Relayed(Relayed::Ended { machine: m }) if m == machine))
    })
    .await;
    align_in(&[], &ssh, &quiet());
    let seen: Vec<Relayed> = got
        .lock()
        .unwrap()
        .iter()
        .filter_map(|f| match f {
            Frame::Relayed(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    let end = seen
        .iter()
        .position(|r| matches!(r, Relayed::Ended { .. }))
        .unwrap();
    let unclear = resident_client::book::unseen(crate::common::cells::Words(
        comms_inward::chan::said::offline(machine),
    ));
    let said_left = |sid: &str| Relayed::Line {
        machine: machine.into(),
        line: serde_json::to_string(&Frame::<serde_json::Value>::SessionLeft(
            crate::wire::SessionLeft {
                sid: sid.into(),
                face: unclear.clone(),
            },
        ))
        .unwrap(),
    };
    assert_eq!(
        seen[1 + talk.len()..=end],
        [
            said_left("s-back"),
            said_left("s-live"),
            Relayed::Ended {
                machine: machine.into()
            },
        ],
        "{seen:#?}"
    );
    assert_eq!(unclear.life, crate::wire::WireLife::Unclear);
}

/// 一台的成品（只填 `wake_in` 读的那几格）。
fn wake_row(
    kind: cmd_wire::machine::MachineStateKind,
    said: &str,
) -> cmd_wire::machine::MachineState {
    cmd_wire::machine::MachineState {
        name: "nas".into(),
        state: kind,
        offline: false,
        reason: None,
        stage: None,
        version: None,
        version_relation: None,
        os: None,
        word: String::new(),
        said: said.into(),
        offline_said: "nas 离线".into(),
        fixes: Vec::new(),
        seen_host_key: None,
        expected_host_key: None,
        detail: None,
        route: None,
        strip: None,
    }
}

#[test]
fn reconnecting_a_machine_answers_with_its_state_and_refuses_unknown_disabled_or_busy_ones() {
    use cmd_wire::machine::MachineStateKind as K;
    let woke = std::cell::RefCell::new(Vec::<String>::new());
    let wake = |m: &str| woke.borrow_mut().push(m.to_string());
    let one = |kind: K| {
        std::collections::BTreeMap::from([("nas".to_string(), wake_row(kind, "nas 那一句"))])
    };
    // 断着 ⇒ 叫醒、回那台此刻的成品。
    let got = wake_in("nas", &one(K::Down), wake).expect("断着的那台照常叫醒");
    assert_eq!(got.state, K::Down);
    assert_eq!(*woke.borrow(), ["nas"]);
    // 机器表里没有 ⇒ unknown_machine，不叫醒。
    match wake_in("ghost", &one(K::Down), wake) {
        Err(MachineWakeErr::UnknownMachine(said)) => assert!(said.contains("ghost"), "{said}"),
        other => panic!("{other:?}"),
    }
    // 停用的 ⇒ disabled，那一句是那台成品写的。
    match wake_in("nas", &one(K::Disabled), wake) {
        Err(MachineWakeErr::Disabled(said)) => assert_eq!(said, "nas 那一句"),
        other => panic!("{other:?}"),
    }
    // 正在连 / 装 / 更新 ⇒ busy，带那一刻的成品。
    for k in [K::Connecting, K::Installing, K::Updating] {
        match wake_in("nas", &one(k), wake) {
            Err(MachineWakeErr::Busy(said, m)) => {
                assert_eq!(said, "nas 那一句");
                assert_eq!(m.state, k);
            }
            other => panic!("{k:?}: {other:?}"),
        }
    }
    assert_eq!(*woke.borrow(), ["nas"], "拒了的那几下都没叫醒");
}
