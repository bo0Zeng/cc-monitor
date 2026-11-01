//! `/proc` 与进程身份这一族平台原语（`platform/` 是唯一允许出现平台原语与平台 `cfg` 的地方）。
//!
//! 判活三件（`pid_alive` / `proc_starttime` / `start_epoch_from_ticks`）有 Linux 与 Windows 两臂（Windows 的 Win32 读法只住
//! `platform/win_proc.rs`：`OpenProcess` ＋ 退出码 ＋ `GetProcessTimes`）；其余平台（macOS 等）是大声的 `unimplemented!()` / `None`。
//! `proc_env_var` / `proc_cmdline` 在 Windows 上是「读不到」（读别的进程的环境与命令行要读对方 PEB，未公开结构）。
//! Windows 臂只买到编得过（`winchk-backend`）＋ 纯换算那一半在 Linux 上的对拍；真机零读数。
//! `platform/fallback_guard.rs` 钉住这一族：fallback 分支不许凭空返回「成功」值。

use crate::platform::fs::Ro;

/// Whether `pid` currently exists as a process on this host (existence only).
///
/// Linux (the backend's real target): `/proc/<pid>` existence. This is the
/// add-time gate; the reuse-proof check is [`session_alive`].
///
/// Windows：开得到句柄且退出码是 `STILL_ACTIVE` ⇒ 在；**「拒绝访问」也算在**
/// （与 Linux 同契约：`/proc/<pid>` 对别的用户的进程照样存在 —— 这里问的是存在性，不是权限）；
/// 开得到句柄但这一刻问不出退出码 ⇒ 按「读不到不判死」算在（`liveness.rs` 那条纪律）；
/// 其余开不出来 ⇒ 不在。
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(windows)]
    {
        super::win_proc::exists(pid)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        // 没有承诺的平台（macOS 等）：大声未实现，不返回 `true`。恒 `true` 的后果是会话永远不被归档、而且没有任何信号说
        // 「这个平台上我根本不知道」；`panic` 是没人能忽略的信号。这条分支在 Linux 与 Windows 上编译期就不存在。
        let _ = pid;
        unimplemented!(
            "pid_alive 在本平台未实现（Linux 读 /proc、Windows 走 platform/win_proc.rs，这里两样都没有）。\
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
pub fn parse_starttime_from_stat(stat: &str) -> Option<u64> {
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

/// 读一次 `/proc/<pid>/environ` 抠某个键的三态结果。
///
/// 一个 `Option` 装不下这里的事实：
///
/// | 支 | 何处 | 事实 |
/// |---|---|---|
/// | 一 | `Ro::read(…)` 回 `Err` | 环境读不到（进程没了 / 权限 / 竞态）—— 很少发生 |
/// | 四 | `Ro::read(…)` 回 `Ok(vec![])` | 读得到，但回 0 字节 —— 常见：exec 窗口（进程刚 `execve`、mm 还没装好）与僵尸进程 |
/// | 二 | 键在，值是空串 | 显式设成了空 |
/// | 三 | 循环走完没命中 | 压根没这个键 |
///
/// 支四与支三说的是相反的两件事（「这一刻读不出来」vs「读到了，确实没设」），所以分开：
/// - `Unreadable` = 环境这一刻取不到 = 支一 ∪ 支四；
/// - `Unset` = 读得到、但这个键不作数 = 支二 ∪ 支三（两个调用方的下游谓词 `pane_is_safe` / `is_safe_config_dir` 都对空串恒 `false`；
///   哪天有调用方把空串当有意义的值，这两支就得再拆）。
/// exec 窗口还有一形不走支四：vfork 父进程被放回来、子进程还没换 mm 的那一刻，读到的是父进程的环境。生产调用方读的都是
/// 自己写了 pidfile 的进程（早 exec 完）碰不到；起子进程的夹具要自己等（`identity_tag_tests::spawn_settled_sleep`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvRead {
    /// 读到了这个键，且值非空。
    Value(String),
    /// 环境**读得到**，而这个键不作数：没有这个键 / 它的值是空串（两支合并，见类型头注）。
    Unset,
    /// 环境这一刻取不到：读失败，或读回 0 字节。它不是「没设」—— 当成没设，`observe/accounts_query.rs` 会把一条真跑在账号 Z 下的会话报成账号 0 的。
    Unreadable,
}

/// 从 `/proc/<pid>/environ` 抠某一个环境变量的值，三态返回（见 [`EnvRead`]）。空值按未设算（回 `Unset`）；
/// 这一刻读不出来（读失败 / 读回 0 字节）回 `Unreadable`。变量名是参数：`platform/` 不认识任何一个 agent 的环境变量。
///
/// 调用形状（`proc_env_var(pid, <键>)` 这一串字面）是量点，别改：
/// `observe/accounts_query_tests.rs::the_only_env_keys_this_module_reads_are_the_two_named_constants` 按这串字面数「backend 真读几个键」。
///
/// `warn!` 在这里而不在调用方：只有这里知道刚才走的是哪一支。
pub fn proc_env_var(pid: u32, name: &str) -> EnvRead {
    #[cfg(target_os = "linux")]
    {
        let bytes = match Ro::read(format!("/proc/{pid}/environ")) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    "读 /proc/{pid}/environ 失败（要取 {name}）：{e} \
                     —— 环境这一刻**取不到**，不是「这个键没设」"
                );
                return EnvRead::Unreadable;
            }
        };
        // 支四：`Ok(vec![])` —— 不许走完下面那个循环、落到「压根没这个键」那条出口。
        if bytes.is_empty() {
            tracing::warn!(
                "/proc/{pid}/environ 读回 0 字节（要取 {name}）\
                 —— 环境这一刻**取不到**，不是「这个键没设」\
                 （exec 窗口，或进程已成僵尸：stat 还在 ⇒ 判活仍为真，而 mm 已释放）"
            );
            return EnvRead::Unreadable;
        }
        for entry in bytes.split(|b| *b == 0) {
            if entry.is_empty() {
                continue;
            }
            let s = String::from_utf8_lossy(entry);
            if let Some(v) = s.strip_prefix(&format!("{name}=")) {
                if v.is_empty() {
                    return EnvRead::Unset;
                }
                return EnvRead::Value(v.to_string());
            }
        }
        EnvRead::Unset
    }
    #[cfg(not(target_os = "linux"))]
    {
        // 非目标平台没有 `/proc` ⇒ 答「这一刻取不到」，不是「读到了、没设」。
        // （`platform/fallback_guard` 的分类器只认字面 `false` / `None` / `unimplemented!(` 当诚实空壳，会把这个块数进「真实现」那一类。）
        let _ = (pid, name);
        EnvRead::Unreadable
    }
}

/// The PID's procStart (start time), used to defend against PID reuse (#34).
///
/// Linux: the `starttime` field (jiffies since boot) from `/proc/<pid>/stat`.
/// Windows：`GetProcessTimes` 的创建时刻，FILETIME 原值（UTC、100ns、自 1601）。单位是平台原生的，与 Linux 的 jiffies 不可互比 ——
/// 只拿来与同一个读法读出来的另一次比相等（`#34` 基线 · `pidwatch` 开句柄后的复核），或经 [`start_epoch_from_ticks`] 换成秒。
/// 所以 `watcher.rs::add_time_verdict` 的「与 pidfile 里的 `procStart` 逐值相等」那一支在 Windows 上按构造不会命中
/// （claude 在 Windows 上写的是 .NET 本地 ticks），落到它自己的兜底启发式（「进程起得比 pidfile 晚 ⇒ 冒名」）—— 保守方向。
/// 其余平台：`None`。`proc_cmdline` 在 Windows 上仍是 `None`（要读 PEB）。
pub fn proc_starttime(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let stat = Ro::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        parse_starttime_from_stat(&stat)
    }
    #[cfg(windows)]
    {
        super::win_proc::start_filetime(pid)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = pid;
        None
    }
}

/// 这个进程的父进程号（读不到 · 这个平台不知道 ⇒ `None`）。agent 自问（`history/session_self.rs`）往上找会话用。
///
/// Linux：`/proc/<pid>/stat` 第 4 栏（名字里的括号照 [`super::linux_tables::parse_stat`] 那一处跳过）。Windows：进程快照里那一格（`win_tables::parent_of`）。
pub fn parent_of(pid: u32) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let stat = Ro::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        super::linux_tables::parse_stat(&stat).map(|s| s.ppid)
    }
    #[cfg(windows)]
    {
        super::win_tables::parent_of(pid)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = pid;
        None
    }
}

/// 这个进程**此刻**有没有控制终端（`/proc/<pid>/stat` 第 7 栏 `tty_nr` 非零）。`None` = 读不到 / 这个平台不知道。
///
/// 会话首领退出、终端被挂断时内核把整组进程的控制终端清掉 ⇒ 这一格从此是 0。
pub fn has_controlling_tty(pid: u32) -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        let stat = Ro::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        parse_tty_nr_from_stat(&stat).map(|n| n != 0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// `tty_nr`（第 7 栏）—— 与 [`parse_starttime_from_stat`] 同一个切法：从**最后一个** `)` 之后数（`comm` 里可以有空格与括号）。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_tty_nr_from_stat(stat: &str) -> Option<i64> {
    const TTY_NR_IDX_AFTER_COMM: usize = 7 - 3;
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm
        .split_whitespace()
        .nth(TTY_NR_IDX_AFTER_COMM)?
        .parse::<i64>()
        .ok()
}

/// Parse the boot time (`btime <epoch-secs>` line) out of `/proc/stat` content.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_btime(proc_stat: &str) -> Option<u64> {
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
/// constant offset that mis-kills every future real session with no self-heal.
/// Session-add is rare; one small /proc read is free.
///
/// Windows：`ticks` 是 [`proc_starttime`] 那一臂交的 FILETIME 原值 ⇒ 纯换算（[`unix_secs_from_filetime`]），不读任何东西、不碰时区。
pub fn start_epoch_from_ticks(ticks: Option<u64>) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let btime = Ro::read_to_string("/proc/stat")
            .ok()
            .and_then(|s| parse_btime(&s))?;
        Some(btime + ticks? / USER_HZ)
    }
    #[cfg(windows)]
    {
        unix_secs_from_filetime(ticks?)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = ticks;
        None
    }
}

/// FILETIME（100ns、自 1601-01-01 UTC）与 Unix 纪元之间差的 100ns 个数（369 年，含 89 个闰日）。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const FILETIME_TICKS_BEFORE_UNIX_EPOCH: u64 = 116_444_736_000_000_000;

/// 每秒多少个 FILETIME 刻度（100ns 一格）。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const FILETIME_TICKS_PER_SEC: u64 = 10_000_000;

/// FILETIME 原值 → Unix 纪元秒（向下取整，与 Linux 臂 `ticks / USER_HZ` 同一种取整）。
/// 早于 1970 的值 ⇒ `None`（不是一个会话进程能有的起始时刻）。纯函数，放在 cfg 外 ⇒ 在 Linux 上就测得到。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn unix_secs_from_filetime(filetime: u64) -> Option<u64> {
    filetime
        .checked_sub(FILETIME_TICKS_BEFORE_UNIX_EPOCH)
        .map(|t| t / FILETIME_TICKS_PER_SEC)
}

/// `pid` 此刻跑的二进制路径（Linux `/proc/<pid>/exe`；别的平台答不上 ⇒ `None`，调用方不杀）。
pub fn exe_of(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        Ro::read_link(format!("/proc/{pid}/exe"))
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// `/proc/<pid>/cmdline`, NUL separators turned into spaces, lossily decoded.
/// None when unreadable (vanished PID, permissions) → check skipped.
pub fn proc_cmdline(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let bytes = Ro::read(format!("/proc/{pid}/cmdline")).ok()?;
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
/// Wires the real `/proc` reads into the pure `liveness::is_same_live_process` decision
/// （那个函数住 `platform/liveness.rs`；这里不写 intra-doc 链接）。
pub fn session_alive(pid: u32, expected_start: Option<u64>) -> bool {
    let exists = pid_alive(pid);
    // Only read the current start if the PID exists (a read on a vanished PID is
    // pointless and would just be `None` anyway).
    let current_start = if exists { proc_starttime(pid) } else { None };
    super::liveness::is_same_live_process(exists, expected_start, current_start)
}

/// 一趟读盘超过这么多字节才把放掉的内存还给系统（[`return_freed_memory`]）：没读什么的那一趟不费这一下。
pub(crate) const TRIM_AFTER_BYTES: u64 = 16 << 20;

/// 一趟大读（读了 `read` 字节）之后，把分配器手里已经放掉、却还占着物理页的内存还给系统；没过 [`TRIM_AFTER_BYTES`] ⇒ 不做。
/// 回：做了没有。
/// glibc 的 `malloc` 放掉的空洞只在堆顶时才自己还 ⇒ 一问含工具的搜索读过几百 MB、只留下 128 MB 的常驻之后，
/// 进程 RSS 停在那一问的高水位（680 MB 合成世界实测 ~570 MB → 调它之后 ~280 MB）。
/// 远端 musl 版（mimalloc）不用这一下：空页一放就还（[`return_freed_pages_at_once`]）。Windows 的系统堆没有这一招 ⇒ 不做。
pub fn return_freed_memory(read: u64) -> bool {
    if read < TRIM_AFTER_BYTES {
        return false;
    }
    // SAFETY: 纯 libc 调用，不带指针；只把空闲页还给内核，不动在用的块。
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
    true
}

/// mimalloc 两个选项在它的选项枚举里的序号（锁里 `libmimalloc-sys 0.1.49` 带的 C 源 3.3.2，`mimalloc.h`）：
/// `purge_delay`（缺省 1000 ms）· `page_full_retain`（缺省 2）。升级之后序号挪了 ⇒ `freed_memory_tests` 读缺省值对不上 ⇒ 红。
#[cfg(target_env = "musl")]
pub(crate) const MI_OPTION_PURGE_DELAY: i32 = 15;
#[cfg(target_env = "musl")]
pub(crate) const MI_OPTION_PAGE_FULL_RETAIN: i32 = 36;

/// 远端 musl 版：分配器放回来的空页当场还给系统。进程起来第一下调一次（`main.rs`）；别的版本什么都不做。
/// mimalloc 缺省把空页记下、过 10 s（`purge_delay` 1 s × 页区那一档的倍数 10）之后**等下一次有分配或放回**才还，
/// 每条线程每种块大小还另留 2 页空的备用（`page_full_retain`）；大读之后各线程闲下来就一直占着，
/// 主线程调 `mi_collect` 也只扫得到自己那条线程的堆。两样都设 0 ⇒ 页一空当场还，不等谁再来分配，哪条线程放的都一样
/// （读数与选它不选「各线程收尾各还一次」的理由住 A17-45-4-13 的验收）。
pub fn return_freed_pages_at_once() {
    // SAFETY: mimalloc 的公开口（`mimalloc` crate 底下那份 C 库自带、链进了远端二进制），只设整数选项，不带指针。
    #[cfg(target_env = "musl")]
    unsafe {
        extern "C" {
            fn mi_option_set(option: i32, value: std::ffi::c_long);
        }
        mi_option_set(MI_OPTION_PURGE_DELAY, 0);
        mi_option_set(MI_OPTION_PAGE_FULL_RETAIN, 0);
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/freed_memory_tests.rs"]
mod freed_memory_tests;

/// [`raise_open_files_limit`] 的结局（`Display` 是记进日志的那一句）。
#[derive(Debug, PartialEq, Eq)]
pub enum OpenFiles {
    /// 抬了：从多少到多少。
    Raised { from: u64, to: u64 },
    /// 本来就在硬上限上。
    Already(u64),
    /// 抬不动（系统调用拒了）：此刻的软上限照旧。
    Stuck { soft: u64, to: u64, why: String },
    /// 这一平台没有这一格（Windows 的句柄数不按这个限）。
    NotHere,
}

impl std::fmt::Display for OpenFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // 只进日志（`main.rs` 流模式起来那一行），不上界面 ⇒ 同 `[relay]` 那几行写英文、不进文案表。
            OpenFiles::Raised { from, to } => {
                write!(f, "open files soft limit {from} -> {to} (the hard limit)")
            }
            OpenFiles::Already(n) => {
                write!(f, "open files soft limit already at the hard limit {n}")
            }
            OpenFiles::Stuck { soft, to, why } => {
                write!(
                    f,
                    "open files soft limit stays {soft}, cannot raise to {to}: {why}"
                )
            }
            OpenFiles::NotHere => {
                f.write_str("open files soft limit: not a thing on this platform")
            }
        }
    }
}

/// 常驻后端起来那一下把本进程的 fd 软上限抬到硬上限（硬上限是「不限」⇒ 抬到 Linux `fs.nr_open` 的缺省值 1<<20：软上限不许超过它）。
/// 桌面会话起进程缺省给软上限 1024，而中转每条在途连接约占三个 fd、各台常驻流与会话文件另占 ⇒ 不抬就是第一堵墙。
/// 只动软上限（不要特权）；硬上限不碰。抬不动照旧起，结局交调用方记一行。
pub fn raise_open_files_limit() -> OpenFiles {
    #[cfg(unix)]
    {
        let mut r = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: 读本进程的上限进栈上这一格；再按读到的硬上限设软上限，不碰内存别处。
        unsafe {
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut r) != 0 {
                return OpenFiles::Stuck {
                    soft: 0,
                    to: 0,
                    why: std::io::Error::last_os_error().to_string(),
                };
            }
            let from = r.rlim_cur as u64;
            let to = if r.rlim_max == libc::RLIM_INFINITY {
                1 << 20
            } else {
                r.rlim_max as u64
            };
            if from >= to {
                return OpenFiles::Already(from);
            }
            r.rlim_cur = to as libc::rlim_t;
            if libc::setrlimit(libc::RLIMIT_NOFILE, &r) != 0 {
                return OpenFiles::Stuck {
                    soft: from,
                    to,
                    why: std::io::Error::last_os_error().to_string(),
                };
            }
            OpenFiles::Raised { from, to }
        }
    }
    #[cfg(not(unix))]
    {
        OpenFiles::NotHere
    }
}

/// 这个错是不是「fd 用尽」：本进程的（`EMFILE`）或整台的（`ENFILE`）；Windows 上是套接字用尽（`WSAEMFILE`）。
pub fn out_of_fds(e: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        matches!(e.raw_os_error(), Some(libc::EMFILE | libc::ENFILE))
    }
    #[cfg(windows)]
    {
        /// `WSAEMFILE`（winsock 错误码表）。
        const WSAEMFILE: i32 = 10024;
        e.raw_os_error() == Some(WSAEMFILE)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = e;
        false
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/fd_limit_tests.rs"]
mod fd_limit_tests;

/// 把**调用它的这条线程**降到低优先级（后台一次性热缓存用：不跟前台的帧命令抢 CPU）。
/// 回真 ＝ 降成了；降不了 / 这一平台不支持 ⇒ 假（照常跑，只是不让）。它之后由这条线程起的线程随它（Linux 按线程记 nice）。
pub fn lower_this_thread() -> bool {
    #[cfg(target_os = "linux")]
    {
        // Linux 上 nice 值按线程记：`setpriority(PRIO_PROCESS, 线程号)` 只动这一条。
        // 已经比这还低（整个进程被 `nice` 起来的）⇒ 不动（往高调要特权，也不该）。
        // SAFETY: 纯系统调用，参数是本线程号与一个常数，不碰内存。
        unsafe {
            let tid = libc::gettid() as libc::id_t;
            if libc::getpriority(libc::PRIO_PROCESS, tid) >= BACKGROUND_NICE {
                return true;
            }
            libc::setpriority(libc::PRIO_PROCESS, tid, BACKGROUND_NICE) == 0
        }
    }
    #[cfg(windows)]
    {
        super::win_proc::lower_this_thread()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        false
    }
}

/// [`lower_this_thread`] 在 Linux 上给的 nice 值。
#[cfg(target_os = "linux")]
pub const BACKGROUND_NICE: libc::c_int = 10;
