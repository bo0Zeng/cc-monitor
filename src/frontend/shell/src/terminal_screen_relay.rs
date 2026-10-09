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

/// 替界面向那台退订的那条帧命令（后端 `control/terminal_follow.rs::UNFOLLOW`）。
const UNFOLLOW: &str = "terminal-unfollow";

/// 退订那一问的期限（就地做完的记账；远端多一趟往返）。
const UNFOLLOW_BUDGET: std::time::Duration = std::time::Duration::from_secs(15);

/// 〔「退订挂在订阅上」〕界面那条 `terminal-screen/<票>` 撤掉了（撤单 · 被重订 · 页面重载 · 窗口没了，`event_replay::on_screen_dropped`
/// 经 `lib.rs` 装的那一口调）⇒ 向那台发一次退订，不等结局：那台没连着 ⇒ 连接走时后端整张票表本来就收了；
/// 那张票早已停了 / 退订先于订阅到了 ⇒ 后端都回 `ok`（退订先到的那张票，之后的订阅那一问不起）。
pub fn unfollow(origin: &crate::origin::Origin, ticket: &str) {
    let Some(client) = crate::inbound_client::client_for(origin.as_wire_str()) else {
        return;
    };
    if !client.accepts(UNFOLLOW) {
        return;
    }
    let (origin, ticket) = (origin.as_wire_str().to_string(), ticket.to_string());
    tauri::async_runtime::spawn(async move {
        let args = serde_json::json!({ "ticket": ticket });
        if let Err(e) = client.call(UNFOLLOW, args, UNFOLLOW_BUDGET).await {
            tracing::debug!("替界面退订 [{origin}] 的终端画面票 {ticket} 没成（{e}）—— 连接走时那台整张票表会收");
        }
    });
}
