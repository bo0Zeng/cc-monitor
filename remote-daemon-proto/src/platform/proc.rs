//! U2（2026-08-01）：**`/proc` 与进程身份**这一族平台原语。
//!
//! §1.1 第一条解耦线：`platform/` 是**唯一**允许出现平台原语与平台 `cfg` 的地方。
//! 本文件里的函数是从 `watcher.rs` **逐字搬来**的（doc 注释一起搬，一个字没改）——
//! U2 是纯重构，行为逐字不变。
//!
//! ⚠ **这段头注一度自己过期了**（Phase D 审计逮出）：它曾写着「`pid_alive` 的非 Linux 分支
//! 恒返回 `true`，是个已登记的静默错误地雷 …… Windows 今天编不过（12 个错）」——
//! 而**这三条事实全部被 U4a 证伪**，且它就在被改的那个函数上方几行。
//!
//! **现状**：`pid_alive` 的非 Linux 分支是 `unimplemented!()`（U4a 把静默说谎换成大声未实现）；
//! Windows 跨 target check **RC=0 且已进 CI**。真语义（`OpenProcess` + 退出码）留 U4b。
//! `platform/fallback_guard.rs` 钉住这一族：fallback 分支不许凭空返回「成功」值。

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
        // ★ U4a（2026-08-01）：**从「静默说谎」改成「大声未实现」。**
        //
        // 这里原本是 `let _ = pid; true`，注释写「treat as alive so the cross-platform
        // smoke still exercises the pipeline」。那个 `true` 是一个**没人会发现的谎**：
        // `pid_alive` 是判活的加表门，恒 `true` 的后果是**会话永远不被归档**，
        // 而且没有任何信号说「这个平台上我根本不知道」。
        //
        // U2 与 U3 两轮都明确把它推迟到本功能，理由是「改它 = 决定 Windows 语义」。
        // 到了 U4a，真语义（`OpenProcess` + 退出码）仍属 **U4b** —— 它需要 Windows 真机验证，
        // 而主计划自己写着「等价性仓里无实测，第一步先验」。
        //
        // 那 U4a 能做的是什么？**把谎换成事实**：
        // - `panic` 是一个没人能忽略的信号，`true` 不是。
        // - **不可能回归 Linux**：这条分支在 Linux 上编译期就不存在。
        // - Windows daemon 今天跑不起来（U4b 才让它能跑），所以不影响任何现存路径。
        // - 它给 U4b 留了一个**编译器/运行时帮你找**的落点，而不是一个「看起来能用」的假实现。
        let _ = pid;
        unimplemented!(
            "pid_alive 在本平台未实现（U4b：OpenProcess + 退出码）。\
             此前这里恒返回一个乐观的存活值 —— 那会让会话永不归档且毫无信号，是比 panic 坏得多的失败模式。（措辞刻意避开那个布尔字面量：`platform/fallback_guard.rs` 连字符串一起扫，写出来会把那条护栏自己打红 —— 同 §41.4 第 1 条纪律。）"
        )
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
/// Non-Linux: `None`（「不知道」的诚实表达）。U4b 换 Win32 实现 —— **注意它不在 U4a 的处置面上**，
/// `/proc` 读取一族里 `proc_starttime` / `proc_cmdline` / `proc_claude_config_dir` /
/// `start_epoch_from_ticks` 四个今天在 Windows 上全部静默返回 `None`。方向保守所以不是雷，
/// 但**没有它们 U4b 的判活只有半条腿**（Phase D 审计指出 U4b 清单漏了这四个）。
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
/// Wires the real `/proc` reads into the pure `liveness::is_same_live_process` decision.
/// （U4a 把那个函数上提到了 `platform/liveness.rs`；此处**不写 intra-doc 链接**，
/// 原来那条 `[\`is_same_live_process\`]` 在函数搬走后成了悬空引用 —— 审计 重要-5。）
pub(crate) fn session_alive(pid: u32, expected_start: Option<u64>) -> bool {
    let exists = pid_alive(pid);
    // Only read the current start if the PID exists (a read on a vanished PID is
    // pointless and would just be `None` anyway).
    let current_start = if exists { proc_starttime(pid) } else { None };
    super::liveness::is_same_live_process(exists, expected_start, current_start)
}
