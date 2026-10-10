//! `/proc` 与进程身份这一族平台原语（`platform/` 是唯一允许出现平台原语与平台 `cfg` 的地方）。
//!
//! 判活三件（`pid_alive` / `proc_starttime` / `start_epoch_from_ticks`）有 Linux 与 Windows 两臂（Windows 的 Win32 读法只住
//! `platform/win_proc.rs`：`OpenProcess` ＋ 退出码 ＋ `GetProcessTimes`）；其余平台（macOS 等）是大声的 `unimplemented!()` / `None`。
//! `proc_env_var` / `proc_cmdline` 在 Windows 上是「读不到」（读别的进程的环境与命令行要读对方 PEB，未公开结构）。
//! Windows 臂只买到编得过（`winchk-backend`）＋ 纯换算那一半在 Linux 上的对拍；真机零读数。
//! `platform/fallback_guard.rs` 钉住这一族：fallback 分支不许凭空返回「成功」值。

/// Whether `pid` currently exists as a process on this host (existence only).
///
/// Linux (the backend's real target): `/proc/<pid>` existence. This is the
/// add-time gate; the reuse-proof check is [`session_alive`].
///
/// Windows：开得到句柄且退出码是 `STILL_ACTIVE` ⇒ 在；**「拒绝访问」也算在**
/// （与 Linux 同契约：`/proc/<pid>` 对别的用户的进程照样存在 —— 这里问的是存在性，不是权限）；
/// 开得到句柄但这一刻问不出退出码 ⇒ 按「读不到不判死」算在（`liveness.rs` 那条纪律）；
/// 其余开不出来 ⇒ 不在。
pub(crate) fn pid_alive(pid: u32) -> bool {
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

/// 读一次 `/proc/<pid>/environ` 抠某个键的三态结果。
///
/// 一个 `Option` 装不下这里的事实：
///
/// | 支 | 何处 | 事实 |
/// |---|---|---|
/// | 一 | `std::fs::read(…)` 回 `Err` | 环境读不到（进程没了 / 权限 / 竞态）—— 很少发生 |
/// | 四 | `std::fs::read(…)` 回 `Ok(vec![])` | 读得到，但回 0 字节 —— 常见：exec 窗口（进程刚 `execve`、mm 还没装好）与僵尸进程 |
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
pub(crate) enum EnvRead {
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
pub(crate) fn proc_env_var(pid: u32, name: &str) -> EnvRead {
    #[cfg(target_os = "linux")]
    {
        let bytes = match std::fs::read(format!("/proc/{pid}/environ")) {
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
pub(crate) fn proc_starttime(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
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

/// 这个进程**此刻**有没有控制终端（`/proc/<pid>/stat` 第 7 栏 `tty_nr` 非零）。`None` = 读不到 / 这个平台不知道。
///
/// 会话首领退出、终端被挂断时内核把整组进程的控制终端清掉 ⇒ 这一格从此是 0。
pub(crate) fn has_controlling_tty(pid: u32) -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
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
/// constant offset that mis-kills every future real session with no self-heal.
/// Session-add is rare; one small /proc read is free.
///
/// Windows：`ticks` 是 [`proc_starttime`] 那一臂交的 FILETIME 原值 ⇒ 纯换算（[`unix_secs_from_filetime`]），不读任何东西、不碰时区。
pub(crate) fn start_epoch_from_ticks(ticks: Option<u64>) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let btime = std::fs::read_to_string("/proc/stat")
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
pub(crate) fn exe_of(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/exe"))
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
/// Wires the real `/proc` reads into the pure `liveness::is_same_live_process` decision
/// （那个函数住 `platform/liveness.rs`；这里不写 intra-doc 链接）。
pub(crate) fn session_alive(pid: u32, expected_start: Option<u64>) -> bool {
    let exists = pid_alive(pid);
    // Only read the current start if the PID exists (a read on a vanished PID is
    // pointless and would just be `None` anyway).
    let current_start = if exists { proc_starttime(pid) } else { None };
    super::liveness::is_same_live_process(exists, expected_start, current_start)
}

/// 把分配器手里已经放掉、却还占着物理页的内存还给系统（全文搜索整份重读了一大批之后调一次）。
/// glibc 的 `malloc` 放掉的空洞只在堆顶时才自己还 ⇒ 一问含工具的搜索读过几百 MB、只留下 128 MB 的常驻之后，
/// 进程 RSS 停在那一问的高水位（680 MB 合成世界实测 ~570 MB → 调它之后 ~280 MB）。
/// 远端 musl 版的分配器是 mimalloc（`main.rs`；它自己隔一会儿把空页还回去）、Windows 的系统堆没有这一招 ⇒ 这两处不做。
pub(crate) fn return_freed_memory() {
    // SAFETY: 纯 libc 调用，不带指针；只把空闲页还给内核，不动在用的块。
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
}

/// 把**调用它的这条线程**降到低优先级（后台一次性热缓存用：不跟前台的帧命令抢 CPU）。
/// 回真 ＝ 降成了；降不了 / 这一平台不支持 ⇒ 假（照常跑，只是不让）。它之后由这条线程起的线程随它（Linux 按线程记 nice）。
pub(crate) fn lower_this_thread() -> bool {
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
pub(crate) const BACKGROUND_NICE: libc::c_int = 10;
