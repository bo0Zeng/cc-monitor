//! 〔U2 · 第三波〕带启动令牌的远端会话，`remote-bind-scan` 那条 15 × 600ms 的标题预扫线程**不起**。
//!
//! 两格：
//! 1. **判定本身**（纯函数 `wants_title_prescan`）：有令牌 ⇒ 不扫；没令牌 ⇒ 照旧扫。
//! 2. **接线**：`lib.rs` 里那条线程的起点前面，真的是拿**令牌账本**里这个 sid 的令牌去问的这一句。
//!    ⚠ 这一格是**文本**判据 —— 〔MIG-1〕那段代码住在 `lib.rs::session_side_effects`（会话成品出口线程调它），
//!    单测起不来 Tauri 应用、也造不出那条通道。它只买「门还在线程起点前、
//!    问的是令牌账本」，买不到「真机上少起了一条线程」。
//!
//! 买不到：带令牌且在 tmux 里的会话从此没有预扫出来的标题绑定 —— 令牌那扇窗关了之后第一次点 ↗
//! 要等现扫（`ON_DEMAND_BIND_*`，最多 4s）。理由写在 `wants_title_prescan` 的头注里；真机读数零。

use super::wants_title_prescan;

#[test]
fn a_session_with_a_launch_token_skips_the_title_prescan() {
    assert!(!wants_title_prescan(Some("0123456789abcdef")));
    assert!(
        wants_title_prescan(None),
        "没令牌的会话标题路是唯一的路 —— 必须照旧预扫"
    );
}

#[test]
fn the_prescan_thread_start_is_gated_by_the_token_book() {
    let src = include_str!("../../src/bridge/src/lib.rs");
    let prod = &src[..src
        .find("#[cfg(test)]\n#[path = \"../../../tests/bridge/lib_nudge_skip_tests.rs\"]")
        .expect("抽取器自检：找不到测试段的起点 —— 本条会在整份文件上空转")];
    // 线程名恰好一处（它就是那条 15 × 600ms 的预扫线程）。
    let spawn_at = guard_core::find_pinned(prod, "\"remote-bind-scan\"")
        .expect("`remote-bind-scan` 线程名应恰好一处");
    // 门：问的是令牌账本里**这个 sid** 的令牌，问完不想扫就跳过本轮。
    let ask_at = guard_core::find_pinned(
        prod,
        "let token = bind::remote_rbind_tokens().token_of(sid);",
    )
    .expect("向令牌账本问这个 sid 的那一句应恰好一处");
    let gate_at = guard_core::find_pinned(prod, "if !wants_title_prescan(token.as_deref()) {")
        .expect("门应恰好一处");
    assert!(
        ask_at < gate_at && gate_at < spawn_at,
        "顺序不对：应当先问令牌账本、再过门、最后才起预扫线程（问 {ask_at} · 门 {gate_at} · 起线程 {spawn_at}）"
    );
    // 门与线程起点之间只隔「跳过」与那几行准备（1200 字节是量出来的余量：现打约 300）。
    assert!(
        spawn_at - gate_at < 1200,
        "门离线程起点太远（{} 字节）—— 多半不是同一段了",
        spawn_at - gate_at
    );
    let between = &prod[gate_at..spawn_at];
    assert!(
        guard_core::contains_word(between, "return"),
        "门后面没有 `return` —— 不想扫也照样起线程"
    );
}
