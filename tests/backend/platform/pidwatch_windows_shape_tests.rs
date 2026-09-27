//! 〔WN1 · U4b 后半〕**`pidwatch/win32.rs` 与 `pidwatch/linux.rs` 逐形对拍。**
//!
//! `win32.rs` 是 `#![cfg(windows)]` ⇒ 本机（Linux）根本不编译它，行为判据结构上不可能存在；
//! 能钉的只有**把源码当数据读**（同 `pidwatch_fallback_shape_tests.rs` 那条的处境）。
//! 对拍的另一侧是 `linux.rs` —— 它**在执行链上**：`linux_tests.rs` 真开 pidfd、真等进程死，
//! 「`poll` 真错误不报死」那一格也有自己的判据。⇒ 两份文件各自取数、再比相等，不是一份拿自己比自己。
//!
//! 钉的是 `mod.rs` 头注「三条判死路径 ＋ 一条**不**判死的路径」那张契约：
//! ① 判死次数两边相等（三条）；② 复核身份用的是同一个函数、同一行；
//! ③ 次序相同：开 → 复核 → 起线程；④ 不判死的那几条臂里一次 `on_dead()` 都没有；
//! ⑤ 等待不带超时（零定时器：那个等待只等内核那一个事件）。
//!
//! 🚫 买不到：Win32 调用在真 Windows 上的语义。那要真机（本路不碰 Win11 虚拟机）。

const LINUX: &str = include_str!("../../../src/backend/platform/pidwatch/linux.rs");
const WIN32: &str = include_str!("../../../src/backend/platform/pidwatch/win32.rs");
const WIN_PROC: &str = include_str!("../../../src/backend/platform/win_proc.rs");

fn prod(src: &str) -> String {
    guard_core::production_code(src)
}

/// 从 `head` 那一行（trim 后逐字相等、恰好一行）起，取到第一条 trim 后只剩 `}` 或 `},` 的行为止。
/// 只用在「臂体里没有嵌套块」的那几条臂上（本文件取的三条都是）。
fn arm_body(text: &str, head: &str) -> String {
    let at = guard_core::pin_line(text, head).unwrap_or_else(|e| {
        panic!("{e}\n⇒ 找不到 `{head}` 那条臂 —— `win32.rs` 的形状变了，本条的读法要跟着改")
    });
    text.lines()
        .skip(at + 1)
        .take_while(|l| !matches!(l.trim(), "}" | "},"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_windows_watch_has_the_same_three_deaths_and_the_same_non_death_as_linux() {
    let (linux, win32) = (prod(LINUX), prod(WIN32));
    // 反空真：两份都被掏空的话，下面「两边相等」会在 0 == 0 上恒真。
    for (who, text) in [("linux.rs", &linux), ("win32.rs", &win32)] {
        assert!(
            text.lines().count() >= 25,
            "`{who}` 生产段只剩 {} 行 —— 剥法坏了或文件被掏空",
            text.lines().count()
        );
    }

    // ① 判死次数两边相等，且就是契约里那三条。
    let deaths = |t: &str| t.matches("on_dead()").count();
    assert_eq!(
        deaths(&linux),
        3,
        "`linux.rs` 的判死不再是三条 —— 契约（`mod.rs` 头注）变了，先回去改契约"
    );
    assert_eq!(
        deaths(&win32),
        deaths(&linux),
        "`win32.rs` 判死 {} 次、`linux.rs` 判死 {} 次 —— 两个平台对「什么时候算死」的回答分叉了。\n\
         多一次多半是把「拒绝访问」或「等待出错」也判了死（那是把一个活进程判死 = 误归档）；\n\
         少一次多半是漏了「开到的是冒名者」那一条（PID 复用溜过去）。",
        deaths(&win32),
        deaths(&linux)
    );

    // ② 复核身份：同一个函数、同一行。
    let recheck = "if !crate::platform::proc::session_alive(pid, expected_start) {";
    for (who, text) in [("linux.rs", &linux), ("win32.rs", &win32)] {
        guard_core::pin_line(text, recheck).unwrap_or_else(|e| {
            panic!("{e}\n⇒ `{who}` 开完之后不再用 `session_alive` 复核身份（挡「读 pidfile → 开」之间的 PID 复用）")
        });
    }

    // ③ 次序：开 → 复核 → 起线程。两边各自量，再比同一个次序。
    let order = |t: &str, open: &str| -> Vec<usize> {
        [open, recheck, "std::thread::Builder::new()"]
            .iter()
            .map(|n| {
                t.find(n)
                    .unwrap_or_else(|| panic!("找不到 `{n}` —— 形状变了"))
            })
            .collect()
    };
    for (who, idx) in [
        ("linux.rs", order(&linux, "pidfd_open(pid)")),
        ("win32.rs", order(&win32, "win_proc::open_for_wait(pid)")),
    ] {
        assert!(
            idx.windows(2).all(|w| w[0] < w[1]),
            "`{who}` 里「开 → 复核身份 → 起看守线程」的次序变了（偏移 {idx:?}）。\n\
             先起线程再复核 ⇒ 冒名者也会被看守；先复核再开 ⇒ 复核与开之间又有一个 PID 复用窗口。"
        );
    }

    // ④ 不判死的臂里一次 `on_dead()` 都没有（Linux 那一格由 `linux_tests.rs` 的
    //    `poll_hard_error_must_not_report_dead` 在执行链上钉着；这里钉 Windows 那两条）。
    for head in ["Opened::Denied => {", "Err(err) => {"] {
        let body = arm_body(&win32, head);
        assert!(
            !body.trim().is_empty(),
            "`{head}` 那条臂取出来是空的 —— 读法跑飞了，下面的判断没有意义"
        );
        assert!(
            !body.contains("on_dead"),
            "`win32.rs` 的 `{head}` 那条臂里出现了 `on_dead` —— 那两条是**不判死**的：\n\
             拒绝访问 = 进程在、我们等不了它；等待出错 = 等这件事坏了，不是进程死了。\n\
             判死就是把一个活着的会话归档（`mod.rs` 头注：宁可留在 live，也不误归档）。\n臂体：\n{body}"
        );
    }
}

/// ⑤ **零定时器**：那个等待不带超时 —— 唯一的参数是「一直等」，而「一直等」就是那个全 1 的值。
///
/// ⚠ `no_timer_guard` 的禁用构件表里没有 Win32 的等待名字（它按 `sleep` / `interval` /
/// `*_timeout` 一族与 `Duration::from_*` 找）⇒ 有人把这里改成「等 2000 毫秒再看一眼」时
/// 那道护栏**零命中地绿**。本条补的就是这一格。
#[test]
fn the_windows_wait_never_times_out() {
    let p = prod(WIN_PROC);
    guard_core::pin_line(&p, "const WAIT_FOREVER: u32 = 0xFFFF_FFFF;").unwrap_or_else(|e| {
        panic!("{e}\n⇒ 「一直等」那个值变了 —— 不是全 1 就是一个超时，看守就成了节拍器")
    });
    guard_core::pin_line(
        &p,
        "let r = unsafe { WaitForSingleObject(h.as_raw_handle(), WAIT_FOREVER) };",
    )
    .unwrap_or_else(|e| {
        panic!("{e}\n⇒ 等待那一行不再只用 `WAIT_FOREVER` —— 带了超时，或者多了一处等待")
    });
    // 〔STOP〕第三处是一次性子命令 `--resident-stop` 那一个带期限的等（`wait_within`）—— 它不在看守路上，
    //   登记在 `no_timer_guard::REGISTERED_ONE_SHOT_CLI_WAITS`（那边两向相等地钉着「带期限的等只有登记的这几处」）。
    assert_eq!(
        p.matches("WaitForSingleObject(").count(),
        3,
        "`WaitForSingleObject(` 应当恰好三处：`extern` 里的声明 ＋ 上面那一次调用 ＋ 一次性子命令那一处（`wait_within`）"
    );
}

/// `death_events_available` 的 Windows 那一格说「有」，**背书那句「有」的就是本文件上面那条对拍**。
/// 这里钉那句声明本身：它紧挨着实现、值是 `true`、且只有一处（不许在别处再写一份「Windows 有」）。
#[test]
fn the_windows_claim_of_a_death_leg_sits_next_to_the_implementation() {
    let win32 = prod(WIN32);
    guard_core::pin_line(&win32, "pub(crate) const WAKES_ON_EXIT: bool = true;")
        .unwrap_or_else(|e| panic!("{e}\n⇒ `win32.rs` 不再声明「这一份会在进程退出时调 on_dead」"));
    // 声明为真，那份实现就必须真的会调 —— 与上一条的「三条判死」同一件事的两面。
    assert!(
        win32.contains("on_dead()"),
        "`win32.rs` 声明自己有死亡事件，而生产段里一次 `on_dead()` 都没有 —— 那是一句没有背书的「有」"
    );
}
