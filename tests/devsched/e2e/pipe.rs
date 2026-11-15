//! 端到端：调度端的库经真管道拉起编译节点的二进制（本包的 `devsched-node-e2e`），在私有临时目录里整条走一趟。
//!
//! 「那台」就是本机的一个沙箱根目录：连接器直接起节点二进制（不经 ssh），送源码照送出清单本地拷过去。
//! 不连真机器、不碰真家目录；ssh localhost 那一档标成 ignored，要 `--ignored` 与 `DEVSCHED_SSH_LOCALHOST=1` 才跑。

use devsched_sched::config::{Config, Host};
use devsched_sched::dispatch::{Connector, Sched, SyncError, Syncer, Timing};
use devsched_sched::plan::{make_plan, Plan, Vars};
use devsched_sched::queue::State;
use devsched_sched::rewrite::Rewriter;
use devsched_wire::conn::{ChildConn, Conn, Recv};
use devsched_wire::{Frame, Job, StopLines, WALL_CEILING};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const NODE: &str = env!("CARGO_BIN_EXE_devsched-node-e2e");

include!("../support.rs");

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        static N: AtomicU32 = AtomicU32::new(0);
        // 路径要短：节点核「临时目录 ＋ socket 后缀 < 108 字节」，编译机的 TMPDIR 可能很深 ⇒ 直接放 /tmp 下
        let p = PathBuf::from(format!(
            "/tmp/dse{:x}-{}{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl Buf {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
    }
}

impl Write for Buf {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// 直接起节点二进制当「那台」（根目录是主机表 `base=`）。
struct SpawnNode;

impl Connector for SpawnNode {
    fn open(&self, host: &Host) -> io::Result<Box<dyn Conn>> {
        let base = host.opt("base").unwrap().to_string();
        let argv = vec![
            NODE.to_string(),
            "--root".into(),
            base,
            "--heartbeat-timeout".into(),
            "5".into(),
        ];
        Ok(Box::new(ChildConn::spawn(&argv)?))
    }
}

/// 照送出清单本地拷到那台的树目录。
struct CopySync;

impl Syncer for CopySync {
    fn sync(&self, _host: &Host, plan: &Plan, rw: &Rewriter) -> Result<(), SyncError> {
        let ex = devsched_policy::ExcludeList::parse(
            plan.project
                .as_ref()
                .map(|p| p.get("exclude"))
                .unwrap_or(""),
        );
        let list = devsched_policy::send_list(&plan.root, &ex)
            .map_err(|e| SyncError::Local(e.to_string()))?;
        let dest = PathBuf::from(rw.remote_tree(plan));
        for rel in list {
            let to = dest.join(&rel);
            std::fs::create_dir_all(to.parent().unwrap())
                .map_err(|e| SyncError::Remote(e.to_string()))?;
            std::fs::copy(plan.root.join(&rel), &to)
                .map_err(|e| SyncError::Remote(e.to_string()))?;
        }
        Ok(())
    }
}

struct NoLocal;

impl devsched_sched::dispatch::LocalRun for NoLocal {
    fn run(&self, _: &Host, _: &Plan, _: u32) -> devsched_sched::dispatch::LocalOutcome {
        panic!("这一趟不该在本机跑");
    }
}

/// 沙箱：配置目录（一台假的「那台」＋ 本机 0 位）· 一棵树 · 那台的根。
fn sandbox(t: &TempDir) -> (Config, PathBuf, PathBuf) {
    let node_root = t.path().join("r");
    let tree = t.path().join("tree");
    let cfg = t.path().join("cfg");
    std::fs::create_dir_all(&node_root).unwrap();
    std::fs::create_dir_all(tree.join("src")).unwrap();
    std::fs::create_dir_all(cfg.join("projects")).unwrap();
    std::fs::write(tree.join("marker.txt"), "m").unwrap();
    std::fs::write(tree.join("src/a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(tree.join(".env"), "SECRET=1\n").unwrap();
    std::fs::write(
        cfg.join("hosts"),
        format!(
            "fake 1 ssh:fake base={} trust=own\nlocal 0 local\n",
            node_root.display()
        ),
    )
    .unwrap();
    std::fs::write(
        cfg.join("projects/demo.conf"),
        "marker = marker.txt\nexclude = .git target\n",
    )
    .unwrap();
    (Config::load(&cfg).unwrap(), tree, node_root)
}

fn run_once(t: &TempDir, cfg: Config, tree: &Path, argv: &[&str]) -> (i32, String, String) {
    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    let (plan, _) = make_plan(&cfg.projects, tree, &argv, &Vars::new(), false);
    let (out, err) = (Buf::default(), Buf::default());
    let (mut o, mut e) = (out.clone(), err.clone());
    let mut s = Sched {
        cfg,
        state: State::new(t.path().join("state")),
        connector: &SpawnNode,
        syncer: &CopySync,
        local: &NoLocal,
        out: &mut o,
        err: &mut e,
        timing: Timing {
            poll: Duration::from_millis(50),
            beat: Duration::from_secs(1),
            hello: Duration::from_secs(10),
            deadline: Some(WALL_CEILING),
        },
    };
    let rc = s.run(plan);
    (rc, out.text(), err.text())
}

#[test]
fn job_goes_through_node_and_comes_back_like_local() {
    let t = TempDir::new("a");
    let (cfg, tree, node_root) = sandbox(&t);
    let (rc, out, err) = run_once(
        &t,
        cfg,
        &tree,
        &[
            "sh",
            "-c",
            "cat src/a.rs; test ! -e .env && echo no-env; echo \"at $PWD\" >&2; exit 3",
        ],
    );
    assert_eq!(rc, 3, "命令自己的退出码原样回来；stderr：{err}");
    assert!(
        out.contains("fn a() {}") && out.contains("no-env"),
        "源码送到了、凭据没送；stdout：{out}"
    );
    // 节点那台的工作目录在输出里改回本机写法
    assert!(
        err.contains(&format!("at {}", tree.display())),
        "stderr：{err}"
    );
    assert!(
        !err.contains(&node_root.display().to_string()),
        "远端路径没漏回来：{err}"
    );
    // 收尾：这一趟的临时目录删掉了
    let left = entries_of(&node_root.join("t"));
    assert!(left.is_empty(), "t/ 下留着：{left:?}");
}

fn pid_alive(pid: i32) -> bool {
    // 僵尸也当没了：读 /proc/<pid>/stat 的状态字
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(s) => !s
            .rsplit(')')
            .next()
            .unwrap_or("")
            .trim_start()
            .starts_with('Z'),
        Err(_) => false,
    }
}

#[test]
fn cut_pipe_makes_node_stop_job_and_exit() {
    let t = TempDir::new("b");
    let root = t.path().join("r");
    let tree = root.join("projects/demo/w");
    std::fs::create_dir_all(&tree).unwrap();
    let pidfile = t.path().join("job.pid");
    let mut child = Command::new(NODE)
        .args([
            "--root",
            &root.display().to_string(),
            "--heartbeat-timeout",
            "3",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let w = child.stdin.take().unwrap();
    let r = child.stdout.take().unwrap();
    let mut conn = devsched_wire::conn::StreamConn::new(r, w);
    let job = Job {
        job: 1,
        project: "demo".into(),
        tree: "w".into(),
        rel: String::new(),
        argv: vec![
            "sh".into(),
            "-c".into(),
            format!("echo $$ > {}; exec sleep 60", pidfile.display()),
        ],
        env: BTreeMap::new(),
        stop: StopLines::default(),
        nice: None,
    };
    conn.send(&Frame::Job(job)).unwrap();
    // 等作业起来（记下它的 pid）
    let t0 = Instant::now();
    let job_pid = loop {
        if let Some(p) = std::fs::read_to_string(&pidfile)
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
        {
            break p;
        }
        match conn.recv_timeout(Duration::from_millis(100)) {
            Recv::Frame(Frame::Exit(e)) => panic!("作业没起来就退了：{e:?}"),
            Recv::Closed => panic!("节点先关了连接"),
            _ => {}
        }
        assert!(t0.elapsed() < WALL_CEILING, "作业一直没起来");
    };
    assert!(pid_alive(job_pid));
    // 掐断：关掉我们这头的写与读
    conn.close();
    drop(conn);
    let t1 = Instant::now();
    let st = loop {
        if let Some(st) = child.try_wait().unwrap() {
            break st;
        }
        if t1.elapsed() > WALL_CEILING {
            let _ = child.kill();
            let _ = child.wait();
            panic!("掐断后节点一直没自己退");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let _ = st;
    let t2 = Instant::now();
    while pid_alive(job_pid) && t2.elapsed() < WALL_CEILING {
        std::thread::sleep(Duration::from_millis(100));
    }
    let alive = pid_alive(job_pid);
    if alive {
        // 判据已经红了：按 pid 收掉漏下的作业，不留给别人
        let _ = Command::new("kill")
            .args(["-KILL", &job_pid.to_string()])
            .status();
    }
    assert!(!alive, "节点退了，作业进程 {job_pid} 还在");
}

/// ssh localhost 那一档：默认不跑（ignored，编译机上不一定 ssh 得到自己）；
/// `DEVSCHED_SSH_LOCALHOST=1 cargo test -p devsched-e2e -- --ignored` 才跑，跑了没设变量就红，不装成过了。
#[test]
#[ignore = "要 ssh localhost：DEVSCHED_SSH_LOCALHOST=1 ＋ --ignored 才跑"]
fn over_ssh_localhost_when_asked() {
    assert_eq!(
        std::env::var("DEVSCHED_SSH_LOCALHOST").as_deref(),
        Ok("1"),
        "要设 DEVSCHED_SSH_LOCALHOST=1（且 ssh localhost 通）才跑这一档"
    );
    let t = TempDir::new("c");
    let (_, tree, node_root) = sandbox(&t);
    // 节点二进制放进那台的 state/，主机表的去法换成 localhost，连接器用真的 ssh
    std::fs::create_dir_all(node_root.join("state")).unwrap();
    std::fs::copy(NODE, node_root.join("state/devsched-node")).unwrap();
    let cfgd = t.path().join("cfg");
    std::fs::write(
        cfgd.join("hosts"),
        format!(
            "lo 1 ssh:localhost base={} trust=own\nlocal 0 local\n",
            node_root.display()
        ),
    )
    .unwrap();
    let cfg = Config::load(&cfgd).unwrap();
    let argv: Vec<String> = ["sh", "-c", "cat src/a.rs"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let (plan, _) = make_plan(&cfg.projects, &tree, &argv, &Vars::new(), false);
    let (out, err) = (Buf::default(), Buf::default());
    let (mut o, mut e) = (out.clone(), err.clone());
    let mut s = Sched {
        cfg,
        state: State::new(t.path().join("state")),
        connector: &devsched_sched::dispatch::SshConnector,
        syncer: &CopySync,
        local: &NoLocal,
        out: &mut o,
        err: &mut e,
        timing: Timing {
            poll: Duration::from_millis(50),
            beat: Duration::from_secs(1),
            hello: Duration::from_secs(20),
            deadline: Some(WALL_CEILING),
        },
    };
    assert_eq!(s.run(plan), 0, "stderr：{}", err.text());
    assert!(out.text().contains("fn a() {}"));
}

#[test]
fn trip_past_its_time_limit_is_stopped_as_timeout_and_cleaned() {
    let t = TempDir::new("d");
    let (_, tree, node_root) = sandbox(&t);
    let cfg_dir = t.path().join("cfg");
    // 项目配置把整趟时限压到 2 秒（主机表写的被项目配置盖过）
    std::fs::write(
        cfg_dir.join("projects/demo.conf"),
        "marker = marker.txt\nexclude = .git target\ntimeout_s = 2\n",
    )
    .unwrap();
    let hosts = std::fs::read_to_string(cfg_dir.join("hosts"))
        .unwrap()
        .replacen("trust=own", "trust=own timeout_s=600", 1);
    std::fs::write(cfg_dir.join("hosts"), hosts).unwrap();
    let cfg = Config::load(&cfg_dir).unwrap();
    let (rc, out, err) = run_once(&t, cfg, &tree, &["sh", "-c", "sleep 300 & echo $!; wait"]);
    assert_eq!(rc, 124, "到点报超时；stderr：{err}");
    assert!(err.contains("超时"), "说一句超时：{err}");
    let pid: i32 = out
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .expect("作业印的进程号");
    let t1 = Instant::now();
    while pid_alive(pid) && t1.elapsed() < WALL_CEILING {
        std::thread::sleep(Duration::from_millis(100));
    }
    let alive = pid_alive(pid);
    if alive {
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
    assert!(!alive, "节点那边作业组收干净");
    let left = entries_of(&node_root.join("t"));
    assert!(left.is_empty(), "t/ 下留着：{left:?}");
}
