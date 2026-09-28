/// ★ **前提触发器：`snapshot_tmux_by_origin` 刻意没有 IPC 出口。**
///
/// # 它的价值不在「拦住谁」，在**它红的时候会说什么**
///
/// 这是一条「今天没有」型断言 —— 没人加出口它就一直绿，天然容易被读成仪式。
/// 但真有人去开那个出口时（一份审计清单就会这么建议），诊断要立刻把
/// 「**快照不刷新 pane 前台命令**」摆到他眼前，逼他先回答「消费者是谁」。
/// 〔V154〕当年那个轮询方 `awaitExitFor` 已随「换号重启直接 kill」删了 ⇒ 今天出口更是没有消费者。
///
/// ⚠ 如实记：它**挡不住**「有人加了出口且顺手把这条判据也改了」——那时只剩评审。
#[test]
fn the_tmux_snapshot_stays_out_of_the_ipc_surface() {
    let src = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let needle = "fn snapshot_tmux_by_origin";
    let at = src
        .find(needle)
        .expect("找不到 `snapshot_tmux_by_origin` —— 它被改名或删了，本条会零命中地绿");
    // 往前看 200 字节：`#[tauri::command]` 属性若存在，必然紧贴在函数签名之前。
    let before = &src[at.saturating_sub(200)..at];
    assert!(
        !before.contains("tauri::command"),
        "`snapshot_tmux_by_origin` 被包成 Tauri 命令了。\n\
             \n\
             ⚠ **开这个出口之前先回答：消费者是谁？**\n\
             当年最常见的动机是「让 `awaitExitFor`（〔V154〕已删）改读快照，省掉每 1s 一条 SSH」——\n\
             **那条路走不通**：backend 的 `TmuxProbeDue` 只在初探发一次，之后每一拍靠 tmux hook，\n\
             而 hook 只有 session-created/closed/renamed 三条。`awaitExitFor` 等的是\n\
             「pane 前台命令从 claude 变回 shell」——**那个变化一条 hook 都不覆盖** ⇒\n\
             快照在那个场景下永不刷新 ⇒ 改读它 = 每次等到 10s 超时再降级 kill，**功能退化**。\n\
             \n\
             解锁条件：先有一个「pane 前台命令变化」的事件源。那条轮询的账已在\n\
             `polling_registry` 的 `src/tabs.ts` 一条里如实登记为未排期。\n\
             详见 `snapshot_tmux_by_origin` 的头注与 devbench F08。"
    );
}

// ── P28：给这条源码扫描型守卫立**负对照** ──
//
// 判的不是产品性质，是「**剥法没把我要扫的那一段剥掉**」。
// 失效形状是现打过的：便宜近似 `src.split("\n#[cfg(test)]").next()` 只在
// 「第一个测试模块之后再没有生产代码」时才对。`ssh_source.rs` 今天 4490 行，
// 第一个测试模块在 **909** 行 ⇒ 那个近似把扫描面砍到前 908 行，
// 而本文件要扫的东西全在它**后面**（逐针行号写在下面）。
// ⇒ 扫描面一旦静默缩水，本文件的判据会**零命中地绿**。
//
// 原语与它买不到什么：`guard_core::assert_stripper_keeps` 的头注。
// 一句话：它不买「针还是那个针」—— 下面这张表必须从本文件真正用的针里抄。

/// ★ 扫描面自检：共享剥法留住了本文件要扫的那几段，而便宜近似留不住。
#[test]
fn the_shared_stripper_keeps_the_function_this_guard_must_scan() {
    // 🔴 本文件是**「今天没有」型**断言（`!before.contains("tauri::command")`）——
    //    这一族对扫描面缩水**格外敏感**：语料少了那一段，零命中照样成立 ⇒ 恒绿。
    //    本文件自己只有一句 `.expect("找不到 … 本条会零命中地绿")` 挡着改名，
    //    **挡不住剥法把那一段整个剥掉之外的方向**（那一句其实也会红，但它说的是「改名了」，
    //    诊断会把人带错方向）。⇒ 逐针钉住：`fn snapshot_tmux_by_origin` 在 1456 行。
    guard_core::assert_stripper_keeps(
        "ssh_source_tmux_snapshot_exposure_tests",
        include_str!("../../src/bridge/src/ssh_source.rs"),
        &["fn snapshot_tmux_by_origin"],
    );
}
