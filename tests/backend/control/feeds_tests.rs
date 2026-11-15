//! 各台的常驻流归本机后端：机器表里要连的那几台各起一条（`control/feeds.rs`），循环交的那一态直接进机器状态（推 `changed {machine}`），
//! 没有谁再交 `machine-round`。拨号那一侧用替身（[`Fake`]：开长 exec 照排好的回 · 一次性 exec 只认 ensure · 装那一步记次数）。
use super::*;
use crate::control::machine_state;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, DuplexStream};

/// 那一台此刻有没有在跑的一条（读常驻流那张表）。
fn running(machine: &str) -> bool {
    lock(table()).contains_key(machine)
}

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

/// 那台拒时自己写好的复制详情（它那边的时刻空位 · 它的机器 · 命令 · 码）。
const THEIR_DETAIL: &str =
    "时刻：{at}\n机器：Linux x86_64 · 后端 their\n命令：history-tail\n码：no_such_file";

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
    /// 同 `Serve`，接上之后照 attach 行带的时区写一帧 `line`（真出口；记录时刻 [`AT_1845Z`]）就断开。
    Clock(String),
}

/// 端到端那一条的记录时刻（2026-11-12 18:45 UTC；洛杉矶那一刻是 10:45）。
const AT_1845Z: &str = "2026-11-12T18:45:00Z";

/// 判据里看各台会话的人的时区（不随跑测试那台的时区变：UTC 的机器上也分得出带没带）。
const LA: &str = "America/Los_Angeles";

fn la() -> crate::common::time::Tz {
    crate::common::time::Tz::iana(LA).expect("认得的 IANA 名")
}

#[derive(Default)]
struct Fake {
    opens: Mutex<VecDeque<Peer>>,
    installs: AtomicUsize,
    words: Mutex<Vec<String>>,
    /// 那台收到的每一行 attach（判据读它带的时区）。
    attaches: Arc<Mutex<Vec<serde_json::Value>>>,
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
        let attaches = Arc::clone(&self.attaches);
        Box::pin(async move {
            let (up_w, up_r) = tokio::io::duplex(64 * 1024);
            let (down_w, down_r) = tokio::io::duplex(256 * 1024);
            tokio::spawn(serve(peer, up_r, down_w, attaches));
            Ok(tokio::io::join(down_r, up_w))
        })
    }

    fn capture(&self, _machine: &str, command: String) -> Boxed<'_, Result<Captured, Problem>> {
        assert!(command.contains("-- --resident-ensure"), "{command}");
        self.words.lock().unwrap().push(command);
        Box::pin(async {
            Ok(Captured {
                ended: cmd_wire::dial::CaptureEnd::Exited { code: 0 },
                stdout: r#"{"pid":7,"replaced":false}"#.into(),
                stderr: String::new(),
            })
        })
    }

    fn install(&self, _machine: &str) -> Boxed<'_, bool> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        self.words.lock().unwrap().push("install".into());
        Box::pin(async { true })
    }

    fn far(&self, _machine: &str) -> bool {
        true
    }
}

async fn serve(
    peer: Peer,
    up: DuplexStream,
    mut down: DuplexStream,
    attaches: Arc<Mutex<Vec<serde_json::Value>>>,
) {
    let mut lines = tokio::io::BufReader::new(up).lines();
    let mut clock = false;
    let (build, talk) = match peer {
        Peer::Absent => {
            let _ = down
                .write_all(b"{\"attach\":\"refused\",\"reason\":\"absent\"}\n")
                .await;
            return;
        }
        Peer::Serve(b) => (b, None),
        Peer::Talk(b, lines) => (b, Some(lines)),
        Peer::Clock(b) => {
            clock = true;
            (b, None)
        }
    };
    let _ = down
        .write_all(format!("{}\n", hello(&build)).as_bytes())
        .await;
    let Ok(Some(attach)) = lines.next_line().await else {
        return;
    };
    let attach: serde_json::Value = serde_json::from_str(&attach).unwrap();
    attaches.lock().unwrap().push(attach.clone());
    let mine = attach["mine"].as_str().unwrap_or_default();
    let reply = serde_json::json!({
        "attach": "ok",
        "relation": deploy_contract::build_relation(&build, mine),
    });
    let _ = down.write_all(format!("{reply}\n").as_bytes()).await;
    if clock {
        // 那台照它收到的 attach 行的时区写推送帧的钟面（同生产：`Tz::of` 读那一格，出口 `FrameExit::stamp`）。
        let tz = crate::common::time::Tz::of(attach["tz"].as_str());
        let rec = crate::agents::record::record(
            "fake".into(),
            "r-clock".into(),
            Some(AT_1845Z.into()),
            crate::agents::record::Body::Retry { text: None },
        );
        let frame = Frame::Line(crate::wire::Line {
            session_id: "sid-clock".into(),
            path: "/p/clock.jsonl".into(),
            seq: 1,
            record: Some(crate::agents::record::BackRecord::from(rec)),
            cwd: None,
            byte_offset: 0,
            rid: None,
        });
        let line = crate::wire::to_line(&crate::wire::FrameExit::stamp(frame, &tz)).unwrap();
        let _ = down.write_all(line.as_bytes()).await;
        return;
    }
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
                "code": "no_such_file", "said": "那台说：没有这个文件", "detail": THEIR_DETAIL,
                "data": {"path": asked["args"]["path"]}})
        } else {
            serde_json::json!({"kind": "reply", "id": asked["id"], "ok": true,
                "data": {"cmd": asked["cmd"], "args": asked["args"], "view": asked["view"], "tz": asked["tz"]}})
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

/// 机器状态推出来的那一路（[`machine_state::changes`]，接之前订）。
type Pushes = tokio::sync::broadcast::Receiver<machine_state::Moved>;

/// 等那台推出成品是 `up` 的那一帧 `changed {machine}`（接上那一刻那一推）。等的是这一推，不读机器状态那张表：
/// 那张表进程里只一份，同一进程里别的判据改机器表时照沙箱机器表把这几台摘掉（[`table_moved_elsewhere`]），读表就等不到了。
async fn up_pushed(changes: &mut Pushes, machine: &str) -> cmd_wire::machine::MachineState {
    use tokio::sync::broadcast::error::RecvError;
    let wait = async {
        loop {
            match changes.recv().await {
                Ok(machine_state::Moved {
                    machine: m,
                    state: p,
                    ..
                }) if m == machine && p.state == cmd_wire::machine::MachineStateKind::Up => {
                    return p
                }
                Ok(_) => {}
                Err(RecvError::Lagged(n)) => {
                    panic!("机器状态那一路积了 {n} 帧没读，那台推的 up 可能就在里面")
                }
                Err(RecvError::Closed) => panic!("机器状态那一路收了"),
            }
        }
    };
    tokio::time::timeout(guard_core::wall::CEILING, wait)
        .await
        .unwrap_or_else(|_| panic!("等不到：{machine} 推一帧成品是 up 的 `changed {{machine}}`"))
}

/// 同一进程里别的判据改机器表的那一下（`machines-set` · `host-keys-forget` 都 `ring` ⇒ `machine_state::table_moved`）：
/// 沙箱那张机器表里没有这几台 ⇒ 它们在机器状态表里的那一行被摘掉。这里造最不巧的那一刻：那台刚推完 `up` 就摘。
fn table_moved_elsewhere(machine: &str) -> tokio::task::JoinHandle<()> {
    let machine = machine.to_string();
    let mut seen = machine_state::changes().subscribe();
    tokio::spawn(async move {
        while let Ok(machine_state::Moved {
            machine: m,
            state: p,
            ..
        }) = seen.recv().await
        {
            if m == machine && p.state == cmd_wire::machine::MachineStateKind::Up {
                machine_state::table_moved_in(&[], &Default::default());
            }
        }
    })
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
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    let mut changes = machine_state::changes().subscribe();
    let neighbour = table_moved_elsewhere(machine);
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    let up = up_pushed(&mut changes, machine).await;
    neighbour.abort();
    assert_eq!(
        up.name, machine,
        "接上那一刻推的那一帧 `changed {{machine}}` 是那台的成品"
    );
    assert_eq!(fake.installs.load(Ordering::SeqCst), 0, "这一版 ⇒ 不装");
    assert_eq!(*fake.words.lock().unwrap(), vec!["attach".to_string()]);
    align_in(&[], &ssh, &quiet(), &la());
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
        o.push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    }
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    up_pushed(&mut changes, machine).await;
    let words = fake.words.lock().unwrap().clone();
    assert_eq!(
        words[..2],
        ["attach".to_string(), "install".to_string()],
        "{words:?}"
    );
    assert!(words[2].ends_with("-- --resident-ensure"), "{words:?}");
    align_in(&[], &ssh, &quiet(), &la());
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
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    up_pushed(&mut changes, machine).await;
    hold(machine);
    assert!(!running(machine), "停了 ⇒ 那一条收掉");
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    assert!(!running(machine), "停着的那一台对齐机器表时也不起");
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    resume_in(machine, &[machine.to_string()], &ssh, &quiet(), &la());
    assert!(running(machine), "起 ⇒ 又有那一条");
    align_in(&[], &ssh, &quiet(), &la());
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
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    up_pushed(&mut changes, machine).await;

    let view = serde_json::json!({"omit": {"record": ["blocks[type=tool_use].input"]}});
    let args = serde_json::json!({"path": "/p/s.jsonl", "n": 2, "deep": {"a": [1, "二", null]}});
    let got = forward(
        machine.to_string(),
        asked("history-tail", args.clone(), view.clone()),
    )
    .await
    .expect("转过去该成")
    .expect("那台回了体");
    assert_eq!(got["cmd"], "history-tail");
    assert_eq!(got["args"], args, "入参整份原样到那台");
    assert_eq!(got["view"], view, "出口声明原样带到那台");
    // 那台回的体一格不多一格不少（假那台把收到的四样装回来）：转回这一跳不往回包里添 · 删东西。
    let mut keys: Vec<&str> = got
        .as_object()
        .expect("回包是那台那一份对象")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["args", "cmd", "tz", "view"], "回包被改了形：{got}");

    let fail = forward(
        machine.to_string(),
        asked(
            "history-tail",
            serde_json::json!({"fail": true, "path": "/p/gone.jsonl"}),
            serde_json::Value::Null,
        ),
    )
    .await
    .expect_err("那台拒了");
    assert_eq!(fail.code.as_deref(), Some("no_such_file"));
    assert_eq!(fail.said, "那台说：没有这个文件");
    // 那台写好的复制详情（它的时刻 · 它的机器 · 命令 · 码）原样带回：这台投出去照抄，不当「原话」再包一层本机的。
    let detail = crate::wire::spec::Settle::settle(fail.clone(), "history-tail").detail;
    assert_eq!(detail, THEIR_DETAIL, "转回那一跳的详情被改写了");
    assert_eq!(
        fail.raw, None,
        "那台的详情不进这台的「原话」：{:?}",
        fail.raw
    );
    assert_eq!(
        fail.data,
        Some(serde_json::json!({"path": "/p/gone.jsonl"})),
        "那台拒时按码带的那一格原样交回"
    );
    align_in(&[], &ssh, &quiet(), &la());
}

/// 转过去的那一问带着发起方看的人的时区：那台照它写回包里的「几点」（不带 ⇒ 那台按 UTC 写，文件窗口的时间就错到 UTC）。
/// 带出口声明的与不带的两形都带。
#[tokio::test]
async fn a_forwarded_call_carries_the_viewers_tz_to_that_machine() {
    let _one = one_at_a_time().await;
    let machine = "feeds-forward-tz";
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    up_pushed(&mut changes, machine).await;

    let with_tz = |view: serde_json::Value| {
        let mut req = asked(
            "history-tail",
            serde_json::json!({"path": "/p/s.jsonl"}),
            view,
        );
        req.tz = crate::common::time::Tz::iana("America/Los_Angeles").expect("认得的 IANA 名");
        req
    };
    for view in [
        serde_json::Value::Null,
        serde_json::json!({"omit": {"record": ["blocks[type=tool_use].input"]}}),
    ] {
        let got = forward(machine.to_string(), with_tz(view.clone()))
            .await
            .expect("转过去该成")
            .expect("那台回了体");
        assert_eq!(got["tz"], "America/Los_Angeles", "view={view}");
    }
    align_in(&[], &ssh, &quiet(), &la());
}

/// ★ 接那台的常驻流时 attach 行带着看的人的时区名（生产 ＝ 这台的，[`viewer`]）；
/// 不带 ⇒ 那台照 UTC 写推送帧的钟面，本机原样转给壳，界面上那台会话的每条消息都是 UTC 的钟面。
#[tokio::test]
async fn attaching_a_machine_carries_the_viewers_tz_name() {
    let _one = one_at_a_time().await;
    let machine = "feeds-attach-tz";
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    up_pushed(&mut changes, machine).await;
    align_in(&[], &ssh, &quiet(), &la());
    let attaches = fake.attaches.lock().unwrap().clone();
    assert!(!attaches.is_empty(), "那台收到过 attach 行");
    for a in &attaches {
        assert_eq!(a["tz"].as_str(), Some(LA), "{a}");
    }
}

/// ★ 端到端：那台照它收到的时区写 `line` 帧，本机原样转出 ⇒ 转出去那一行的 `timeText` 是看的人钟上的那一刻
/// （看的人在洛杉矶：记录 18:45Z ⇒ `10:45`，不是 UTC 的 `18:45`）。
#[tokio::test]
async fn a_relayed_line_from_a_machine_reads_on_the_viewers_clock() {
    use cmd_wire::frame::Relayed;
    let _one = one_at_a_time().await;
    let machine = "feeds-clock";
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Clock(crate::common::id::BUILD_ID.to_string()));
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    align_in(&[machine.to_string()], &ssh, &relay, &la());
    let line_of = |g: &[Frame]| {
        g.iter().find_map(|f| match f {
            Frame::Relayed(Relayed::Line { machine: m, line })
                if m == machine && line.contains("r-clock") =>
            {
                Some(line.clone())
            }
            _ => None,
        })
    };
    until("那台写的那一帧 line 转出来", || {
        line_of(&got.lock().unwrap()).is_some()
    })
    .await;
    align_in(&[], &ssh, &quiet(), &la());
    let line = line_of(&got.lock().unwrap()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["record"]["timeText"].as_str(), Some("10:45"), "{line}");
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

/// 替身那台照答的那一条有类型的命令：线上形照 `machine-interrupts` 的那几格（真命令住帧面宿主，控制面这一层够不到；
/// 这里只验 `call` 那条路怎么带入参 · 收回包）。
struct MachineInterrupts;

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MachineInterruptsReq {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app_exit: Option<bool>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MachineInterruptsReply {
    relayed_sessions: Option<u32>,
    live_streams: u32,
}

crate::wire::typed::codes! {
    enum MachineInterruptsErr {}
}

impl crate::wire::typed::Command for MachineInterrupts {
    const NAME: &'static str = "machine-interrupts";
    type Req = MachineInterruptsReq;
    type Reply = MachineInterruptsReply;
    type Err = MachineInterruptsErr;
}

/// 本机后端自己问那台一条有类型的命令（`call`，并好再答的那几条用）：经同一条常驻流，入参照类型带过去、回包照类型收；
/// 没有那一条 ⇒ 同 `forward` 那个码。
#[tokio::test]
async fn a_typed_call_from_this_backend_goes_through_the_same_stream() {
    let _one = one_at_a_time().await;
    let machine = "feeds-call";
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    up_pushed(&mut changes, machine).await;
    let within =
        crate::platform::child::Until::after(crate::platform::child::Deadline::millis(30_000));
    let exit = MachineInterruptsReq {
        app_exit: Some(true),
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
    align_in(&[], &ssh, &quiet(), &la());
    let fail = call::<MachineInterrupts>("feeds-call-nowhere", &exit, within)
        .await
        .expect_err("没有那一条就不发");
    assert_eq!(fail.code.as_deref(), Some(UNREACHABLE));
}

/// 转那台的 hello 之前，做不到的每一项置灰那一句换成称呼那台的名字（那台自己写的是称呼它自己的）；别的格照旧；
/// 没有做不到的 · 解不开 ⇒ 字节不动。两侧异源：右边照码与那台的名字现取。
#[test]
fn a_relayed_hello_has_each_gap_said_naming_that_machine() {
    let theirs = format!(
        r#"{{"kind":"hello","build_id":"b","commands":["kill","files-chmod"],"unavailable":[{{"command":"kill","code":"no_tmux","said":"{}"}},{{"command":"files-chmod","code":"no_unix_mode","said":"x"}}]}}"#,
        copy_core::unavailable("no_tmux", &copy_core::local_machine())
    );
    let out: serde_json::Value = serde_json::from_str(&name_gaps(&theirs, "devbox")).unwrap();
    let saids: Vec<&str> = out["unavailable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["said"].as_str().unwrap())
        .collect();
    assert_eq!(
        saids,
        [
            copy_core::unavailable("no_tmux", "devbox"),
            copy_core::unavailable("no_unix_mode", "devbox"),
        ]
    );
    assert_eq!(out["commands"], serde_json::json!(["kill", "files-chmod"]));
    assert_eq!(out["unavailable"][1]["code"], "no_unix_mode");
    let plain = hello("b");
    assert_eq!(
        name_gaps(&plain, "devbox"),
        plain,
        "没有做不到的 ⇒ 字节不动"
    );
    assert_eq!(name_gaps("不是 JSON", "devbox"), "不是 JSON");
}

/// 那台 `resync` 的回包转回来之前，同 hello：做不到的每一项置灰那一句按那台的名字重写；别的命令的回包不碰。
#[test]
fn a_forwarded_resync_reply_has_each_gap_said_naming_that_machine() {
    let theirs = || {
        serde_json::json!({"added": 0, "watchers": 1, "unavailable": [
            {"command": "kill", "code": "no_tmux", "said": copy_core::unavailable("no_tmux", &copy_core::local_machine())}
        ]})
    };
    let mut reply = Some(theirs());
    name_reply(resident_client::client::RESYNC_OP, &mut reply, "devbox");
    let reply = reply.unwrap();
    assert_eq!(
        reply["unavailable"][0]["said"],
        copy_core::unavailable("no_tmux", "devbox")
    );
    assert_eq!(reply["watchers"], 1);
    let mut other = Some(theirs());
    name_reply("ping", &mut other, "devbox");
    assert_eq!(other, Some(theirs()), "别的命令的回包不碰");
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
    fake.opens.lock().unwrap().push_back(Peer::Talk(
        crate::common::id::BUILD_ID.to_string(),
        vec![said.clone()],
    ));
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    align_in(&[machine.to_string()], &ssh, &relay, &la());
    let ended = |g: &[Frame]| {
        g.iter()
            .any(|f| matches!(f, Frame::Relayed(Relayed::Ended { machine: m }) if m == machine))
    };
    until("那台这一条断了 ⇒ 转一帧 ended", || {
        ended(&got.lock().unwrap())
    })
    .await;
    align_in(&[], &ssh, &quiet(), &la());
    let seen: Vec<Relayed> = got
        .lock()
        .unwrap()
        .iter()
        .filter_map(|f| match f {
            // 会话帧后面紧跟的那一格成品另有判据（[`a_machines_session_line_is_followed_by_its_seat_and_a_live_card_goes_out_folded`]）。
            Frame::Relayed(Relayed::Seats { .. }) => None,
            Frame::Relayed(r) => Some(r.clone()),
            // 接上 · 断了那一声组变了（[`a_machine_attaching_and_dropping_each_pushes_groups_changed`] 看它）。
            Frame::Changed(cmd_wire::frame::Changed::Groups {}) => None,
            other => panic!("只转 relayed（外加组变了那一声）：{other:?}"),
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
                line: hello(crate::common::id::BUILD_ID),
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
    while let Ok(machine_state::Moved {
        machine: m,
        state: p,
        ..
    }) = changes.try_recv()
    {
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
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    let hello_now = || {
        attached_hellos().into_iter().any(|f| {
            matches!(&f, Frame::Relayed(Relayed::Line { machine: m, line }) if m == machine && *line == hello(crate::common::id::BUILD_ID))
        })
    };
    align_in(&[machine.to_string()], &ssh, &relay, &la());
    until("那台接上了、它的 hello 转出去了", || {
        got.lock()
            .unwrap()
            .iter()
            .any(|f| matches!(f, Frame::Relayed(Relayed::Line { machine: m, .. }) if m == machine))
    })
    .await;
    assert!(hello_now(), "那台接着的时候新流拿不到它的 hello");
    align_in(&[], &ssh, &quiet(), &la());
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
    assert_eq!(desk_of(machine).watched(), set(&["s1", "s2", "s3"]));
    assert_eq!(union_of(machine).1, set(&["k1"]));
    drop(a);
    assert_eq!(
        desk_of(machine).watched(),
        set(&["s2", "s3"]),
        "那条流走了，它报的那几条还在那台的名单里"
    );
    drop(b);
    assert!(desk_of(machine).watched().is_empty());
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
        v["title"] = serde_json::json!(sid);
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
    fake.opens.lock().unwrap().push_back(Peer::Talk(
        crate::common::id::BUILD_ID.to_string(),
        talk.clone(),
    ));
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    align_in(&[machine.to_string()], &ssh, &relay, &la());
    until("那台这一条断了 ⇒ 转一帧 ended", || {
        got.lock()
            .unwrap()
            .iter()
            .any(|f| matches!(f, Frame::Relayed(Relayed::Ended { machine: m }) if m == machine))
    })
    .await;
    align_in(&[], &ssh, &quiet(), &la());
    let seen: Vec<Relayed> = got
        .lock()
        .unwrap()
        .iter()
        .filter_map(|f| match f {
            // 每一行会话帧后面紧跟的那一格成品另有判据（[`a_lost_link_says_each_open_seat_unclear_voids_listed_and_empties_the_board`]）。
            Frame::Relayed(Relayed::Seats { .. }) => None,
            Frame::Relayed(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    let end = seen
        .iter()
        .position(|r| matches!(r, Relayed::Ended { .. }))
        .unwrap();
    let unclear = face_core::book::unseen(crate::common::cells::Words(copy_core::offline(machine)));
    let said_left = |sid: &str| Relayed::Line {
        machine: machine.into(),
        line: serde_json::to_string(&cmd_wire::frame::Frame::<serde_json::Value>::SessionLeft(
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

/// ★ 先红的那一条（断开后组那一半没人告诉两端）：那台这一条接上 · 断了，本机后端各从转出口（watcher 出方向，不可丢）推一帧 `changed {groups}`，
/// 两端照它重问 `groups-read`（断了那台那一半照上次所知出）。接上那一帧在 hello 之后；断了那一帧排在 `ended` 之后（重问时那台已答不上）。
#[tokio::test]
async fn a_machine_attaching_and_dropping_each_pushes_groups_changed() {
    use cmd_wire::frame::{Changed, Relayed};
    let machine = "feeds-groups";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Talk(crate::common::id::BUILD_ID.to_string(), vec![]));
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    align_in(&[machine.to_string()], &ssh, &relay, &la());
    let is_groups = |f: &Frame| matches!(f, Frame::Changed(Changed::Groups {}));
    let is_end =
        |f: &Frame| matches!(f, Frame::Relayed(Relayed::Ended { machine: m }) if m == machine);
    until(
        "那台这一条断了 ⇒ ended 之后推一帧 changed {groups}",
        || {
            let g = got.lock().unwrap();
            g.iter()
                .position(is_end)
                .is_some_and(|i| g[i..].iter().any(is_groups))
        },
    )
    .await;
    align_in(&[], &ssh, &quiet(), &la());
    let tags: Vec<&str> = got
        .lock()
        .unwrap()
        .iter()
        .map(|f| match f {
            f if is_groups(f) => "groups",
            f if is_end(f) => "ended",
            Frame::Relayed(Relayed::Line { .. }) => "line",
            _ => "other",
        })
        .collect();
    let end = tags.iter().position(|t| *t == "ended").unwrap();
    assert_eq!(
        tags[..=end + 1],
        ["line", "groups", "ended", "groups"],
        "接上（hello 之后）· 断了（ended 之后）各推一帧：{tags:?}"
    );
    assert_eq!(
        tags.iter().filter(|t| **t == "groups").count(),
        2,
        "接不上的那几轮（不在 ⇒ 没接上过）不推：{tags:?}"
    );
}

/// 一台的成品（只填 `wake_in` 读的那几格）。
fn wake_row(
    kind: cmd_wire::machine::MachineStateKind,
    said: &str,
) -> cmd_wire::machine::MachineState {
    cmd_wire::machine::MachineState {
        name: "nas".into(),
        state: kind,
        dot: copy_core::machine::dot(kind),
        linked: deploy::machine::linked(kind),
        offline: false,
        reason: None,
        stage: None,
        version: None,
        os: None,
        word: String::new(),
        said: said.into(),
        offline_said: "nas 离线".into(),
        held_said: "nas 离线 · 连上后补交".into(),
        withdraw_held: "nas w-withdraw".into(),
        rewind_held: "nas w-rewind".into(),
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

/// ★ 「停」先查表：表里没有 · 本机 ⇒ unknown_machine（同「重新连接」那一句），那条流不 hold；表里有（停用的也算）⇒ 先 hold 再停。
#[test]
fn stopping_a_machine_not_in_the_table_is_refused_before_its_stream_is_held() {
    use cmd_wire::machine::MachineStateKind as K;
    let held = std::cell::RefCell::new(Vec::<String>::new());
    let hold = |m: &str| held.borrow_mut().push(m.to_string());
    let table =
        std::collections::BTreeMap::from([("nas".to_string(), wake_row(K::Disabled, "停用着"))]);
    for ghost in ["ghost", cmd_wire::machine::LOCAL] {
        match stop_in(ghost, &table, hold) {
            Err(MachineStopErr::UnknownMachine(said)) => assert_eq!(
                said,
                copy_core::copy_text("beMachineWake.refuse.unknown", &[("machine", ghost)])
            ),
            other => panic!("{ghost}: {other:?}"),
        }
    }
    assert!(held.borrow().is_empty(), "拒了就没 hold");
    stop_in("nas", &table, hold).expect("表里有就停");
    assert_eq!(*held.borrow(), ["nas"]);
}

// ── 各台的会话簿 · 活卡板由本机后端折好、推成品（壳只换缓存、原样转）──

/// 那张卡的第 `n` 帧（0 ＝ 整卡一块空正文；之后每帧那一段多一个 `x`）。
fn card(n: u64) -> String {
    if n == 0 {
        serde_json::json!({"kind":"live_card","sid":"s-card","card":0,"n":0,"op":"open","rid":"m",
            "phase":"streaming","clipped":false,"blocks":[{"kind":"text","md":[]}]})
    } else {
        serde_json::json!({"kind":"live_card","sid":"s-card","card":0,"n":n,"op":"text","i":0,"keep":0,
            "tail":[{"t":"p","k":["x".repeat(n as usize)]}]})
    }
    .to_string()
}

/// ★ 先红的那一条：那台流上一行会话帧照原样转，紧跟一格 `seats`（那一条此刻整条）；一帧 `live_card` 不原样转，转的是折好的 `live`。
#[tokio::test]
async fn a_machines_session_line_is_followed_by_its_seat_and_a_live_card_goes_out_folded() {
    use crate::wire::Life;
    use cmd_wire::frame::{Relayed, Seat};
    let machine = "feeds-seats";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    let added = told("session_added", "s-1", Life::live(None, None));
    fake.opens.lock().unwrap().push_back(Peer::Talk(
        crate::common::id::BUILD_ID.to_string(),
        vec![added.clone(), card(0), card(1)],
    ));
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    align_in(&[machine.to_string()], &ssh, &relay, &la());
    until("那台这一条断了 ⇒ 转一帧 ended", || {
        got.lock()
            .unwrap()
            .iter()
            .any(|f| matches!(f, Frame::Relayed(Relayed::Ended { machine: m }) if m == machine))
    })
    .await;
    align_in(&[], &ssh, &quiet(), &la());
    let seen: Vec<Relayed> = got
        .lock()
        .unwrap()
        .iter()
        .filter_map(|f| match f {
            Frame::Relayed(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    let at = seen
        .iter()
        .position(|r| matches!(r, Relayed::Line { line, .. } if *line == added))
        .expect("会话帧照原样转");
    let Relayed::Seats { machine: m, seats } = &seen[at + 1] else {
        panic!("会话帧后面没紧跟一格 seats：{seen:#?}")
    };
    assert_eq!(m, machine);
    assert!(!seats.whole && !seats.listed, "{seats:?}");
    assert!(
        matches!(&seats.seats[..], [s] if s.sid == "s-1" && matches!(&s.seat, Some(Seat::Live { meta, .. }) if meta.sid == "s-1")),
        "{seats:?}"
    );
    assert!(
        !seen.iter().any(|r| matches!(r, Relayed::Line { line, .. }
                if cmd_wire::frame::kind_of(line).ok().as_deref() == Some("live_card"))),
        "live_card 原样那一行转出去了：{seen:#?}"
    );
    let live: Vec<&cmd_wire::frame::LiveNow> = seen
        .iter()
        .filter_map(|r| match r {
            Relayed::Live { live, .. } => Some(live),
            _ => None,
        })
        .collect();
    assert_eq!(live.len(), 2, "开卡 · 一段字各一格：{seen:#?}");
    assert!(matches!(
        live[0].change,
        cmd_wire::live::LiveChange::Whole { .. }
    ));
    assert!(matches!(
        live[1].change,
        cmd_wire::live::LiveChange::Tail { .. }
    ));
    assert_eq!(
        live[1].whole.cards[0].body.len(),
        1,
        "整份是那一处此刻的样子"
    );
}

/// 那台这一条断的那一刻：说不清的每一条照旧转、各紧跟一格 `seats`（那一条 `left`，清单报完了作废）；板清空 ——
/// 下一条连接上同一张卡号的增量不接旧卡；簿里的条目还在。
#[test]
fn a_lost_link_says_each_open_seat_unclear_voids_listed_and_empties_the_board() {
    use crate::wire::Life;
    use cmd_wire::frame::{Relayed, Seat};
    let m = "feeds-lost";
    let mut f = Folds::default();
    for l in [
        told("session_added", "s-1", Life::live(None, None)),
        r#"{"kind":"sessions_replayed"}"#.to_string(),
        card(0),
    ] {
        fold_line(&mut f, m, &l);
    }
    assert!(f.book.listed);
    let out = ended_now(&mut f, m);
    assert!(
        matches!(&out[..], [Relayed::Line { line, .. }, Relayed::Seats { seats, .. }]
            if line.contains("session_left")
                && !seats.listed
                && matches!(&seats.seats[..], [s] if matches!(&s.seat, Some(Seat::Left { .. })))),
        "{out:#?}"
    );
    assert!(f.book.left.contains_key("s-1"), "簿里的条目随断线丢了");
    let Relayed::Seats { seats, .. } = &out[1] else {
        unreachable!()
    };
    assert_eq!(seats.seats[0], f.book.seat_now("s-1"));
    let (next, _) = fold_line(&mut f, m, &card(1));
    assert!(next.is_empty(), "下一条连接的增量接到了旧卡上：{next:#?}");
}

/// ★ 那台这一条断的那一刻，未读是那条连接上数的 ⇒ 整份作废：已结束、不再说说不清的那一条也补一格 `seats`（不带未读），壳那一格跟着摘。
#[test]
fn a_lost_link_voids_the_unread_of_every_session_even_the_ended_ones() {
    use crate::wire::Life;
    use cmd_wire::frame::Relayed;
    let m = "feeds-lost-unread";
    let mut f = Folds::default();
    for l in [
        told("session_added", "s-1", Life::live(None, None)),
        told("session_left", "s-2", Life::Ended),
        r#"{"kind":"session_unread","sid":"s-1","said":"3"}"#.to_string(),
        r#"{"kind":"session_unread","sid":"s-2","said":"5"}"#.to_string(),
    ] {
        fold_line(&mut f, m, &l);
    }
    let out = ended_now(&mut f, m);
    let seated: Vec<(&str, bool)> = out
        .iter()
        .filter_map(|r| match r {
            Relayed::Seats { seats, .. } => Some(seats.seats.iter()),
            _ => None,
        })
        .flatten()
        .map(|s| (s.sid.as_str(), s.unread.is_some()))
        .collect();
    assert_eq!(seated, [("s-1", false), ("s-2", false)], "{out:#?}");
    assert!(f.book.unread.is_empty());
}

/// 新连上来的流：每台先 hello、紧跟一格 `seats` 带 `whole`（那台整本）。
#[tokio::test]
async fn a_new_stream_gets_each_attached_machines_whole_book_after_its_hello() {
    use cmd_wire::frame::Relayed;
    let machine = "feeds-whole";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    let ssh: Arc<dyn Ssh> = fake.clone();
    let (relay, got) = caught();
    align_in(&[machine.to_string()], &ssh, &relay, &la());
    until("那台接上了", || {
        got.lock()
            .unwrap()
            .iter()
            .any(|f| matches!(f, Frame::Relayed(Relayed::Line { machine: m, .. }) if m == machine))
    })
    .await;
    // 那台那一本里有一条（照那台流上一行折进去，与转出去的同一处）。
    let want = {
        let mut all = lock(folds());
        let f = all.get_mut(machine).expect("起一台就有它那一份");
        fold_line(
            f,
            machine,
            &told("session_added", "s-w", crate::wire::Life::live(None, None)),
        );
        f.book.seat_now("s-w")
    };
    let now: Vec<Relayed> = attached_hellos()
        .into_iter()
        .filter_map(|f| match f {
            Frame::Relayed(r) if r.machine() == machine => Some(r),
            _ => None,
        })
        .collect();
    align_in(&[], &ssh, &quiet(), &la());
    assert!(
        matches!(&now[..], [Relayed::Line { .. }, Relayed::Seats { seats, .. }] if seats.whole && seats.seats == [want.clone()]),
        "{now:#?}"
    );
}

// ── 对齐起收成对 · 停失败交那台的原话 ──

/// ★ 对齐那一下起与收成对：机器表里还是那几台 ⇒ 在跑的那一条不动、不再起第二回（同一条、只接过一次）；
/// 表里没了 ⇒ 那一条收掉，它的循环放手（没人再握着那一条）。
#[tokio::test]
async fn aligning_starts_a_machine_once_and_closes_what_it_started() {
    let machine = "feeds-pair";
    let _one = one_at_a_time().await;
    let fake = Arc::new(Fake::default());
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::common::id::BUILD_ID.to_string()));
    let mut changes = machine_state::changes().subscribe();
    let ssh: Arc<dyn Ssh> = fake.clone();
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    up_pushed(&mut changes, machine).await;
    let first = lock(table()).get(machine).cloned().expect("起了那一条");
    align_in(&[machine.to_string()], &ssh, &quiet(), &la());
    let again = lock(table()).get(machine).cloned().expect("还在跑");
    assert!(Arc::ptr_eq(&first, &again), "表没变又起了第二回");
    drop(again);
    let attaches = || {
        fake.words
            .lock()
            .unwrap()
            .iter()
            .filter(|w| *w == "attach")
            .count()
    };
    assert_eq!(attaches(), 1, "表没变又接了一次");
    align_in(&[], &ssh, &quiet(), &la());
    assert!(!running(machine), "表里没了 ⇒ 那一条收掉");
    until("收掉的那一条，循环放手", || {
        Arc::strong_count(&first) == 1
    })
    .await;
}

/// 一次性 exec 只认 `--resident-stop`、照排好的那一样回（停那台常驻那一下的替身）。
struct StopSsh(Mutex<Option<Result<Captured, Problem>>>);

impl Ssh for StopSsh {
    fn open(&self, _: &str, command: String, _: Progress) -> Boxed<'_, Result<Pipe, Problem>> {
        panic!("停那一下不开长 exec：{command}")
    }
    fn capture(&self, _machine: &str, command: String) -> Boxed<'_, Result<Captured, Problem>> {
        assert!(command.ends_with("-- --resident-stop"), "{command}");
        let got = self.0.lock().unwrap().take().expect("只跑一次");
        Box::pin(async move { got })
    }
    fn install(&self, _: &str) -> Boxed<'_, bool> {
        panic!("停那一下不装")
    }
    fn far(&self, _: &str) -> bool {
        false
    }
}

/// ★ 停那台常驻没成 ⇒ `stop_failed`，那一句与原话都是那台说的、不改写：那台退非零、stderr 是它的失败那一行 ⇒ 它的 `said` · `detail`；
/// 那一跳没够着 ⇒ 拨号那一层的那一句与原话。停成了 ⇒ 那台那一行的结局。
#[tokio::test]
async fn a_failed_stop_hands_back_that_machines_own_words() {
    let run = |r: Result<Captured, Problem>| async move {
        stop_with(&StopSsh(Mutex::new(Some(r))), "feeds-stop").await
    };
    let theirs = serde_json::json!({"code": "failed", "said": "那台说：常驻停不下来", "detail": "kill 4242: EPERM"});
    let e = run(Ok(Captured {
        ended: cmd_wire::dial::CaptureEnd::Exited { code: 1 },
        stdout: String::new(),
        stderr: theirs.to_string(),
    }))
    .await
    .expect_err("那台说停没成");
    assert_eq!(e.code.as_deref(), Some("stop_failed"));
    assert_eq!(e.said, "那台说：常驻停不下来");
    assert_eq!(e.raw.as_deref(), Some("kill 4242: EPERM"));

    let e = run(Err(Problem::new(
        "够不到 feeds-stop",
        "ssh: connect refused",
    )))
    .await
    .expect_err("那一跳没够着");
    assert_eq!(e.code.as_deref(), Some("stop_failed"));
    assert_eq!(e.said, "够不到 feeds-stop");
    assert_eq!(e.raw.as_deref(), Some("ssh: connect refused"));

    let ok = run(Ok(Captured {
        ended: cmd_wire::dial::CaptureEnd::Exited { code: 0 },
        stdout: r#"{"stopped":"stopped","pid":4242,"said":"那台·已停"}"#.into(),
        stderr: String::new(),
    }))
    .await
    .expect("停成了");
    assert_eq!(
        (ok.stopped.as_str(), ok.pid, ok.said.as_str()),
        ("stopped", Some(4242), "那台·已停"),
        "那台写好的那一句原样交上去"
    );
}
