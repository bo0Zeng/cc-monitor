//! 中转吃不下的两种时候（fd 用尽 · 在途顶满）：来的连接拿到 503（不是干等到下游自己超时），这一阵只说一次，缓过来说一次、照常转发。
//! 要把整个进程的 fd 占满、要数中转往 stderr 说了几句 ⇒ 在自己起的子进程里跑，父进程看它的 stdout 与 stderr。

use super::observe_retry_tests::{
    shoot, spawn_judging_upstream, spawn_relay_over, sse_200, temp_dir, upstreams_at,
};
use crate::accounts::upstream_select::{table::RoutingTable, Accounts};
use crate::relay::test_support::INFLIGHT_CONNECTIONS;

const CHILD_MARK: &str = "CCM_RELAY_EXHAUST_CHILD";
const TARGET: &str = "/t/claude-code/q/v1/messages";
const HEADERS: &str = "authorization: Bearer from-the-agent\r\nx-claude-code-session-id: s-1\r\n";

/// 软上限压到这么低再占满（子进程此刻开着的远少于它；占满只要几百次 `open`）。
#[cfg(unix)]
const SOFT_FOR_HOGGING: u64 = 256;

/// 把软上限压到 [`SOFT_FOR_HOGGING`]，再用 `/dev/null` 占到一个都不剩。回占着的那些。
#[cfg(unix)]
fn hog_every_fd() -> Vec<std::fs::File> {
    let mut r = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: 只读、只动这个子进程自己的软上限。
    unsafe {
        assert_eq!(libc::getrlimit(libc::RLIMIT_NOFILE, &mut r), 0);
        r.rlim_cur = SOFT_FOR_HOGGING as libc::rlim_t;
        assert_eq!(libc::setrlimit(libc::RLIMIT_NOFILE, &r), 0);
    }
    let mut hogs = Vec::new();
    hog_the_rest(&mut hogs);
    hogs
}

/// 再占一遍：这之间别的线程放出来的也占上。
#[cfg(unix)]
fn hog_the_rest(hogs: &mut Vec<std::fs::File>) {
    loop {
        match std::fs::File::open("/dev/null") {
            Ok(f) => hogs.push(f),
            Err(e) if e.raw_os_error() == Some(libc::EMFILE) => return,
            Err(e) => panic!("占 fd 时出了别的错：{e}"),
        }
    }
}

/// 子进程那一半（判据带着 [`CHILD_MARK`] 重起自己时走这里）。每一步印一行 `EXHAUST <步> <回包状态行>`。
#[cfg(unix)]
fn as_child_out_of_fds() {
    let (up, _got) = spawn_judging_upstream(|_| sse_200(""));
    let d = temp_dir("exhaust");
    let relay = spawn_relay_over(Accounts::new(
        RoutingTable::build(std::iter::empty()),
        upstreams_at(up),
        crate::platform::fs::Own::issue_data_at(&d),
    ));
    let status = |resp: &str| resp.lines().next().unwrap_or("（没有回包）").to_string();
    // 占满之前不先打一发：那一发的连接线程收尾（记账写盘）晚于占满，会在占满之后放出 fd，量的就不是用尽了。
    let mut hogs = hog_every_fd();
    // 每一发之前放出一个：给打压这一侧的 socket（中转那一侧仍然一个都没有）。
    for step in ["full-1", "full-2"] {
        hog_the_rest(&mut hogs);
        hogs.pop();
        println!(
            "EXHAUST {step} {}",
            status(&shoot(relay, TARGET, HEADERS, "{}"))
        );
    }
    drop(hogs);
    println!(
        "EXHAUST after {}",
        status(&shoot(relay, TARGET, HEADERS, "{}"))
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 子进程那一半：在途数顶到上限连打三发，在上限边上放进一条又顶满，放回 0 再打一发。
fn as_child_over_the_cap() {
    let (up, _got) = spawn_judging_upstream(|_| sse_200(""));
    let d = temp_dir("over-cap");
    let accounts = Accounts::new(
        RoutingTable::build(std::iter::empty()),
        upstreams_at(up),
        crate::platform::fs::Own::issue_data_at(&d),
    );
    let relay = std::sync::Arc::new(comms_outward::Relay::new(
        std::sync::Arc::new(accounts),
        crate::relay::test_support::test_keys(),
        comms_outward::TeeSink::to_port(std::sync::Arc::new(Mute)),
        crate::relay::open_for(std::sync::Arc::new(|_| None)),
        crate::relay::test_support::DOWNSTREAM_DEADLINE,
        crate::relay::test_support::UPSTREAM_DEADLINE,
        crate::relay::test_support::INFLIGHT_BODY_CAP,
    ));
    let listener = crate::relay::test_support::listen(0).expect("listen");
    let addr = listener.local_addr().expect("addr");
    let inflight = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(
        crate::relay::test_support::INFLIGHT_CONNECTIONS,
    ));
    let counted = std::sync::Arc::clone(&inflight);
    std::thread::spawn(move || crate::relay::test_support::serve(listener, relay, counted));
    let status = |resp: &str| resp.lines().next().unwrap_or("（没有回包）").to_string();
    let shot = |step: &str| {
        println!(
            "EXHAUST {step} {}",
            status(&shoot(addr, TARGET, HEADERS, "{}"))
        )
    };
    use std::sync::atomic::Ordering::SeqCst;
    for step in ["full-1", "full-2", "full-3"] {
        shot(step);
    }
    // 顶在上限边上来回（放进一条 · 又顶满）：还是同一阵，不再说一遍。
    inflight.store(INFLIGHT_CONNECTIONS - 1, SeqCst);
    shot("edge");
    // 那一条的连接线程收工时减一次：等它减完再顶满，不然那一减落在顶满之后。
    let until = std::time::Instant::now() + guard_core::wall::CEILING;
    while inflight.load(SeqCst) != INFLIGHT_CONNECTIONS - 1 {
        assert!(
            std::time::Instant::now() < until,
            "边上那一条的连接线程没收工"
        );
        std::thread::yield_now();
    }
    inflight.store(INFLIGHT_CONNECTIONS, SeqCst);
    shot("full-4");
    inflight.store(0, SeqCst);
    println!(
        "EXHAUST after {}",
        status(&shoot(addr, TARGET, HEADERS, "{}"))
    );
    let _ = std::fs::remove_dir_all(&d);
}

struct Mute;
impl comms_outward::TapPort for Mute {
    fn offer(&self, _ev: comms_outward::TapEvent) -> bool {
        false
    }
}

/// 在途顶满的那一阵：每一条都拿到 503，`[relay]` 只说一次顶满、退到上限四分之三以下说一次（不是每拒一条印一行 ——
/// 压测 400 路时那一行 15 秒印了一万多遍，全进常驻后端的日志文件；也不是在上限边上来回一次说一遍 —— 1200 路时那样印了 250 行）；
/// 放回之后照常 200。
#[test]
fn over_the_cap_every_connection_gets_503_and_the_relay_says_so_once() {
    // 带着记号重起的那一趟是子进程（只打、只印）；没带的这一趟是判据（起子进程、看它印的）。
    match std::env::var_os(CHILD_MARK) {
        Some(_) => as_child_over_the_cap(),
        None => judge_over_the_cap(),
    }
}

fn judge_over_the_cap() {
    let (steps, stderr) = run_child(
        "cross_relay::exhaust_tests::over_the_cap_every_connection_gets_503_and_the_relay_says_so_once",
    );
    assert_eq!(
        steps,
        [
            ("full-1".to_string(), "503".to_string()),
            ("full-2".to_string(), "503".to_string()),
            ("full-3".to_string(), "503".to_string()),
            ("edge".to_string(), "200".to_string()),
            ("full-4".to_string(), "503".to_string()),
            ("after".to_string(), "200".to_string()),
        ],
        "stderr：\n{stderr}"
    );
    let said = |needle: &str| stderr.lines().filter(|l| l.contains(needle)).count();
    assert_eq!(
        (
            said("[relay] refusing"),
            said("[relay] in flight back under")
        ),
        (1, 1),
        "顶满这一阵只说一次、回到上限之下说一次：\n{stderr}"
    );
}

/// fd 用尽时排着的连接拿到 503；连着两条都拿到（备用的那一个接回来了）；这一阵 `[relay]` 只说一次 fd 用尽、缓过来说一次；
/// 放出 fd 之后下一条照常 200。
#[cfg(unix)]
#[test]
fn a_relay_out_of_fds_answers_503_says_so_once_and_recovers() {
    match std::env::var_os(CHILD_MARK) {
        Some(_) => as_child_out_of_fds(),
        None => judge_out_of_fds(),
    }
}

#[cfg(unix)]
fn judge_out_of_fds() {
    let (steps, stderr) = run_child(
        "cross_relay::exhaust_tests::a_relay_out_of_fds_answers_503_says_so_once_and_recovers",
    );
    assert_eq!(
        steps,
        [
            ("full-1".to_string(), "503".to_string()),
            ("full-2".to_string(), "503".to_string()),
            ("after".to_string(), "200".to_string()),
        ],
        "stderr：\n{stderr}"
    );
    let said = |needle: &str| stderr.lines().filter(|l| l.contains(needle)).count();
    assert_eq!(
        (
            said("[relay] out of file descriptors"),
            said("[relay] file descriptors free again")
        ),
        (1, 1),
        "fd 用尽这一阵只说一次、缓过来说一次：\n{stderr}"
    );
}

/// 起子进程跑 `test`，回它每一步的（步名 · 状态码）与整段 stderr。
fn run_child(test: &str) -> (Vec<(String, String)>, String) {
    let child = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_MARK, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("起子进程");
    // 子进程里每一发都带读期限（`shoot`），挂住会变成「没有回包」那一行，不会把这里挂死。
    let out = child.wait_with_output().expect("等子进程");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let steps = stdout
        .lines()
        // libtest 的「test … 」前缀与第一行印在同一行上 ⇒ 按记号所在处切。
        .filter_map(|l| l.find("EXHAUST ").map(|at| &l[at + "EXHAUST ".len()..]))
        .filter_map(|l| l.split_once(' '))
        .map(|(k, v)| (k.to_string(), v.split(' ').nth(1).unwrap_or("").to_string()))
        .collect();
    (steps, format!("{stdout}\n{stderr}"))
}
