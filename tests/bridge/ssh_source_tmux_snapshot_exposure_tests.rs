/// ★ **前提触发器：`snapshot_tmux_by_origin` 刻意没有 IPC 出口。**
///
/// # 它的价值不在「拦住谁」，在**它红的时候会说什么**
///
/// 这是一条「今天没有」型断言 —— 没人加出口它就一直绿，天然容易被读成仪式。
/// 但真有人去开那个出口时（一份审计清单就会这么建议），诊断要立刻把
/// 「**快照不刷新、`awaitExitFor` 用不了**」摆到他眼前，逼他先回答「消费者是谁」。
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
             最常见的动机是「让 `tabs.ts` 的 `awaitExitFor` 改读快照，省掉每 1s 一条 SSH」——\n\
             **那条路走不通**：daemon 的 `TmuxProbeDue` 只在初探发一次，之后每一拍靠 tmux hook，\n\
             而 hook 只有 session-created/closed/renamed 三条。`awaitExitFor` 等的是\n\
             「pane 前台命令从 claude 变回 shell」——**那个变化一条 hook 都不覆盖** ⇒\n\
             快照在那个场景下永不刷新 ⇒ 改读它 = 每次等到 10s 超时再降级 kill，**功能退化**。\n\
             \n\
             解锁条件：先有一个「pane 前台命令变化」的事件源。那条轮询的账已在\n\
             `polling_registry` 的 `src/tabs.ts` 一条里如实登记为未排期。\n\
             详见 `snapshot_tmux_by_origin` 的头注与 devbench F08。"
    );
}
