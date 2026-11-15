//! 测试用：私有临时目录 · 收输出的缓冲 · 假节点（照 wire 的帧回话）与假连接器 · 假送源码 · 假本机那一支。

use crate::config::{Host, Project};
use crate::dispatch::{Connector, LocalOutcome, LocalRun, SyncError, Syncer};
use crate::plan::{Plan, Vars};
use crate::rewrite::Rewriter;
use devsched_wire::conn::{Conn, MemConn, Recv};
use devsched_wire::{Cap, Exit, Facts, Frame, Hello, Job, Out, StopReason, Stream};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// 写可执行夹具（假 ssh）跟节点端到端那几份共用一处
include!("../support.rs");

/// 在测试二进制自己的一个子进程里只跑 `test` 这一条（`--exact`，单线程），成了才返回、没成就 panic 并带上它的输出。
/// 给判 flock「放了再拿当场拿得到」的那几条用：flock 跟着打开的文件走，同一份测试二进制里别的测试此刻 fork 出的
/// 子进程（exec 之前）也攥着一份，放了的锁那一瞬还在别人手里；子进程里只有这一条，不起别的进程，判的就是锁本身。
pub fn run_alone(test: &str) {
    let o = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--test-threads=1", "--quiet"])
        .env(ALONE_ENV, test)
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success() && out.contains("1 passed"),
        "{test} 在单跑它的子进程里没成：{out}{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

/// 这里就是 [`run_alone`] 起的那个子进程 ⇒ true（接着往下判）；不是 ⇒ 起一个去判、成了返回 false（这一条到此为止）。
pub fn alone(test: &str) -> bool {
    if std::env::var_os(ALONE_ENV).is_some_and(|v| v == test) {
        return true;
    }
    run_alone(test);
    false
}

const ALONE_ENV: &str = "DEVSCHED_TEST_ALONE";

/// 真起本机那一支的测试排着来（信号处理是整个进程一份）。
pub static REAL_LOCAL: Mutex<()> = Mutex::new(());

/// 本进程私有的临时目录，drop 时删掉。
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "devsched-sched-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 收输出的缓冲。
#[derive(Clone, Default)]
pub struct Buf(pub Arc<Mutex<Vec<u8>>>);

impl Buf {
    pub fn text(&self) -> String {
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

/// 假节点收到作业之后怎么回。
#[derive(Clone, Debug)]
pub enum Reply {
    /// 先吐这几截输出，再回退出帧
    Exit {
        outs: Vec<(Stream, String)>,
        code: Option<i32>,
        stop: StopReason,
        breach: Option<devsched_wire::Breach>,
        skipped: Vec<String>,
    },
    /// 收了作业就断
    Drop,
    /// 收了作业什么都不回（也不回心跳）
    Silent,
}

impl Reply {
    pub fn ok(code: i32) -> Reply {
        Reply::Exit {
            outs: vec![],
            code: Some(code),
            stop: StopReason::Done,
            breach: None,
            skipped: vec![],
        }
    }
    pub fn stop(stop: StopReason) -> Reply {
        Reply::Exit {
            outs: vec![],
            code: None,
            stop,
            breach: None,
            skipped: vec![],
        }
    }
}

/// 一台假节点的样子。
#[derive(Clone, Debug)]
pub struct Node {
    pub caps: Vec<String>,
    pub programs: Vec<String>,
    pub facts: Facts,
    pub proto: String,
    pub root: String,
    pub heartbeat_timeout_s: u64,
    pub reply: Reply,
    /// 收到 clean 时回「有作业在跑」、不删的那几棵树
    pub busy: Vec<String>,
}

impl Node {
    pub fn new(caps: &[&str]) -> Node {
        Node {
            caps: caps.iter().map(|s| s.to_string()).collect(),
            programs: vec![],
            facts: Facts {
                mem: Some(64 * 1024),
                psi: Some(0.0),
                disk: Some(1 << 20),
                tmp: Some(1 << 20),
                threads: Some(10),
                nproc: Some(1000),
                ours: Some(0),
                jobs: 0,
            },
            proto: devsched_wire::proto().to_string(),
            root: "/srv/ds".into(),
            heartbeat_timeout_s: 30,
            reply: Reply::ok(0),
            busy: vec![],
        }
    }
    pub fn reply(mut self, r: Reply) -> Node {
        self.reply = r;
        self
    }

    fn serve(
        self,
        mut c: MemConn,
        host: String,
        log: Arc<Mutex<Vec<(String, Job)>>>,
        beats: Arc<AtomicUsize>,
        cleaned: Arc<Mutex<Vec<(String, String)>>>,
    ) {
        let facts = self.facts.clone();
        let hello = Hello {
            proto: self.proto.clone(),
            version: "fake".into(),
            caps: self
                .caps
                .iter()
                .map(|n| Cap {
                    name: n.clone(),
                    version: "1".into(),
                })
                .collect(),
            programs: self.programs.clone(),
            root: self.root.clone(),
            heartbeat_timeout_s: self.heartbeat_timeout_s,
            cargo_config: Vec::new(),
        };
        if c.send(&Frame::Hello(hello)).is_err()
            || c.send(&Frame::Facts(self.facts.clone())).is_err()
        {
            return;
        }
        let mut silent = false;
        loop {
            // 跟真节点一样：过了心跳时限收不到东西就断
            match c.recv_timeout(Duration::from_secs(self.heartbeat_timeout_s)) {
                Recv::Frame(Frame::Job(j)) => {
                    let id = j.job;
                    log.lock().unwrap().push((host.clone(), j));
                    match &self.reply {
                        Reply::Drop => return,
                        Reply::Silent => silent = true,
                        Reply::Exit {
                            outs,
                            code,
                            stop,
                            breach,
                            skipped,
                        } => {
                            for (s, d) in outs {
                                let _ = c.send(&Frame::Out(Out {
                                    job: id,
                                    stream: *s,
                                    data: d.clone(),
                                }));
                            }
                            let _ = c.send(&Frame::Exit(Exit {
                                job: id,
                                code: *code,
                                stop: *stop,
                                breach: *breach,
                                skipped: skipped.clone(),
                            }));
                        }
                    }
                }
                Recv::Frame(Frame::Beat) if !silent => {
                    beats.fetch_add(1, Ordering::SeqCst);
                    let _ = c.send(&Frame::Facts(facts.clone()));
                }
                Recv::Frame(Frame::Clean(k)) => {
                    if self.busy.contains(&k.tree) {
                        let _ = c.send(&Frame::Error(devsched_wire::ErrorFrame {
                            job: None,
                            msg: format!("{}/{} 有作业在跑，不删", k.project, k.tree),
                        }));
                    } else {
                        cleaned.lock().unwrap().push((host.clone(), k.tree));
                    }
                }
                Recv::Frame(_) | Recv::Bad(_) => {}
                Recv::Timeout | Recv::Closed => return,
            }
        }
    }
}

/// 假连接器：每台一份假节点（一串的话一次连接用掉一个，最后一个一直用）；没写的台连接不成。
#[derive(Default)]
pub struct FakeConnector {
    pub nodes: Mutex<BTreeMap<String, Vec<Node>>>,
    /// 交到各台的作业（台, 作业帧）
    pub jobs: Arc<Mutex<Vec<(String, Job)>>>,
    /// 连过几次（台）
    pub opened: Mutex<Vec<String>>,
    /// 这几台在本机这边就起不了连接（`open` 报错）
    pub refuse: Mutex<std::collections::BTreeSet<String>>,
    /// 各台假节点一共收到几次心跳
    pub beats: Arc<AtomicUsize>,
    /// 各台假节点照 clean 删掉的树（台, 树），按先后
    pub cleaned: Arc<Mutex<Vec<(String, String)>>>,
}

impl FakeConnector {
    pub fn with(nodes: &[(&str, Node)]) -> FakeConnector {
        let f = FakeConnector::default();
        for (h, n) in nodes {
            f.nodes
                .lock()
                .unwrap()
                .entry(h.to_string())
                .or_default()
                .push(n.clone());
        }
        f
    }
    pub fn job_hosts(&self) -> Vec<String> {
        self.jobs
            .lock()
            .unwrap()
            .iter()
            .map(|(h, _)| h.clone())
            .collect()
    }
}

impl Connector for FakeConnector {
    fn open(&self, host: &Host) -> io::Result<Box<dyn Conn>> {
        self.opened.lock().unwrap().push(host.name.clone());
        if self.refuse.lock().unwrap().contains(&host.name) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "假的：本机起不了 ssh",
            ));
        }
        let node = {
            let mut m = self.nodes.lock().unwrap();
            let Some(list) = m.get_mut(&host.name) else {
                // 连接不成：跟 ssh 连接不成一样，起得来、一帧不回就断
                let (mine, theirs) = MemConn::pair();
                drop(theirs);
                return Ok(Box::new(mine));
            };
            if list.len() > 1 {
                list.remove(0)
            } else {
                list[0].clone()
            }
        };
        let (mine, theirs) = MemConn::pair();
        let log = self.jobs.clone();
        let name = host.name.clone();
        let beats = self.beats.clone();
        let cleaned = self.cleaned.clone();
        std::thread::spawn(move || node.serve(theirs, name, log, beats, cleaned));
        Ok(Box::new(mine))
    }
}

/// 假送源码：记下送去哪台，按设定成败。
#[derive(Default)]
pub struct FakeSync {
    pub fail: Mutex<Option<SyncError>>,
    pub sent: Mutex<Vec<(String, String)>>,
    /// 送的时候等这个计数到这个数才算送完（像送得久：让调度端在送的期间照发心跳）
    pub until_beats: Mutex<Option<(Arc<AtomicUsize>, usize)>>,
    /// 送的时候 panic
    pub panic: Mutex<bool>,
    /// 送源码要那台报的能力（真的 rsync 那一路要 `rsync`）
    pub needs: Mutex<Option<&'static str>>,
    /// 各台上这个项目底下有哪几棵远端树（列树时照它回；没写的台列出来是空的）
    pub trees: Mutex<BTreeMap<String, Vec<String>>>,
    /// 叫过对一遍镜像的台
    pub swept: Mutex<Vec<String>>,
}

impl Syncer for FakeSync {
    fn sync(&self, host: &Host, plan: &Plan, rw: &Rewriter) -> Result<(), SyncError> {
        self.sent
            .lock()
            .unwrap()
            .push((host.name.clone(), rw.remote_tree(plan)));
        if let Some((n, want)) = self.until_beats.lock().unwrap().clone() {
            let t = std::time::Instant::now();
            while n.load(Ordering::SeqCst) < want && t.elapsed() < devsched_wire::WALL_CEILING {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if *self.panic.lock().unwrap() {
            panic!("假的：送源码 panic");
        }
        match self.fail.lock().unwrap().take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    fn needs(&self) -> Option<&'static str> {
        *self.needs.lock().unwrap()
    }
    fn trees(&self, host: &Host, _plan: &Plan, _rw: &Rewriter) -> Result<Vec<String>, String> {
        Ok(self
            .trees
            .lock()
            .unwrap()
            .get(&host.name)
            .cloned()
            .unwrap_or_default())
    }
    fn sweep_mirror(&self, host: &Host, _plan: &Plan, _rw: &Rewriter) -> Result<(), String> {
        self.swept.lock().unwrap().push(host.name.clone());
        Ok(())
    }
}

/// 假本机那一支：记下起了什么，按设定回。
#[derive(Default)]
pub struct FakeLocal {
    pub outcomes: Mutex<Vec<LocalOutcome>>,
    pub runs: Mutex<Vec<(String, u32, Vec<String>)>>,
}

impl LocalRun for FakeLocal {
    fn run(&self, host: &Host, plan: &Plan, slot: u32) -> LocalOutcome {
        self.runs
            .lock()
            .unwrap()
            .push((host.name.clone(), slot, plan.argv.clone()));
        let mut o = self.outcomes.lock().unwrap();
        if o.is_empty() {
            LocalOutcome::Exit(0)
        } else {
            o.remove(0)
        }
    }
}

/// 树根在 `root`、项目叫 alpha 的一趟（不要能力、能出本机）。
pub fn plan_in(root: &Path, argv: &[&str]) -> Plan {
    Plan {
        project: Some(Project {
            name: "alpha".into(),
            keys: BTreeMap::new(),
        }),
        root: root.to_path_buf(),
        tree: crate::plan::tree_name(root),
        cwd: root.to_path_buf(),
        rel: String::new(),
        argv: argv.iter().map(|s| s.to_string()).collect(),
        needs: Default::default(),
        own: false,
        local: None,
        heavy: false,
        prio: false,
        target: None,
        vars: Vars::new(),
    }
}
