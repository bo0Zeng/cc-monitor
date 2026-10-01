//! 起子进程这一族的平台读法〔阶段 H：；原住 `spawn_managed.rs`，逐字搬〕：三条策略里
//! 「要不要窗口」「随不随我死」在这个平台上**怎么说** —— Windows：creation flags ＋ Job Object；POSIX：`process_group(0)`。
//!
//! 三个策略枚举与唯一出口（`spawn_managed` · `spawn_managed_cmd` · `spawn_managed_tokio`）留在 `spawn_managed.rs`；
//! 两个平台说法一样的那几格（stderr 往哪去 · tokio 那侧 `JobKillOnClose` 多设 `kill_on_drop`）也留在那边。

use crate::spawn_managed::{ConsolePolicy, Lifetime};

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
            // 形状照 `session_map::is_process_alive`〔散文墓碑〕：一个 `unsafe` 块，句柄显式关。
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

/// 起之前：把「要不要窗口 · 随不随我死」落到这个平台的 `std::process::Command` 上。
///
/// ⚠ **合成必须在一处做**（见 [`creation_flags_for`]）：全壳唯一设 creation flags 的地方 ——
/// tokio 那一侧也交这里（`spawn_managed_tokio` 传 `as_std_mut()`：tokio 的 `creation_flags` / `process_group` 本来就是转给里面那个 std `Command` 的）。
pub fn prepare(cmd: &mut std::process::Command, console: ConsolePolicy, lifetime: Lifetime) {
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
}

/// 起之后：`JobKillOnClose` ⇒ 把它连同它自己起的一切收进一个 Job（Windows）；别处是空壳。
#[cfg(windows)]
pub fn attach_lifetime(
    child: &std::process::Child,
    lifetime: Lifetime,
    what: &str,
) -> LifetimeGuard {
    use std::os::windows::io::AsRawHandle;
    match lifetime {
        Lifetime::JobKillOnClose => assign_to_job(child.as_raw_handle(), what),
        Lifetime::Detached => LifetimeGuard(None),
    }
}

#[cfg(not(windows))]
pub fn attach_lifetime(
    _child: &std::process::Child,
    _lifetime: Lifetime,
    _what: &str,
) -> LifetimeGuard {
    // POSIX 上「随我死」这一格没有 Job Object 这种东西 —— 见模块头注的诚实边界 2。
    // `Detached` 那一半已经在 [`prepare`] 里用 `process_group(0)` 落过了。
    LifetimeGuard
}

#[cfg(windows)]
#[cfg_attr(not(test), allow(dead_code))]
pub fn attach_lifetime_tokio(
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
#[cfg_attr(not(test), allow(dead_code))]
pub fn attach_lifetime_tokio(
    _child: &tokio::process::Child,
    _lifetime: Lifetime,
    _what: &str,
) -> LifetimeGuard {
    LifetimeGuard
}
