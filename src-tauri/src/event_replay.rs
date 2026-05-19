//! 事件持久化重播：解决前端 F5 刷新后状态丢失的问题。
//!
//! 设计：所有 JSONL line emit 都先入 history，再按 ready 标志决定是否当场 emit。
//! frontend-ready 时把整个 history 重新 emit 一次 —— F5 后前端清零的状态完整恢复。
//! emit 与 replay 共用一把锁，保证顺序：live 永远在 replay 完成后才发出，不会乱序。
//!
//! 容量：history 有上限（HISTORY_CAP），溢出后 FIFO 丢最旧，避免长会话/大量工具
//! 调用把 monitor 内存撑爆。命中上限会 log warn。

use crate::bridge::{events, JsonlLinePayload};
use parking_lot::Mutex;
use std::collections::VecDeque;
use tauri::{AppHandle, Emitter, Runtime};

/// 单进程内 history 最多保留多少条 JSONL line。约 5000 条覆盖一般长会话；
/// 平均一条 1KB → 约 5MB 内存。
const HISTORY_CAP: usize = 5000;

pub struct EventReplay {
    inner: Mutex<Inner>,
}

struct Inner {
    history: VecDeque<JsonlLinePayload>,
    ready: bool,
    dropped: usize,
}

impl EventReplay {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                history: VecDeque::with_capacity(HISTORY_CAP + 16),
                ready: false,
                dropped: 0,
            }),
        }
    }

    /// 收到一条新事件：写 history（超 cap 丢最旧）；前端 ready 后顺带 live emit。
    pub fn record<R: Runtime>(&self, handle: &AppHandle<R>, payload: JsonlLinePayload) {
        let mut inner = self.inner.lock();
        inner.history.push_back(payload.clone());
        while inner.history.len() > HISTORY_CAP {
            inner.history.pop_front();
            inner.dropped += 1;
            if inner.dropped == 1 || inner.dropped.is_power_of_two() {
                tracing::warn!(
                    "event_replay history exceeded cap={HISTORY_CAP}, dropped {} oldest entries so far",
                    inner.dropped
                );
            }
        }
        if inner.ready {
            if let Err(e) = handle.emit(events::JSONL_LINE, &payload) {
                tracing::warn!("emit jsonl-line failed: {e}");
            }
        }
    }

    /// 前端 (重) ready：把整个 history 重新 emit；之后 record 直接 live emit。
    /// 持锁期间先 clone history snapshot 再释放锁，emit 走在锁外，避免 IPC
    /// 阻塞 watcher 的 record 入队。
    pub fn replay_and_mark_ready<R: Runtime>(&self, handle: &AppHandle<R>) {
        let (snapshot, n) = {
            let mut inner = self.inner.lock();
            let snap: Vec<JsonlLinePayload> = inner.history.iter().cloned().collect();
            inner.ready = true;
            let n = snap.len();
            (snap, n)
        };
        for p in &snapshot {
            if let Err(e) = handle.emit(events::JSONL_LINE, p) {
                tracing::warn!("replay emit failed: {e}");
            }
        }
        tracing::info!("replayed {n} buffered events to frontend");
    }

    /// 把指定 session_id 的全部历史从 buffer 移除。
    /// 用户主动关闭 archived Tab 时调用 —— 否则 F5 刷新 history 会重放出来"复活" Tab。
    pub fn forget(&self, session_id: &str) {
        let mut inner = self.inner.lock();
        let before = inner.history.len();
        inner.history.retain(|p| p.session_id != session_id);
        let removed = before - inner.history.len();
        if removed > 0 {
            tracing::info!("event_replay forget {session_id}: dropped {removed} entries");
        }
    }
}

impl Default for EventReplay {
    fn default() -> Self {
        Self::new()
    }
}
