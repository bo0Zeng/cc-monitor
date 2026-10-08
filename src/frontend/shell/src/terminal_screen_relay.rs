//! **终端实时预览的消费侧（monitor 这一半）**：本机 / 远端后端推来的 `terminal_screen` · `terminal_follow_end` 帧
//! （`src/backend/control/terminal_follow.rs`），原样交给会话流的句柄（`event_replay::EventReplay::on_terminal_screen`），
//! 界面经通道 `subscribe(<那台>, "terminal-screen/<票>")` 收（`src/frontend/ui/terminal-follow.ts`）。
//!
//! 与 [`crate::probe_relay`] 同形，多一格「哪台」：读循环（本机 `local_backend::absorb_local_frame` · 远端 `stream_source::run`）手里没有
//! 那本订阅表 ⇒ 出口由 `lib.rs` 的 setup 装一次（[`install_sink`]）；装之前来的直接丢（界面那时也还没在订）。**零解释**：那一格是什么由界面严格收。

use std::sync::OnceLock;

type Sink = Box<dyn Fn(&crate::origin::Origin, &str, String) + Send + Sync>;

fn sink() -> &'static OnceLock<Sink> {
    static SINK: OnceLock<Sink> = OnceLock::new();
    &SINK
}

/// 装出口。**只装一次**（第二次装被忽略并记一句 —— setup 只跑一次，多装一定是接线错了）。
pub fn install_sink(f: impl Fn(&crate::origin::Origin, &str, String) + Send + Sync + 'static) {
    if sink().set(Box::new(f)).is_err() {
        tracing::warn!("terminal_screen_relay 的出口装了第二次 —— 忽略（setup 只该装一次）");
    }
}

/// 交一格（哪台 ＋ 票 ＋ 原样那一格的 JSON 文本）。出口还没装 ⇒ 丢（界面还没在订）。
pub fn deliver(origin: &crate::origin::Origin, ticket: &str, cell: String) {
    if let Some(f) = sink().get() {
        f(origin, ticket, cell);
    }
}
