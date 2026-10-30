//! fd 用尽时中转的接受循环：排着队的那条连接拿到 503（不是干等到下游自己超时），这一阵只说一次，缓过来照常转发。
//! 要把整个进程的 fd 占满 ⇒ 在自己起的子进程里跑（压低软上限、用 `/dev/null` 占满），父进程看它的 stdout 与 stderr。

use crate::accounts::upstream_select::{table::RoutingTable, Accounts};
use crate::relay::observe_retry_tests::{
    shoot, spawn_judging_upstream, spawn_relay_over, sse_200, temp_dir, upstreams_at,
};

const CHILD_MARK: &str = "CCM_RELAY_EXHAUST_CHILD";
const CHILD_TEST: &str = "relay::listen::exhaust_tests::child_relay_runs_out_of_fds";
const TARGET: &str = "/t/claude-code/q/v1/messages";
const HEADERS: &str = "authorization: Bearer from-the-agent\r\nx-claude-code-session-id: s-1\r\n";

/// 本进程开着的 socket 里，不在听的还有几条（读 `/proc/net/tcp` 的状态列：`0A` 是在听）。
#[cfg(unix)]
fn sockets_not_listening() -> usize {
    let mut listening = std::collections::BTreeSet::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        for line in std::fs::read_to_string(table)
            .unwrap_or_default()
            .lines()
            .skip(1)
        {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.get(3) == Some(&"0A") {
                if let Some(inode) = cols.get(9) {
                    listening.insert(format!("socket:[{inode}]"));
                }
            }
        }
    }
    std::fs::read_dir("/proc/self/fd").map_or(0, |d| {
        d.flatten()
            .filter_map(|e| std::fs::read_link(e.path()).ok())
            .map(|t| t.to_string_lossy().into_owned())
            .filter(|t| t.starts_with("socket:") && !listening.contains(t))
            .count()
    })
}

/// 热身那一发的几条连接（打压这一侧 · 中转两半 · 假上游）各在自己的线程上关：等它们都关了再占 fd，
/// 不然占满之后才放出来的那几个会让中转「接得上」，量的就不是 fd 用尽了。
#[cfg(unix)]
fn wait_for_connections_to_close() {
    let until = std::time::Instant::now() + guard_core::wall::CEILING;
    while sockets_not_listening() > 0 {
        assert!(std::time::Instant::now() < until, "热身那一发的连接一直没关");
        std::thread::yield_now();
    }
}

/// 把软上限压到「此刻开着的 ＋ 一点」，再用 `/dev/null` 占到一个都不剩。回占着的那些。
#[cfg(unix)]
fn hog_every_fd() -> Vec<std::fs::File> {
    wait_for_connections_to_close();
    let open = std::fs::read_dir("/proc/self/fd").map_or(64, |d| d.count()) as u64;
    let mut r = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: 只读、只动这个子进程自己的软上限。
    unsafe {
        assert_eq!(libc::getrlimit(libc::RLIMIT_NOFILE, &mut r), 0);
        r.rlim_cur = (open + 32) as libc::rlim_t;
        assert_eq!(libc::setrlimit(libc::RLIMIT_NOFILE, &r), 0);
    }
    let mut hogs = Vec::new();
    hog_the_rest(&mut hogs);
    hogs
}

/// 再占一遍：这之间别的线程放出来的（热身那一发写完的账之类）也占上。
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

/// 子进程入口（不是判据 ⇒ `#[ignore]`）。每一步印一行 `EXHAUST <步> <回包状态行>`。
#[cfg(unix)]
#[test]
#[ignore = "子进程入口：只在被父判据用 CCM_RELAY_EXHAUST_CHILD 拉起时才跑"]
fn child_relay_runs_out_of_fds() {
    if std::env::var(CHILD_MARK).is_err() {
        return;
    }
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
        println!("EXHAUST {step} {}", status(&shoot(relay, TARGET, HEADERS, "{}")));
    }
    drop(hogs);
    println!("EXHAUST after {}", status(&shoot(relay, TARGET, HEADERS, "{}")));
    let _ = std::fs::remove_dir_all(&d);
}

/// fd 用尽时排着的连接拿到 503；连着两条都拿到（备用的那一个接回来了）；这一阵 `[relay]` 只说一次 fd 用尽、缓过来说一次；
/// 放出 fd 之后下一条照常 200。
#[cfg(unix)]
#[test]
fn a_relay_out_of_fds_answers_503_says_so_once_and_recovers() {
    let mut child = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([CHILD_TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"])
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
    let steps: Vec<(&str, &str)> = stdout
        .lines()
        // libtest 的「test … 」前缀与第一行印在同一行上 ⇒ 按记号所在处切。
        .filter_map(|l| l.find("EXHAUST ").map(|at| &l[at + "EXHAUST ".len()..]))
        .filter_map(|l| l.split_once(' '))
        .collect();
    let code = |s: &str| s.split(' ').nth(1).unwrap_or("").to_string();
    assert_eq!(
        steps.iter().map(|(k, v)| (*k, code(v))).collect::<Vec<_>>(),
        [
            ("full-1", "503".to_string()),
            ("full-2", "503".to_string()),
            ("after", "200".to_string()),
        ],
        "子进程的 stdout：\n{stdout}\nstderr：\n{stderr}"
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
