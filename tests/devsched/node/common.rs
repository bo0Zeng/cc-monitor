// 编译节点端到端几份测试（node · thread_cap · thread_cap_drop）共用的：沙箱根、起节点、收帧、按 PID 收。各份用 include! 引进来。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use devsched_node::{limits, procs};
use devsched_wire::conn::{Conn, Recv, StreamConn};
use devsched_wire::{Frame, StopReason, WALL_CEILING};

const BIN: &str = env!("CARGO_BIN_EXE_devsched-node");

include!("../support.rs");

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> TestRoot {
        use std::os::unix::fs::PermissionsExt;
        let mut n =
            std::process::id().wrapping_mul(2654435761) ^ (Instant::now().elapsed().subsec_nanos());
        loop {
            n = n.wrapping_mul(1103515245).wrapping_add(12345);
            let p = PathBuf::from(format!("/tmp/dn{:04x}", (n >> 8) & 0xffff));
            if std::fs::create_dir(&p).is_ok() {
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
                std::fs::create_dir_all(p.join("projects/p/w")).unwrap();
                return TestRoot(p);
            }
        }
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 按「此刻账号线程数」定线的那几条（thread_cap · thread_cap_drop）一台机器上一次只跑一条：
/// 两条同时跑，一条的作业一停、账号一下落回二十几条，另一条判的就不是它自己那一趟了（几棵树同时跑快检也是这样）。
/// 拿着返回的那份文件就拿着锁，放掉它（drop）就放了。
#[allow(dead_code)] // node 那份不用
fn line_test_lock() -> std::fs::File {
    use std::os::fd::AsRawFd;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/devsched-line-test.lock")
        .expect("开得了线程线测试的锁文件");
    assert_eq!(unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) }, 0, "拿不到锁");
    f
}

/// 线程线那几条判不判得了：要跑在初始用户命名空间里。门禁整趟包在 bwrap 里（新的用户命名空间），
/// 内核在那里按命名空间记这个账号的线程数（只算命名空间里的），节点从 /proc 数的却是整台的 ⇒ 定出来的线跟内核拒 fork 的线不是一回事，
/// 线程线那两条的另一个前提：定线之后账号里别的进程没大落（同账号别的作业一下收掉几百条线程，线就定高了，
/// 作业起满也撞不上）。`started` 是作业起成了的 sleep 个数，`tried` 是它要起的个数：起满了、一次都没被拒 ⇒ 没撞上线，
/// 这条失败并说清是没判（不当绿）。
#[allow(dead_code)] // node 那份不用
fn assert_job_hit_the_line(started: usize, tried: usize) {
    assert!(
        started < tried,
        "判不了：作业起满 {tried} 个都没撞上线——定线之后账号里别的进程少了（同账号别的作业在收线程），线定高了；\
         这条只在账号里没有别的重活时判（编译节点上跟别的作业共用账号，不在那里判）"
    );
}

/// 作业撞不上线、照常退 2。判不了 ⇒ 这条失败并说清是没判（不当绿）；门禁那一格用 `--skip` 不跑它们、记进 GATE_BLIND。
#[allow(dead_code)] // node 那份不用
fn assert_line_tests_judgeable() {
    let map = std::fs::read_to_string("/proc/self/uid_map").unwrap_or_default();
    let initial = map
        .split_whitespace()
        .collect::<Vec<_>>()
        .as_slice()
        == ["0", "0", "4294967295"];
    assert!(
        initial,
        "判不了：跑在用户命名空间里（uid_map {}），内核按命名空间记线程数、跟 /proc 数出来的不是一回事，这条只在命名空间外判",
        map.trim()
    );
}

struct Node {
    child: Child,
    conn: StreamConn,
    /// 作业印出来的进程号：测完按它收
    pids: Vec<i32>,
}

impl Node {
    fn start(root: &Path, extra: &[&str]) -> Node {
        Node::start_env(root, extra, &[])
    }

    fn start_env(root: &Path, extra: &[&str], env: &[(&str, String)]) -> Node {
        let mut child = Command::new(BIN)
            .arg("--root")
            .arg(root)
            .args(extra)
            .env("DSN_NODE_ONLY", "node-env")
            .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let w = child.stdin.take().unwrap();
        let r = child.stdout.take().unwrap();
        Node {
            child,
            conn: StreamConn::new(r, w),
            pids: Vec::new(),
        }
    }

    fn pid(&self) -> i32 {
        self.child.id() as i32
    }

    fn recv(&mut self, d: Duration) -> Option<Frame> {
        match self.conn.recv_timeout(d) {
            Recv::Frame(f) => Some(f),
            Recv::Timeout | Recv::Closed => None,
            Recv::Bad(e) => panic!("坏帧：{e}"),
        }
    }

    /// 收到满足条件的那一帧为止（中间的丢掉）。
    fn until(&mut self, d: Duration, mut f: impl FnMut(&Frame) -> bool) -> Frame {
        let t = Instant::now();
        while t.elapsed() < d {
            if let Some(fr) = self.recv(Duration::from_millis(200)) {
                if f(&fr) {
                    return fr;
                }
            }
        }
        panic!("{d:?} 里没等到要的帧");
    }

    fn hello(&mut self) -> devsched_wire::Hello {
        match self.until(WALL_CEILING, |f| matches!(f, Frame::Hello(_))) {
            Frame::Hello(h) => h,
            _ => unreachable!(),
        }
    }

    fn job(&mut self, id: u64, script: &str) {
        self.conn.send(&Frame::Job(job_frame(id, script))).unwrap();
    }

    /// 收这一趟的输出直到退出帧：(标准输出, 退出帧)。
    fn collect(&mut self, id: u64, d: Duration) -> (String, devsched_wire::Exit) {
        let mut out = String::new();
        let t = Instant::now();
        while t.elapsed() < d {
            match self.recv(Duration::from_millis(200)) {
                Some(Frame::Out(o)) if o.job == id && o.stream == devsched_wire::Stream::Stdout => {
                    out.push_str(&o.data)
                }
                Some(Frame::Exit(e)) if e.job == id => return (out, e),
                _ => {}
            }
        }
        panic!("第 {id} 趟 {d:?} 里没收尾；输出：{out}");
    }

    /// 读输出里的一行进程号（作业 `echo $!` 印的），记下来测完收。
    fn read_pids(&mut self, id: u64, n: usize) -> Vec<i32> {
        let mut buf = String::new();
        let t = Instant::now();
        while t.elapsed() < WALL_CEILING {
            if let Some(Frame::Out(o)) = self.recv(Duration::from_millis(200)) {
                if o.job == id && o.stream == devsched_wire::Stream::Stdout {
                    buf.push_str(&o.data);
                    let v: Vec<i32> = buf
                        .split_whitespace()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    if v.len() >= n && buf.ends_with('\n') {
                        self.pids.extend(&v);
                        return v;
                    }
                }
            }
        }
        panic!("没等到 {n} 个进程号：{buf}");
    }

    fn wait_exit(&mut self, d: Duration) -> Option<std::process::ExitStatus> {
        let t = Instant::now();
        while t.elapsed() < d {
            if let Ok(Some(s)) = self.child.try_wait() {
                return Some(s);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        for p in &self.pids {
            unsafe { libc::kill(*p, libc::SIGKILL) };
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 一趟 `sh -c <script>` 的作业帧（项目 p · 树 w · 树根下起），别的格空着；要改哪格拿去改。
fn job_frame(id: u64, script: &str) -> devsched_wire::Job {
    devsched_wire::Job {
        job: id,
        project: "p".into(),
        tree: "w".into(),
        rel: String::new(),
        argv: vec!["sh".into(), "-c".into(), script.into()],
        env: BTreeMap::new(),
        stop: Default::default(),
        nice: None,
    }
}

/// 没了（僵尸也算：它的父进程会收）。
fn gone(pid: i32) -> bool {
    procs::stat(pid).map(|s| s.state == b'Z').unwrap_or(true)
}

fn wait_gone(pids: &[i32], d: Duration) -> bool {
    let t = Instant::now();
    while t.elapsed() < d {
        if pids.iter().all(|p| gone(*p)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// 父进程是 `parent` 的僵尸。
fn zombies_of(parent: i32) -> Vec<i32> {
    procs::pids()
        .into_iter()
        .filter(|p| procs::stat(*p).is_some_and(|s| s.ppid == parent && s.state == b'Z'))
        .collect()
}
