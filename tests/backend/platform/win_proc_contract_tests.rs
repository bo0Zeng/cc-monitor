//! **Windows 判活那一臂：本机（Linux）够得着的那几半。**
//!
//! Windows 臂的函数体在本机被 `cfg` 掉，`cargo test` 执行不到它 ⇒ 能进执行链的只有三样：
//!
//! 1. **纯换算**（`proc.rs::unix_secs_from_filetime`）—— 放在 cfg 外，本机直接喂值。
//!    对拍的另一侧是一个**与本仓无关的公知常量**（2000-01-01T00:00:00Z 的 FILETIME），
//!    不是拿实现里那个偏移量自己算一遍（那样两侧同源、恒真）。
//! 2. **与 Linux 臂同契约的那几格**（存在性 ≠ 权限；读不到不判死）—— 把源码当数据读，
//!    钉的是 `win_proc.rs` 里那张三分支映射的每一行。
//! 3. **Win32 读法只有一个住址**：那四个 `kernel32` 名字在后端生产段里出现的文件集合
//!    == {`platform/win_proc.rs`}（两向相等）。
//!
//! 🚫 买不到：Win32 调用在真 Windows 上的行为（错误码、句柄语义、`WaitForSingleObject` 何时醒）。
//! 那要真机；本路不碰 Win11 虚拟机（未拍）。编得过由门禁 `winchk-backend` 那一格买。

use super::proc::{
    unix_secs_from_filetime, FILETIME_TICKS_BEFORE_UNIX_EPOCH, FILETIME_TICKS_PER_SEC,
};

/// 2000-01-01T00:00:00Z 的 FILETIME（公知值，Win32 文档与各家换算器同答）与它的 Unix 秒。
/// ⚠ 刻意写成字面量、不从本仓任何常量推出来 —— 这是对拍的**另一侧**。
const Y2K_FILETIME: u64 = 125_911_584_000_000_000;
const Y2K_UNIX_SECS: u64 = 946_684_800;

#[test]
fn filetime_converts_to_unix_seconds_against_an_outside_anchor() {
    assert_eq!(
        unix_secs_from_filetime(Y2K_FILETIME),
        Some(Y2K_UNIX_SECS),
        "FILETIME → Unix 秒 对不上 2000-01-01 那个公知锚点 —— 偏移量或刻度写错了。\n\
         这个值喂进 `watcher.rs::add_time_verdict` 的「进程起得比 pidfile 晚」那一格：\n\
         偏一个小时就会把活会话判成冒名、或把冒名放过去。"
    );
    // 纪元本身 ⇒ 0；早一个刻度 ⇒ 不是一个会话进程能有的起始时刻 ⇒ None（不编一个 0 出来）。
    assert_eq!(
        unix_secs_from_filetime(FILETIME_TICKS_BEFORE_UNIX_EPOCH),
        Some(0)
    );
    assert_eq!(
        unix_secs_from_filetime(FILETIME_TICKS_BEFORE_UNIX_EPOCH - 1),
        None,
        "早于 1970 的 FILETIME 被换成了一个数 —— 那是给答不上来的问题编答案（`fallback_guard` 那条纪律）"
    );
    // 向下取整：与 Linux 臂 `ticks / USER_HZ` 同一种取整（同一秒内的两个刻度答同一秒）。
    assert_eq!(
        unix_secs_from_filetime(Y2K_FILETIME + FILETIME_TICKS_PER_SEC - 1),
        Some(Y2K_UNIX_SECS),
        "不满一秒的刻度被进位了 —— Linux 臂是向下取整，两臂在「同一秒」上要答同一个数"
    );
    assert_eq!(
        unix_secs_from_filetime(Y2K_FILETIME + FILETIME_TICKS_PER_SEC),
        Some(Y2K_UNIX_SECS + 1)
    );
    // 极值不溢出、不 panic。
    assert!(unix_secs_from_filetime(u64::MAX).is_some());
    assert_eq!(unix_secs_from_filetime(0), None);
}

const WIN_PROC: &str = include_str!("../../../src/backend/platform/win_proc.rs");
const PROC: &str = include_str!("../../../src/backend/platform/proc.rs");

/// `pid_alive` 的契约是**存在性**：Linux 臂是 `/proc/<pid>` 在不在（别的用户的进程照样在），
/// Windows 臂必须同口径 —— 「没权限开它」算**在**，「开得到但这一刻问不出退出码」也算**在**
/// （`liveness.rs` 那条「读不到绝不误归档」），只有「开不出来且不是权限问题」才算不在。
///
/// ⚠ 钉整行（`pin_line`），不钉子串：事实的单位是「这一格映射到哪个值」。
#[test]
fn the_windows_existence_read_keeps_the_linux_contract() {
    let prod = guard_core::production_code(WIN_PROC);
    // 反空真：剥法跑飞 / 文件被掏空时下面几条会在空文本上各自报「找不到」，
    // 那样红得对，但话不对 —— 先说清楚是哪一种。
    assert!(
        prod.lines().count() >= 60,
        "`win_proc.rs` 生产段只剩 {} 行 —— 剥法坏了或文件被掏空",
        prod.lines().count()
    );
    for (line, why) in [
        (
            "Opened::Denied => true,",
            "「没权限」被读成了「不在」—— Linux 的 `/proc/<pid>` 对别的用户的进程照样存在，\
             这里问的是存在性，不是权限（monitor 侧 `session_map.rs` 那一份问的是另一件事，别照抄那一格）",
        ),
        (
            "Opened::Handle(h) => still_running(&h).unwrap_or(true),",
            "「开得到但这一刻问不出退出码」不再按「在」处置 —— 那是一次瞬时读失败，\
             `liveness.rs` 头注逐字：瞬时读失败**绝不**误归档",
        ),
        (
            "Opened::Gone(_) => false,",
            "「开不出来且不是权限问题」不再是「不在」—— 那会让一份死进程留下的 pidfile 永远 live",
        ),
    ] {
        guard_core::pin_line(&prod, line).unwrap_or_else(|e| panic!("{e}\n⇒ {why}"));
    }
}

/// 三个判活函数在 `proc.rs` 里**各自恰好一条 Windows 臂**，且那一臂只转调 `win_proc`
/// （Win32 读法不许散进 `proc.rs`）。
#[test]
fn each_liveness_fact_has_exactly_one_windows_arm_that_delegates() {
    let prod = guard_core::production_code(PROC);
    for line in [
        "super::win_proc::exists(pid)",
        "super::win_proc::start_filetime(pid)",
        "unix_secs_from_filetime(ticks?)",
    ] {
        let at = guard_core::pin_line(&prod, line).unwrap_or_else(|e| {
            panic!("{e}\n⇒ Windows 那一臂不再转调 `win_proc` / 纯换算了（或者多了一份）")
        });
        // 那一行的上一条非空、非花括号行必须是 `#[cfg(windows)]` —— 臂真的挂在 Windows 门后。
        let above = prod
            .lines()
            .take(at)
            .map(str::trim)
            .filter(|l| !l.is_empty() && *l != "{")
            .last()
            .unwrap_or("");
        assert_eq!(
            above, "#[cfg(windows)]",
            "`{line}` 不是挂在 `#[cfg(windows)]` 门后的那一臂（它上面是 `{above}`）"
        );
    }
    // 剩下的平台仍是诚实空壳那一臂 —— 三处各一条，且不再写 `not(target_os = "linux")`
    // （那样 Windows 会同时落进两条臂，编不过；这一格是写给改 cfg 的人看的）。
    assert_eq!(
        prod.matches("#[cfg(not(any(target_os = \"linux\", windows)))]")
            .count(),
        3,
        "`pid_alive` / `proc_starttime` / `start_epoch_from_ticks` 的兜底臂不是恰好三条"
    );
}

/// **Win32 读法只有一个住址**：`OpenProcess` 一族四个名字在后端生产段里出现的文件集合
/// == {`platform/win_proc.rs`}，两向相等。
///
/// 多一个文件 ⇒ 有人在别处又写了一份 Win32 判活（两份读法迟早漂）；
/// 少了 `win_proc.rs` ⇒ 读法搬走了而本条的锚没跟上。
#[test]
fn the_win32_process_reads_live_in_exactly_one_file() {
    let root = crate::guard_support::src_root();
    let names = ["OpenProcess", "GetExitCodeProcess", "GetProcessTimes"];
    // `WaitForSingleObject` 从这张名单里拆出去单列：它是 Win32 的**通用等待**，不只等进程 ——
    //   `platform/lock.rs` 用它等第四层那把跨进程锁（命名互斥量）。那一格不是「又一份判活读法」，
    //   所以它的人群单独两向相等（下面 `waiters`），而进程读法这三个名字的人群照旧只有 `win_proc.rs`。
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut waiters: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut scanned = 0usize;
    for (path, raw) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        let prod = guard_core::production_code(&raw);
        // 顺着 `#[path]` 收进来的按模块住址认。
        let rel = guard_core::module_address(&root, &path);
        if names.iter().any(|n| guard_core::contains_word(&prod, n)) {
            seen.insert(rel.clone());
        }
        if guard_core::contains_word(&prod, "WaitForSingleObject") {
            waiters.insert(rel);
        }
    }
    let want_waiters: std::collections::BTreeSet<String> = [
        "platform/win_proc.rs".to_string(),
        "platform/lock.rs".to_string(),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        waiters, want_waiters,
        "后端生产段里用 `WaitForSingleObject` 的文件集合变了（登记：等进程退出 · 等第四层的跨进程锁）"
    );
    assert!(
        scanned > 50,
        "只扫到 {scanned} 份 `.rs` —— 遍历坏了，本条在空转"
    );
    let want: std::collections::BTreeSet<String> =
        ["platform/win_proc.rs".to_string()].into_iter().collect();
    assert_eq!(
        seen, want,
        "后端生产段里提到 `OpenProcess` 一族的文件集合变了。\n\
         Windows 上「进程在不在 / 何时起 / 等它死」的读法只许住 `platform/win_proc.rs`（\n\
         platform 是唯一的翻译官）。"
    );
}
