//! 后端起子进程的**唯一原语**：生产段的 `Command::new` 只住这里。
//!
//! 四种用法：[`Child::run`]（起它、等它，**期限必填**）· [`Child::detach`]（脱离起、不等）·
//! [`Child::exec_replace`]（ccm 最终那一跳：POSIX 就地 exec，Windows 起它、等它、透传退出码）·
//! [`Child::stream`]（长寿、读它的输出流，拿着的那一方放手 ⇒ 杀整组、收尸：终端实时预览的 tmux 控制模式客户端）。
//!
//! # 环境
//!
//! 继承来的 [`OWN_ENVS`]（起常驻后端的那一方只交给它自己用的那几格）**无条件摘掉**，没有开关。
//! 显式往下交自有格只有 [`Child::pass_own`] 一个口；通用的 [`Child::env`] 遇到自有键拒收。
//!
//! # 期限
//!
//! `run` 把子进程放进它自己的进程组（Windows：一个 Job）。等退出与读两条输出分在不同线程；
//! 发起的那一侧只做**一次有界等待**。到点 ⇒ 杀整组 / 终止整个 Job ⇒ 直接子进程被收尸 ⇒ 回 [`ChildFail::TimedOut`]。
//! 逃出组的孙进程（自己 `setsid` 的）若攥着输出管道，读线程放手、记一行日志，不拖住发起方。
//! 期限值归发起方：常量住调用点模块，类型是 [`Deadline`]（只交给 `run`，不对外换成时长）。
//!
//! # 整条命令的总期限
//!
//! 一条阻塞档命令里连发几发时，分派那一层装一个 [`Budget`]：各发共用剩下的时间（`run` 取「自己的期限」与「剩下的」里小的那个），
//! 没剩 ⇒ 不起、直接回超时。总期限 ＝ 后端登记的上限与发起方给的截止时刻（[`Until`]）里早的那个：发起方放手之前，先答的是后端那句准话。

use crate::platform::child_env::OWN_ENVS;
use copy_core::copy_text;
use std::cell::Cell;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::marker::PhantomData;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 一次 [`Child::run`] 的期限。只由 [`Deadline::secs`] / [`Deadline::millis`] 造，只在本模块里换成时长。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Deadline(Duration);

impl Deadline {
    pub(crate) const fn millis(n: u64) -> Deadline {
        Deadline(Duration::from_millis(n))
    }

    pub(crate) const fn secs(n: u64) -> Deadline {
        Deadline::millis(n * 1000)
    }

    /// 给文案用的秒数（向上取整）。
    pub(crate) fn shown_secs(self) -> u64 {
        u64::try_from(self.0.as_millis().div_ceil(1000)).unwrap_or(u64::MAX)
    }
}

/// 一条阻塞档命令的**总期限**：这条命令里各发子进程共用剩下的时间。
///
/// 挂在执行这条命令的线程上（阻塞档一条命令占一根线程），**只由分派那一层装**（[`Budget::capped`]），命令本身不装；
/// 守卫掉了就还原成装之前的样子（线程池复用这根线程跑下一条命令时不残留）。嵌套只收紧、不放宽。
#[must_use] // 总期限只在守卫活着时有效
pub(crate) struct Budget {
    prev: Option<Span>,
    /// 不许跨线程交出去（还原的是装它的那根线程）。
    _here: PhantomData<*const ()>,
}

/// 这根线程此刻的总期限：报给人看的总长 · 到点时刻。
#[derive(Debug, Clone, Copy)]
struct Span {
    total: Deadline,
    ends: Instant,
}

thread_local! {
    static SPAN: Cell<Option<Span>> = const { Cell::new(None) };
}

/// 发起方给的截止时刻：分派那一层收到请求时把「发起方期限 − 余量」换成的绝对时刻（之后只收紧、不重新计时）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Until(Instant);

impl Until {
    /// 从现在起 `d`；远到钟面装不下 ⇒ `None`（等于没给）。
    pub(crate) fn after(d: Deadline) -> Option<Until> {
        now().checked_add(d.0).map(Until)
    }
}

impl Budget {
    /// 按上限 `cap` 装；发起方给了截止时刻 ⇒ 取上限与「到那一刻还剩多少」里小的那个（已过 ⇒ 零：一发都不起）。
    pub(crate) fn capped(cap: Deadline, until: Option<Until>) -> Budget {
        let total = match until {
            Some(Until(t)) => Deadline(cap.0.min(t.saturating_duration_since(now()))),
            None => cap,
        };
        Budget::start(total)
    }

    /// 从现在起 `total`。这根线程上已有更紧的 ⇒ 沿用那个（只收紧、不放宽）。
    fn start(total: Deadline) -> Budget {
        let mine = Span {
            total,
            ends: now() + total.0,
        };
        let prev = SPAN.get();
        SPAN.set(Some(match prev {
            Some(p) if p.ends <= mine.ends => p,
            _ => mine,
        }));
        Budget {
            prev,
            _here: PhantomData,
        }
    }
}

impl Drop for Budget {
    fn drop(&mut self) {
        SPAN.set(self.prev);
    }
}

impl Span {
    /// 这根线程上装着的那一个（没装 ⇒ `None`）。
    fn here() -> Option<Span> {
        SPAN.get()
    }

    /// 还剩多少；没剩 ⇒ `None`。
    fn remaining(self) -> Option<Deadline> {
        self.ends
            .checked_duration_since(now())
            .filter(|d| !d.is_zero())
            .map(Deadline)
    }
}

/// 读一次单调钟（后端生产段唯一一处）：只用来算总期限还剩多少，不驱动任何等待。
fn now() -> Instant {
    std::time::Instant::now()
}

/// 超时杀组之后，等读线程收尾的宽限：过了仍没收尾 ⇒ 输出管道被逃出组的进程攥着，放手。
const DRAIN_AFTER_KILL: Deadline = Deadline::millis(1_000);

/// 起不成 / 等不到。
#[derive(Debug)]
pub(crate) enum ChildFail {
    /// 程序不在（`ENOENT` 一类）。
    NotFound(std::io::Error),
    /// 到了期限仍没结束：整组已杀、直接子进程已收。
    TimedOut { program: String, after: Deadline },
    /// 其余 IO 错（含本平台没有这一格：`ErrorKind::Unsupported`）。
    Io(std::io::Error),
}

impl std::fmt::Display for ChildFail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChildFail::NotFound(e) | ChildFail::Io(e) => write!(f, "{e}"),
            ChildFail::TimedOut { program, after } => f.write_str(&copy_text(
                "beChild.run.timedOut",
                &[
                    ("program", program.as_str()),
                    (
                        "dur",
                        &copy_core::format_duration(after.shown_secs().saturating_mul(1000)),
                    ),
                ],
            )),
        }
    }
}

impl ChildFail {
    fn of_spawn(e: std::io::Error) -> ChildFail {
        if e.kind() == std::io::ErrorKind::NotFound {
            ChildFail::NotFound(e)
        } else {
            ChildFail::Io(e)
        }
    }

    pub(crate) fn is_timed_out(&self) -> bool {
        matches!(self, ChildFail::TimedOut { .. })
    }

    /// 阻塞档命令把这次失败交成命令级 `(码, 话)`：超时 ⇒ [`TIMED_OUT`] ＋ 原语的话；其余 ⇒ `other` ＋ 调用方的话。
    pub(crate) fn into_cmd_err(
        self,
        other: &'static str,
        said: impl FnOnce(&ChildFail) -> String,
    ) -> (&'static str, String) {
        match self {
            ChildFail::TimedOut { .. } => (TIMED_OUT, self.to_string()),
            _ => (other, said(&self)),
        }
    }
}

impl ChildFail {
    /// 同 [`ChildFail::into_cmd_err`]，但句子里只给原因词（`said` 拿到的是 [`copy_core::spawn_reason`] 那一词），
    /// 系统原话另带（进复制详情）。超时那一档照旧：原语那句就是成品，没有原话。
    pub(crate) fn into_cmd_said(
        self,
        other: &'static str,
        said: impl FnOnce(&str) -> String,
    ) -> (&'static str, copy_core::said::Said) {
        match self {
            ChildFail::TimedOut { .. } => {
                (TIMED_OUT, copy_core::said::Said::from(self.to_string()))
            }
            ChildFail::NotFound(e) | ChildFail::Io(e) => (
                other,
                copy_core::said::Said::with_raw(said(&copy_core::spawn_reason(e.kind())), &e),
            ),
        }
    }
}

/// 命令级码：这条命令起的子进程过了期限没结束（已杀整组）。
pub(crate) const TIMED_OUT: &str = "child_timed_out";

/// 起子进程前最后一道：生产构建里是空的；测试构建换成 `tests/` 那一份（裸名 `tmux` 落到本进程自己的空 socket 目录）。
#[cfg_attr(test, path = "../../../tests/backend/platform/child_tmux_fence.rs")]
mod tmux_fence;

/// 要起的那个子进程（程序 · argv · 显式环境）。stdin 恒为空设备（今天没有调用方要喂它）。
pub(crate) struct Child {
    program: OsString,
    args: Vec<OsString>,
    /// `Some(键)` ⇒ 先清空，只从宿主继承这几键（自有格列了也不继承）。
    inherit_only: Option<Vec<&'static str>>,
    /// 显式交的：`Some` 设、`None` 摘。排在继承之后（显式压过继承）。
    envs: Vec<(OsString, Option<OsString>)>,
    no_console_window: bool,
    /// 工作目录；`None` ＝ 照宿主的。
    cwd: Option<std::path::PathBuf>,
}

impl Child {
    pub(crate) fn new(program: impl AsRef<OsStr>) -> Child {
        Child {
            program: program.as_ref().to_os_string(),
            args: Vec::new(),
            inherit_only: None,
            envs: Vec::new(),
            no_console_window: false,
            cwd: None,
        }
    }

    /// 在这个目录里起它。
    pub(crate) fn current_dir(mut self, dir: impl AsRef<std::path::Path>) -> Child {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    pub(crate) fn arg(mut self, a: impl AsRef<OsStr>) -> Child {
        self.args.push(a.as_ref().to_os_string());
        self
    }

    pub(crate) fn args<I, S>(mut self, it: I) -> Child
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(it.into_iter().map(|a| a.as_ref().to_os_string()));
        self
    }

    /// 显式交一格环境。自有格（[`OWN_ENVS`]）不收：那一类只经 [`Child::pass_own`]。
    pub(crate) fn env(mut self, k: impl AsRef<OsStr>, v: impl AsRef<OsStr>) -> Child {
        let k = k.as_ref();
        if is_own(k) {
            debug_assert!(false, "自有环境 {k:?} 只经 pass_own 交");
            tracing::warn!("起 {:?}：自有环境 {k:?} 经通用口交，已忽略", self.program);
            return self;
        }
        self.envs
            .push((k.to_os_string(), Some(v.as_ref().to_os_string())));
        self
    }

    pub(crate) fn env_remove(mut self, k: impl AsRef<OsStr>) -> Child {
        self.envs.push((k.as_ref().to_os_string(), None));
        self
    }

    /// 显式往下交一格**自有**环境（只有起常驻后端那一处要）。`key` 必须在 [`OWN_ENVS`] 里。
    pub(crate) fn pass_own(mut self, key: &str, value: impl AsRef<OsStr>) -> Child {
        if !OWN_ENVS.contains(&key) {
            debug_assert!(false, "{key} 不是自有环境");
            tracing::warn!("起 {:?}：{key} 不是自有环境，pass_own 忽略", self.program);
            return self;
        }
        self.envs
            .push((OsString::from(key), Some(value.as_ref().to_os_string())));
        self
    }

    /// 清空环境，只从宿主继承 `keys` 里的这几键（自有格列了也不继承）。显式交的照旧排在后面。
    pub(crate) fn inherit_only(mut self, keys: &[&'static str]) -> Child {
        self.inherit_only = Some(keys.to_vec());
        self
    }

    /// Windows：不给控制台程序新开黑框（`CREATE_NO_WINDOW`）。别处什么都不做。
    pub(crate) fn no_console_window(mut self) -> Child {
        self.no_console_window = true;
        self
    }

    fn label(&self) -> String {
        std::path::Path::new(&self.program)
            .file_name()
            .unwrap_or(&self.program)
            .to_string_lossy()
            .into_owned()
    }

    /// 判据看要起的那条命令（程序 · argv · 环境）。
    #[cfg(test)]
    pub(crate) fn built(&self) -> Command {
        self.command()
    }

    fn command(&self) -> Command {
        let mut c = Command::new(&self.program);
        c.args(&self.args);
        if let Some(keys) = &self.inherit_only {
            c.env_clear();
            for k in keys.iter().filter(|k| !is_internal_name(k)) {
                if let Some(v) = std::env::var_os(k) {
                    c.env(k, v);
                }
            }
        }
        for k in OWN_ENVS {
            c.env_remove(k);
        }
        // 后端内部那几族（不论谁交的、是不是本进程自己的）一律不往下传。
        for (k, _) in std::env::vars_os() {
            if is_internal_name(&k.to_string_lossy()) {
                c.env_remove(&k);
            }
        }
        for (k, v) in &self.envs {
            match v {
                Some(v) => c.env(k, v),
                None => c.env_remove(k),
            };
        }
        if let Some(d) = &self.cwd {
            c.current_dir(d);
        }
        if self.no_console_window {
            os::no_console_window(&mut c);
        }
        tmux_fence::apply(&self.program, &self.envs, &mut c);
        c
    }

    /// 起它、等它、收两条输出；**期限必填**。到点 ⇒ 杀整组（Windows：终止 Job）、收尸、回 `TimedOut`。
    /// 装着总期限（[`Budget`]）⇒ 只等「自己的期限」与「剩下的」里小的那个；没剩 ⇒ 不起、直接回 `TimedOut`。
    /// 被总期限截短的那一次超时，话里报总期限（整条命令等了这么久）。
    pub(crate) fn run(self, within: Deadline) -> Result<Output, ChildFail> {
        let program = self.label();
        let (within, said) = match Span::here() {
            None => (within, within),
            Some(s) => match s.remaining() {
                None => {
                    return Err(ChildFail::TimedOut {
                        program,
                        after: s.total,
                    })
                }
                Some(left) if left.0 < within.0 => (left, s.total),
                Some(_) => (within, within),
            },
        };
        let mut cmd = self.command();
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        os::own_group(&mut cmd);
        let mut child = cmd.spawn().map_err(ChildFail::of_spawn)?;
        let group = os::Group::adopt(&child);

        // 读两条输出（一个线程一条），读完交 `Ev::Piped`。
        let (ev_tx, ev_rx) = mpsc::channel::<Ev>();
        let (out, err) = (child.stdout.take(), child.stderr.take());
        let piped_tx = ev_tx.clone();
        std::thread::spawn(move || {
            let e = std::thread::spawn(move || read_all(err));
            let o = read_all(out);
            let e = e.join().unwrap_or_default();
            let _ = piped_tx.send(Ev::Piped(o, e));
        });

        // 等退出（不收尸）→ 等输出读完或发起方放弃 → 收尸。收尸与杀组互斥：收尸之前 pid 仍被占着，杀组不会误伤。
        let reaped = Arc::new(Mutex::new(false));
        let (done_tx, done_rx) = mpsc::channel::<std::io::Result<Output>>();
        let reaped_w = Arc::clone(&reaped);
        let label = program.clone();
        std::thread::spawn(move || {
            os::exited_unreaped(&mut child);
            let ev = ev_rx.recv();
            let status = {
                let mut r = reaped_w.lock().unwrap_or_else(|p| p.into_inner());
                *r = true;
                child.wait()
            };
            match ev {
                Ok(Ev::Piped(o, e)) => {
                    let _ = done_tx.send(status.map(|s| Output {
                        status: s,
                        stdout: o,
                        stderr: e,
                    }));
                }
                Ok(Ev::GaveUp) | Err(_) => {
                    if !matches!(ev_rx.recv_timeout(DRAIN_AFTER_KILL.0), Ok(Ev::Piped(..))) {
                        tracing::warn!(
                            "{label} 超时已杀组，输出管道仍被逃出组的进程攥着 ⇒ 读线程放手"
                        );
                    }
                }
            }
        });

        match done_rx.recv_timeout(within.0) {
            Ok(r) => r.map_err(ChildFail::Io),
            Err(RecvTimeoutError::Timeout) => {
                {
                    let r = reaped.lock().unwrap_or_else(|p| p.into_inner());
                    // 已收尸 ⇒ 等待线程已拿到输出、答案正在路上；不再杀（组号可能已不归它）。
                    if !*r {
                        group.kill();
                        let _ = ev_tx.send(Ev::GaveUp);
                    }
                }
                Err(ChildFail::TimedOut {
                    program,
                    after: said,
                })
            }
            Err(RecvTimeoutError::Disconnected) => Err(ChildFail::Io(std::io::Error::other(
                copy_text("beChild.run.lost", &[("program", program.as_str())]),
            ))),
        }
    }

    /// 脱离起（自成进程组，stdio 全空），不等；回它的 pid。本平台不能脱离（[`cannot_detach`]）：`Io(Unsupported)`，带那一句。
    pub(crate) fn detach(self) -> Result<u32, ChildFail> {
        let mut cmd = self.command();
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        os::detach(&mut cmd)?;
        cmd.spawn().map(|c| c.id()).map_err(ChildFail::of_spawn)
    }

    /// ccm 最终那一跳：POSIX 就地 exec（成功不返回）；Windows 起它、等它、交回它的退出码（由 `main` 退，模块里不退进程）。
    /// 起不来 ⇒ `Err`（调用方用自己的主语说）。stdio 照旧继承。
    /// `started(pid)`：将要跑这个程序的那个进程的 pid —— POSIX 是自己（exec 之前调），Windows 是起好的子进程（等它之前调）。
    pub(crate) fn exec_replace(self, started: &dyn Fn(u32)) -> Result<i32, ChildFail> {
        os::exec_replace(self.command(), started)
    }
}

/// 本平台能不能「脱离当前进程单独跑」（常驻后端的前提）：能 ⇒ `None`；不能 ⇒ 那一句话（Windows）。
/// 判只住这一处：[`Child::detach`] 与常驻那两个入口（`--resident-ensure` · 带常驻开关的流模式，经 `control::resident::unsupported_here`）都问它。
pub(crate) fn cannot_detach() -> Option<String> {
    os::cannot_detach()
}

/// [`Child::stream`] 起的那个长寿子进程：stdout 交给读的那一方，stdin 一直开着（有的程序见 stdin 关了就退）。
/// **放手即收**：`Drop` ⇒ 杀整组（Windows：终止 Job）、收尸。没有期限 —— 它活多久由拿着它的那一方定，不是节拍。
pub(crate) struct Streaming {
    child: std::process::Child,
    group: os::Group,
    _stdin: Option<std::process::ChildStdin>,
}

impl Streaming {
    /// 它的输出流（只给一次）。读到头 ＝ 它退了。
    pub(crate) fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.child.stdout.take()
    }
}

// Windows 的 `Group` 里是两个内核句柄（Job · 直接子进程，后者归同一结构里的 `child`），裸指针让它不是 Send；
// 内核句柄不认线程，整个 `Streaming` 一起挪到读它的线程上用、在那边 Drop 是安全的（终端订阅就这么用）。
unsafe impl Send for Streaming {}

impl Drop for Streaming {
    fn drop(&mut self) {
        // 收尸之前它的 pid 一直占着 ⇒ 杀组不会落到别的组上（它自己先退了也一样：僵尸照样占着）。
        self.group.kill();
        let _ = self.child.wait();
    }
}

impl Child {
    /// 起一个长寿子进程、读它的输出流（stderr 丢掉、stdin 开着不写）；自成进程组（Windows：一个 Job）。
    /// 回来的 [`Streaming`] 一放手就杀组收尸。
    pub(crate) fn stream(self) -> Result<Streaming, ChildFail> {
        let mut cmd = self.command();
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        os::own_group(&mut cmd);
        let mut child = cmd.spawn().map_err(ChildFail::of_spawn)?;
        let group = os::Group::adopt(&child);
        let stdin = child.stdin.take();
        Ok(Streaming {
            child,
            group,
            _stdin: stdin,
        })
    }
}

enum Ev {
    Piped(Vec<u8>, Vec<u8>),
    GaveUp,
}

fn read_all(r: Option<impl Read>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(mut r) = r {
        let _ = r.read_to_end(&mut buf);
    }
    buf
}

fn is_own(k: &OsStr) -> bool {
    k.to_str().is_some_and(|k| {
        OWN_ENVS
            .iter()
            .any(|o| crate::platform::child_env::same_name(o, k))
    })
}

fn is_internal_name(k: &str) -> bool {
    crate::platform::child_env::is_internal(k)
}

#[cfg(unix)]
mod os {
    use super::ChildFail;
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    pub(super) fn own_group(cmd: &mut Command) {
        cmd.process_group(0);
    }

    /// 这一组的组号（= 直接子进程的 pid，`process_group(0)` 的结果）。
    pub(super) struct Group(libc::pid_t);

    impl Group {
        pub(super) fn adopt(child: &std::process::Child) -> Group {
            Group(child.id() as libc::pid_t)
        }

        /// 整组 SIGKILL。只在直接子进程还没被收尸时调（组号仍被它占着，不会落到别的组上）。
        pub(super) fn kill(&self) {
            unsafe {
                libc::kill(-self.0, libc::SIGKILL);
            }
        }
    }

    /// 等直接子进程退出但**不收尸**（`WNOWAIT`）：收尸之前它的 pid 一直占着，杀组那一下不会误伤。
    pub(super) fn exited_unreaped(child: &mut std::process::Child) {
        let pid = child.id() as libc::id_t;
        loop {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let r =
                unsafe { libc::waitid(libc::P_PID, pid, &mut info, libc::WEXITED | libc::WNOWAIT) };
            if r == 0 || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return;
            }
        }
    }

    pub(super) fn detach(cmd: &mut Command) -> Result<(), ChildFail> {
        cmd.process_group(0);
        Ok(())
    }

    pub(super) fn cannot_detach() -> Option<String> {
        None
    }

    pub(super) fn exec_replace(mut cmd: Command, started: &dyn Fn(u32)) -> Result<i32, ChildFail> {
        started(std::process::id());
        Err(ChildFail::of_spawn(cmd.exec()))
    }

    pub(super) fn no_console_window(_cmd: &mut Command) {}
}

#[cfg(windows)]
mod os {
    use super::ChildFail;
    use std::os::windows::io::{AsRawHandle, RawHandle};
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attrs: *mut core::ffi::c_void, name: *const u16) -> RawHandle;
        fn AssignProcessToJobObject(job: RawHandle, process: RawHandle) -> i32;
        fn TerminateJobObject(job: RawHandle, exit_code: u32) -> i32;
        fn TerminateProcess(process: RawHandle, exit_code: u32) -> i32;
        fn CloseHandle(h: RawHandle) -> i32;
    }

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub(super) fn own_group(_cmd: &mut Command) {}

    /// 一个 Job（子进程起来后放进去，它再起的默认都在里面）。不设「关句柄即杀」：
    /// 正常结束后留下的后台进程（如第一次起的 tmux server）不跟着没；只在超时时终止整个 Job。
    /// 放不进去（老系统不许嵌套 Job）⇒ 退成只杀直接子进程。
    pub(super) struct Group {
        job: Option<RawHandle>,
        /// 直接子进程的句柄（归等待线程里的 `Child` 所有；只在它还没被收尸时用）。
        process: RawHandle,
    }

    impl Group {
        pub(super) fn adopt(child: &std::process::Child) -> Group {
            let job = unsafe { CreateJobObjectW(core::ptr::null_mut(), core::ptr::null()) };
            let job = (!job.is_null()).then_some(job).filter(|&j| {
                let ok = unsafe { AssignProcessToJobObject(j, child.as_raw_handle()) } != 0;
                if !ok {
                    unsafe { CloseHandle(j) };
                }
                ok
            });
            Group {
                job,
                process: child.as_raw_handle(),
            }
        }

        pub(super) fn kill(&self) {
            match self.job {
                Some(j) => unsafe {
                    TerminateJobObject(j, 1);
                },
                None => unsafe {
                    TerminateProcess(self.process, 1);
                },
            }
        }
    }

    impl Drop for Group {
        fn drop(&mut self) {
            if let Some(j) = self.job {
                unsafe { CloseHandle(j) };
            }
        }
    }

    /// Windows 上句柄占着 pid，不存在收尸前后的误伤 ⇒ 直接等它退出。
    pub(super) fn exited_unreaped(child: &mut std::process::Child) {
        let _ = child.wait();
    }

    pub(super) fn detach(_cmd: &mut Command) -> Result<(), ChildFail> {
        Err(ChildFail::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            cannot_detach().unwrap_or_default(),
        )))
    }

    pub(super) fn cannot_detach() -> Option<String> {
        Some(copy_core::copy_text("beDetach.detach.notUnix", &[]))
    }

    pub(super) fn exec_replace(mut cmd: Command, started: &dyn Fn(u32)) -> Result<i32, ChildFail> {
        let mut child = cmd.spawn().map_err(ChildFail::of_spawn)?;
        started(child.id());
        child
            .wait()
            .map(|s| s.code().unwrap_or(1))
            .map_err(ChildFail::of_spawn)
    }

    pub(super) fn no_console_window(cmd: &mut Command) {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/child_tests.rs"]
mod tests;
