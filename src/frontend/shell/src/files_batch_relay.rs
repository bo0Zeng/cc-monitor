//! **一组的进度的消费侧（monitor 这一半）**：本机 / 远端后端推来的 `files_batch` 帧（一组复制 · 删除 · 改权限 · 两台之间的复制
//! 此刻做到哪了），按票交给订了 `files-batch/<票>` 的那扇文件窗口（`chan/host.rs` 的订阅口）。
//!
//! - **按票，不按哪台**：票是窗口自己铸的（一组一张 UUID），同一张票只在一扇窗里订；两台之间的复制在本机后端里做，
//!   帧从本机那条流上来，同台那三条的帧从那台的流上来 —— 窗口不必知道是哪条流推的。
//! - **只留最新那一格**：每一格都是一整份快照（后端按变更合并），订的人慢了丢中间几格不丢信息；没人订的票直接丢。
//! - **零解释**：那一格是什么由窗口严格收（`{done, total, current?, bytes_done?, bytes_total?}` 的文本）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::watch;

type Table = HashMap<String, Vec<Arc<watch::Sender<Option<String>>>>>;

fn table() -> std::sync::MutexGuard<'static, Table> {
    static T: std::sync::OnceLock<Mutex<Table>> = std::sync::OnceLock::new();
    T.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// 订的那一格的 `kind` 前缀：`files-batch/<票>`（窗口那一侧 `filewin_contract::FILES_BATCH_KIND_PREFIX`）。
pub const KIND_PREFIX: &str = filewin_contract::FILES_BATCH_KIND_PREFIX;

/// 交一格（读循环调，从不阻塞）：有人订这张票 ⇒ 换掉它手上那一格；没人订 ⇒ 丢。
pub fn deliver(ticket: &str, cell: String) {
    if let Some(ears) = table().get(ticket) {
        for e in ears {
            e.send_replace(Some(cell.clone()));
        }
    }
}

/// 订一张票：每来一格交一格（只留最新的）。流被丢 ⇒ 不再订（只摘自己那一份）。
pub fn watch(ticket: &str) -> futures::stream::BoxStream<'static, String> {
    let (tx, rx) = watch::channel::<Option<String>>(None);
    let tx = Arc::new(tx);
    table()
        .entry(ticket.to_string())
        .or_default()
        .push(Arc::clone(&tx));
    let guard = Unwatch {
        ticket: ticket.to_string(),
        tx,
    };
    Box::pin(futures::stream::unfold(
        (rx, guard),
        |(mut rx, guard)| async move {
            loop {
                rx.changed().await.ok()?;
                let cell = rx.borrow_and_update().clone();
                if let Some(cell) = cell {
                    return Some((cell, (rx, guard)));
                }
            }
        },
    ))
}

struct Unwatch {
    ticket: String,
    tx: Arc<watch::Sender<Option<String>>>,
}

impl Drop for Unwatch {
    fn drop(&mut self) {
        let mut t = table();
        if let Some(v) = t.get_mut(&self.ticket) {
            v.retain(|e| !Arc::ptr_eq(e, &self.tx));
            if v.is_empty() {
                t.remove(&self.ticket);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/files_batch_relay_tests.rs"]
mod tests;
