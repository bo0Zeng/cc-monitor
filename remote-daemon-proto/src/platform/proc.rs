//! U2（2026-08-01）：**`/proc` 与进程身份**这一族平台原语。
//!
//! §1.1 第一条解耦线：`platform/` 是**唯一**允许出现平台原语与平台 `cfg` 的地方。
//! 本文件里的函数是从 `watcher.rs` **逐字搬来**的（doc 注释一起搬，一个字没改）——
//! U2 是纯重构，行为逐字不变。
//!
//! ⚠ **`pid_alive` 的非 Linux 分支恒返回 `true`，是个已登记的静默错误地雷。**
//! U2 **刻意不修**：改它 = 决定「Windows 上进程是否存活怎么答」，那是 U4 的正题；
//! 在一个声明「行为逐字不变」的纯重构里夹带语义决策是错的，而且 Windows 今天编不过
//! （`cargo check --all-targets --target x86_64-pc-windows-msvc` 12 个错），改了也无从验证。
//! **U4 的 DoD 里必须有它。**

/// Whether `pid` currently exists as a process on this host (existence only).
///
/// Linux (the daemon's real target): `/proc/<pid>` existence. This is the
/// add-time gate; the reuse-proof check is [`session_alive`].
pub(crate) fn pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Non-Linux (Windows compile/smoke only — not the real target): treat as
        // alive so the cross-platform smoke still exercises the pipeline.
        let _ = pid;
        true
    }
}

/// Parse the `starttime` (field 22) out of a `/proc/<pid>/stat` line.
///
/// **The comm gotcha**: field 2 is `(comm)` and the executable name can contain
/// spaces and parentheses (e.g. `(my proc)` or `((odd))`). Splitting the whole
/// line on whitespace is therefore wrong. The robust parse — used by ps/htop —
/// is to find the **last** `')'`, then count fields in the remainder: the first
/// token after it is field 3 (`state`), so `starttime` (field 22) is token index
/// `22 - 3 = 19` (0-based) of the post-`)` whitespace split.
///
/// Only called from the Linux branch of [`proc_starttime`] (and by unit tests on
/// every platform); on a non-Linux build the function body is unreferenced.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_starttime_from_stat(stat: &str) -> Option<u64> {
    /// 0-based index of `starttime` (field 22) within the tokens that follow the
    /// closing paren of `comm` (field 3 = `state` is token 0).
    const STARTTIME_IDX_AFTER_COMM: usize = 22 - 3;
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm
        .split_whitespace()
        .nth(STARTTIME_IDX_AFTER_COMM)?
        .parse::<u64>()
        .ok()
}

/// 从 `/proc/<pid>/environ` 抠 `CLAUDE_CONFIG_DIR` 的值（**只这一个键**）。
/// 读不到（进程已消失 / 非同 uid）→ `None`。形状照同模块的 [`proc_cmdline`]。
///
/// U2 从 `accounts_query.rs` 搬来（Phase D 审计：它带着两个 `target_os` cfg 留在 observe 侧文件里，
/// U3 一划层就会当场违反「`platform/` 是唯一允许平台 cfg 的层」）。**原头注引的 `watcher::proc_cmdline`
/// 在 U2 之后已是悬空引用** —— `proc_cmdline` 也搬到这里了。
pub(crate) fn proc_claude_config_dir(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let bytes = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
        for entry in bytes.split(|b| *b == 0) {
            if entry.is_empty() {
                continue;
            }
            let s = String::from_utf8_lossy(entry);
            if let Some(v) = s.strip_prefix("CLAUDE_CONFIG_DIR=") {
                if v.is_empty() {
                    return None;
                }
                return Some(v.to_string());
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// The PID's procStart (start time), used to defend against PID reuse (#34).
///
/// Linux: the `starttime` field (jiffies since boot) from `/proc/<pid>/stat`.
/// Non-Linux (Windows smoke): `None` — liveness then degrades to existence only.
pub(crate) fn proc_starttime(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        parse_starttime_from_stat(&stat)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Parse the boot time (`btime <epoch-secs>` line) out of `/proc/stat` content.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_btime(proc_stat: &str) -> Option<u64> {
    proc_stat.lines().find_map(|l| {
        l.strip_prefix("btime ")
            .and_then(|v| v.trim().parse::<u64>().ok())
    })
}

/// `/proc` time values are exported in USER_HZ ticks, which is a compile-time
/// constant 100 on every mainstream Linux arch (independent of the kernel's
/// internal HZ) — hardcoding avoids a libc dependency for sysconf(_SC_CLK_TCK).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) const USER_HZ: u64 = 100;

/// Starttime ticks → wall-clock epoch seconds: `/proc/stat` btime + ticks/USER_HZ.
///
/// btime is read FRESH on every call, deliberately un-cached: the kernel
/// computes it per-read as (wall clock − CLOCK_BOOTTIME), so an NTP **step**
/// moves it. A cached value taken before a backwards step would leave a
/// constant offset that mis-kills every future real session with no self-heal
/// (F20 audit I-1). Session-add is rare; one small /proc read is free.
pub(crate) fn start_epoch_from_ticks(ticks: Option<u64>) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let btime = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|s| parse_btime(&s))?;
        Some(btime + ticks? / USER_HZ)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = ticks;
        None
    }
}

/// `/proc/<pid>/cmdline`, NUL separators turned into spaces, lossily decoded.
/// None when unreadable (vanished PID, permissions) → check skipped.
pub(crate) fn proc_cmdline(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let spaced: Vec<u8> = bytes
            .into_iter()
            .map(|b| if b == 0 { b' ' } else { b })
            .collect();
        Some(String::from_utf8_lossy(&spaced).into_owned())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Reuse-proof liveness for an ACTIVE session (#34): the PID must still exist
/// **and** (when a procStart was captured at add-time) its current procStart
/// must match. A mismatch means the OS reused the PID for a different process —
/// the original session has ended.
///
/// Wires the real `/proc` reads into the pure [`is_same_live_process`] decision.
pub(crate) fn session_alive(pid: u32, expected_start: Option<u64>) -> bool {
    let exists = pid_alive(pid);
    // Only read the current start if the PID exists (a read on a vanished PID is
    // pointless and would just be `None` anyway).
    let current_start = if exists { proc_starttime(pid) } else { None };
    is_same_live_process(exists, expected_start, current_start)
}

/// Pure liveness decision (testable without a real `/proc`), given whether the
/// PID currently **exists**, the procStart **captured** at add-time, and the
/// procStart **read now**.
///
/// Key correctness rule (#34): a PID reuse only ever shows up as a
/// *successfully-read, DIFFERENT* current start. So the only case that declares
/// "dead by reuse" is `(Some(captured), Some(current))` with `captured != current`.
/// Every other arm where the PID still exists returns alive — in particular a
/// **transient `/proc/<pid>/stat` read failure** (`current == None`) must NOT
/// false-archive a process that demonstrably still exists (that would be a
/// regression vs. the Phase-0 existence-only check). If the process is truly
/// gone, `exists` is already `false` and we return dead.
pub(crate) fn is_same_live_process(
    exists: bool,
    expected_start: Option<u64>,
    current_start: Option<u64>,
) -> bool {
    if !exists {
        return false;
    }
    match (expected_start, current_start) {
        // Baseline captured AND current readable: same process iff equal.
        (Some(captured), Some(current)) => captured == current,
        // No baseline, or current unreadable right now: existence is all we can
        // assert. Do not archive a still-existing PID on missing start info.
        _ => true,
    }
}
