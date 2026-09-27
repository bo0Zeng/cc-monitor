//! 〔CF1 · 2026-09-24〕**本机会话内容的入口通道** —— 本机那条流上的内容帧从两条读循环送到
//! [`crate::ssh_source::consume_local`]（再进与远端同一个 `LineIntake`）。
//!
//! # 为什么要它
//!
//! 本机会话的内容以前由 monitor 自己的 jsonl watcher 读（`watcher.rs`，已删），
//! 同一批文件本机常驻后端也在读，它发来的 `line` 帧被本机读循环整个丢掉（`真相源/10 §7.1`）。
//! `设计/00 §2.5 ②`：本机改走后端的 Line 帧。⇒ 读循环把内容帧交到这里，一个进程级任务收它们。
//!
//! # 两种送法，因为两条读循环一条异步一条不是
//!
//! - 常驻载体（`local_backend_host.rs` 的读循环）是 tokio 任务 ⇒ [`deliver`]（`send().await`）。
//! - stdio 载体（`local_backend::local_stdio_consumer`）是裸 `std::thread` ⇒ [`deliver_blocking`]。
//!   ⚠ 反过来就错：在 tokio 任务里 `blocking_send` 会 panic，在裸线程里没有 runtime 可 `.await`。
//!
//! # 背压：级 1（`设计/05 §3.3.4`）
//!
//! 通道**有界**（[`LOCAL_LINES_CAPACITY`]）。消费者跟不上 ⇒ 送的那一方停在送上 ⇒ 读循环不再读 ⇒
//! 本机后端写阻塞、自己减速。**不丢、不无界堆。**
//! ⚠ 这要求消费者**绝不**等一个经本机通道回来的应答（那条应答得由正停在这里的读循环去读 ⇒ 互等）——
//! 快照那几问住在分发器自己的任务里，消费者只做攒批与发事件。
//!
//! # 装之前
//!
//! 通道由 [`install`] 造（`lib.rs` 起步段，在起本机后端**之前**调）。没装就送来的东西只有测试进程会碰到：
//! 大声 `warn` 并丢 —— 不为它造一条「先攒着」的无界队列。

use std::sync::{Arc, OnceLock};

use crate::event_replay::EventReplay;
use crate::ssh_source::{InboundFrame, LocalItem};

/// 通道容量（件数，一件 = 一帧）。
///
/// 量级：`ssh_source` 远端那条「读帧任务 → 主循环」的通道是 1024 行，同一种东西、同一个消费者形状 ⇒ 取同一个数。
/// 满了是**背压**（送的一方等），不是丢。
pub(crate) const LOCAL_LINES_CAPACITY: usize = 1024;

static SENDER: OnceLock<tokio::sync::mpsc::Sender<LocalItem>> = OnceLock::new();

/// 造通道、起消费者。**只调一次**（第二次调是用法错：大声说，不起第二个消费者）。
pub(crate) fn install(app: tauri::AppHandle, replay: Arc<EventReplay>) {
    let (tx, rx) = tokio::sync::mpsc::channel::<LocalItem>(LOCAL_LINES_CAPACITY);
    if SENDER.set(tx).is_err() {
        tracing::error!("local_lines::install 被调了第二次 —— 本机内容消费者只许有一个，这次不起");
        return;
    }
    tauri::async_runtime::spawn(crate::ssh_source::consume_local(rx, replay, app));
}

fn sender() -> Option<&'static tokio::sync::mpsc::Sender<LocalItem>> {
    let s = SENDER.get();
    if s.is_none() {
        tracing::warn!("本机内容消费者还没装上（local_lines::install 没调过）—— 这一件丢掉");
    }
    s
}

/// 常驻载体（tokio 任务）送一帧。
pub(crate) async fn deliver(frame: InboundFrame) {
    send(LocalItem::Frame(frame)).await
}

/// stdio 载体（裸线程）送一帧。
pub(crate) fn deliver_blocking(frame: InboundFrame) {
    send_blocking(LocalItem::Frame(frame))
}

/// 〔RENDER2 · `99 §2.1` ㉓①〕常驻载体上一行超长、整行丢了（下游原位给订阅一格 `Gap`）。
pub(crate) async fn line_lost() {
    send(LocalItem::LineLost).await
}

/// 〔RENDER2〕stdio 载体上一行超长、整行丢了。
pub(crate) fn line_lost_blocking() {
    send_blocking(LocalItem::LineLost)
}

/// 常驻载体的流结束了。
pub(crate) async fn stream_ended() {
    send(LocalItem::StreamEnded).await
}

/// stdio 载体的流结束了。
pub(crate) fn stream_ended_blocking() {
    send_blocking(LocalItem::StreamEnded)
}

async fn send(item: LocalItem) {
    let Some(tx) = sender() else { return };
    if tx.send(item).await.is_err() {
        tracing::warn!("本机内容消费者已经退出 —— 这一件丢掉");
    }
}

fn send_blocking(item: LocalItem) {
    let Some(tx) = sender() else { return };
    if tx.blocking_send(item).is_err() {
        tracing::warn!("本机内容消费者已经退出 —— 这一件丢掉");
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/local_lines_tests.rs"]
mod tests;
