//! ★ **`15 §5.1 A3` / `00 §1.5.2`：起子进程的唯一出口，三个策略都没有 `Default`。**
//!
//! # 它治的是什么
//!
//! `设计/15 §1.2` 逐字：「起子进程这件事没有唯一出口 —— 同一形状已经出现三次」。
//! `local_backend.rs` 那一处监护 spawn **同时**犯三个错（无 `CREATE_NO_WINDOW` ·
//! 无 job 绑定 · `stderr(Stdio::null())`），而**三样的正确做法仓里都已经有**：
//!
//! | 需要的 | 仓里原来住哪 | 今天住哪 |
//! |---|---|---|
//! | `CREATE_NO_WINDOW` | `lib.rs::open_with_os` ＋ `local_daemon` 的 `hide_console_window`〔散文墓碑〕 | [`ConsolePolicy::Hidden`] |
//! | Job Object `KILL_ON_JOB_CLOSE` | `cc_bus` 的 `reap_whole_tree_on_drop`〔散文墓碑〕 | [`Lifetime::JobKillOnClose`] |
//!
//! ⚠ 中间那一列的两个名字**今天已经不在盘上了** —— 它们就是被本模块收编的那两份，
//! 逐字留着是为了说清「这三样不是新发明的，是仓里各自长了一份的东西」。
//! | stderr → 滚动日志 | `local_backend::drain_child_stderr_into_log` | [`StderrSink::ToLog`] |
//!
//! ⚠ **第三样刻意不搬家**：`drain_child_stderr_into_log` 的三条设计约束
//! （读到 EOF 才不会把子进程堵死 · 上界只砍「记」不砍「读」 · 一律 `warn` 不 `error`）
//! 都写在它自己的头注里。本模块**调用它**，不抄第二份 —— 抄一份就有两条会各自漂的泵。
//!
//! # 「无法被表达」是怎么买到的
//!
//! 三个枚举**一个都不给 `Default`**（由 [`tests::the_three_policies_have_no_default`] 钉），
//! 于是每一个落点在**编译期**被迫各自回答三个问题：要不要窗口 · 要不要随我死 · 错误往哪去。
//! 「全局加一个 flag」买不到这一条 —— 它会把 `launch.rs::launch_powershell_window`
//! 那处**刻意**的 `CREATE_NEW_CONSOLE`（给用户开一个真终端）一起改掉。
//! 那正是要唯一出口而不要全局开关的理由，也正是 [`ConsolePolicy::NewVisible`] 存在的理由。
//!
//! # 🔴 它为什么住在这里（宿主知识层），而不住 `backend/`
//!
//! `backend/mod.rs::the_backend_half_stays_platform_agnostic` 的禁针含 `#[cfg(windows)`
//! 与 `std::os::windows` / `std::os::unix` ⇒ 写进 `backend/` 当场红；
//! 而「加一条平台例外」被**递减棘轮**堵着（`PLATFORM_EXCEPTIONS.len() <= 1`，今天正好 1 条）。
//! ⇒ `backend/` 的两个落点（`local_backend::supervise_with_stdio` ·
//! `local_query::run_query`）**只收注入参数**，形状照 `start_or_extract` 的
//! `make_executable: &dyn Fn(...)` 那个先例（见 [`ManagedSpawn`]）。
//!
//! # ⚠ 诚实边界：三条，都写下来
//!
//! 1. **Windows 那一支今天没有任何机器验过。** 宿主是 Linux，`#[cfg(windows)]` 里的东西
//!    在这里连编译都不参与。「编得过」由门禁 `winchk`（`--target x86_64-pc-windows-gnu`）买；
//!    「**真的不弹窗了 / 真的连孙子一起收掉了**」要一台真 Windows（`99 §4.5.8` 的 `G2a`）。
//!    ⇒ 别把绿读成验过。
//! 2. **`Lifetime::JobKillOnClose` 在非 Windows 上是空壳。** POSIX 没有 Job Object，
//!    而 `std::process::Child` 也没有 `kill_on_drop` —— 那一格今天由各落点自己的
//!    `wait()` / 显式 `kill()` 承担，**本模块不假装它在那边也买到了同一样东西**。
//!    tokio 那一侧不同：[`spawn_managed_tokio`] 的 `JobKillOnClose` 会真的设
//!    `kill_on_drop(true)`（那是 `cc_bus` / `ssh_source` 今天就有的做法）。
//! 3. **`Lifetime::Detached` 在 Windows 上不加任何 flag。** `process_group(0)` 是 POSIX 的东西；
//!    Windows 那边「跟不跟着我死」由**有没有进 Job** 决定，而 `Detached` 就是「不进 Job」。
//!    刻意不顺手加 `CREATE_NEW_PROCESS_GROUP` —— 那会改掉 `launch_powershell_window`
//!    今天的行为，而这一轮没有任何 Windows 读数能证明那个改动是对的。

use std::path::Path;

/// 要不要给它开一个控制台窗口。**没有 `Default`** —— 每个落点自己回答。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolePolicy {
    /// **不开窗**（Windows `CREATE_NO_WINDOW`）。
    ///
    /// monitor 是 `windows_subsystem = "windows"` 的 GUI app ⇒ 它自己没有控制台，
    /// 不带这个 flag 时 Windows 会**给子进程新开一个控制台窗口**：桌面上凭空弹一个黑框，
    /// 而那个框是**可关的** —— 用户一关，`CTRL_CLOSE_EVENT` 打到子进程 ⇒ 它被杀。
    /// `真相源/70` 那条 BUG 的三个症状（弹窗 · 报失败 · 本机后端不工作）是同一个 flag。
    Hidden,
    /// **开一个真的、用户看得见的终端**（Windows `CREATE_NEW_CONSOLE`）。
    ///
    /// 今天只有一个用户：`launch.rs::launch_powershell_window` —— 那是本产品的主用途，
    /// 给用户的 claude 会话开一个能敲字的窗口。**别把 `Hidden` 铺到它头上。**
    NewVisible,
    /// **不表态**：继承宿主今天有什么（Windows 上不设任何 creation flag）。
    Inherit,
}

/// 它跟不跟着 monitor 一起走。**没有 `Default`**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime {
    /// **随我死**：Windows 上进一个 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 的 Job Object
    /// —— 句柄一关，**它连同它自己起的一切**整棵树收掉。
    ///
    /// 🔴 它比「退出时 kill 一下」强的那一格：**monitor 崩溃 / 被强杀时也生效**
    /// （句柄由内核关）。`cc_bus` 那条云端读数逐字证过为什么非 Job 不可：
    /// Windows 上 `Git\bin\bash.exe` 是个**外壳**，起完真 bash 自己就退，
    /// `TerminateProcess` 打在一个已经死了的 pid 上。
    JobKillOnClose,
    /// **不随我死**：POSIX 上 `process_group(0)`（否则终端里 Ctrl-C 的 SIGINT
    /// 会打到整个前台进程组，monitor 和它一起走）；Windows 上不进 Job。
    ///
    /// ⚠ `process_group` **不改变父子关系** ⇒ 不 `wait` 就留僵尸。收尸是各落点自己的事。
    Detached,
}

/// 它的 stderr 去哪。**没有 `Default`**。
///
/// # ⚠ 这里比 `设计/00 §1.5.2` 多两个变体，理由写下来
///
/// 设计稿写的是 `{ ToLog, Null }` —— 那是**从一个落点（`local_backend.rs::supervise_with_stdio`）看出去**
/// 得到的两格。把人群现打一遍（`write_site_registry::SPAWNS`，14 个运行期落点）之后，
/// 今天真实存在的是**四**格：还有「不接管，跟着界面进程的 stderr 走」
/// （`ssh_source::spawn_dial_proxy` 逐字写着为什么）与「接出来当返回值读」
/// （四处 `.output()`：`local_query` · `profile_installer` · `launch::ssh_client_available` ·
/// `ssh_source::resolve_ssh_host`）。
///
/// ⇒ 少这两格的话，那六处要么被迫改行为（拿现有两格之一硬套），要么绕开这个出口 ——
/// **而后者正是本模块在关的那扇门**。两格换六处绕行，不划算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StderrSink {
    /// **接进 monitor 的滚动日志**（实现就是 `local_backend::drain_child_stderr_into_log`）。
    ///
    /// 后端整层 `tracing::{warn,error,info}!` 的唯一出口就是 stderr，而 GUI app 没有
    /// stderr 控制台 ⇒ 不接出来就是「它每次死前说的话，无条件丢弃，全平台」。
    ToLog,
    /// **丢掉**。今天有真用户，逐个都是刻意的（脱离那条 stdio 必须全 null ·
    /// cc-bus 与 ccm 探针只要 stdout · 开窗那条不该往我们的日志里灌）。
    Null,
    /// **不接管**：跟着宿主进程的 stderr 走同一个地方。
    Inherit,
    /// **接出来交给调用方**（`Stdio::piped()`，随后由 [`ManagedChild::wait_with_output`] 读）。
    /// 这是 `Command::output()` 那一族的表达方式 —— 错误不是被丢了，是**成了返回值**。
    Captured,
}

// ══════════════════════════════════════════════════════════════════════════
// 平台原语：**全仓只此一份**
// ══════════════════════════════════════════════════════════════════════════

/// `CreateProcess` 的 `CREATE_NO_WINDOW`。**不是字节上限**（登记在
/// `byte_cap_registry` 的排除表里）。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `CreateProcess` 的 `CREATE_NEW_CONSOLE`。同上，是位掩码不是尺寸。
#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// 这三条策略在 Windows 上合成的 creation flags。
///
/// ⚠ **合成必须在一处做**：`Command::creation_flags` 是**覆盖**不是或上去，
/// 两处各调一次的结果是后一次把前一次抹掉 —— 那是一个不会报错、只会静默错的形状。
#[cfg(windows)]
fn creation_flags_for(console: ConsolePolicy, _lifetime: Lifetime) -> u32 {
    match console {
        ConsolePolicy::Hidden => CREATE_NO_WINDOW,
        ConsolePolicy::NewVisible => CREATE_NEW_CONSOLE,
        ConsolePolicy::Inherit => 0,
    }
}

/// 收尾凭据：**句柄一关，那棵进程树整体收掉**。
///
/// Windows 上它握着一个 Job Object；别处它是个空壳（那边这一格由各落点自己的
/// `wait` / `kill` 承担，见模块头注的诚实边界 2）。
#[cfg(windows)]
pub struct LifetimeGuard(Option<windows::Win32::Foundation::HANDLE>);

/// 见 Windows 那一支。
#[cfg(not(windows))]
pub struct LifetimeGuard;

#[cfg(windows)]
impl Drop for LifetimeGuard {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            // 关掉 Job 的最后一个句柄 = 连同 Job 里**所有**进程一起收掉
            // （`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`）。这就是本件要买的那一下。
            // 形状照 `session_map::is_process_alive`：一个 `unsafe` 块，句柄显式关。
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(h);
            }
        }
    }
}

/// 把一个**已经起来的**子进程连同它自己起的一切收进一个 Job Object。
///
/// # ⚠ 它买不到什么（写下来，别读成比它强）
///
/// 1. **有一个小竞态**：`AssignProcessToJobObject` 只能在 `spawn()` **之后**做，
///    而中间那个外壳若已经把真进程起出来了，那个孙子就没进 Job。
///    真正无窗口的做法要 `CREATE_SUSPENDED` ＋ 拿线程句柄 `ResumeThread`，
///    而 `std::process::Child` 不给线程句柄 ⇒ 今天做不到。
///    ⚠ 落进这个窗口时**不比今天坏**（原有的显式 kill / `kill_on_drop` 照旧）。
/// 2. **失败不静默、但也不失败整条路**：建不出 Job / 认领不上时走 `tracing::error!`
///    并**明说这台机器上收尾这一格没人守**。刻意**不**回 `Err`：起进程本身没坏，
///    把它变成 `Err` 会让一台拒绝 Job 的机器**彻底用不了这条路** —— 那是拿一个更大的坏
///    去换一个小的。
#[cfg(windows)]
fn assign_to_job(raw: std::os::windows::io::RawHandle, what: &str) -> LifetimeGuard {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    unsafe {
        let job = match CreateJobObjectW(None, windows::core::PCWSTR::null()) {
            Ok(h) if !h.is_invalid() => h,
            other => {
                tracing::error!(
                    "起 {what}：建不出 Job Object（{other:?}），收尾这一格\
                     **在这台机器上没人守** —— 它退出/被掐断之后可能漏下子孙进程。"
                );
                return LifetimeGuard(None);
            }
        };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
        let ptr = std::ptr::addr_of!(info) as *const core::ffi::c_void;
        if let Err(e) = SetInformationJobObject(job, JobObjectExtendedLimitInformation, ptr, size) {
            let _ = CloseHandle(job);
            tracing::error!(
                "起 {what}：Job Object 设不上 KILL_ON_JOB_CLOSE（{e:?}），\
                 收尾这一格**在这台机器上没人守**。"
            );
            return LifetimeGuard(None);
        }
        if let Err(e) = AssignProcessToJobObject(job, HANDLE(raw as isize)) {
            let _ = CloseHandle(job);
            tracing::error!(
                "起 {what}：子进程认领不进 Job Object（{e:?}），\
                 收尾这一格**在这台机器上没人守**。"
            );
            return LifetimeGuard(None);
        }
        LifetimeGuard(Some(job))
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 同步那一侧（`std::process`）
// ══════════════════════════════════════════════════════════════════════════

/// 一条**已经按三条策略起好**的子进程。
///
/// `Deref`/`DerefMut` 到 [`std::process::Child`] ⇒ 落点照旧 `.id()` / `.kill()` /
/// `.wait()` / `.stdout.take()`，只是那个 Job 句柄跟着这个值一起活着。
pub struct ManagedChild {
    /// 只为持有生命周期：丢掉它 = 关掉 Job = 那棵树走。
    ///
    /// ⚠ **它声明在 `child` 之前是有意的**：结构体字段按声明序析构 ⇒ 丢掉这个值时
    /// **先关 Job（整棵树连同孙子一起收掉）、再丢句柄**。反过来写的话，
    /// 句柄那一侧的收尾（显式 kill / `kill_on_drop`）会先打在那个可能已经退了的外壳上。
    _lifetime: LifetimeGuard,
    child: std::process::Child,
}

impl std::ops::Deref for ManagedChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl std::ops::DerefMut for ManagedChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

impl ManagedChild {
    /// `Command::output()` 的对侧：等它退出、把两根管子读干净。
    ///
    /// ⚠ **Job 句柄活到 `wait` 之后才丢** —— 反过来写（先 `into_inner` 再等）
    /// 会在 Windows 上把还没跑完的它当场收掉。
    pub fn wait_with_output(self) -> std::io::Result<std::process::Output> {
        let ManagedChild { _lifetime, child } = self;
        let out = child.wait_with_output();
        drop(_lifetime);
        out
    }

    /// 等它退出。理由同上：句柄活到等完。
    pub fn wait_for_status(mut self) -> std::io::Result<std::process::ExitStatus> {
        let st = self.child.wait();
        drop(self);
        st
    }
}

/// **唯一出口**（`00 §1.5.2` 那个签名）。三个策略都是必填参数，一个都不给 `Default`。
///
/// 只起「一个二进制 ＋ 一串 argv」那种最简形态。要另设 env / cwd / stdin / stdout 的，
/// 走 [`spawn_managed_cmd`] —— 那是**同一条路的内层**，不是旁路（本函数自己就调它）。
pub fn spawn_managed(
    bin: &Path,
    args: &[String],
    console: ConsolePolicy,
    lifetime: Lifetime,
    stderr: StderrSink,
) -> std::io::Result<ManagedChild> {
    let mut cmd = std::process::Command::new(bin);
    cmd.args(args);
    spawn_managed_cmd(&mut cmd, console, lifetime, stderr)
}

/// [`spawn_managed`] 的内层：调用方已经把「跑什么」（argv / env / cwd / stdin / stdout）
/// 装好，这里只管**三条策略 ＋ 真正那一下 `spawn`**。
///
/// ⚠ **stderr 由本函数决定，调用方在此之前设的会被覆盖** —— 那正是「无法表达坏默认值」
/// 的落点：`stderr` 这个问题只有一个地方能回答。
pub fn spawn_managed_cmd(
    cmd: &mut std::process::Command,
    console: ConsolePolicy,
    lifetime: Lifetime,
    stderr: StderrSink,
) -> std::io::Result<ManagedChild> {
    let what = cmd.get_program().to_string_lossy().into_owned();
    apply_stdio(cmd, stderr);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(creation_flags_for(console, lifetime));
    }
    #[cfg(unix)]
    {
        let _ = console; // POSIX 上没有「控制台窗口」这回事 —— 参数照收，签名两边一致。
        if matches!(lifetime, Lifetime::Detached) {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
    }
    // 非 Windows 非 Unix（今天没有这样的目标）：参数照收，签名各处一致。
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (console, lifetime);
    }
    let mut child = cmd.spawn()?;
    let guard = attach_lifetime(&child, lifetime, &what);
    if matches!(stderr, StderrSink::ToLog) {
        if let Some(e) = child.stderr.take() {
            let pid = child.id();
            // ⚠ 另起一条线程而不是在本线程读：调用方接下来多半要阻塞在 stdout 上，
            //   两根管子由一条线程串着读的那一刻，没被读的那根写满就把子进程**卡死**。
            std::thread::spawn(move || {
                crate::backend::control::local_backend::drain_child_stderr_into_log(e, pid)
            });
        }
    }
    Ok(ManagedChild {
        _lifetime: guard,
        child,
    })
}

/// stderr 那一格。**四个变体各自只有一种写法**，别在落点上再写第二种。
fn apply_stdio(cmd: &mut std::process::Command, stderr: StderrSink) {
    match stderr {
        StderrSink::ToLog | StderrSink::Captured => {
            cmd.stderr(std::process::Stdio::piped());
        }
        StderrSink::Null => {
            cmd.stderr(std::process::Stdio::null());
        }
        // 不表态 = 不动它（`Command` 的默认就是继承）。
        StderrSink::Inherit => {}
    }
}

#[cfg(windows)]
fn attach_lifetime(child: &std::process::Child, lifetime: Lifetime, what: &str) -> LifetimeGuard {
    use std::os::windows::io::AsRawHandle;
    match lifetime {
        Lifetime::JobKillOnClose => assign_to_job(child.as_raw_handle(), what),
        Lifetime::Detached => LifetimeGuard(None),
    }
}

#[cfg(not(windows))]
fn attach_lifetime(
    _child: &std::process::Child,
    _lifetime: Lifetime,
    _what: &str,
) -> LifetimeGuard {
    // POSIX 上「随我死」这一格没有 Job Object 这种东西 —— 见模块头注的诚实边界 2。
    // `Detached` 那一半已经在 `spawn_managed_cmd` 里用 `process_group(0)` 落过了。
    LifetimeGuard
}

// ══════════════════════════════════════════════════════════════════════════
// 注入给 `backend/` 的那一半
// ══════════════════════════════════════════════════════════════════════════

/// 交给 `backend/` 的**注入参数**：backend 那一半不认识平台，也不该认识这三个策略
/// 由谁来定 —— 它只知道「起进程这一下由宿主给的这个东西来做」。
///
/// 形状照 `local_backend::start_or_extract` 的 `make_executable: &dyn Fn(...)`：
/// 平台差异收敛成**一个注入点**，backend 那侧原样往下传。
pub type ManagedSpawn =
    dyn Fn(&mut std::process::Command) -> std::io::Result<ManagedChild> + Send + Sync;

/// 把三条策略绑成一个 [`ManagedSpawn`]。**三个参数仍然必填** ——
/// 注入没有把那三个问题变没，只是把回答的地方挪到了宿主这一侧。
pub fn managed_spawner(
    console: ConsolePolicy,
    lifetime: Lifetime,
    stderr: StderrSink,
) -> std::sync::Arc<ManagedSpawn> {
    std::sync::Arc::new(move |cmd: &mut std::process::Command| {
        spawn_managed_cmd(cmd, console, lifetime, stderr)
    })
}

/// 本机后端那条**被监护**的路：`local_backend::supervise_with_stdio` / `supervise` 收它。
///
/// 三条答案写在这里、只写一次 —— `设计/00 §1.5.2` 点名的 `local_backend.rs::supervise_with_stdio`
/// **同时犯的三个错**，正好就是这三格：
/// · `Hidden` —— 先前没带 `CREATE_NO_WINDOW`：Windows 上弹一个**可关的**黑框，
///   用户一关就是 `CTRL_CLOSE_EVENT` 打到后端 ⇒ 后端死 ⇒ 中转不再监听（`真相源/70`）。
/// · `JobKillOnClose` —— 先前没有 job 绑定：monitor 崩溃 / 被强杀时后端成孤儿。
/// · `ToLog` —— 先前是 `Stdio::null()`：后端整层 91 处 `tracing::` 的唯一出口被无条件丢弃。
///
/// ⚠ **这不是 `Default` 换了个名字**：`Default` 是「不写也行」，本函数是
/// 「**这条路**的答案叫这个名字，而且它三格都写出来了」。谁都不能不给参数就调
/// `supervise_with_stdio` —— 那个签名不答应（`the_three_policies_have_no_default` 守着
/// 另一半：这三个枚举本身不许长出 `Default`）。
pub fn local_backend_supervised() -> std::sync::Arc<ManagedSpawn> {
    managed_spawner(
        ConsolePolicy::Hidden,
        Lifetime::JobKillOnClose,
        StderrSink::ToLog,
    )
}

/// 本机后端那条**一次性只读查询**的路：`backend::observe::local_query::run_query` 收它。
///
/// 与上一条只差最后一格：查询的 stderr **是返回值**（`QueryOutcome` 按它分类），
/// 接进滚动日志反而会把「调用方本来就拿得到的那句话」再抄一遍。
/// `Hidden` 那格在这里同样是**先前没人回答过**的 —— 它先前是一句裸 `.output()`。
pub fn local_backend_one_shot_query() -> std::sync::Arc<ManagedSpawn> {
    managed_spawner(
        ConsolePolicy::Hidden,
        Lifetime::JobKillOnClose,
        StderrSink::Captured,
    )
}

// ══════════════════════════════════════════════════════════════════════════
// 异步那一侧（`tokio::process`）
// ══════════════════════════════════════════════════════════════════════════

/// [`ManagedChild`] 的 tokio 对侧。`Deref`/`DerefMut` 到 [`tokio::process::Child`]。
pub struct ManagedTokioChild {
    /// 同 [`ManagedChild`]：**先关 Job，再丢句柄**（析构按声明序）。
    _lifetime: LifetimeGuard,
    child: tokio::process::Child,
}

impl std::ops::Deref for ManagedTokioChild {
    type Target = tokio::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl std::ops::DerefMut for ManagedTokioChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

/// 异步那一侧的唯一出口。
///
/// # 为什么这里非有第二个函数不可（而这**不是**「抄了第二份」）
///
/// sync 与 async 之间跨不过去：`tokio::process::Command` 不是 `std::process::Command`。
/// ⇒ 真正会漂的两样东西**都只有一份**：creation flags 的合成（[`creation_flags_for`]）
/// 与 Job 的建法（[`assign_to_job`]）。本函数只是把它们接到另一种 `Command` 上。
///
/// ⚠ **`JobKillOnClose` 在这一侧比同步那侧多买一样**：`kill_on_drop(true)`。
/// 那是 tokio 的 `Child` 才有的东西（它默认**不**因句柄被 drop 而杀子进程），
/// 而 `cc_bus` / `ssh_source` 今天就靠它 —— 三条（Job · `kill_on_drop` · 显式 kill）
/// 都留着：Job 没建成时另外两条至少还在。
pub fn spawn_managed_tokio(
    cmd: &mut tokio::process::Command,
    console: ConsolePolicy,
    lifetime: Lifetime,
    stderr: StderrSink,
) -> std::io::Result<ManagedTokioChild> {
    let what = cmd.as_std().get_program().to_string_lossy().into_owned();
    match stderr {
        StderrSink::ToLog | StderrSink::Captured => {
            cmd.stderr(std::process::Stdio::piped());
        }
        StderrSink::Null => {
            cmd.stderr(std::process::Stdio::null());
        }
        StderrSink::Inherit => {}
    }
    #[cfg(windows)]
    {
        cmd.creation_flags(creation_flags_for(console, lifetime));
    }
    #[cfg(unix)]
    {
        let _ = console; // 同上。
        if matches!(lifetime, Lifetime::Detached) {
            cmd.process_group(0);
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = console;
    }
    if matches!(lifetime, Lifetime::JobKillOnClose) {
        cmd.kill_on_drop(true);
    }
    let child = cmd.spawn()?;
    let guard = attach_lifetime_tokio(&child, lifetime, &what);
    // ⚠ tokio 那一侧今天没有 `ToLog` 的用户；真要接，得再写一条 **async** 的泵
    //   （`drain_child_stderr_into_log` 吃的是 `std::process::ChildStderr`）。
    //   在有人真要之前**不预造**，但也别让它静默变成「丢掉」：
    debug_assert!(
        !matches!(stderr, StderrSink::ToLog),
        "tokio 那一侧还没有 `StderrSink::ToLog` 的实现 —— \
         别让它静默地等于 `Captured`（管子接出来了，没人读 ⇒ 写满就把子进程堵死）"
    );
    Ok(ManagedTokioChild {
        _lifetime: guard,
        child,
    })
}

#[cfg(windows)]
fn attach_lifetime_tokio(
    child: &tokio::process::Child,
    lifetime: Lifetime,
    what: &str,
) -> LifetimeGuard {
    match lifetime {
        Lifetime::JobKillOnClose => match child.raw_handle() {
            Some(raw) => assign_to_job(raw, what),
            None => {
                tracing::error!("起 {what}：拿不到子进程句柄，收尾这一格**在这台机器上没人守**。");
                LifetimeGuard(None)
            }
        },
        Lifetime::Detached => LifetimeGuard(None),
    }
}

#[cfg(not(windows))]
fn attach_lifetime_tokio(
    _child: &tokio::process::Child,
    _lifetime: Lifetime,
    _what: &str,
) -> LifetimeGuard {
    LifetimeGuard
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本模块自己的源码（生产段）。
    fn me() -> String {
        guard_core::production_code(include_str!("spawn_managed.rs"))
    }

    /// ★★ **三个策略一个都不许有 `Default`。**
    ///
    /// 这是整件事的**承重墙**：有 `Default` 的那一刻，「每个落点被迫回答三个问题」
    /// 就退化成「不写就按某个人当年的顺手值来」—— 而 `真相源/70` 那条 BUG
    /// 正是「某个人当年的顺手值」。
    ///
    /// ⚠ 它认的是源码形态（`derive(... Default ...)` 与 `impl Default for`），
    /// 挡得住顺手加一个，挡不住换个名字绕过去。**比没有强，别读成证明。**
    #[test]
    fn the_three_policies_have_no_default() {
        let src = me();
        let d = format!("{}efault", "D");
        for ty in ["ConsolePolicy", "Lifetime", "StderrSink"] {
            let bad = format!("impl {d} for {ty}");
            assert!(
                !src.contains(&bad),
                "`{ty}` 有了 `{d}` 实现 —— 那三个必填参数当场变成「不写也行」，\n\
                 而本模块存在的全部理由就是让那件事**表达不出来**。"
            );
        }
        // `derive` 那一半：本模块生产段里的 derive 列表一个 `Default` 都不许有。
        for line in src.lines().filter(|l| l.contains("derive(")) {
            assert!(
                !line.contains(&d),
                "本模块的 derive 里出现了 `{d}`：{line}\n\
                 ⚠ 三个策略枚举共用这一条 —— 它们的 derive 列表逐字是\
                 `Debug, Clone, Copy, PartialEq, Eq`。"
            );
        }
    }

    /// ★ **反向自检**：上一条是「什么都没找到」型断言，它零命中地绿可能只是因为
    /// 剥法把整份源码剥空了。这里钉住剥完之后那三个枚举**还在**。
    #[test]
    fn the_no_default_guard_is_actually_looking_at_the_enums() {
        let src = me();
        for ty in ["ConsolePolicy", "Lifetime", "StderrSink"] {
            assert!(
                src.contains(&format!("pub enum {ty}")),
                "剥完生产段之后找不到 `pub enum {ty}` —— 上一条此刻在空转"
            );
        }
        assert!(
            src.contains("derive("),
            "剥完之后一个 `derive(` 都没有 —— 上一条的第二半在空转"
        );
    }

    /// ★★★ **唯一出口这件事本身要有人数。**
    ///
    /// # 它守什么
    ///
    /// 全树生产段里，「真正起一个进程那一下」与「三条策略的平台原语」
    /// **只许出现在本模块**。人群从源码派生（默认拒绝），不是手写清单。
    ///
    /// # 为什么不是「数 `Command::new(` 」
    ///
    /// `Command::new(` 是**装东西**那一下（argv / env / cwd 各落点自己的事），
    /// 数它只会逼出一个什么都往里传的上帝函数。
    /// 真正会出错的是**收尾那一下**：`.spawn()` / `.output()` / `.status()`
    /// 以及三条平台原语。⇒ 人群锚在那儿。
    /// 「谁在起进程」那张表另有其人：`write_site_registry::SPAWNS`（本条不替它）。
    ///
    /// # ⚠ `build.rs` 刻意不在人群里
    ///
    /// 它跑在**构建期**、在开发者机器上，`00 §1.5.2` 那三个问题
    /// （窗口 / 随谁死 / 错误往哪去）对它一个都不成立：没有 GUI 宿主可弹窗，
    /// 没有 monitor 进程可随，错误就该打到 `cargo` 的 stderr 上。
    /// ⇒ 它在 `SPAWNS` 里申报、但**不进这个出口**。这一格是刻意的，不是漏了。
    #[test]
    fn the_spawn_verbs_and_platform_primitives_live_only_here() {
        /// 「真正起一个进程那一下」＋三条平台原语。**无条件**：这几个点调用
        /// 在本仓只可能长在 `Command` 上。
        const VERBS: &[&str] = &[
            ".spawn()",
            ".creation_flags(",
            ".process_group(",
            ".kill_on_drop(",
        ];
        /// 🔴 **只在「这份源码里确实造了 `Command`」时才算**。
        ///
        /// 射程边界，写下来：`.output()` / `.status()` 这两个名字**别的东西也有**
        /// （现打：`search.rs` 的 `index.status()` 是索引器的状态，和进程没关系）。
        /// 把它们无条件禁掉就是造一族假红，而假红会逼人去登记一条与事实无关的例外。
        /// ⇒ 用「同一份源码里有没有 `Command::new(`」当前提。
        /// **代价也写下来**：一个既造 `Command`、又在别处对别的类型调 `.output()` 的文件
        /// 会被误判 —— 那时该做的是把那一处改名/拆文件，不是把这两个词删掉。
        const VERBS_IF_BUILDS_A_COMMAND: &[&str] = &[".output()", ".status()"];
        let files = guard_core::scan_tree!(&crate::guard_support::crate_src_root(), &["rs"]);
        let mut offenders: Vec<String> = Vec::new();
        let mut scanned = 0usize;
        for (path, raw) in &files {
            let prod = guard_core::production_code(raw);
            scanned += prod.len();
            for v in VERBS {
                if prod.contains(v) {
                    offenders.push(format!("  {}: `{v}`", path.display()));
                }
            }
            if prod.contains(concat!("Command::", "new(")) {
                for v in VERBS_IF_BUILDS_A_COMMAND {
                    if prod.contains(v) {
                        offenders.push(format!("  {}: `{v}`", path.display()));
                    }
                }
            }
        }
        assert!(
            scanned > 100_000,
            "剥掉测试段后只剩 {scanned} 字节可扫 —— 这条会零命中地绿"
        );
        // 反向自检②：条件那一支**必须真的有人走过**。全树一份 `Command::new(` 都扫不到时，
        // `VERBS_IF_BUILDS_A_COMMAND` 那两条就是死规则，而死规则看起来和「干净」一模一样。
        let builders = files
            .iter()
            .filter(|(_, raw)| {
                guard_core::production_code(raw).contains(concat!("Command::", "new("))
            })
            .count();
        assert!(
            builders >= 5,
            "全树生产段只有 {builders} 份源码在造 `Command` ——              `VERBS_IF_BUILDS_A_COMMAND` 那一支此刻近乎死规则（08-08 起这个数一直是两位数）"
        );
        assert!(
            offenders.is_empty(),
            "这些地方绕开了 `spawn_managed` 这个唯一出口：\n{}\n\n\
             ★ `15 §5.1 A3` / `00 §1.5.2`：起子进程的三个问题（要不要窗口 · 要不要随我死 · \
             错误往哪去）**必须在编译期各自回答一遍**。\n\
             ⇒ 把 `cmd.spawn()` 换成 `crate::spawn_managed::spawn_managed_cmd(&mut cmd, …)`，\
             三个策略照实写；`backend/` 那一半收注入参数（`ManagedSpawn`），不许自己认平台。\n\
             ⚠ 别在落点上直接写 `creation_flags` / `process_group` / `kill_on_drop` —— \
             那正是「同一形状出现三次」的来路。",
            offenders.join("\n")
        );
    }

    /// ★ **上一条的阴性对照**：那六个词不是瞎的 —— 本模块自己的生产段必须全都命中。
    ///
    /// ⚠ 没有这一条的话，把 `VERBS` 写错一个字母（或者剥法哪天把整份源码剥空）
    /// 会让主判据**零命中地绿**，而「唯一出口」这件事悄悄一个人都不数了。
    #[test]
    fn the_exit_itself_still_contains_every_verb_it_forbids_elsewhere() {
        let src = me();
        // ⚠ 三条平台原语各自在 `#[cfg]` 里 —— 剥生产段不剥 `cfg`，所以两边都看得到。
        for v in [
            ".spawn()",
            ".creation_flags(",
            ".process_group(",
            ".kill_on_drop(",
        ] {
            assert!(
                src.contains(v),
                "本模块自己的生产段里找不到 `{v}` —— 要么它搬走了（那主判据该换住址），\
                 要么剥法坏了（那主判据此刻在空转）"
            );
        }
    }
}
