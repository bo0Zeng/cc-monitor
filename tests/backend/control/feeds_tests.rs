//! 各台的常驻流归本机后端：机器表里要连的那几台各起一条（`control/feeds.rs`），循环交的那一态直接进机器状态（推 `changed {machine}`），
//! 没有谁再交 `machine-round`。拨号那一侧用替身（[`Fake`]：开长 exec 照排好的回 · 一次性 exec 只认 ensure · 装那一步记次数）。
use super::*;
use crate::control::machine_state;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, DuplexStream};

/// 那台握手那一行（帧表照本侧编进来的那一份，`build` 是那台自报的身份）。
fn hello(build: &str) -> String {
    let frames: serde_json::Map<String, serde_json::Value> = cmd_wire::frame_shapes::FRAMES
        .iter()
        .map(|(k, h)| (k.to_string(), serde_json::json!(h)))
        .collect();
    serde_json::json!({
        "kind": "hello", "build_id": build, "host_arch": "x86_64",
        "commands": ["ping"], "unavailable": [], "shapes": {}, "frames": frames,
    })
    .to_string()
}

/// 开成之后那台怎么回 attach。
enum Peer {
    /// 回 hello、照 attach 行带的那一版比新旧。
    Serve(String),
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
}

async fn serve(peer: Peer, up: DuplexStream, mut down: DuplexStream) {
    let mut lines = tokio::io::BufReader::new(up).lines();
    let build = match peer {
        Peer::Absent => {
            let _ = down
                .write_all(b"{\"attach\":\"refused\",\"reason\":\"absent\"}\n")
                .await;
            return;
        }
        Peer::Serve(b) => b,
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
    // 之后照答（问活）：连接一直留着。
    while let Ok(Some(l)) = lines.next_line().await {
        let asked: serde_json::Value = serde_json::from_str(&l).unwrap();
        let reply = serde_json::json!({"kind": "reply", "id": asked["id"], "ok": true});
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
    let all = machine_state::read_in(&[(machine.to_string(), true)]);
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
    align_in(&[machine.to_string()], &ssh);
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
    align_in(&[], &ssh);
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
    align_in(&[machine.to_string()], &ssh);
    until("那台的成品是 up", || state_of(machine) == "up").await;
    let words = fake.words.lock().unwrap().clone();
    assert_eq!(
        words[..2],
        ["attach".to_string(), "install".to_string()],
        "{words:?}"
    );
    assert!(words[2].ends_with("-- --resident-ensure"), "{words:?}");
    align_in(&[], &ssh);
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
    align_in(&[machine.to_string()], &ssh);
    until("先接上", || state_of(machine) == "up").await;
    hold(machine);
    assert!(!running(machine), "停了 ⇒ 那一条收掉");
    align_in(&[machine.to_string()], &ssh);
    assert!(!running(machine), "停着的那一台对齐机器表时也不起");
    fake.opens
        .lock()
        .unwrap()
        .push_back(Peer::Serve(crate::BUILD_ID.to_string()));
    resume_in(machine, &[machine.to_string()], &ssh);
    assert!(running(machine), "起 ⇒ 又有那一条");
    align_in(&[], &ssh);
}
