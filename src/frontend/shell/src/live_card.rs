//! **活卡的消费侧（monitor 这一半）**：那台后端发来的 `live_card` 帧（核心拼好的活卡成品），原样交给会话流的句柄
//! （`event_replay::EventReplay::on_live_card`），前端经通道 `subscribe(origin, "session-live-card")` 按 credit 收。
//!
//! # 它只做一件事：转交，零解释
//!
//! - 读循环（`stream_source::route`）手里没有 `AppHandle` ⇒ 与 [`crate::session_facts`] 同形：
//!   出口由 `lib.rs` 的 setup 装一次（[`install_sink`]），交来的每一帧交给出口；装之前来的直接丢（前端那时也还没在听）。
//! - **不进任何缓冲**：不进 `local_lines`（内容通道）、不进重放缓冲 —— 收一帧，交给此刻订了 `session-live-card` 的订阅
//!   （有 credit 当场交，没 credit 丢、位置照占、原位 `Gap`：级 2）。⇒ 活卡那一路断 / 丢 / 挤都碰不到记录那条对的路。
//! - **不认识活卡**：帧体（`sid` · `run` · `card` · `n` · `op` 及其各格）原样交前端；对账 · 缺口 · 覆盖都在后端（`stream/live.rs`），
//!   前端只照画。
//! - 本机那条流与远端那条流都有它，origin 各是各的。

use cmd_wire::live::LiveCard;
use std::sync::OnceLock;

type Sink = Box<dyn Fn(crate::ui_contract::SessionLiveCardPayload) + Send + Sync>;

fn sink() -> &'static OnceLock<Sink> {
    static SINK: OnceLock<Sink> = OnceLock::new();
    &SINK
}

/// 装出口。**只装一次**（第二次装被忽略并记一句 —— setup 只跑一次，多装一定是接线错了）。
pub fn install_sink(
    f: impl Fn(crate::ui_contract::SessionLiveCardPayload) + Send + Sync + 'static,
) {
    if sink().set(Box::new(f)).is_err() {
        tracing::warn!("live_card 的出口装了第二次 —— 忽略（setup 只该装一次）");
    }
}

/// 一帧 → 发给前端的那一件（纯函数：帧体原样，`origin` 由调用方给）。
pub fn to_payload(origin: &str, l: LiveCard) -> crate::ui_contract::SessionLiveCardPayload {
    crate::ui_contract::SessionLiveCardPayload {
        origin: crate::origin::Origin(origin.to_string()),
        card: crate::ui_contract::RecordBody::from_json(
            serde_json::to_string(&l).unwrap_or_default(),
        )
        .unwrap_or_else(|| {
            crate::ui_contract::RecordBody::from_json("{}".to_string()).expect("{} is JSON")
        }),
    }
}

/// 读循环交来一帧：交给出口（没装 ⇒ 丢）。**从不阻塞**（出口是句柄的 `on_live_card`：只做 credit 记账 ＋ 投递）。
pub fn deliver(origin: &str, l: LiveCard) {
    if let Some(f) = sink().get() {
        f(to_payload(origin, l));
    }
}
