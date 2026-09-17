//! U2（2026-08-01）：**pidfd 看守**这一族平台原语。
//!
//! # 这里为什么要切一刀
//!
//! 搬家前 `spawn_pid_watcher` 一个函数里同时装着两件事：
//! ① `pidfd_open` + 身份复核 + `poll(2)` + 起线程（**平台**）；
//! ② 醒了往哪个 channel 发哪一种 `WatchEvent`（**observe 的域知识**）。
//!
//! 主计划把回边判成了 `session_alive`（「`spawn_pid_watcher:228` 调它」），
//! **那条判断偏了一个函数** —— `session_alive` = `pid_alive` + `proc_starttime` +
//! `is_same_live_process`，三者分别是平台原语、平台原语、纯函数，整条都在 platform 域内。
//! 真正的回边是 `spawn_pid_watcher` 自己依赖 `PidWatchTarget` / `WatchEvent` 这两个
//! observe 域类型。**一个平台原语不该知道「醒了要往哪个 channel 发什么帧」。**
//!
//! ⇒ 切开而不是参数化谓词：本模块只提供 [`watch_pid_until_exit`]，
//! `watcher.rs` 留一层薄包装把 `on_dead` 实现成 `tx.send(target.death_event(pid))`。
//!
//! # 三条判死路径 + 一条**不**判死的路径
//!
//! 判死（调 `on_dead`）：① `pidfd_open` 失败 ② 身份复核不符（PID 复用）③ `poll` 醒。
//! **不判死**：`poll` 返回真错误（非 EINTR）—— 原实现逐字写着「真错误：**不**报死」，
//! 这条语义在切分时必须原样保住，切错了就是「看守线程挂了却把会话判成活的/死的」。
//! 这条**没有普通测试能覆盖**（要让 `poll(2)` 真出错），故由本文件末尾的源码扫描钉住。
//!
//! # 三条判据的原文（搬自 `watcher.rs`，一字未改）
//!
//! 三条判据，按顺序：
//! 1. `pidfd_open` 失败（`ESRCH` 等）⇒ 目标已不在 ⇒ 立刻发 `PidDied`。
//! 2. open 成功后**再读一次** `proc_starttime` 与 add 时捕获的基线比对
//!    （复用既有纯函数 `is_same_live_process`）：不符 = 在"读 pidfile"与
//!    "开 pidfd"之间发生了 PID 复用 ⇒ 我们开到的是冒名者 ⇒ 发 `PidDied`。
//!    **这就是原先那套 procStart 启发式的全部去处**——从"每 2s 复查一遍"
//!    降级为"开 pidfd 时校验一次"，之后靠内核，不再需要周期比对。
//! 3. 起线程 `poll(pidfd, POLLIN, -1)`；醒了发 `PidDied`。
//!
//! **线程数的界**：每个被追踪的 (pidfile, pid) 最多一条，实际是个位数
//! （一台机器上同时活着的 CC 交互会话数）。线程活到目标进程真正退出为止——
//! 若 pidfile 先被删而进程仍在，那条线程会继续等，等到进程退出时发一条
//! **陈旧唤醒**，被消费侧的 pid 比对挡掉（无副作用）。
//!
//! **`poll` 真出错（非 `EINTR`）时刻意不发 `PidDied`**：宁可让会话留在 live、
//! 等 pidfile 删除或断连来收，也不因一次系统调用失败就误归档——与本文件
//! `is_same_live_process` 头注那条「瞬时读失败绝不误归档」同一条纪律。
//!
//! > 这段说明 U2 之前**贴在 `enum PidWatchTarget` 头上**（隔着 enum + impl 才到它描述的
//! > `spawn_pid_watcher`）—— 是 U2 之前就有的错位，U2 把它从「贴错 item」升级成了「跨文件悬空」。
//! > Phase D 审计逮出，搬到它真正描述的代码旁边。

//! ---
//!
//! # U4a：按平台分文件
//!
//! `linux` 是原实现（逐字搬，`#![cfg(target_os = "linux")]`）；
//! `fallback` 是**诚实的空壳**，不是假实现 —— 见它自己的头注。
//!
//! （这两个名字**刻意不用 intra-doc 链接**：两个 mod 各自带 cfg，在任一 target 上只有一个存在，
//! 写成 `[\`fallback\`]` 会在 Linux 上产生一条悬空链接 —— Phase D 审计 重要-5 逮到的正是它。）
//!
//! 分文件而不是在函数里塞 `#[cfg]`：这一族的平台差异是**整套机制不同**
//! （pidfd+poll vs OpenProcess+WaitForSingleObject），不是某一行不同。
//! 塞在一个函数里会让两套实现的 `unsafe` 与所有权推理互相纠缠。

#[cfg(not(target_os = "linux"))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub(crate) use linux::watch_pid_until_exit;
// `pidfd_open` 只被 `watcher.rs` 的测试段用（生产段的唯一调用点在 `linux.rs` 内部）。
#[cfg(not(target_os = "linux"))]
pub(crate) use fallback::watch_pid_until_exit;
#[cfg(all(test, target_os = "linux"))]
pub(crate) use linux::pidfd_open;

// ══════════════════════════════════════════════════════════════════════════
// `K-P3` `KP3D`（09-04）：**「这个平台上有没有自愈」这件事要说得出口。**
//
// `K-P3` `§0-4` 代价 1：`fallback.rs` 那句逐字（「pidfd 看守在本平台未实现 —— 进程退出
// **不会**产生死亡事件」）说的是**一整条判活路径不在**。而自愈是靠**内核送的死亡事件**
// 醒的（`§0-4` 的结论）⇒ **非 Linux 上自愈结构性不成立**。
// 那不是漏洞，是这族原语已经写好的诚实降级；而「如实登记」的意思是**说出口** ——
// 不是写在一条源码注释里（`src-tauri/src/daemon_policy.rs` 头注对 `K14` 那半
// 用的正是同一句话：「如实登记的意思就是**说出口**」）。
//
// ⚠ **这里刻意不是一个三态。** 非 Linux 上这件事不是「不知道」，是**确证没有**。
// 把「确证没有」压进「不知道」正是 `K-P4` 下一拍拆开的那条病（`main.rs::TmuxPlatform`
// 头注逐字：「一个值装了两件事」）—— 而在这一格，「不知道」会被下游读成「也许有」。
//
// 🔴 **形状是刻意的：两条 `#[cfg]` 臂 + 一个裸布尔字面量，不是一行 `cfg!()`。**
//    非 Linux 那臂一旦被改成乐观值，`platform/fallback_guard.rs` 的
//    `fallback_branches_must_not_fabricate_success` 当场红 —— 它的判红条件之一逐字就是
//    「块体里出现裸 `true`」，而它的人群是「每一个非主分支的平台 cfg 属性紧跟着的那一个
//    item 或块」。写成 `const X: bool = cfg!(target_os = "linux");` 看着更干净，
//    但那一行**不带 `#[cfg]` 属性 ⇒ 整个掉出那道护栏的人群**，牙就没了。
//
// ⚠ **如实登记一条今天的读数**（`KP3D` 题面里那句「`fallback_guard` 不许返回乐观值」
//    对 `pidwatch/fallback.rs` 本身**并不成立**）：`fallback_guard::platform_sources` 收
//    `platform/` 递归全部 `.rs`，但它只在**每份文件自己的文本里**找平台 cfg 属性，
//    而 `fallback.rs` 内部**一个 `#[cfg]` 都没有**（它整份文件是被 mod.rs 这一行
//    `#[cfg(not(target_os = "linux"))] mod fallback;` 选进来的）⇒ 它对那道护栏贡献
//    **0 个受检块**，文件体一行都没被判过。那道护栏自己的头注早就逐字承认了这一格：
//    「**不构成阻塞**：惯用写法（`#[cfg(平台)] mod x;` + 独立文件）整文件绕过」。
//    ⇒ 真正在钉 `fallback.rs` 的是本文件末尾那条 `fallback_shape_tests`（`include_str!`
//    + 整行相等）。**本件把这一格钉在 `mod.rs`（人群够得着的地方），不是钉在 `fallback.rs`。**
// ══════════════════════════════════════════════════════════════════════════

/// Linux：**有**那条腿 —— `pidfd_open` + `poll(pidfd, POLLIN, -1)`，
/// 阻塞在内核事件上（`mod.rs` 头注：`poll` 的超时是 `-1`，不是节拍）⇒ 零定时器。
#[cfg(target_os = "linux")]
#[allow(dead_code)] // `K-P3` 第一档：能填不真填 —— 接线是一次发布决策，不是忘了。
pub(crate) const fn death_events_available() -> bool {
    true
}

/// 非 Linux：**没有**。`fallback::watch_pid_until_exit` 什么都不做，
/// `on_dead` 永远不会被调用（那是它头注里刻意选的保守方向）。
#[cfg(not(target_os = "linux"))]
#[allow(dead_code)] // 同上。
pub(crate) const fn death_events_available() -> bool {
    false
}

/// ★ **那句话本身** —— `KP3D` 买的是「说出口」这一半，所以它是一句话，不是一个 bool。
///
/// 只有那个布尔的话，下游完全可以读完它然后什么都不说 ——
/// 而「静默当成有」正是这一条要挡的东西（`§0-4` 代价 1 逐字：
/// 「非 Linux 上『有没有自愈』这件事要说出口，不许静默当成有」）。
#[allow(dead_code)] // 同上。
pub(crate) const NO_DEATH_EVENTS_HERE: &str =
    "本平台没有「进程死了会有人被叫醒」这条腿：pidfd 看守未实现（真形态是 \
     OpenProcess + WaitForSingleObject，属 U4b）⇒ 后端崩了不会有任何东西发现它，\
     也不会有任何一行账记下来。自愈在本平台上结构性不成立 —— 这是如实降级，不是漏洞。";

/// 这台机此刻该不该说那句话。`None` = 有那条腿，没什么要声明的。
///
/// ⚠ **`None` 在这里是「不必声明」，不是「不知道」** —— 那两件事在本模块里
/// 由 [`death_events_available`] 那个**二值**先分开了，本函数只负责挑话说。
#[allow(dead_code)] // 同上。
pub(crate) fn self_healing_caveat() -> Option<&'static str> {
    if death_events_available() {
        None
    } else {
        Some(NO_DEATH_EVENTS_HERE)
    }
}

/// ★ 一条**只在非 Linux 编译时存在**的编译期断言。
///
/// 🔴 **它在本仓门禁上给不出任何读数** —— 本机是 Linux，这个 item 在这儿编译期就不存在。
/// 它开口的时刻是任何一次非 Linux 编译（daemon「必须在 Windows 上编得过」那条纪律见
/// `plugin/discover.rs::is_executable` 头注）。形状照 `main.rs` 那条 `#[cfg(windows)] const _`
/// （`K-P4` 立的）原样写：Linux 那侧只有源码文本判据（读的是「那一支写在那儿」），
/// 而这一条读的是「**那一支真的被编进去了**」—— 两者证的不是同一件事。
#[cfg(not(target_os = "linux"))]
const _: () = assert!(
    !death_events_available(),
    "非 Linux 上这一格必须是「确证没有」——`fallback::watch_pid_until_exit` 什么都不做，\
     它永远不调 on_dead。这里回一个乐观值等于替一条不存在的腿担保，\
     而下游会拿它当「这台机上崩了会有人管」。"
);

/// 〔audit-0805 08-06〕**非 Linux 那份空壳的两条承诺，只能靠源码级守卫钉。**
///
/// `fallback.rs` 是 `#[cfg(not(target_os = "linux"))]` ⇒ **本机（Linux）根本不编译**，
/// 往里写一个不存在的标识符都不会报错（本区诚实边界 3y 记的就是这一族）。
/// ⇒ 行为判据在这里结构上不可能存在；能钉的只有**把源码当数据读**。
///
/// # 钉哪两条，为什么是这两条
///
/// 实测（08-06，两次变异各跑一遍 daemon 全套）：
/// - 把 `on_dead()` **立刻调掉**（`fallback.rs` 头注自己列为「最坏」的那个选项：
///   进程活得好好的、会话被判死）—— **daemon 279 全绿**；
/// - 把 `tracing::error!` **降级成 `warn!`**（E4：缺一整条判活路径不是可容忍的降级）
///   —— **daemon 279 全绿**。
///
/// 而「起个轮询线程」那条**已经有人管**（E6 的 `no_timer_guard`，实测当场红 2 条），
/// 所以本模块**不重复钉它**（E3：一个事实一个权威源）。
///
/// # 形态：整行相等，不是子串
///
/// 用 `pin_line` 而不是 `contains` —— 事实的单位是「那一行长这样」。
/// 子串匹配会让「在同一行后面追加一个调用」这种改动溜过去，
/// 而本仓治 F24 的递减棘轮也正盯着裸 `contains`。
#[cfg(test)]
mod fallback_shape_tests {
    /// ⚠ 这里**刻意不用 `#[cfg(not(linux))]`**：那样本守卫在本机就永远不跑，
    /// 而它存在的全部理由就是「本机跑不到那份代码」。`include_str!` 与平台无关。
    const FALLBACK: &str = include_str!("fallback.rs");

    #[test]
    fn the_non_linux_stub_stays_an_honest_stub() {
        let prod = guard_core::production_code(FALLBACK);
        // ★ 抽取器自检：文件被清空/改名时，下面两条会零命中地绿。
        assert!(
            prod.lines().count() >= 5,
            "`fallback.rs` 的生产段只剩 {} 行 —— 读法坏了或文件被掏空",
            prod.lines().count()
        );

        // ① `on_dead` 只许被**丢弃**，不许被调用。
        //    这一行变了，就说明有人改了「永远不调 on_dead」这个刻意的保守方向。
        guard_core::pin_line(&prod, "let _ = (expected_start, on_dead);").unwrap_or_else(|e| {
            panic!(
                "{e}\n\
                 ⇒ `fallback.rs` 里那句「`on_dead` 只丢弃、不调用」不见了。\n\
                 它的头注把「立刻调 `on_dead`」逐字列为**最坏**的选项：\n\
                 进程活得好好的，会话却被判死（误归档）。\n\
                 真要改成会调用，请先把 U4b（OpenProcess + WaitForSingleObject）做出来。"
            )
        });

        // ② 级别必须是 `error!`。E4：这不是可容忍的降级，是缺了一整条判活路径。
        guard_core::pin_line(&prod, "tracing::error!(").unwrap_or_else(|e| {
            panic!(
                "{e}\n\
                 ⇒ 那条日志不再是 `error!` 了。头注逐字写着为什么不是 `warn!`：\n\
                 「这不是『可容忍的降级』，是**缺了一整条判活路径**，级别要与事实相称」。"
            )
        });
    }
}

/// 〔`K-P3` `KP3D`〕**「这个平台上有没有自愈」这件事说得出口。**
///
/// # 三条各证一半，缺一条都不成立
///
/// - 本机（Linux）那一格：有腿 ⇒ 没有要声明的话 —— 这是**负例**，
///   少了它，把 `self_healing_caveat` 写成 `Some(…)` 恒真也照样绿。
/// - 非 Linux 那一支：本机**编不到它** ⇒ 只能把源码当数据读
///   （形状照 `main.rs::the_windows_arm_is_wired_into_the_source`，`K-P4` 立的）。
/// - 那句话本身：必须真的说「没有」，而不是一句读不出结论的散文。
///
/// ⚠ **诚实边界，照 `K-P4` 那条原样写**：本机没有 Windows ⇒ 这几条证的是
/// 「**那一支写在源码里**」，**证不了** Windows 上编出来的二进制真走了那一支。
/// 后者的落点是同文件那条 `#[cfg(not(target_os = "linux"))] const _`，
/// 它**在本仓门禁上给不出任何读数**，开口的时刻是任何一次非 Linux 编译。
#[cfg(test)]
mod death_event_leg_tests {
    use super::{death_events_available, self_healing_caveat, NO_DEATH_EVENTS_HERE};

    /// ★ 本机这一格（**负例**）：Linux 上有那条腿 ⇒ 没有要声明的话。
    #[cfg(target_os = "linux")]
    #[test]
    fn on_linux_the_leg_is_there_so_there_is_nothing_to_declare() {
        assert!(
            death_events_available(),
            "Linux 上这一格回了「没有」—— 而 `pidwatch::linux` 的 `pidfd_open` + \
             `poll(pidfd, POLLIN, -1)` 就在旁边、今天在门禁里绿着。\n\
             回「没有」会让上层在**唯一一个真有这条腿的平台**上也说自愈不成立。"
        );
        assert_eq!(
            self_healing_caveat(),
            None,
            "Linux 上多说了一句声明 —— 那句话是给**没有**那条腿的平台准备的。\n\
             这一格是本组的负例：它恒 `Some(..)` 的话，上面那条源码判据照样绿。"
        );
    }

    /// ★★ 非 Linux 那一支**写在源码里**，且它给的是「确证没有」。
    #[test]
    fn the_non_linux_arm_is_wired_into_the_source() {
        let prod = guard_core::production_code(include_str!("mod.rs"));
        const SIG: &str = "pub(crate) const fn death_events_available() -> bool {";
        let lines: Vec<&str> = prod.lines().map(str::trim).collect();
        // 反空真①：剥法跑飞 / 文件被掏空时，下面几条会在一份空文本上恒真。
        assert!(
            lines.len() >= 20,
            "生产段只剩 {} 行 —— 剥法坏了或文件被掏空，本条此刻是空转的",
            lines.len()
        );
        // 反空真②：**恰好两条臂**。多一条 = 多了一个平台答案（第二份真相）；
        //           少一条 = 有人把平台这一维塞回了一行 `cfg!()`，护栏的人群就够不着它了。
        assert_eq!(
            lines.iter().filter(|l| **l == SIG).count(),
            2,
            "`death_events_available` 的臂数不是 2 —— 平台这一维只许在这两条 `#[cfg]` 臂上成值。\n\
             写成 `const X: bool = cfg!(target_os = \"linux\");` 那种一行式**看着更干净**，\n\
             但那一行不带 `#[cfg]` 属性 ⇒ 整个掉出 `platform/fallback_guard.rs` 的人群，牙就没了。"
        );
        // 每条臂：往上找最近的那个 cfg 属性行，往下取**第一条非空行**当它的值。
        //
        // ⚠ 刻意不去配对花括号：这两条臂的体各只有一行（`true` / `false`），
        //   而「取到收尾大括号为止」要在测试里写一个裸的右大括号字面量 ——
        //   而本文件正被 `platform/fallback_guard.rs` 与本条自己当**数据**读，
        //   往里塞结构字符是给两条剥法各添一个陷阱。
        let mut arms: Vec<(&str, &str)> = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            if *l != SIG {
                continue;
            }
            let cfg = lines[..i]
                .iter()
                .rev()
                .find(|p| p.starts_with("#[cfg("))
                .copied()
                .unwrap_or("<这条臂上没有 cfg 属性>");
            let body = lines[i + 1..]
                .iter()
                .find(|p| !p.is_empty())
                .copied()
                .unwrap_or("");
            arms.push((cfg, body));
        }
        // 反空真③：抽取器真的取到了两条，且体不是空的（取行跑飞会给空串）。
        assert_eq!(arms.len(), 2, "抽出来 {} 条臂 —— 抽取器跑飞了", arms.len());
        for (cfg, body) in &arms {
            assert!(
                !body.trim().is_empty(),
                "`{cfg}` 那条臂抽出来是空的 —— 取行跑飞了，下面的判断没有意义"
            );
        }
        let arm_of = |cfg: &str| -> String {
            arms.iter()
                .find(|(c, _)| *c == cfg)
                .map(|(_, b)| b.trim().to_string())
                .unwrap_or_else(|| {
                    panic!("找不到 `{cfg}` 那条臂 —— 两条臂的 cfg 被换过了：{arms:?}")
                })
        };
        assert_eq!(
            arm_of("#[cfg(not(target_os = \"linux\"))]"),
            "false",
            "非 Linux 那条臂不再是「确证没有」。\n\
             `fallback::watch_pid_until_exit` 什么都不做、`on_dead` 永远不会被调用 ——\n\
             这里给一个乐观值就是替一条不存在的腿担保，而上层会拿它当「崩了会有人管」。\n\
             ⚠ 也不许改成「不知道」：那正是 `K-P4` 拆开的那条病（一个值装了两件事），\n\
             而「不知道」在这一格会被下游读成「也许有」。"
        );
        assert_eq!(
            arm_of("#[cfg(target_os = \"linux\")]"),
            "true",
            "Linux 那条臂不再说「有」—— 而那条腿（pidfd + poll）今天就在旁边绿着"
        );
        // ★ 编译期那一半也要真的写在那儿：本机编不到它，只能读源码文本。
        //   （`#[cfg(not(target_os = "linux"))]` 在本文件里出现四处 —— `mod fallback;` ·
        //    那条 `use` · 上面那条臂 · 本断言 ⇒ 不用 `pin_line`，它要求整份文件里恰好一行。）
        guard_core::pin_line(&prod, "const _: () = assert!(").unwrap_or_else(|why| {
            panic!(
                "{why}\n\
                 ⇒ 那条**只在非 Linux 编译时开口**的编译期断言不见了。\n\
                 源码文本判据只证「那一支写在那儿」，证不了「那一支真的被编进去了」——\n\
                 两者证的不是同一件事，所以两条都要在\n\
                 （形状照 `main.rs` 那条 `#[cfg(windows)] const _`，`K-P4` 立的）。"
            )
        });
    }

    /// ★ 那句话必须真的**说出「没有」**，不是一句读不出结论的散文。
    #[test]
    fn the_caveat_says_out_loud_that_there_is_no_self_healing() {
        // 承重词**运行时拼** —— 写成整串会让本条命中本文件自己的散文。
        let no_leg = format!("没有{}", "「进程死了会有人被叫醒」这条腿");
        let structural = format!("结构性{}", "不成立");
        for needle in [no_leg.as_str(), structural.as_str()] {
            assert!(
                NO_DEATH_EVENTS_HERE.contains(needle),
                "那句话里没有 `{needle}` —— `KP3D` 买的是「说出口」，\
                 一句读不出「没有」的散文等于没说"
            );
        }
        // 反向：不许在这句话里承诺自愈。
        for forbidden in ["会自动重起", "会自己再起来"] {
            assert!(
                !NO_DEATH_EVENTS_HERE.contains(forbidden),
                "那句话里出现了 `{forbidden}` —— 它是给**没有**那条腿的平台说的，\
                 在那儿承诺自愈是一句做不到的话"
            );
        }
        // 反空真：那句话不许被掏空成一个占位串。
        assert!(
            NO_DEATH_EVENTS_HERE.chars().count() >= 40,
            "那句话只有 {} 个字 —— 掏空到这个长度，上面几条就靠一个占位串恒绿",
            NO_DEATH_EVENTS_HERE.chars().count()
        );
    }
}
