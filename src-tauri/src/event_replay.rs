//! 事件持久化重播：解决前端 F5 刷新后状态丢失的问题。
//!
//! ## 顺序保证（关键设计）
//!
//! 早期实现用"锁外 emit snapshot + 锁内 push 后 ready 判断 live emit"模式，
//! 但 replay 释放锁后 emit snapshot 期间，watcher 的 record 已能并发拿锁、
//! 看到 ready=true 走 live emit → 前端先收到新 record 的 live emit、再收到
//! snapshot 的旧 emit，**顺序错乱、时间线断裂**。
//!
//! 当前实现：replay **持锁完整 emit snapshot 后再设 ready**。期间 record() 拿
//! 不到锁就排队等（watcher rx 是 unbounded mpsc，channel 排队不丢）。代价是
//! replay 期间 watcher 阻塞数十毫秒到秒级（取决于 history 大小），换来严格
//! 按 history 顺序到达前端的保证。
//!
//! ## 容量
//!
//! 不设上限。jsonl 行的内存占用 ≈ 文件大小，对监控这种"内存 = 历史 + 实时
//! 增量"的场景可接受。极端情况（跑几个月几十万条），重启 monitor 即清。

use crate::bridge::{events, JsonlLinePayload};
use parking_lot::Mutex;
use std::collections::VecDeque;
use tauri::{AppHandle, Emitter, Runtime};

pub struct EventReplay {
    inner: Mutex<Inner>,
}

struct Inner {
    history: VecDeque<JsonlLinePayload>,
    ready: bool,
}

impl EventReplay {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                history: VecDeque::new(),
                ready: false,
            }),
        }
    }

    /// 收到一条新事件：写 history；前端 ready 后顺带 live emit。
    pub fn record<R: Runtime>(&self, handle: &AppHandle<R>, payload: JsonlLinePayload) {
        let mut inner = self.inner.lock();
        inner.history.push_back(payload.clone());
        if inner.ready {
            if let Err(e) = handle.emit(events::JSONL_LINE, &payload) {
                tracing::warn!("emit jsonl-line failed: {e}");
            }
        }
    }

    /// 前端 (重) ready：完整 emit 整个 history 再设 ready。
    ///
    /// **持锁** 期间 emit，保证 record 排队等待 —— 前端不会先收到 live emit
    /// 再收到 replay snapshot 而错乱。
    pub fn replay_and_mark_ready<R: Runtime>(&self, handle: &AppHandle<R>) {
        let started = std::time::Instant::now();
        let mut inner = self.inner.lock();
        let n = inner.history.len();
        for p in inner.history.iter() {
            if let Err(e) = handle.emit(events::JSONL_LINE, p) {
                tracing::warn!("replay emit failed: {e}");
            }
        }
        inner.ready = true;
        drop(inner);
        tracing::info!(
            "replayed {n} events to frontend (order-strict) in {}ms",
            started.elapsed().as_millis()
        );
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
