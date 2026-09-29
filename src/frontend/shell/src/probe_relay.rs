//! 〔MIG-1 收尾 · 主会话裁「测试连接的进度不许倒退」〕**测试连接进度的消费侧（monitor 这一半）**：本机常驻后端发来的
//! `probe` 帧（`dial/probe.rs` 边拨边推的一格），原样交给会话流的句柄（`event_replay::EventReplay::on_probe`），
//! 界面经通道 `subscribe(<local>, "probe-progress/<票>")` 收（`src/frontend/ui/remote-probe.ts`）。
//!
//! 与 [`crate::session_tap`] 同形：读循环（`local_backend::absorb_local_frame`）手里没有 `AppHandle` ⇒ 出口由 `lib.rs` 的 setup
//! 装一次（[`install_sink`]）；装之前来的直接丢（界面那时也还没在订）。**零解释**：那一格是什么由界面严格收。

use std::sync::OnceLock;

type Sink = Box<dyn Fn(String, String) + Send + Sync>;

fn sink() -> &'static OnceLock<Sink> {
    static SINK: OnceLock<Sink> = OnceLock::new();
    &SINK
}

/// 装出口。**只装一次**（第二次装被忽略并记一句 —— setup 只跑一次，多装一定是接线错了）。
pub fn install_sink(f: impl Fn(String, String) + Send + Sync + 'static) {
    if sink().set(Box::new(f)).is_err() {
        tracing::warn!("probe_relay 的出口装了第二次 —— 忽略（setup 只该装一次）");
    }
}

/// 交一格（票 ＋ 原样那一格的 JSON 文本）。出口还没装 ⇒ 丢（界面还没在订）。
pub fn deliver(ticket: &str, cell: String) {
    if let Some(f) = sink().get() {
        f(ticket.to_string(), cell);
    }
}
