//! pidfd 看守这一族平台原语。本模块只提供 [`watch_pid_until_exit`]；「醒了往哪个 channel 发哪一种 `WatchEvent`」是 observe 的域知识，
//! `watcher.rs` 留一层薄包装把 `on_dead` 实现成 `tx.send(target.death_event(pid))`。
//!
//! # 三条判死路径 + 一条不判死的路径
//!
//! 1. `pidfd_open` 失败（`ESRCH` 等）⇒ 目标已不在 ⇒ 判死。
//! 2. open 成功后再读一次 `proc_starttime` 与 add 时捕获的基线比对（`is_same_live_process`）：不符 = 在「读 pidfile」与「开 pidfd」之间
//!    发生了 PID 复用，开到的是冒名者 ⇒ 判死。procStart 校验只在开 pidfd 时做一次，之后靠内核。
//! 3. 起线程 `poll(pidfd, POLLIN, -1)`；醒了判死。
//!
//! `poll` 真出错（非 `EINTR`）时刻意不判死：宁可让会话留在 live、等 pidfile 删除或断连来收，也不因一次系统调用失败就误归档。
//! 这条没有普通测试能覆盖（要让 `poll(2)` 真出错），由本文件末尾的源码扫描钉住。
//!
//! 线程数的界：每个被追踪的 (pidfile, pid) 最多一条（一台机器上同时活着的交互会话数）。pidfile 先被删而进程仍在时，
//! 那条线程等到进程退出发一条陈旧唤醒，被消费侧的 pid 比对挡掉。

//!
//! ---
//!
//! # 按平台分文件
//!
//! `linux` 是 pidfd 实现，`win32` 是 Windows 实现（开带 `SYNCHRONIZE` 的进程句柄 ＋ 不带超时地等；Win32 读法住 `platform/win_proc.rs`），
//! `fallback` 是没有承诺的平台（macOS 等）的诚实空壳（见它自己的头注）。不写 intra-doc 链接：几个 mod 各自带 cfg，任一 target 上只有一个存在。
//! 分文件而不是在函数里塞 `#[cfg]`：这一族的平台差异是整套机制不同，不是某一行不同。
//! `win32` 与 `linux` 逐形对拍（三条判死 ＋ 一条不判死 ＋ Windows 独有的「拒绝访问 ⇒ 不判死」），判据住
//! `tests/backend/platform/pidwatch_windows_shape_tests.rs`；Windows 那一份只买到编得过 ＋ 源码对拍，真机零读数。


use copy_core::copy_text;

#[cfg(not(any(target_os = "linux", windows)))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod win32;

#[cfg(target_os = "linux")]
pub(crate) use linux::watch_pid_until_exit;
// `pidfd_open`：`watcher.rs` 的测试段 ＋`platform/signal.rs::stoppable`（一次性子命令 `--resident-stop` 的进程把手）。
#[cfg(not(any(target_os = "linux", windows)))]
pub(crate) use fallback::watch_pid_until_exit;
#[cfg(target_os = "linux")]
pub(crate) use linux::pidfd_open;
#[cfg(windows)]
pub(crate) use win32::watch_pid_until_exit;

//
// ══════════════════════════════════════════════════════════════════════════
// 「这个平台上有没有自愈」要说得出口：自愈靠内核送的死亡事件醒，没有那条腿的平台上自愈结构性不成立。
//
// 不是三态：在那些平台上这件事不是「不知道」，是确证没有（「不知道」会被下游读成「也许有」）。
// 形状是刻意的：两条 `#[cfg]` 臂 + 一个裸布尔字面量，不是一行 `cfg!()` —— 非主臂一旦被改成乐观值，
// `platform/fallback_guard.rs` 的 `fallback_branches_must_not_fabricate_success` 当场红；写成 `cfg!()` 那一行不带 `#[cfg]` 属性，整个掉出那道护栏的人群。
// `fallback.rs` 本身对那道护栏贡献 0 个受检块（它整份文件是被 `#[cfg(...)] mod fallback;` 选进来的），钉它的是本文件末尾那条 `fallback_shape_tests`。
// ══════════════════════════════════════════════════════════════════════════

/// Linux：**有**那条腿 —— `pidfd_open` + `poll(pidfd, POLLIN, -1)`，
/// 阻塞在内核事件上（`mod.rs` 头注：`poll` 的超时是 `-1`，不是节拍）⇒ 零定时器。
#[cfg(target_os = "linux")]
#[allow(dead_code)] // `K-P3` 第一档：能填不真填 —— 接线是一次发布决策，不是忘了。
pub(crate) const fn death_events_available() -> bool {
    true
}

/// Windows：有那条腿 —— 进程句柄 ＋ 不带超时的等待（`win32.rs`）。值不在这里写字面量（非主分支的块里出现裸 `true` 一律判伪造成功），
/// 读的是 `win32.rs` 紧挨着实现的声明；背书它的是 `pidwatch_windows_shape_tests`（判死路径与 `linux.rs` 逐形相等）。
/// 那个「有」是源码写对了，不是真机验过。
#[cfg(windows)]
#[allow(dead_code)] // 同上。
pub(crate) const fn death_events_available() -> bool {
    win32::WAKES_ON_EXIT
}

/// 其余平台（macOS 等）：**没有**。`fallback::watch_pid_until_exit` 什么都不做，
/// `on_dead` 永远不会被调用（那是它头注里刻意选的保守方向）。
#[cfg(not(any(target_os = "linux", windows)))]
#[allow(dead_code)] // 同上。
pub(crate) const fn death_events_available() -> bool {
    false
}

/// 那句话本身：要的是「说出口」，所以它是一句话，不是一个 bool —— 只有布尔的话，下游可以读完它什么都不说。
#[allow(dead_code)] // 同上。
pub(crate) static NO_DEATH_EVENTS_HERE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("bePidwatch.noDeathEventsHere.say", &[]));

/// 这台机此刻该不该说那句话。`None` = 有那条腿、不必声明（不是「不知道」：那两件事由 [`death_events_available`] 那个二值先分开了）。
#[allow(dead_code)] // 同上。
pub(crate) fn self_healing_caveat() -> Option<&'static str> {
    if death_events_available() {
        None
    } else {
        Some(NO_DEATH_EVENTS_HERE.as_str())
    }
}

/// 一条只在「没有那条腿」的平台上编译时存在的编译期断言。本机门禁上给不出读数（这个 item 在 Linux 上编译期就不存在），
/// 开口的时刻是任何一次既非 Linux 也非 Windows 的编译。Linux 那侧只有源码文本判据（证「那一支写在那儿」），这一条证「那一支真的被编进去了」。
#[cfg(not(any(target_os = "linux", windows)))]
const _: () = assert!(
    !death_events_available(),
    "没有进程看守的平台上这一格必须是「确证没有」——`fallback::watch_pid_until_exit` 什么都不做，\
     它永远不调 on_dead。这里回一个乐观值等于替一条不存在的腿担保，\
     而下游会拿它当「这台机上崩了会有人管」。"
);

/// 非 Linux 那份空壳的两条承诺，只能靠源码级守卫钉：`fallback.rs` 在本机根本不编译，行为判据结构上不可能存在。
///
/// 钉的两条：不立刻调 `on_dead()`（那是最坏的选项：进程活得好好的、会话被判死）· 缺判活路径要 `tracing::error!`、不许降成 `warn!`。
/// 变异这两处后端全套都照样绿，所以必须钉；「起个轮询线程」那条已有 `no_timer_guard` 管，不重复钉。
/// 用 `pin_line`（整行相等）而不是 `contains`：子串匹配会让「在同一行后面追加一个调用」溜过去。
#[cfg(test)]
#[path = "../../../../tests/backend/platform/pidwatch_fallback_shape_tests.rs"]
mod fallback_shape_tests;

/// 「这个平台上有没有自愈」这件事说得出口。三条各证一半：
/// - 本机（Linux）有腿 ⇒ 没有要声明的话（负例：少了它，把 `self_healing_caveat` 写成 `Some(…)` 恒真也照样绿）；
/// - 非 Linux 那一支本机编不到 ⇒ 把源码当数据读（形状照 `main_fourth_face_tests.rs::the_windows_arm_is_wired_into_the_source`）；
/// - 那句话本身必须真的说「没有」。
/// 这几条证的是「那一支写在源码里」，证不了别的平台上编出来的二进制真走了那一支（那是同文件那条编译期断言的事）。
#[cfg(test)]
#[path = "../../../../tests/backend/platform/pidwatch_death_event_leg_tests.rs"]
mod death_event_leg_tests;

/// **Windows 那一份与 Linux 那一份逐形对拍**（把源码当数据读 —— 本机编不到它）。
#[cfg(test)]
#[path = "../../../../tests/backend/platform/pidwatch_windows_shape_tests.rs"]
mod windows_shape_tests;
