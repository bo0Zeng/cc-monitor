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
//! **现状**〔09-24 改写〕：判活三件（`pid_alive` / `proc_starttime` / `start_epoch_from_ticks`）
//! 有了 **Windows 臂**（U4b 那一半：`OpenProcess` ＋ 退出码 ＋ `GetProcessTimes`，Win32 读法只住
//! `platform/win_proc.rs`）；其余平台（macOS 等）仍是 U4a 那个大声的 `unimplemented!()` / `None`。
//! `proc_env_var` / `proc_cmdline` 在 Windows 上仍是「读不到」（读别的进程的环境与命令行要读对方
//! PEB，未公开结构，交叉编译验不了语义 ⇒ 本轮不做，登记在件 E）。
//! 🚫 Windows 臂**只买到编得过**（`winchk-backend`）＋ 纯换算那一半在 Linux 上的对拍；真机零读数。
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
        // Windows 那一格已由上面的 `#[cfg(windows)]` 臂接走；本臂今天只剩
        // 没有承诺的平台（macOS 等）。下面这段是 U4a 的原话，照留。
        //
        // ★ U4a（2026-08-01）：**从「静默说谎」改成「大声未实现」。**
        //
        // 这里原本是 `let _ = pid; true`，注释写「treat as alive so the cross-platform
        // smoke still exercises the pipeline」。那个 `true` 是一个**没人会发现的谎**：
        // `pid_alive` 是判活的加表门，恒 `true` 的后果是**会话永远不被归档**，
        // 而且没有任何信号说「这个平台上我根本不知道」。
        //
        // U2 与 U3 两轮都明确把它推迟到本功能，理由是「改它 = 决定 Windows 语义」。
        // 到了 U4a，真语义（`OpenProcess` + 退出码）仍属 **U4b** —— 它需要 Windows 真机验证，
        // 而自己写着「等价性仓里无实测，第一步先验」。
        //
        // 那 U4a 能做的是什么？**把谎换成事实**：
        // - `panic` 是一个没人能忽略的信号，`true` 不是。
        // - **不可能回归 Linux**：这条分支在 Linux 上编译期就不存在。
        // - Windows backend 今天跑不起来（U4b 才让它能跑），所以不影响任何现存路径。
        // - 它给 U4b 留了一个**编译器/运行时帮你找**的落点，而不是一个「看起来能用」的假实现。
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

/// 读一次 `/proc/<pid>/environ` 抠某个键的**三态结果**〔`K-R21`，2026-09-03〕。
///
/// # 它为什么存在：那个 `Option` 装的不是一件事，是四件
///
/// [`proc_env_var`] 从前回 `Option<String>`，而那个 `None` 是**四条不同的事实**的共同出口：
///
/// | 支 | 何处 | 事实 | 09-03 现打：实测发不发生 |
/// |---|---|---|---|
/// | 一 | `std::fs::read(…)` 回 `Err` | 环境**读不到**（进程没了 / 权限 / 竞态） | **400 次 `Err = 0`** —— 几乎不发生 |
/// | 四 | `std::fs::read(…)` 回 `Ok(vec![])` | **读得到，但回 0 字节** | **`397 / 400`** —— 真正在咬人的就是它 |
/// | 二 | 键在，值是空串 | 显式设成了空 | — |
/// | 三 | 循环走完没命中 | **压根没这个键** | — |
///
/// **支四与支三从前走同一条出口、不可区分** —— 而它俩说的是相反的两件事：
/// 一个是「这一刻我读不出来」，一个是「我读到了，它确实没设」。
/// 支四的两个真实来源：**exec 窗口**（60–140 µs，进程刚 `execve`、mm 还没装好）
/// 与**僵尸进程**（`/proc/<pid>/stat` 还在 ⇒ 判活仍是 `true`，而 mm 已释放 ⇒ environ 读回 0 字节）。
/// ⚠ exec 窗口还有一形**不走支四**：vfork 父进程被放回来、子进程还没换 mm 的那一刻，读到的是**父进程**的环境
/// （非空 ⇒ `Unset` 或父的值）。生产调用方读的都是自己写了 pidfile 的进程（早 exec 完）碰不到；起子进程的夹具要自己等（`identity_tag_tests::spawn_settled_sleep`）。
///
/// # 🔴 它**只**拆出一支，另两支**刻意仍然合并**（`K-R21` PM 裁定选「乙」不选「甲」）
///
/// - `Unreadable` = 「**环境这一刻取不到**」= 支一 ∪ 支四；
/// - `Unset` = 「读得到、但这个键不作数」= 支二 ∪ 支三，**仍然合并**。
///
/// 合并那两支的依据是 09-03 逐个调用方现打的等价性：三个调用方的下游谓词
/// （`launch_id_is_safe` / `pane_is_safe` / `is_safe_config_dir`）**都对空串恒 `false`**
/// ⇒ 「键不在」与「值是空串」在今天的每一个调用方那里都落到同一格。
/// ⚠ 这条等价性**是量出来的、不是永真的**：哪天有调用方开始把空串当有意义的值，
/// 这两支就得再拆一次。
///
/// ⚠ **「甲」（每一支各给一格、调用方逐个决定）没有作废** —— 它是这一族的终局形状，
/// `K-R21` 把它登记成后续清理，不在那一拍射程里。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnvRead {
    /// 读到了这个键，且值非空。
    Value(String),
    /// 环境**读得到**，而这个键不作数：没有这个键 / 它的值是空串（两支合并，见类型头注）。
    Unset,
    /// **环境这一刻取不到**：读失败，或读回 0 字节。
    ///
    /// 🔴 它**不是**「没设」—— 把它当成「没设」正是 `K-R21` 治的那句假话
    /// （`observe/accounts_query.rs` 会据此把一条真跑在账号 Z 下的会话报成账号 0 的）。
    Unreadable,
}

impl EnvRead {
    /// 把「取不到」与「没设」合回一个 `None` —— **今天不需要区分**的调用方用这个。
    ///
    /// ⚠ 用它就等于声明「这两件事对我等价」。`K-R21` 逐个核过：
    /// `control/identity_tag.rs` 与 `accounts_query.rs` 的 `CCM_LAUNCH_ID` 那一处
    /// 今天都是 **fail-closed**（两条路都得同一个保守答案）⇒ 对它们确实等价。
    /// 而 `accounts_query.rs` 的 `CLAUDE_CONFIG_DIR` 那一处**不等价**（它 fail-open），
    /// 所以那一处**不用这个方法**，它自己 `match` 三支。
    pub(crate) fn value(self) -> Option<String> {
        match self {
            EnvRead::Value(v) => Some(v),
            EnvRead::Unset | EnvRead::Unreadable => None,
        }
    }
}

/// 从 `/proc/<pid>/environ` 抠**某一个**环境变量的值，三态返回（见 [`EnvRead`]）。
/// 空值按未设算（回 `Unset`）。**这一刻读不出来**（读失败 / 读回 0 字节）回 `Unreadable`。
/// 形状照同模块的 [`proc_cmdline`]。
///
/// U2 从 `accounts_query.rs` 搬来（Phase D 审计：它带着两个 `target_os` cfg 留在 observe 侧文件里，
/// U3 一划层就会当场违反「`platform/` 是唯一允许平台 cfg 的层」）。
///
/// ⚠ `S3` 把**变量名**参数化了：原来它叫 `proc_claude_config_dir`、把
/// `CLAUDE_CONFIG_DIR` 写死在这一层。`platform/` 是"唯一允许平台原语"的层，
/// 它不该认识任何一个 agent 的环境变量叫什么 —— 那是 `agents/<名>/` 的事。
/// 这一处是本件让 `platform/` 整层变干净、从而能进 `S1` 的 `CORE_FILES` 的**唯一**改动。
///
/// ⚠⚠ **调用形状（`proc_env_var(pid, <键>)` 这一串字面）是两把尺子的量点**，别改：
/// `observe/accounts_query_tests.rs::the_only_env_keys_this_module_reads_are_the_two_named_constants`
/// 与**跨 crate** 的 `src/frontend/shell/src/doc_claim_registry_tests.rs::env_keys_actually_read`
/// 都按这串字面数「backend 今天真读几个键」，后者再拿那个数去与盘上五份文档的计数词对拍。
/// `K-R21` 只换返回类型、**一个字面都没动**，正是为了不惊动它们。
///
/// 🔴 **`warn!` 在这里而不在调用方**：这是那次读的**唯一发生地**，
/// 只有这里知道刚才走的是哪一支（`K-R21` PM 裁定 ㈢：「今天先加 `warn!` 而不拆状态，
/// 等于打印一个自己也分不清的值」⇒ 拆状态与留痕必须同拍）。
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
        // 🔴 支四：`Ok(vec![])`。从前它会走完下面那个循环、落到函数尾，
        // 与「压根没这个键」用同一条出口 —— 而它俩说的是相反的两件事。
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
        // 非目标平台没有 `/proc` ⇒ 诚实答「这一刻取不到」，**不是**「读到了、没设」。
        // ⚠ 顺带如实登记一格：本文件这个块从前的值是字面 `None`，
        // 而 `platform/fallback_guard::the_platform_blocks_are_still_a_mixed_population`
        // 的分类器只认字面 `false` / `None` / `unimplemented!(` 当「诚实空壳」
        // ⇒ 换成 `EnvRead::Unreadable` 之后它会把这个块数进「真实现」那一类。
        // 两条断言（地板 4 · `real > 0`）都还过，但那张表的 stub/real 读数从此偏了一格。
        // 那个文件不在 `K-R21` 的写区，已上报（`§6`）。
        let _ = (pid, name);
        EnvRead::Unreadable
    }
}

/// The PID's procStart (start time), used to defend against PID reuse (#34).
///
/// Linux: the `starttime` field (jiffies since boot) from `/proc/<pid>/stat`.
/// Windows：`GetProcessTimes` 的创建时刻，**FILETIME 原值**（UTC、100ns、自 1601）。
/// ⚠ **单位是平台原生的，与 Linux 的 jiffies 不可互比** —— 本值只拿来与**同一个读法**读出来的
/// 另一次比相等（`#34` 基线 · `pidwatch` 开句柄后的复核），或经 [`start_epoch_from_ticks`] 换成秒。
/// ⚠ 于是 `watcher.rs::add_time_verdict` 的「与 pidfile 里的 `procStart` 逐值相等」那一支在 Windows 上
/// **按构造不会命中**（claude 在 Windows 上写的是 .NET 本地 ticks，`src/frontend/shell/src/utils.rs::NetTicks`），
/// 那一趟落到它自己的兜底启发式（「进程起得比 pidfile 晚 ⇒ 冒名」）—— 保守方向；
/// 把两种单位对上是那个判定自己的事（`observe/`），不在翻译官这一层。
/// 其余平台：`None`（「不知道」的诚实表达）。`proc_cmdline` 在 Windows 上仍是 `None`（要读 PEB）。
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
/// constant offset that mis-kills every future real session with no self-heal
/// (F20 audit I-1). Session-add is rare; one small /proc read is free.
///
/// Windows：`ticks` 是 [`proc_starttime`] 那一臂交的 FILETIME 原值 ⇒
/// 纯换算（[`unix_secs_from_filetime`]），不读任何东西、不碰时区。
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
/// 早于 1970 的值 ⇒ `None`（不是一个会话进程能有的起始时刻；不给它编一个 0）。
///
/// 纯函数，放在 cfg 外 ⇒ **在 Linux 上就测得到**（照 [`parse_btime`] 那条先例）。
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
