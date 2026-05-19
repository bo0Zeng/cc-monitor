//! 事件持久化重播：解决前端 F5 刷新后状态丢失的问题。
//!
//! 设计：所有 JSONL line emit 都先入 history，再按 ready 标志决定是否当场 emit。
//! frontend-ready 时把整个 history 重新 emit 一次 —— F5 后前端清零的状态完整恢复。
//! emit 与 replay 共用一把锁，保证顺序：live 永远在 replay 完成后才发出，不会乱序。

use crate::bridge::{events, JsonlLinePayload};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Runtime};

pub struct EventReplay {
    inner: Mutex<Inner>,
}

struct Inner {
    history: Vec<JsonlLinePayload>,
    ready: bool,
}

impl EventReplay {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                history: Vec::new(),
                ready: false,
            }),
        }
    }

    /// 收到一条新事件：写 history；前端 ready 后顺带 live emit。
    pub fn record<R: Runtime>(&self, handle: &AppHandle<R>, payload: JsonlLinePayload) {
        let mut inner = self.inner.lock();
        inner.history.push(payload.clone());
        if inner.ready {
            if let Err(e) = handle.emit(events::JSONL_LINE, &payload) {
                tracing::warn!("emit jsonl-line failed: {e}");
            }
        }
    }

    /// 前端 (重) ready：把整个 history 重新 emit；之后 record 直接 live emit。
    /// 锁住期间 record 会被阻塞 → 不会出现"replay 中途插入 live"的乱序。
    pub fn replay_and_mark_ready<R: Runtime>(&self, handle: &AppHandle<R>) {
        let mut inner = self.inner.lock();
        let n = inner.history.len();
        for p in inner.history.iter() {
            if let Err(e) = handle.emit(events::JSONL_LINE, p) {
                tracing::warn!("replay emit failed: {e}");
            }
        }
        inner.ready = true;
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
