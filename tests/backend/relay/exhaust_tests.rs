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

/// 软上限压到这么低再占满（子进程此刻开着的远少于它；占满只要几百次 `open`）。硬上限更低 ⇒ 取硬上限。
#[cfg(target_os = "linux")]
const SOFT_FOR_HOGGING: u64 = 256;

/// 子进程里睡在 `accept` 上的线程该有几条：中转的接受循环 · 假上游的接受循环。
#[cfg(target_os = "linux")]
const ACCEPTING: usize = 2;

/// 把软上限压到 [`SOFT_FOR_HOGGING`]，等两条接受线程就位，再用 `/dev/null` 占到一个都不剩。回占着的那些。
#[cfg(target_os = "linux")]
fn hog_every_fd() -> Vec<std::fs::File> {
    let mut r = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: 只读、只动这个子进程自己的软上限。
    unsafe {
        assert_eq!(libc::getrlimit(libc::RLIMIT_NOFILE, &mut r), 0);
        r.rlim_cur = (SOFT_FOR_HOGGING as libc::rlim_t).min(r.rlim_max);
        assert_eq!(libc::setrlimit(libc::RLIMIT_NOFILE, &r), 0);
    }
    let mut hogs = Vec::new();
    hog_once_accepting(&mut hogs);
    hogs
}

/// 等两条接受线程都睡到 `accept` 上，再把放出来的全占上。
///
/// Linux 的 `accept` 阻塞之前先占好一个 fd 号。线程还没走到那一步（刚 spawn、负载下没排上；中转刚回完一条 503、
/// 换下旧备用还没回到 `accept`）就占满 ⇒ 它的 `accept` 当场报用尽：中转手里没有备用可放就空转，假上游 `continue` 空转，
/// 两条都会把判据放给打压一侧的那一个 fd 抢走 —— 打压一侧拿不到 fd，或连上了没人接、干等到读期限。
/// 读 `/proc` 要开 fd ⇒ 先放出几个再看，看完连同这之间被谁放出来的一起占回。
#[cfg(target_os = "linux")]
fn hog_once_accepting(hogs: &mut Vec<std::fs::File>) {
    const ROOM: usize = 4;
    let keep = hogs.len().saturating_sub(ROOM);
    hogs.truncate(keep);
    let mut seen = 0;
    for () in guard_core::wall::rounds() {
        seen = asleep_in_accept();
        if seen >= ACCEPTING {
            hog_the_rest(hogs);
            return;
        }
        std::thread::yield_now();
    }
    panic!("睡在 accept 上的线程只等到 {seen} 条（该有 {ACCEPTING} 条）");
}

/// 这个进程里此刻睡在 `accept4` 上的线程条数（`/proc/self/task/<tid>/stat` 的状态是 `S`、`syscall` 的号是 `accept4`）。
#[cfg(target_os = "linux")]
fn asleep_in_accept() -> usize {
    // 读的是这个进程的线程表，不是源码树：经后端的只读句柄读。
    let Ok(tasks) = crate::platform::fs::Ro::read_dir("/proc/self/task") else {
        return 0;
    };
    tasks
        .flatten()
        .filter(|t| {
            let read = |f: &str| std::fs::read_to_string(t.path().join(f)).unwrap_or_default();
            // `stat` 的进程名带括号、可能有空格 ⇒ 状态取最后一个 `) ` 之后的第一格。
            let state = read("stat")
                .rsplit_once(") ")
                .and_then(|(_, rest)| rest.split(' ').next().map(str::to_string));
            let call = read("syscall")
                .split(' ')
                .next()
                .and_then(|n| n.trim().parse::<libc::c_long>().ok());
            state.as_deref() == Some("S") && call == Some(libc::SYS_accept4)
        })
        .count()
}

/// 子进程整个钉在此刻所在的那一个核上（之后 spawn 的线程跟着）。
/// 一个核上刚 spawn 的线程排在主线程后面 —— 负载下才偶尔碰上的那一种先后（接受线程还没就位就占满）这里每一趟都是它，
/// 夹具若还靠线程起得快才绿，这一条当场挂住，不等负载来撞。
#[cfg(target_os = "linux")]
fn on_one_cpu() {
    // SAFETY: 只读此刻所在的核号、只改这个子进程自己的 CPU 亲和。
    unsafe {
        let cpu = libc::sched_getcpu();
        assert!(cpu >= 0, "读不到此刻所在的核");
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_SET(cpu as usize, &mut set);
        assert_eq!(
            libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set),
            0,
            "钉不到一个核上"
        );
    }
}

/// 再占一遍：这之间别的线程放出来的也占上。
#[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
fn as_child_out_of_fds() {
    on_one_cpu();
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
        hog_once_accepting(&mut hogs);
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
/// 只在 Linux 上跑：靠的是 Linux 的 `accept` 阻塞之前先占号（中转的备用 fd 就是为它留的），等接受线程就位读的是 `/proc`。
#[cfg(target_os = "linux")]
#[test]
fn a_relay_out_of_fds_answers_503_says_so_once_and_recovers() {
    match std::env::var_os(CHILD_MARK) {
        Some(_) => as_child_out_of_fds(),
        None => judge_out_of_fds(),
    }
}

#[cfg(target_os = "linux")]
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
/// 等的是子进程的 stdout 与 stderr 都读到头；过了 [`guard_core::wall::CEILING`] 还没到头 ⇒ 按 PID 杀掉它，把收到的那一截印进报错。
fn run_child(test: &str) -> (Vec<(String, String)>, String) {
    use std::io::Read;
    let mut child = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_MARK, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("起子进程");
    let drain = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut got = Vec::new();
            let _ = pipe.read_to_end(&mut got);
            String::from_utf8_lossy(&got).into_owned()
        })
    };
    let out = drain(Box::new(child.stdout.take().expect("stdout")));
    let err = drain(Box::new(child.stderr.take().expect("stderr")));
    let (done, ended) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let both = (
            out.join().unwrap_or_default(),
            err.join().unwrap_or_default(),
        );
        let _ = done.send(both);
    });
    let (stdout, stderr) = match ended.recv_timeout(guard_core::wall::CEILING) {
        Ok(both) => {
            let _ = child.wait();
            both
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            let (stdout, stderr) = ended.recv().unwrap_or_default();
            panic!("子进程过了期限还没收工，已按 PID 杀掉：\n{stdout}\n{stderr}");
        }
    };
    let steps = stdout
        .lines()
        // libtest 的「test … 」前缀与第一行印在同一行上 ⇒ 按记号所在处切。
        .filter_map(|l| l.find("EXHAUST ").map(|at| &l[at + "EXHAUST ".len()..]))
        .filter_map(|l| l.split_once(' '))
        .map(|(k, v)| (k.to_string(), v.split(' ').nth(1).unwrap_or("").to_string()))
        .collect();
    (steps, format!("{stdout}\n{stderr}"))
}
