//! 帧循环：启动参数 · 主节点与转发 · 每条连接的 hello / 作业 / 心跳 · 看门狗。
//!
//! 主节点的主线程跑事件循环，作业一律从主线程起（垫片的父进程死亡信号认的是起它的那条线程）。
//! 读连接 · 读作业输出各一条线程，经通道把事件交给主线程；作业的输出由读的那条线程直接写回连接，
//! 退出帧由主线程在两路输出都读完之后写，所以退出帧总在最后一截输出之后。

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use devsched_wire as wire;
use wire::{Breach, Frame, StopReason};

use crate::facts;
use crate::instance::{self, Role};
use crate::limits::{self, Limits, Verdict};
use crate::procs::{self, Exit, SpawnError};
use crate::root::{self, Root, RunTmp, TmpError};
use crate::sccache;

/// 心跳时限默认（`--heartbeat-timeout`，秒）。
pub const DEFAULT_HEARTBEAT_TIMEOUT_S: u64 = 30;
/// 看门狗量一次的间隔（停线「连着两次」按它数）。
pub const WATCH_EVERY: Duration = Duration::from_secs(2);
/// 作业退了之后等它两路输出读完的时限（漏出组又清了环境的进程攥着管道时不一直等）。
const DRAIN_GRACE: Duration = Duration::from_secs(2);
const MIB: u64 = 1024 * 1024;

// ════════════════ 启动参数 ════════════════

#[derive(Debug, Clone, PartialEq)]
pub struct Opts {
    pub root: PathBuf,
    pub limits: Limits,
    pub heartbeat_timeout: Duration,
    /// `--slurm <srun 参数，逗号分隔>`：当上主节点时经 srun 把自己起在分到的那一份里
    pub slurm: Option<Slurm>,
}

/// 经 srun 重起自己要的：srun 的参数，与去掉 `--slurm` 的原参数。
#[derive(Debug, Clone, PartialEq)]
pub struct Slurm {
    pub srun_args: Vec<String>,
    pub inner_args: Vec<String>,
}

pub const USAGE: &str = "用法：devsched-node --root <登记根> [--max-procs N] [--user-margin N] [--heartbeat-timeout 秒] [--nproc-limit N] [--slurm <srun 参数，逗号分隔>]";

pub fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut root = None;
    let mut l = Limits::default();
    let mut hb = DEFAULT_HEARTBEAT_TIMEOUT_S;
    // `--slurm` 那一对先摘出来：剩下的就是在 srun 里重起自己时的参数
    let mut srun_args = None;
    let mut inner_args = Vec::new();
    let mut all = args.iter();
    while let Some(a) = all.next() {
        if a == "--slurm" {
            let v = all.next().ok_or("--slurm 后面要一个值")?;
            let s: Vec<String> = v
                .split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            if s.is_empty() {
                return Err("--slurm 的值是空的".into());
            }
            srun_args = Some(s);
        } else {
            inner_args.push(a.clone());
        }
    }
    let mut it = inner_args.iter();
    while let Some(a) = it.next() {
        let mut val = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} 后面要一个值"))
        };
        let num = |name: &str, v: String| {
            v.parse::<u64>()
                .map_err(|_| format!("{name} 要一个非负整数：{v}"))
        };
        match a.as_str() {
            "--root" => root = Some(PathBuf::from(val("--root")?)),
            "--max-procs" => l.max_procs = num("--max-procs", val("--max-procs")?)?,
            "--user-margin" => l.margin = num("--user-margin", val("--user-margin")?)?,
            "--heartbeat-timeout" => hb = num("--heartbeat-timeout", val("--heartbeat-timeout")?)?,
            "--nproc-limit" => l.cap = Some(num("--nproc-limit", val("--nproc-limit")?)?),
            other => return Err(format!("认不得的参数：{other}")),
        }
    }
    let root = root.ok_or("要 --root")?;
    if !root.is_absolute() {
        return Err(format!("--root 要绝对路径：{}", root.display()));
    }
    if hb == 0 {
        return Err("--heartbeat-timeout 不能是 0".into());
    }
    let slurm = srun_args.map(|srun_args| Slurm {
        srun_args,
        inner_args: inner_args.clone(),
    });
    Ok(Opts {
        root,
        limits: l,
        heartbeat_timeout: Duration::from_secs(hb),
        slurm,
    })
}

// ════════════════ 入口 ════════════════

static NODE_SIG: AtomicI32 = AtomicI32::new(0);

extern "C" fn on_node_sig(sig: libc::c_int) {
    NODE_SIG.store(sig, Ordering::SeqCst);
}

fn install_node_signals() {
    unsafe {
        for s in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_node_sig as *const () as usize;
            sa.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(s, &sa, std::ptr::null_mut());
        }
    }
}

/// 起节点：拿到锁就当主节点，拿不到就转发。返回退出码。
pub fn run(opts: Opts) -> i32 {
    let root = Root::new(&opts.root);
    if let Err(e) = root.ensure() {
        eprintln!(
            "devsched-node：建根目录 {} 没成：{e}",
            root.base().display()
        );
        return 2;
    }
    // 主节点刚退、锁还没放 · 套接字还没建好：几秒里来回试
    let mut t0 = Instant::now();
    loop {
        match instance::acquire(&root.lock_file()) {
            Ok(Role::Main(lock)) => match &opts.slurm {
                // 经 slurm：放掉锁、把自己换成 srun 里的那一份（它再拿锁当主节点）；转发不走这里
                Some(s) => {
                    drop(lock);
                    return exec_under_srun(s);
                }
                None => return Main::new(root, opts, lock).serve(),
            },
            Ok(Role::Forward) => match UnixStream::connect(root.socket()) {
                Ok(s) => match forward(s) {
                    Some(code) => return code,
                    // 主节点一个字节都没回就断了（正赶上它要退）：回去重新拿锁
                    None => {
                        t0 = Instant::now();
                        std::thread::sleep(Duration::from_millis(50));
                    }
                },
                Err(e) if t0.elapsed() > Duration::from_secs(10) => {
                    eprintln!(
                        "devsched-node：已有主节点，可连接不成它的套接字 {}：{e}",
                        root.socket().display()
                    );
                    return 2;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            },
            Err(e) => {
                eprintln!(
                    "devsched-node：拿锁 {} 没成：{e}",
                    root.lock_file().display()
                );
                return 2;
            }
        }
    }
}

/// `srun <参数…> <自己> <去掉 --slurm 的原参数>`，进程号不变、标准输入输出原样；只在 exec 不成时返回（退 2）。
fn exec_under_srun(s: &Slurm) -> i32 {
    use std::os::unix::process::CommandExt;
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("devsched-node：认不出自己的路径，经 srun 起不成：{e}");
            return 2;
        }
    };
    // 只一个任务：多起一份就是两个节点抢同一条标准输入输出（集群默认的任务数可能不止 1）
    let e = std::process::Command::new("srun")
        .arg("--ntasks=1")
        .args(&s.srun_args)
        .arg(exe)
        .args(&s.inner_args)
        .exec();
    eprintln!("devsched-node：起 srun 没成：{e}");
    2
}

/// 转发：标准输入 / 输出原样接到主节点的套接字上；哪一头到了结尾就收。自己不起作业。
/// 先等主节点回第一段字节（hello）才接自己的标准输入：一个字节都没等到就断了 ⇒ `None`（调用方回去重新拿锁），
/// 这时标准输入一个字节都还没读，换谁接都不丢。
fn forward(sock: UnixStream) -> Option<i32> {
    let mut from_main = sock;
    let mut out = io::stdout().lock();
    let mut buf = [0u8; 64 * 1024];
    match from_main.read(&mut buf) {
        Ok(0) | Err(_) => return None,
        Ok(n) => {
            if out.write_all(&buf[..n]).and_then(|_| out.flush()).is_err() {
                return Some(0);
            }
        }
    }
    let Ok(mut to_main) = from_main.try_clone() else {
        return Some(2);
    };
    let up = std::thread::Builder::new().spawn(move || {
        let _ = io::copy(&mut io::stdin().lock(), &mut to_main);
        let _ = to_main.shutdown(std::net::Shutdown::Write);
    });
    if up.is_err() {
        return Some(2);
    }
    loop {
        match from_main.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if out.write_all(&buf[..n]).and_then(|_| out.flush()).is_err() {
                    break;
                }
            }
        }
    }
    Some(0)
}

// ════════════════ 主节点 ════════════════

type ConnId = u32;
type SharedW = Arc<Mutex<Box<dyn Write + Send>>>;
type JobKey = (ConnId, u64);

enum Ev {
    Conn(ConnId, SharedW, Option<UnixStream>),
    Frame(ConnId, Frame),
    Bad(ConnId, String),
    Closed(ConnId),
    StreamDone(JobKey),
}

struct ConnState {
    w: SharedW,
    last: Instant,
    /// 套接字来的连接：收完它上面的作业后整条关掉（转发的那一头读到结尾就退）。
    sock: Option<UnixStream>,
}

struct JobState {
    pgid: i32,
    /// 作业帧的项目名与树名：clean 拿它判那棵树上有没有作业在跑
    tree: (String, String),
    run: RunTmp,
    watch: wire::StopWatch,
    exit: Option<Exit>,
    exited_at: Option<Instant>,
    streams_open: u8,
    stop: Option<(StopReason, Option<Breach>)>,
    seq: u64,
    /// 跑着的时候看门判到过账号线程到了线：命令没跑成就算撞了线（它可能早撞了、等别的子进程收完才退）。
    touched_line: bool,
}

/// 命令退了、节点没停过它：算不算撞了这趟的线。垫片留了记号 ⇒ 算；跑着时到过线、命令没跑成（非 0 · 被信号打死）⇒ 算。
fn hit_line(marker: bool, touched_line: bool, exit: Option<Exit>) -> bool {
    marker || (touched_line && exit.is_some_and(|e| e.code() != 0))
}

struct Main {
    root: Root,
    opts: Opts,
    _lock: instance::NodeLock,
    tx: Sender<Ev>,
    rx: Receiver<Ev>,
    conns: HashMap<ConnId, ConnState>,
    /// 断了（读到结尾 · 心跳超时）、上面还有作业在收尾的连接：退出帧照写，写不出不管；作业收完就关。
    closing: HashMap<ConnId, ConnState>,
    caps: Option<(Vec<wire::Cap>, Vec<String>)>,
    jobs: HashMap<JobKey, JobState>,
    by_pid: HashMap<i32, JobKey>,
    seq: u64,
    line_watch: limits::LineWatch,
    alarm: bool,
    log: Option<fs::File>,
    /// 节点的 sccache server（跨作业常驻；只主节点起）。
    sccache: Option<sccache::Server>,
    /// 上次起 sccache server 没成（只记头一回）。
    sccache_failing: bool,
}

fn send(w: &SharedW, f: &Frame) -> bool {
    match w.lock() {
        Ok(mut g) => wire::write_frame(&mut **g, f).is_ok(),
        Err(_) => false,
    }
}

fn spawn_thread(f: impl FnOnce() + Send + 'static) -> io::Result<()> {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(f)
        .map(|_| ())
}

/// 读一条连接的帧，交给主线程。
fn reader_thread(id: ConnId, r: impl Read + Send + 'static, tx: Sender<Ev>) -> io::Result<()> {
    spawn_thread(move || {
        let mut fr = wire::FrameReader::new(r);
        loop {
            let ev = match fr.read_frame() {
                Ok(Some(f)) => Ev::Frame(id, f),
                Ok(None) | Err(wire::WireError::Io(_)) => break,
                Err(e) => Ev::Bad(id, e.to_string()),
            };
            if tx.send(ev).is_err() {
                return;
            }
        }
        let _ = tx.send(Ev::Closed(id));
    })
}

/// 读作业的一路输出，一截一帧写回连接；不完整的 UTF-8 尾巴留给下一截。
fn output_thread(
    key: JobKey,
    stream: wire::Stream,
    mut r: io::PipeReader,
    w: SharedW,
    tx: Sender<Ev>,
) -> io::Result<()> {
    spawn_thread(move || {
        let mut buf = vec![0u8; 64 * 1024];
        let mut carry: Vec<u8> = Vec::new();
        loop {
            let n = match r.read(&mut buf) {
                Ok(0) | Err(_) => 0,
                Ok(n) => n,
            };
            if n == 0 {
                if !carry.is_empty() {
                    let data = String::from_utf8_lossy(&carry).into_owned();
                    send(
                        &w,
                        &Frame::Out(wire::Out {
                            job: key.1,
                            stream,
                            data,
                        }),
                    );
                }
                break;
            }
            carry.extend_from_slice(&buf[..n]);
            let cut = utf8_cut(&carry);
            if cut > 0 {
                let data = String::from_utf8_lossy(&carry[..cut]).into_owned();
                carry.drain(..cut);
                send(
                    &w,
                    &Frame::Out(wire::Out {
                        job: key.1,
                        stream,
                        data,
                    }),
                );
            }
        }
        let _ = tx.send(Ev::StreamDone(key));
    })
}

/// 到哪里为止是完整的 UTF-8（尾巴上最多 3 个字节可能是半个字）。
pub fn utf8_cut(b: &[u8]) -> usize {
    match std::str::from_utf8(b) {
        Ok(_) => b.len(),
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => b.len(), // 中间有坏字节：照替换字符写出去
    }
}

impl Main {
    fn new(root: Root, opts: Opts, lock: instance::NodeLock) -> Main {
        let (tx, rx) = mpsc::channel();
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.log().join("node.log"))
            .ok();
        Main {
            root,
            opts,
            _lock: lock,
            tx,
            rx,
            conns: HashMap::new(),
            closing: HashMap::new(),
            caps: None,
            jobs: HashMap::new(),
            by_pid: HashMap::new(),
            seq: 0,
            line_watch: limits::LineWatch::default(),
            alarm: false,
            log,
            sccache: None,
            sccache_failing: false,
        }
    }

    fn logline(&mut self, s: &str) {
        if let Some(f) = self.log.as_mut() {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = writeln!(f, "{t}\t{s}");
        }
    }

    fn serve(mut self) -> i32 {
        install_node_signals();
        procs::become_subreaper();
        // 起来先清上次残留
        let mine: Vec<String> = fs::read_dir(self.root.jobs_dir())
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        let groups = procs::clear_residue(&self.root.jobs_dir());
        let tmps = self.root.clear_tmp_of(&mine);
        // 上回 clean 改了名、没删完就退了的
        for t in self.root.leftover_trash() {
            root::remove_in_background(t);
        }
        self.rotate_log();
        self.logline(&format!(
            "起来 pid {} · 清了上次残留 {groups} 组 · {tmps} 个临时目录",
            std::process::id()
        ));
        self.ensure_sccache();
        self.listen();
        // 第 0 条连接：本进程的标准输入 / 输出
        let w: SharedW = Arc::new(Mutex::new(Box::new(io::stdout())));
        if reader_thread(0, io::stdin(), self.tx.clone()).is_err() {
            return 2;
        }
        self.open_conn(0, w, None);

        let mut last_watch = Instant::now();
        loop {
            match self.rx.recv_timeout(Duration::from_millis(200)) {
                Ok(ev) => self.on_event(ev),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.reap();
            if NODE_SIG.load(Ordering::SeqCst) != 0 {
                self.logline("收到信号：停掉全部作业、退出");
                self.stop_all(StopReason::Stopped);
                break;
            }
            if self.alarm {
                break;
            }
            self.check_heartbeats();
            if last_watch.elapsed() >= WATCH_EVERY {
                last_watch = Instant::now();
                self.watchdog();
            }
            self.finish_ready();
            // 没连接也没作业就当场退：调度端读到退出帧关了输入，ssh 跟着收
            if self.conns.is_empty() && self.jobs.is_empty() {
                break;
            }
        }
        self.shutdown();
        if self.alarm {
            3
        } else {
            0
        }
    }

    /// 在 `state/n.sock` 上收后来的连接（路径超 108 字节就不收，后来的转发连接不成、自己退）。
    fn listen(&mut self) {
        let p = self.root.socket();
        if !root::unix_path_ok(&p) {
            self.logline(&format!(
                "套接字路径 {} 超过 {} 字节：不收后来的连接",
                p.display(),
                root::SUN_PATH_MAX
            ));
            return;
        }
        let _ = fs::remove_file(&p); // 拿着锁 ⇒ 留下的是上次的
        let l = match UnixListener::bind(&p) {
            Ok(l) => l,
            Err(e) => {
                self.logline(&format!("套接字 {} 建不成：{e}", p.display()));
                return;
            }
        };
        let tx = self.tx.clone();
        let _ = spawn_thread(move || {
            let mut next: ConnId = 1;
            for s in l.incoming() {
                let Ok(s) = s else { continue };
                let Ok(r) = s.try_clone() else { continue };
                let id = next;
                next += 1;
                let Ok(closer) = s.try_clone() else { continue };
                let w: SharedW = Arc::new(Mutex::new(Box::new(s)));
                if tx.send(Ev::Conn(id, w, Some(closer))).is_err() {
                    return;
                }
                if reader_thread(id, r, tx.clone()).is_err() {
                    let _ = tx.send(Ev::Closed(id));
                }
            }
        });
    }

    fn open_conn(&mut self, id: ConnId, w: SharedW, sock: Option<UnixStream>) {
        // 能力探测要起进程：一个节点进程只探一次
        let root = &self.root;
        let (caps, programs) = self
            .caps
            .get_or_insert_with(|| {
                let caps = facts::probe_capabilities(root)
                    .into_iter()
                    .map(|c| wire::Cap {
                        name: c.name.to_string(),
                        version: c.version,
                    })
                    .collect();
                (caps, facts::present_programs(root))
            })
            .clone();
        let hello = Frame::Hello(wire::Hello {
            proto: wire::proto().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            caps,
            programs,
            root: self.root.base().to_string_lossy().into_owned(),
            heartbeat_timeout_s: self.opts.heartbeat_timeout.as_secs(),
            cargo_config: facts::cargo_configs(&self.root),
        });
        send(&w, &hello);
        send(&w, &self.facts_frame());
        self.conns.insert(
            id,
            ConnState {
                w,
                last: Instant::now(),
                sock,
            },
        );
    }

    fn facts_frame(&self) -> Frame {
        let f = facts::measure(&self.root);
        let (_, threads) = procs::count_in_groups(&self.pgids());
        let me = procs::stat(std::process::id() as i32)
            .map(|s| s.threads)
            .unwrap_or(1);
        Frame::Facts(wire::Facts {
            ours: Some(me + threads),
            jobs: self.jobs.len() as u64,
            ..wire_facts(&f)
        })
    }

    /// 根下有 sccache、此刻没起着 ⇒ 起一份（起来时 · 每趟作业起之前；撞停线停掉之后下一趟就再起）。
    fn ensure_sccache(&mut self) {
        if self.sccache.is_some() {
            return;
        }
        match sccache::start(&self.root, self.opts.limits.line(limits::rlimit_nproc())) {
            Ok(Some(s)) => {
                self.logline(&format!(
                    "起了 sccache server pid {} · 端口 {}",
                    s.pid, s.port
                ));
                self.sccache = Some(s);
            }
            Ok(None) => {}
            Err(e) => {
                // 起不来只在头一回记，之后每趟照试、不再记（起来了就从头算）
                if !self.sccache_failing {
                    self.logline(&format!(
                        "sccache server 起不来：{e}（作业不设端口，照常接活）"
                    ));
                }
                self.sccache_failing = true;
                return;
            }
        }
        self.sccache_failing = false;
    }

    /// 节点名下的组：各趟作业，加 sccache server 那一组（它起的编译器也是我们的）。
    fn pgids(&self) -> Vec<i32> {
        self.jobs
            .values()
            .map(|j| j.pgid)
            .chain(self.sccache.as_ref().map(|s| s.pid))
            .collect()
    }

    fn conn_send(&self, id: ConnId, f: &Frame) {
        if let Some(c) = self.conns.get(&id) {
            send(&c.w, f);
        }
    }

    fn error(&self, id: ConnId, job: Option<u64>, msg: String) {
        self.conn_send(id, &Frame::Error(wire::ErrorFrame { job, msg }));
    }

    fn on_event(&mut self, ev: Ev) {
        match ev {
            Ev::Conn(id, w, sock) => self.open_conn(id, w, sock),
            Ev::Frame(id, f) => {
                // 断了的连接（心跳超时 · 关着收尾的）上后来的帧不收：作业不起、输出没处去
                let Some(c) = self.conns.get_mut(&id) else {
                    return;
                };
                c.last = Instant::now();
                self.on_frame(id, f);
            }
            Ev::Bad(id, why) => self.error(id, None, why),
            Ev::Closed(id) => self.close_conn(id, "连接断了"),
            Ev::StreamDone(key) => {
                if let Some(j) = self.jobs.get_mut(&key) {
                    j.streams_open = j.streams_open.saturating_sub(1);
                }
            }
        }
    }

    fn on_frame(&mut self, id: ConnId, f: Frame) {
        match f {
            Frame::Beat => {
                let ff = self.facts_frame();
                self.conn_send(id, &ff);
            }
            Frame::Job(j) => self.start_job(id, j),
            Frame::Stop(s) => {
                if self.jobs.contains_key(&(id, s.job)) {
                    self.stop_job((id, s.job), StopReason::Stopped, None);
                } else {
                    self.error(id, Some(s.job), "没有这一趟".into());
                }
            }
            Frame::Clean(c) => self.clean(id, c),
            Frame::Error(e) => self.logline(&format!("连接 {id} 报错：{}", e.msg)),
            other => self.error(id, None, format!("节点不收 {} 帧", other.name())),
        }
    }

    fn clean(&mut self, id: ConnId, c: wire::Clean) {
        let dir = match self.root.job_cwd(&c.project, &c.tree, "") {
            Ok(d) => d,
            Err(e) => return self.error(id, None, e),
        };
        let busy = self
            .jobs
            .values()
            .any(|j| j.tree.0 == c.project && j.tree.1 == c.tree);
        if busy {
            return self.error(
                id,
                None,
                format!("{}/{} 有作业在跑，不删", c.project, c.tree),
            );
        }
        // 当场只改名进 `.del-*`（同一盘，瞬时）、另起线程删：删一棵大树不停住帧循环（别的连接的心跳照收）
        self.seq += 1;
        let trash = self.root.trash_for(&dir, self.seq);
        match fs::rename(&dir, &trash) {
            Ok(()) => root::remove_in_background(trash),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => self.error(id, None, format!("删 {} 没成：{e}", dir.display())),
        }
    }

    /// 不起进程就收尾的一趟（拒掉的）。
    fn refuse(&self, id: ConnId, job: u64, stop: StopReason) {
        self.conn_send(
            id,
            &Frame::Exit(wire::Exit {
                job,
                code: None,
                stop,
                breach: None,
                skipped: vec![],
            }),
        );
    }

    fn start_job(&mut self, id: ConnId, j: wire::Job) {
        let key = (id, j.job);
        if self.jobs.contains_key(&key) {
            return self.error(id, Some(j.job), "这个号的作业还在跑".into());
        }
        // 线程上限：到线就不接
        let rl = limits::rlimit_nproc();
        let (ours, _) = procs::count_in_groups(&self.pgids());
        let verdict = self
            .opts
            .limits
            .judge(rl, limits::account_threads(), ours + 1);
        if !verdict.accepts_new() {
            self.logline(&format!("拒第 {} 趟：{verdict:?}", j.job));
            return self.refuse(id, j.job, StopReason::Threads);
        }
        let cwd = match self.root.job_cwd(&j.project, &j.tree, &j.rel) {
            Ok(d) => d,
            Err(e) => {
                self.error(id, Some(j.job), e);
                return self.refuse(id, j.job, StopReason::Spawn);
            }
        };
        if j.argv.is_empty() {
            self.error(id, Some(j.job), "命令数组是空的".into());
            return self.refuse(id, j.job, StopReason::Spawn);
        }
        let run = match self.root.make_run_tmp() {
            Ok(r) => r,
            Err(e @ TmpError::TooLong { .. }) => {
                self.error(id, Some(j.job), e.to_string());
                return self.refuse(id, j.job, StopReason::TmpPath);
            }
            Err(e) => {
                self.error(id, Some(j.job), e.to_string());
                return self.refuse(id, j.job, StopReason::Spawn);
            }
        };
        self.ensure_sccache();
        let mut env: Vec<(OsString, OsString)> =
            j.env.iter().map(|(k, v)| (k.into(), v.into())).collect();
        let frame_path = j.env.get("PATH").map(std::ffi::OsStr::new);
        env.extend(
            self.root
                .job_env(&run, frame_path, self.sccache.as_ref().map(|s| s.port))
                .into_iter()
                .map(|(k, v)| (OsString::from(k), v)),
        );
        let spec = procs::JobSpec {
            argv: j.argv.iter().map(OsString::from).collect(),
            env,
            cwd,
            nice: j.nice.unwrap_or(0),
            nproc_soft: Some(self.opts.limits.line(rl)),
            tag: run.skipped_file().into(),
        };
        let pipes = io::pipe().and_then(|o| io::pipe().map(|e| (o, e)));
        let ((out_r, out_w), (err_r, err_w)) = match pipes {
            Ok(p) => p,
            Err(e) => {
                run.remove();
                return self.spawn_alarm(id, j.job, &e.to_string(), false);
            }
        };
        let pgid =
            match procs::spawn_job(&shim_exe(), &spec, Stdio::from(out_w), Stdio::from(err_w)) {
                Ok(p) => p,
                Err(SpawnError::NoProcess(e)) => {
                    run.remove();
                    // 节点自己（不带这趟的软限）还起得了进程 ⇒ 挡住的是这趟的线（门槛判过之后别人把账号线程推过了线）：
                    // 照门槛拒那样报线程上限，不警报；自己也起不了 ⇒ 账号真满了，照旧警报
                    if can_fork() {
                        self.logline(&format!("拒第 {} 趟：起作业撞上线程线（{e}）", j.job));
                        return self.refuse(id, j.job, StopReason::Threads);
                    }
                    return self.spawn_alarm(id, j.job, &e.to_string(), false);
                }
                Err(SpawnError::Other(e)) => {
                    run.remove();
                    self.error(id, Some(j.job), format!("起作业没成：{e}"));
                    return self.refuse(id, j.job, StopReason::Spawn);
                }
            };
        let _ = procs::write_record(&self.root.jobs_dir(), &run.short, pgid, &spec.tag);
        self.seq += 1;
        let watch = wire::StopWatch::new(j.stop.clone());
        self.jobs.insert(
            key,
            JobState {
                pgid,
                tree: (j.project.clone(), j.tree.clone()),
                run,
                watch,
                exit: None,
                exited_at: None,
                streams_open: 2,
                stop: None,
                seq: self.seq,
                touched_line: false,
            },
        );
        self.by_pid.insert(pgid, key);
        let Some(w) = self.conns.get(&id).map(|c| c.w.clone()) else {
            return;
        };
        let ok = output_thread(key, wire::Stream::Stdout, out_r, w.clone(), self.tx.clone())
            .is_ok()
            && output_thread(key, wire::Stream::Stderr, err_r, w, self.tx.clone()).is_ok();
        if !ok {
            self.spawn_alarm(id, j.job, "起不了读输出的线程", true);
        }
    }

    /// 起进程失败：立刻停掉自己的全部作业、报一句、让位（节点退出）。
    /// `answered`：这一趟已在作业表里（退出帧随收尾发）或已经发过；否则这里补发一帧。
    fn spawn_alarm(&mut self, id: ConnId, job: u64, why: &str, answered: bool) {
        let msg = format!("起不了进程（{why}）：停掉全部作业、节点退出");
        self.logline(&msg);
        for c in self.conns.values() {
            send(
                &c.w,
                &Frame::Error(wire::ErrorFrame {
                    job: Some(job),
                    msg: msg.clone(),
                }),
            );
        }
        if !answered {
            self.refuse(id, job, StopReason::Spawn);
        }
        self.stop_all(StopReason::Spawn);
        self.alarm = true;
    }

    fn stop_job(&mut self, key: JobKey, why: StopReason, breach: Option<Breach>) {
        let Some(j) = self.jobs.get_mut(&key) else {
            return;
        };
        if j.stop.is_none() {
            j.stop = Some((why, breach));
        }
        procs::kill_group(j.pgid, Some(j.run.skipped_file().as_os_str()), 0);
    }

    fn stop_all(&mut self, why: StopReason) {
        let keys: Vec<JobKey> = self.jobs.keys().copied().collect();
        for k in keys {
            self.stop_job(k, why, None);
        }
    }

    /// 一条连接断了（读到结尾 · 心跳超时）：停它上面的作业；作业收完再整条关掉。
    fn close_conn(&mut self, id: ConnId, why: &str) {
        let Some(c) = self.conns.remove(&id) else {
            return;
        };
        let keys: Vec<JobKey> = self.jobs.keys().filter(|k| k.0 == id).copied().collect();
        self.logline(&format!("连接 {id} {why}：停它上面的 {} 趟", keys.len()));
        for k in keys {
            self.stop_job(k, StopReason::Heartbeat, None);
        }
        self.closing.insert(id, c);
    }

    fn check_heartbeats(&mut self) {
        let dead: Vec<ConnId> = self
            .conns
            .iter()
            .filter(|(_, c)| c.last.elapsed() > self.opts.heartbeat_timeout)
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            self.close_conn(id, "超过心跳时限没动静");
        }
    }

    /// 断了的连接上作业都收完了 ⇒ 关掉（套接字两头都关，转发的那一头读到结尾退出）。
    fn drop_closed(&mut self) {
        let live: Vec<ConnId> = self.jobs.keys().map(|k| k.0).collect();
        self.closing.retain(|id, c| {
            if live.contains(id) {
                return true;
            }
            if let Some(s) = &c.sock {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
            false
        });
    }

    fn reap(&mut self) {
        for (pid, exit) in procs::reap_any() {
            if self.sccache.as_ref().is_some_and(|s| s.pid == pid) {
                if let Some(s) = self.sccache.take() {
                    s.stop(&self.root);
                }
                self.logline(&format!(
                    "sccache server pid {pid} 没了（{exit:?}）：下一趟作业起时再起"
                ));
                continue;
            }
            if let Some(key) = self.by_pid.remove(&pid) {
                if let Some(j) = self.jobs.get_mut(&key) {
                    j.exit = Some(exit);
                    j.exited_at = Some(Instant::now());
                    // 垫片退了 ⇒ 组里该停的它停过了；再扫一遍兜底
                    procs::kill_group(j.pgid, Some(j.run.skipped_file().as_os_str()), 0);
                }
            }
        }
    }

    fn watchdog(&mut self) {
        let f = facts::measure(&self.root);
        // 停线
        let wf = wire_facts(&f);
        let keys: Vec<JobKey> = self.jobs.keys().copied().collect();
        for k in &keys {
            let hit = self.jobs.get_mut(k).and_then(|j| {
                if j.exit.is_none() {
                    j.watch.observe(&wf)
                } else {
                    None
                }
            });
            if let Some(b) = hit {
                self.logline(&format!("第 {} 趟撞停线 {b:?}", k.1));
                self.stop_job(*k, StopReason::Breach, Some(b));
                // 内存 · 压力：交给 sccache server 的编译也在吃，连它那一组一起停（下一趟起时再起一份）
                if matches!(b, Breach::Mem | Breach::Psi) {
                    if let Some(s) = self.sccache.take() {
                        s.stop(&self.root);
                        self.logline(&format!(
                            "撞 {b:?} 停线：sccache server pid {} 一起停了",
                            s.pid
                        ));
                    }
                }
            }
        }
        // 线程上限：判成停 ⇒ 停最新的一趟
        let (ours, _) = procs::count_in_groups(&self.pgids());
        let raw = self
            .opts
            .limits
            .judge(f.nproc_limit, f.account_threads, ours + 1);
        // 账号线程到了线：在跑的每一趟记一笔，命令之后没跑成就报线程上限
        if matches!(
            raw,
            Verdict::Refuse(limits::Breach::Threads { .. })
                | Verdict::Stop(limits::Breach::Threads { .. })
        ) {
            for j in self.jobs.values_mut().filter(|j| j.exit.is_none()) {
                j.touched_line = true;
            }
        }
        let v = self.line_watch.observe(raw);
        if let Verdict::Stop(b) = v {
            let newest = self
                .jobs
                .iter()
                .filter(|(_, j)| j.stop.is_none() && j.exit.is_none())
                .max_by_key(|(_, j)| j.seq)
                .map(|(k, _)| *k);
            if let Some(k) = newest {
                self.logline(&format!("{b}：停第 {} 趟", k.1));
                self.stop_job(k, StopReason::Threads, None);
            }
        }
        self.rotate_log();
    }

    /// 日志：node.log 过了上限的一半就换一份新的，再从最旧的删到不超上限。
    fn rotate_log(&mut self) {
        let cur = self.root.log().join("node.log");
        if fs::metadata(&cur)
            .map(|m| m.len() > root::LOG_CAP_BYTES / 2)
            .unwrap_or(false)
        {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = fs::rename(&cur, self.root.log().join(format!("node.{t}.log")));
            self.log = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&cur)
                .ok();
        }
        self.root.prune_logs_except(root::LOG_CAP_BYTES, &cur);
    }

    /// 退了、两路输出也读完（或等过了时限）的作业：清记录与临时目录，再写退出帧。
    fn finish_ready(&mut self) {
        let ready: Vec<JobKey> = self
            .jobs
            .iter()
            .filter(|(_, j)| {
                j.exit.is_some()
                    && (j.streams_open == 0
                        || j.exited_at.is_some_and(|t| t.elapsed() > DRAIN_GRACE))
            })
            .map(|(k, _)| *k)
            .collect();
        for key in ready {
            let Some(j) = self.jobs.remove(&key) else {
                continue;
            };
            let skipped = j.run.read_skipped();
            // 垫片（压在这趟的软限下）起不了命令，节点自己却还起得了进程 ⇒ 挡住它的是这趟的线：算撞线，不警报
            let shim_blocked =
                procs::shim_nospawn_marker(j.run.skipped_file().as_os_str()).exists();
            let nospawn = shim_blocked && !can_fork();
            let atline = shim_blocked
                || hit_line(
                    procs::shim_atline_marker(j.run.skipped_file().as_os_str()).exists(),
                    j.touched_line,
                    j.exit,
                );
            let (stop, breach) = match j.stop {
                Some(s) => s,
                None if nospawn => (StopReason::Spawn, None),
                None if atline => (StopReason::Threads, None),
                None => (StopReason::Done, None),
            };
            // 跟本机一样：命令被信号打死折成 128 ＋ 信号；节点停掉的（stop ≠ done）没有退出码
            let code = match (stop, j.exit) {
                (StopReason::Done, Some(e)) => Some(e.code()),
                _ => None,
            };
            // 先清记录与临时目录、再发退出帧：读到退出帧 ⇒ 这一趟在这台留下的临时东西已经没了
            procs::remove_record(&self.root.jobs_dir(), &j.run.short);
            j.run.remove();
            let f = Frame::Exit(wire::Exit {
                job: key.1,
                code,
                stop,
                breach,
                skipped,
            });
            if let Some(c) = self.conns.get(&key.0).or_else(|| self.closing.get(&key.0)) {
                send(&c.w, &f);
            }
            if nospawn && j.stop.is_none() {
                let id = key.0;
                self.spawn_alarm(id, key.1, "作业里起子进程没成", true);
            }
        }
        self.drop_closed();
    }

    /// 退出前：停掉全部作业、等它们收掉、清记录与套接字。
    fn shutdown(&mut self) {
        // 先删套接字：之后来的转发连接不成、等锁放了自己当主节点；已经连上却没等到 hello 的回去重新拿锁
        let _ = fs::remove_file(self.root.socket());
        if !self.jobs.is_empty() {
            self.stop_all(StopReason::Stopped);
            let t = Instant::now();
            while !self.jobs.is_empty() && t.elapsed() < Duration::from_secs(5) {
                while let Ok(ev) = self.rx.try_recv() {
                    if let Ev::StreamDone(_) = ev {
                        self.on_event(ev);
                    }
                }
                self.reap();
                self.finish_ready();
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        if let Some(s) = self.sccache.take() {
            s.stop(&self.root);
            let _ = procs::wait_pid(s.pid, false);
        }
        // 收养来的孤儿也收掉
        let _ = procs::reap_any();
        self.logline("退出");
    }
}

/// 本机量的（字节）折成帧的单位（MiB）：报出去的与停线判的是同一份。
fn wire_facts(f: &facts::Facts) -> wire::Facts {
    wire::Facts {
        mem: f.mem_available.map(|b| b / MIB),
        psi: f.mem_pressure,
        disk: f.disk_free.map(|b| b / MIB),
        tmp: f.tmp_free.map(|b| b / MIB),
        threads: f.account_threads,
        nproc: f.nproc_limit,
        ours: None,
        jobs: 0,
    }
}

/// 节点自己此刻还起得了进程吗。
fn can_fork() -> bool {
    procs::try_fork().is_ok()
}

/// 垫片就是节点自己：`/proc/self/exe` 在二进制被替换 · 删掉之后照样起得来。
fn shim_exe() -> PathBuf {
    PathBuf::from("/proc/self/exe")
}

/// 节点入口（`main` 转调这里）。
pub fn main_with(args: Vec<String>) -> i32 {
    if args.first().map(String::as_str) == Some("--shim") {
        let rest = if args.get(1).map(String::as_str) == Some("--") {
            &args[2..]
        } else {
            &args[1..]
        };
        let rest: Vec<OsString> = rest.iter().map(OsString::from).collect();
        return procs::run_shim(&rest);
    }
    if args.iter().any(|a| a == "--version") {
        println!(
            "devsched-node {} proto {}",
            env!("CARGO_PKG_VERSION"),
            wire::proto()
        );
        return 0;
    }
    match parse_args(&args) {
        Ok(o) => run(o),
        Err(e) => {
            eprintln!("devsched-node：{e}\n{USAGE}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn args_parse() {
        let o = parse_args(&s(&[
            "--root",
            "/r",
            "--max-procs",
            "9",
            "--user-margin",
            "3",
            "--heartbeat-timeout",
            "4",
            "--nproc-limit",
            "100",
        ]))
        .unwrap();
        assert_eq!(o.root, PathBuf::from("/r"));
        assert_eq!(
            o.limits,
            Limits {
                cap: Some(100),
                margin: 3,
                max_procs: 9
            }
        );
        assert_eq!(o.heartbeat_timeout, Duration::from_secs(4));
        let d = parse_args(&s(&["--root", "/r"])).unwrap();
        assert_eq!(d.limits, Limits::default());
        assert!(parse_args(&s(&[])).is_err());
        assert!(parse_args(&s(&["--root", "rel"])).is_err());
        assert!(parse_args(&s(&["--root", "/r", "--bogus"])).is_err());
        assert!(parse_args(&s(&["--root", "/r", "--max-procs", "x"])).is_err());
        assert!(parse_args(&s(&["--root", "/r", "--heartbeat-timeout", "0"])).is_err());
    }

    #[test]
    fn touched_line_counts_only_when_the_command_failed() {
        assert!(hit_line(true, false, Some(Exit::Code(0))));
        assert!(hit_line(false, true, Some(Exit::Code(101))));
        assert!(hit_line(false, true, Some(Exit::Signal(9))));
        assert!(!hit_line(false, true, Some(Exit::Code(0))));
        assert!(!hit_line(false, false, Some(Exit::Code(2))));
    }

    #[test]
    fn utf8_cut_keeps_half_char() {
        let b = "一二".as_bytes();
        assert_eq!(utf8_cut(&b[..4]), 3);
        assert_eq!(utf8_cut(b), 6);
        assert_eq!(utf8_cut(b"ab\xffcd"), 5);
    }

    #[test]
    fn facts_go_out_in_mib() {
        let f = facts::Facts {
            mem_available: Some(3 * MIB + 5),
            disk_free: Some(2 * MIB),
            ..facts::Facts::default()
        };
        let w = wire_facts(&f);
        assert_eq!((w.mem, w.disk), (Some(3), Some(2)));
        assert_eq!(
            wire::StopLines {
                mem: Some(4),
                psi: None,
                disk: None
            }
            .first_hit(&w),
            Some(Breach::Mem)
        );
    }
}
