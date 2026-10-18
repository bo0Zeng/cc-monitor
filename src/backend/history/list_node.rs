//! **会话清单变了的节点**：这台的会话清单（`history-list` · `history-listing` 答的那一份）可能动了的那一刻。
//! 流的 tap 订它推 `changed {history_list}`（可丢、不带 `key`）；收到的一方带着手上那份的记号重问。
//!
//! - 发端各一处：会话宣告 · 结束 · 名字 / 状态变 · 记录长了（watcher 的发帧口，按会话）；注解 · 隐藏规则动了（watcher 重判隐藏那一处，不分会话）。
//! - **一会话一个节点**：同一个会话说过「变了」、这台还没答过清单 ⇒ 再变不再响（记录每长一行不是每行一帧）；
//!   答清单那一刻（[`ListNode::asked`]）全部放开 —— 放开在读之前，之后再变的一定再响一次。
//! - 通道只留最新（`watch`）：连着响几下、tap 还没取 ⇒ 每条连接只出一帧。没人订也照收，不是错。

use std::collections::HashSet;
use std::sync::Mutex;

/// 一份节点：已经响过、还没答过清单的那几个会话 ＋ 只留最新的那条通道。
pub(crate) struct ListNode {
    rung: Mutex<HashSet<String>>,
    tx: tokio::sync::watch::Sender<u64>,
}

impl Default for ListNode {
    fn default() -> Self {
        Self {
            rung: Mutex::new(HashSet::new()),
            tx: tokio::sync::watch::channel::<u64>(0).0,
        }
    }
}

impl ListNode {
    /// 这个会话那一行可能变了（`None` ＝ 不分会话：注解 · 隐藏规则，每次都响）。
    pub(crate) fn moved(&self, sid: Option<&str>) {
        if let Some(sid) = sid {
            let fresh = self
                .rung
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(sid.to_string());
            if !fresh {
                return;
            }
        }
        self.tx.send_modify(|n| *n = n.wrapping_add(1));
    }

    /// 这台要答一份清单了（读之前调）：响过的那几个会话放开，之后再变的再响。
    pub(crate) fn asked(&self) {
        self.rung
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    /// 订它：每响一下（连着几下只算一下）醒一次。
    pub(crate) fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.tx.subscribe()
    }
}

/// 本进程那一份。
pub(crate) fn node() -> &'static std::sync::Arc<ListNode> {
    static NODE: std::sync::OnceLock<std::sync::Arc<ListNode>> = std::sync::OnceLock::new();
    NODE.get_or_init(Default::default)
}

#[cfg(test)]
#[path = "../../../tests/backend/history/list_node_tests.rs"]
mod tests;
