//! 进程：每趟一个进程组，组长是节点自己的垫片（`--shim`）；收尾 · 停作业 · 连接断都整组收掉；起来先清上次残留。
//!
//! 执法只发信号、读 `/proc`，不起新进程（账号线程满了的时候起不来）。
//!
//! 一趟的样子：节点 ─fork→ 垫片（`setsid` 当组长、父进程死亡信号 SIGTERM、收养孤儿）─fork→ 作业命令
//! （同组、父进程死亡信号 SIGKILL）。节点被 SIGKILL ⇒ 垫片收到 SIGTERM ⇒ 把同组与同记号的进程一个个 SIGKILL 再退。
//! 记号就是这趟 `DEVSCHED_SKIPPED` 的值：`setsid` 出组的（私有 tmux server 一类）照它从 `/proc/<pid>/environ` 认出来。

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};

use crate::root::SKIPPED_VAR;

// ════════════════ /proc ════════════════

/// `/proc/<pid>/stat` 里要用的几格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcStat {
    pub pid: i32,
    pub state: u8,
    pub ppid: i32,
    pub pgrp: i32,
    pub session: i32,
    pub threads: u64,
    /// 起来的时刻（开机以来的时钟滴答）：组号被重用时靠它认出不是同一个。
    pub start: u64,
}

pub fn parse_stat(pid: i32, s: &str) -> Option<ProcStat> {
    // 名字在括号里、可能带空格与括号 ⇒ 从最后一个 ')' 往后数
    let rest = &s[s.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_ascii_whitespace().collect();
    // f[0] = state（第 3 格）· f[1] ppid · f[2] pgrp · f[3] session · f[17] num_threads（第 20 格）· f[19] starttime（第 22 格）
    Some(ProcStat {
        pid,
        state: *f.first()?.as_bytes().first()?,
        ppid: f.get(1)?.parse().ok()?,
        pgrp: f.get(2)?.parse().ok()?,
        session: f.get(3)?.parse().ok()?,
        threads: f.get(17)?.parse().ok()?,
        start: f.get(19)?.parse().ok()?,
    })
}

pub fn stat(pid: i32) -> Option<ProcStat> {
    parse_stat(pid, &fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// `/proc` 下此刻的各进程号。
pub fn pids() -> Vec<i32> {
    let Ok(rd) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse().ok())
        .collect()
}

/// 属本账号的进程（`/proc/<pid>` 的属主）。
pub fn my_pids() -> Vec<i32> {
    let uid = unsafe { libc::getuid() };
    pids()
        .into_iter()
        .filter(|p| {
            fs::metadata(format!("/proc/{p}"))
                .map(|m| m.uid() == uid)
                .unwrap_or(false)
        })
        .collect()
}

/// 这个进程的环境里有没有 `键=值` 这一项（读不了的当没有）。
pub fn environ_has(pid: i32, entry: &[u8]) -> bool {
    match fs::read(format!("/proc/{pid}/environ")) {
        Ok(b) => b.split(|c| *c == 0).any(|e| e == entry),
        Err(_) => false,
    }
}

/// 一趟的记号：`DEVSCHED_SKIPPED=<那份文件>`。
pub fn tag_entry(tag: &OsStr) -> Vec<u8> {
    let mut v = format!("{SKIPPED_VAR}=").into_bytes();
    v.extend_from_slice(tag.as_bytes());
    v
}

// ════════════════ 发信号 ════════════════

fn kill(pid: i32, sig: i32) -> bool {
    unsafe { libc::kill(pid, sig) == 0 }
}

/// 停掉一组：组号上发 SIGKILL，再把同组 · 同记号的漏网的一个个停（不停 `except`，垫片停别人时把自己留下）。
/// 返回发了信号的进程数（不算组上那一下）。
pub fn kill_group(pgid: i32, tag: Option<&OsStr>, except: i32) -> usize {
    if pgid > 1 && except != pgid {
        kill(-pgid, libc::SIGKILL);
    }
    sweep(pgid, tag, except)
}

/// 本账号名下 组号 = `pgid` 或 环境里带这个记号的进程，一个个 SIGKILL（不碰 `except` 与自己）。
pub fn sweep(pgid: i32, tag: Option<&OsStr>, except: i32) -> usize {
    let me = std::process::id() as i32;
    let entry = tag.map(tag_entry);
    let mut n = 0;
    for p in my_pids() {
        if p == me || p == except {
            continue;
        }
        let Some(st) = stat(p) else { continue };
        if st.state == b'Z' {
            continue;
        }
        let hit =
            (pgid > 1 && st.pgrp == pgid) || entry.as_deref().is_some_and(|e| environ_has(p, e));
        if hit && kill(p, libc::SIGKILL) {
            n += 1;
        }
    }
    n
}

/// 节点名下此刻的进程数与线程数：组号或会话号是这几个组之一的（漏出组又没离开会话的也算）。
pub fn count_in_groups(pgids: &[i32]) -> (u64, u64) {
    if pgids.is_empty() {
        return (0, 0);
    }
    let (mut procs, mut threads) = (0, 0);
    for p in my_pids() {
        let Some(st) = stat(p) else { continue };
        if pgids.contains(&st.pgrp) || pgids.contains(&st.session) {
            procs += 1;
            threads += st.threads.max(1);
        }
    }
    (procs, threads)
}

// ════════════════ 回收 ════════════════

/// 退出法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Signal(i32),
}

impl Exit {
    fn from_status(st: i32) -> Exit {
        if libc::WIFSIGNALED(st) {
            Exit::Signal(libc::WTERMSIG(st))
        } else {
            Exit::Code(libc::WEXITSTATUS(st))
        }
    }
    /// 照 shell 的写法折成一个退出码（信号 ⇒ 128 ＋ 信号）。
    pub fn code(self) -> i32 {
        match self {
            Exit::Code(c) => c,
            Exit::Signal(s) => 128 + s,
        }
    }
}

/// 等一个子进程（`nohang` ⇒ 不等，没退返回 None）。
pub fn wait_pid(pid: i32, nohang: bool) -> Option<Exit> {
    let mut st = 0;
    loop {
        let r = unsafe { libc::waitpid(pid, &mut st, if nohang { libc::WNOHANG } else { 0 }) };
        if r == pid {
            return Some(Exit::from_status(st));
        }
        if r == -1 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        return None;
    }
}

/// 收掉此刻退了的任何子进程（节点主循环用；收养来的孤儿也在这里收）。返回 (pid, 退出法)。
pub fn reap_any() -> Vec<(i32, Exit)> {
    let mut out = Vec::new();
    loop {
        let mut st = 0;
        let r = unsafe { libc::waitpid(-1, &mut st, libc::WNOHANG) };
        if r <= 0 {
            return out;
        }
        out.push((r, Exit::from_status(st)));
    }
}

/// 让这个进程收养子孙里的孤儿（它们退了由我们回收，不留僵尸）。
pub fn become_subreaper() {
    unsafe {
        libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0);
    }
}

// ════════════════ 起一趟 ════════════════

/// 起一趟要的。
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub argv: Vec<OsString>,
    /// 叠在节点自己的环境上（作业帧的 · 节点设的三类），后写的盖前写的。
    pub env: Vec<(OsString, OsString)>,
    pub cwd: PathBuf,
    pub nice: i32,
    /// 作业的 RLIMIT_NPROC 软限（账号线程数到这里内核就不给它 fork）；None ⇒ 不改。
    pub nproc_soft: Option<u64>,
    /// 这趟的记号（`DEVSCHED_SKIPPED` 的值）。
    pub tag: OsString,
}

/// 起进程没成。
#[derive(Debug)]
pub enum SpawnError {
    /// 资源不够（EAGAIN · ENOMEM）：节点当警报，停掉自己的全部作业。
    NoProcess(io::Error),
    Other(io::Error),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::NoProcess(e) => write!(f, "起不了进程：{e}"),
            SpawnError::Other(e) => write!(f, "起作业没成：{e}"),
        }
    }
}

pub fn classify_spawn_error(e: io::Error) -> SpawnError {
    match e.raw_os_error() {
        Some(libc::EAGAIN) | Some(libc::ENOMEM) => SpawnError::NoProcess(e),
        _ => SpawnError::Other(e),
    }
}

/// 垫片在子进程（fork 之后、exec 之前）里做的：新会话当组长 · 父进程死亡信号 · 线程软限 · nice。
fn child_setup(
    parent: i32,
    deathsig: i32,
    nproc_soft: Option<u64>,
    nice: i32,
    new_session: bool,
) -> io::Result<()> {
    unsafe {
        if new_session && libc::setsid() == -1 {
            return Err(io::Error::last_os_error());
        }
        libc::prctl(libc::PR_SET_PDEATHSIG, deathsig as libc::c_ulong, 0, 0, 0);
        // 父进程在 fork 与 prctl 之间就没了：信号不会再来 ⇒ 自己不起
        if libc::getppid() != parent {
            libc::_exit(1);
        }
        if let Some(n) = nproc_soft {
            let mut rl = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_NPROC, &mut rl) == 0 {
                let n = n as libc::rlim_t;
                if rl.rlim_cur == libc::RLIM_INFINITY || n < rl.rlim_cur {
                    rl.rlim_cur = if rl.rlim_max != libc::RLIM_INFINITY && n > rl.rlim_max {
                        rl.rlim_max
                    } else {
                        n
                    };
                    libc::setrlimit(libc::RLIMIT_NPROC, &rl);
                }
            }
        }
        if nice != 0 {
            libc::setpriority(libc::PRIO_PROCESS, 0, nice);
        }
    }
    Ok(())
}

/// 起一趟：`shim` 是节点自己的二进制（节点里用 `/proc/self/exe`），以 `--shim -- <命令…>` 起。
/// 返回垫片的进程号，也就是这趟的组号。垫片要从活得跟节点一样久的线程里起（父进程死亡信号认的是起它的那条线程）。
pub fn spawn_job(
    shim: &Path,
    spec: &JobSpec,
    stdout: Stdio,
    stderr: Stdio,
) -> Result<i32, SpawnError> {
    let parent = std::process::id() as i32;
    let mut c = Command::new(shim);
    c.arg("--shim").arg("--").args(&spec.argv);
    // 环境只有作业帧带来的与节点设的三类（不带节点自己的环境出去）
    c.env_clear()
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    for (k, v) in &spec.env {
        c.env(k, v);
    }
    c.env(SKIPPED_VAR, &spec.tag);
    let (soft, nice) = (spec.nproc_soft, spec.nice);
    unsafe {
        c.pre_exec(move || child_setup(parent, libc::SIGTERM, soft, nice, true));
    }
    let child = c.spawn().map_err(classify_spawn_error)?;
    // 不经 std 的 Child 等：回收统一由 wait_pid / reap_any 做
    Ok(child.id() as i32)
}

/// 起一个跟着节点活的常驻（sccache server）：新会话当组长、父进程死亡信号 SIGTERM、线程软限压到 `nproc_soft`（同作业）、
/// 环境只放 `env`、不带任何一趟的记号。
/// 跟垫片一样要从活得跟节点一样久的线程里起。返回进程号（也是组号）；回收归 `reap_any`。
pub fn spawn_detached(
    exe: &Path,
    env: &[(OsString, OsString)],
    nproc_soft: Option<u64>,
    stdout: Stdio,
    stderr: Stdio,
) -> io::Result<i32> {
    let parent = std::process::id() as i32;
    let mut c = Command::new(exe);
    c.env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    for (k, v) in env {
        c.env(k, v);
    }
    unsafe {
        c.pre_exec(move || child_setup(parent, libc::SIGTERM, nproc_soft, 0, true));
    }
    Ok(c.spawn()?.id() as i32)
}

// ════════════════ 垫片 ════════════════

static SHIM_SIG: AtomicI32 = AtomicI32::new(0);

extern "C" fn on_sig(sig: libc::c_int) {
    SHIM_SIG.store(sig, Ordering::SeqCst);
}

/// 垫片起命令时 fork 失败（资源不够）留的记号文件：放在这趟临时目录里，节点收尾看它报「起不了进程」。
pub fn shim_nospawn_marker(tag: &OsStr) -> PathBuf {
    Path::new(tag).with_file_name("shim-nospawn")
}

/// 命令没跑成时账号离这趟的线（垫片的 RLIMIT_NPROC 软限）不到这么多条，就算它撞了线：
/// 命令撞线到垫片看见它退出那几毫秒里，账号里别的进程起落会让账号落回线下：一份多线程的进程（rustc · 测试二进制）一走就是二十几条。
pub const ATLINE_SLACK: u64 = 32;

/// 账号离这趟的线不到 `ATLINE_SLACK` 条吗：软限临时再压低那么多，试 fork 一下（子进程立刻退、当场收），EAGAIN ⇒ 是。
/// 软限无限（这趟没设线）⇒ 不是。垫片是单线程的，直接 fork 安全。
fn near_line() -> bool {
    let mut rl = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_NPROC, &mut rl) } != 0
        || rl.rlim_cur == libc::RLIM_INFINITY
    {
        return false;
    }
    let lowered = libc::rlimit {
        rlim_cur: rl.rlim_cur.saturating_sub(ATLINE_SLACK as libc::rlim_t),
        rlim_max: rl.rlim_max,
    };
    unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &lowered) };
    let refused = fork_refused();
    unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &rl) };
    refused
}

/// 试 fork 一下：fork 一个立刻退出的子进程（不 exec，子进程里只调 `_exit`）、当场收掉；起不来回 fork 的错。
/// 节点（还起不起得了进程）与垫片（撞没撞这趟的软限）都用它。
pub fn try_fork() -> io::Result<()> {
    // SAFETY: 子进程里只调异步信号安全的 `_exit`
    unsafe {
        let p = libc::fork();
        if p == 0 {
            libc::_exit(0);
        }
        if p < 0 {
            return Err(io::Error::last_os_error());
        }
        libc::waitpid(p, std::ptr::null_mut(), 0);
    }
    Ok(())
}

/// EAGAIN ⇒ 账号已到这个进程的 RLIMIT_NPROC 软限。
fn fork_refused() -> bool {
    try_fork().is_err_and(|e| e.raw_os_error() == Some(libc::EAGAIN))
}

/// 命令没跑成、账号还贴着这趟的线（`near_line`）时留的记号：节点报「线程上限」。
pub fn shim_atline_marker(tag: &OsStr) -> PathBuf {
    Path::new(tag).with_file_name("shim-atline")
}

/// `devsched-node --shim -- <命令…>`：当组长、起命令、等它；收到信号或命令退了 ⇒ 停掉同组 · 同记号的其余进程，按命令的退出法退。
pub fn run_shim(argv: &[OsString]) -> i32 {
    let me = std::process::id() as i32;
    unsafe {
        for s in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_sig as *const () as usize;
            sa.sa_flags = 0; // 不带 SA_RESTART：waitpid 被打断才看得见
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(s, &sa, std::ptr::null_mut());
        }
    }
    become_subreaper();
    let tag = std::env::var_os(SKIPPED_VAR);
    let Some((prog, args)) = argv.split_first() else {
        eprintln!("devsched-node --shim：没有命令");
        return 2;
    };
    let mut c = Command::new(prog);
    c.args(args);
    unsafe {
        c.pre_exec(move || child_setup(me, libc::SIGKILL, None, 0, false));
    }
    let child = match c.spawn() {
        Ok(ch) => ch.id() as i32,
        Err(e) => {
            let code = match e.raw_os_error() {
                Some(libc::EAGAIN) | Some(libc::ENOMEM) => {
                    if let Some(t) = &tag {
                        let _ = fs::write(shim_nospawn_marker(t), e.to_string());
                    }
                    126
                }
                Some(libc::ENOENT) => 127,
                _ => 126,
            };
            eprintln!("devsched-node：起不了 {}：{e}", prog.to_string_lossy());
            return code;
        }
    };
    let exit = loop {
        if SHIM_SIG.load(Ordering::SeqCst) != 0 {
            break None;
        }
        // 先只看谁退了、不收（WNOWAIT）：命令退了时它的僵尸还占着账号的一个名额，下面试 fork 才和它撞线那一刻一样
        let mut si: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let r = unsafe { libc::waitid(libc::P_ALL, 0, &mut si, libc::WEXITED | libc::WNOWAIT) };
        if r == -1 {
            if SHIM_SIG.load(Ordering::SeqCst) != 0 {
                break None;
            }
            if io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break None;
        }
        let pid = unsafe { si.si_pid() };
        if pid != child {
            // 收养来的孤儿：当场收掉，不留僵尸
            unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
            continue;
        }
        let failed = !(si.si_code == libc::CLD_EXITED && unsafe { si.si_status() } == 0);
        // 命令没跑成：此刻（它的僵尸与它起的进程都还在）账号还贴着这趟的线 ⇒ 是内核在这趟的软限上拒了它，留记号给节点报「线程上限」
        if failed && near_line() {
            if let Some(t) = &tag {
                let _ = fs::write(shim_atline_marker(t), "命令没跑成时账号贴着线");
            }
        }
        let mut st = 0;
        unsafe { libc::waitpid(child, &mut st, 0) };
        break Some(Exit::from_status(st));
    };
    // 收尾：同组 · 同记号的其余进程停掉；收养来的孤儿收掉（停到没人为止，多扫几圈防扫的时候又生出来的）
    for _ in 0..20 {
        let n = kill_group(me, tag.as_deref(), me);
        unsafe { while libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) > 0 {} }
        if n == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    match exit {
        Some(Exit::Code(c)) => c,
        Some(Exit::Signal(s)) => unsafe {
            // 照命令的死法死（默认动作是退出的那几种）；不成再退 128 ＋ 信号
            libc::signal(s, libc::SIG_DFL);
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, s);
            libc::sigprocmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
            libc::kill(me, s);
            128 + s
        },
        None => 128 + SHIM_SIG.load(Ordering::SeqCst),
    }
}

// ════════════════ 进程记录与残留 ════════════════

/// `state/jobs/<短号>`：`组号 起来的时刻 记号`，一行。
pub fn write_record(jobs_dir: &Path, short: &str, pgid: i32, tag: &OsStr) -> io::Result<()> {
    let start = stat(pgid).map(|s| s.start).unwrap_or(0);
    let mut line = format!("{pgid} {start} ").into_bytes();
    line.extend_from_slice(tag.as_bytes());
    line.push(b'\n');
    fs::write(jobs_dir.join(short), line)
}

pub fn remove_record(jobs_dir: &Path, short: &str) {
    let _ = fs::remove_file(jobs_dir.join(short));
}

/// 起来先清上次残留：记录里还活着的组（组长起来的时刻对得上才算同一个）停掉、按记号扫一遍，记录删掉。返回停了几组。
pub fn clear_residue(jobs_dir: &Path) -> usize {
    let Ok(rd) = fs::read_dir(jobs_dir) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if let Ok(b) = fs::read(&p) {
            let line = b.strip_suffix(b"\n").unwrap_or(&b);
            let mut it = line.splitn(3, |c| *c == b' ');
            let pgid: i32 = it
                .next()
                .and_then(|x| std::str::from_utf8(x).ok()?.parse().ok())
                .unwrap_or(0);
            let start: u64 = it
                .next()
                .and_then(|x| std::str::from_utf8(x).ok()?.parse().ok())
                .unwrap_or(0);
            let tag = it
                .next()
                .filter(|t| !t.is_empty())
                .map(|t| OsStr::from_bytes(t).to_os_string());
            let alive = pgid > 1 && stat(pgid).is_some_and(|s| s.start == start && s.pgrp == pgid);
            let swept = if alive {
                kill_group(pgid, tag.as_deref(), 0)
            } else {
                // 组长没了、组号可能已被别人用：只按记号扫
                sweep(0, tag.as_deref(), 0)
            };
            if alive || swept > 0 {
                n += 1;
            }
        }
        let _ = fs::remove_file(&p);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root::testdir::TestDir;
    use std::time::{Duration, Instant};

    fn gone(pid: i32) -> bool {
        // 僵尸也算没了（等它的父进程收）
        stat(pid).map(|s| s.state == b'Z').unwrap_or(true)
    }

    fn wait_gone(pids: &[i32]) -> bool {
        let t = Instant::now();
        while t.elapsed() < devsched_wire::WALL_CEILING {
            if pids.iter().all(|p| gone(*p)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// 起一个自成一组的 `sh -c`，印出后台那几个的进程号；返回 (组长, 后台的)。
    fn spawn_group(script: &str, env: &[(&str, &OsStr)]) -> (i32, Vec<i32>) {
        let mut c = Command::new("sh");
        c.arg("-c")
            .arg(script)
            .stdout(Stdio::piped())
            .stdin(Stdio::null());
        for (k, v) in env {
            c.env(k, v);
        }
        unsafe {
            c.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let mut ch = c.spawn().unwrap();
        let mut out = ch.stdout.take().unwrap();
        let mut line = Vec::new();
        let mut b = [0u8; 1];
        use std::io::Read;
        while out.read(&mut b).unwrap() == 1 && b[0] != b'\n' {
            line.push(b[0]);
        }
        let kids = String::from_utf8(line)
            .unwrap()
            .split_whitespace()
            .map(|x| x.parse().unwrap())
            .collect();
        let pid = ch.id() as i32;
        std::mem::forget(ch);
        (pid, kids)
    }

    #[test]
    fn parse_stat_handles_parens_in_name() {
        let s = "42 (a (b) c) S 1 42 42 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 3 0 12345 0 0";
        let st = parse_stat(42, s).unwrap();
        assert_eq!(
            (st.state, st.ppid, st.pgrp, st.session, st.threads, st.start),
            (b'S', 1, 42, 42, 3, 12345)
        );
        assert!(parse_stat(1, "garbage").is_none());
    }

    #[test]
    fn kill_group_takes_whole_group_and_tagged_escapee() {
        // 记号带本进程号：几棵树同时跑这份测试时，别的那份按记号停的不是这一份的进程
        let tag = OsString::from(format!("/x/t/{}/skipped", std::process::id()));
        // 组里一个后台 sleep，另一个 setsid 出了组、只靠记号认
        let (leader, kids) = spawn_group(
            "sleep 300 & a=$!; setsid sleep 300 & b=$!; echo $a $b; wait",
            &[(SKIPPED_VAR, tag.as_os_str())],
        );
        assert_eq!(kids.len(), 2);
        let escaped = kids[1];
        // setsid 可能还没生效，等它离开这一组
        let t = Instant::now();
        while stat(escaped).map(|s| s.pgrp) == Some(leader)
            && t.elapsed() < devsched_wire::WALL_CEILING
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let (procs, _) = count_in_groups(&[leader]);
        assert!(procs >= 2, "组里至少组长和一个 sleep：{procs}");
        kill_group(leader, Some(&tag), 0);
        wait_pid(leader, false);
        assert!(wait_gone(&kids), "组里的与漏出组带记号的都该停掉");
    }

    #[test]
    fn residue_from_records_is_cleared() {
        let d = TestDir::new("residue");
        let jobs = d.0.join("jobs");
        fs::create_dir_all(&jobs).unwrap();
        let tag = OsString::from(d.0.join("t/zz01/skipped"));
        let (leader, kids) = spawn_group(
            "sleep 300 & echo $!; wait",
            &[(SKIPPED_VAR, tag.as_os_str())],
        );
        write_record(&jobs, "zz01", leader, &tag).unwrap();
        // 一份对不上的旧记录（组长早没了）也一并删掉
        fs::write(jobs.join("old1"), "999999 1 /nowhere/skipped\n").unwrap();
        assert!(clear_residue(&jobs) >= 1);
        wait_pid(leader, false);
        assert!(wait_gone(&kids));
        assert_eq!(fs::read_dir(&jobs).unwrap().count(), 0);
    }

    #[test]
    fn spawn_error_classes() {
        assert!(matches!(
            classify_spawn_error(io::Error::from_raw_os_error(libc::EAGAIN)),
            SpawnError::NoProcess(_)
        ));
        assert!(matches!(
            classify_spawn_error(io::Error::from_raw_os_error(libc::ENOENT)),
            SpawnError::Other(_)
        ));
    }

    #[test]
    fn exit_code_folds_signal() {
        assert_eq!(Exit::Code(3).code(), 3);
        assert_eq!(Exit::Signal(9).code(), 137);
    }
}
