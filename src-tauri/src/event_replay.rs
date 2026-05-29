//! 事件持久化重播：解决前端 F5 刷新后状态丢失的问题。
//!
//! ## 顺序保证（P5.4 B 重构后）
//!
//! **前端 RecordTimeline 按 seq 自动排序**——后端 emit 顺序不再影响视觉。
//! 之前为了"顺序保证"需要的 `replaying` flag + catch-up tail + 持锁 emit
//! 全部删除。chunked emit 期间 watcher push 进来的新行直接 emit jsonl-line，
//! 前端 timeline.insert 按 seq 自动放到正确位置（INVARIANT § 9 不再需要后端
//! 单独维护，由 seq 单调性保证）。
//!
//! 历史背景：v2.3.0 引入 chunked emit 后曾出现"replay 期间用户敲键 → 新行被
//! 吞到 F5 才出现"，靠加 replaying flag + catch-up 兜（P0.1 修），但代价是
//! 状态机更复杂。B 重构后 seq 一举消除这层。
//!
//! ## 容量
//!
//! 不设上限。jsonl 行的内存占用 ≈ 文件大小，对监控这种"内存 = 历史 + 实时
//! 增量"的场景可接受。极端情况（跑几个月几十万条），重启 monitor 即清。

use crate::bridge::{events, JsonlBatchPayload, JsonlLinePayload};
use parking_lot::Mutex;
use std::collections::VecDeque;
use tauri::{AppHandle, Emitter, Runtime};

pub struct EventReplay {
    inner: Mutex<Inner>,
}

struct Inner {
    history: VecDeque<JsonlLinePayload>,
    /// frontend 已收到 replay；可走 live emit。
    /// P5.4 B 重构：删了 `replaying` flag —— chunked emit 期间 watcher push 直接
    /// emit，前端 timeline 按 seq 自动放到正确位置。
    ready: bool,
}

/// 切块阈值（v2.3.1 issue #1 启动加速 + P5.4 B 重构简化）。
///
/// - history N < SINGLE_CHUNK_THRESHOLD → 单次 emit（无切块开销）
/// - N ≥ SINGLE_CHUNK_THRESHOLD → 按 CHUNK_SIZE 切块，**末块先发**（最新一段）
///
/// P5.4：不再区分 head / mid —— 前端 RecordTimeline 按 seq 自动排到正确位置，
/// 块内顺序对 DOM 无影响。chunks[0] = 最新一段，chunks[N-1] = 最老一段。
const SINGLE_CHUNK_THRESHOLD: usize = 200;
const CHUNK_SIZE: usize = 600;
/// chunk 之间停顿，让 IPC 派发线程喘息 + watcher 新行在缝隙间 emit。
const CHUNK_PAUSE_MS: u64 = 10;

/// v2.4.2 issue #2: incremental batch 切换到 chunked emit 的阈值。
///
/// watcher 一次 process_file 读到的行数 >= 此值时（典型场景：用户
/// `claude --resume <sid>` 灌历史），后端把这批走 jsonl-batch 切块 emit；
/// 否则（用户日常敲键 1-N 行）走 jsonl-line 单条 live emit 保持低延迟。
///
/// 经验值 50：日常增量绝对低于这个数（claude 流式回复一行一条 jsonl 也只有
/// 几条到十几条）；/resume 历史灌入轻松几百几千行。50 是清晰的分水岭。
const INCREMENTAL_BATCH_THRESHOLD: usize = 50;

impl EventReplay {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                history: VecDeque::new(),
                ready: false,
            }),
        }
    }

    /// watcher 一次 process_file 收集到的 batch 入口。
    ///
    /// **未 ready 时**（启动 replay 还没触发）：仅 push history buffer，等
    /// `frontend-ready` 时 `replay_and_mark_ready` 一并发。
    ///
    /// **已 ready 时**（debouncer 监听阶段 / chunked replay 进行中）：按 batch
    /// 大小分流：
    /// - `< INCREMENTAL_BATCH_THRESHOLD`：逐条 `emit(JSONL_LINE)`，保持实时低延迟
    /// - `>= INCREMENTAL_BATCH_THRESHOLD`：切块 `emit(JSONL_BATCH)`，触发前端
    ///   batch 模式（lazy hljs）。/resume 历史灌入场景。
    ///
    /// P5.4 B 重构：删了原 `replaying` flag 路径 —— 前端 RecordTimeline 按 seq
    /// 自动排序，chunked replay 期间 watcher 新行直接 emit jsonl-line 即可，
    /// 前端 timeline.insert 会放到正确位置（不再需要 catch-up tail 兜底）。
    pub fn on_line_batch<R: Runtime>(
        &self,
        handle: &AppHandle<R>,
        payloads: Vec<JsonlLinePayload>,
    ) {
        if payloads.is_empty() {
            return;
        }

        // 先持锁 push history + 看 ready 状态
        let (ready, big_batch) = {
            let mut inner = self.inner.lock();
            for p in &payloads {
                inner.history.push_back(p.clone());
            }
            (inner.ready, payloads.len() >= INCREMENTAL_BATCH_THRESHOLD)
        };

        if !ready {
            // 启动 replay 还没触发：仅 push history，frontend-ready 时统一发。
            return;
        }

        if !big_batch {
            // 小 batch：逐条 jsonl-line（保留实时低延迟语义）
            for p in payloads {
                if let Err(e) = handle.emit(events::JSONL_LINE, &p) {
                    tracing::warn!("emit jsonl-line failed: {e}");
                }
            }
            return;
        }

        // 大 batch：切块 jsonl-batch（前端按 seq 自动排，无 head/older 区分）
        let n = payloads.len();
        let started = std::time::Instant::now();
        let chunks = build_chunks(&payloads);
        let chunk_total = chunks.len() as u32;
        tracing::info!(
            "[perf] incremental batch chunked: total={n}, chunks={chunk_total} (likely /resume or large append)"
        );
        for (idx, chunk) in chunks.into_iter().enumerate() {
            let chunk_started = std::time::Instant::now();
            let payload = JsonlBatchPayload {
                chunk_index: idx as u32,
                chunk_total,
                payloads: chunk,
            };
            if let Err(e) = handle.emit(events::JSONL_BATCH, &payload) {
                tracing::warn!("emit incremental jsonl-batch chunk {idx} failed: {e}");
            }
            tracing::info!(
                "[perf] incremental chunk {idx}/{chunk_total} emit in {}ms",
                chunk_started.elapsed().as_millis()
            );
            if idx as u32 + 1 < chunk_total {
                std::thread::sleep(std::time::Duration::from_millis(CHUNK_PAUSE_MS));
            }
        }
        tracing::info!(
            "[perf] incremental batch chunked emit done in {}ms total",
            started.elapsed().as_millis()
        );
    }

    /// frontend-ready 时调一次：切块 emit 整个 history 后置 `ready = true`。
    ///
    /// **顺序保证**（P5.4 B 重构后）：前端 RecordTimeline 按 seq 自动排序，
    /// chunk 到达顺序 / 内部顺序对 DOM 视觉无影响。本函数只负责：
    /// 1. 切块（性能：避免单次 emit 几千条 IPC 序列化卡主线程）
    /// 2. **末块先发**：让用户立刻看到最新内容（DOM 自然 stickToBottom 到最新）
    /// 3. 块间小 pause：让 IPC 派发线程喘息，watcher 新行可以在缝隙间 emit
    ///    （直接走 on_line_batch live 路径，前端 timeline 自动排序，**无需 catch-up**）
    ///
    /// v1.7.13: 之前对每条 history 单独 `emit(JSONL_LINE, p)` —— N=3000 时
    /// Tauri IPC 每次 emit 都有序列化 + 派发 overhead，实测 ~400ms 阻塞主线程。
    /// v2.2: 改成单次 `emit(JSONL_BATCH, Vec<...>)`，序列化只跑一次。
    /// v2.3.1: 切块 emit，用户感知 ~22s → ~2s（仅渲染最新 100 条立刻可交互）。
    /// P5.4: 删了原 catch-up 路径，前端按 seq 排序使其不再必要。
    pub fn replay_and_mark_ready<R: Runtime>(&self, handle: &AppHandle<R>) {
        let started = std::time::Instant::now();

        // 阶段 1：拿 snapshot + 立即置 ready
        // P5.4 B 重构：no more replaying flag。chunked emit 期间 watcher 真新行
        // 直接走 on_line_batch live 路径（ready=true）→ emit jsonl-line → 前端
        // timeline 按 seq 自动排序到正确位置。不需要 catch-up tail。
        let snapshot: Vec<JsonlLinePayload> = {
            let mut inner = self.inner.lock();
            inner.ready = true;
            inner.history.iter().cloned().collect()
        };
        let n = snapshot.len();

        // N < 阈值 → 单次 emit
        if n < SINGLE_CHUNK_THRESHOLD {
            let payload = JsonlBatchPayload {
                chunk_index: 0,
                chunk_total: 1,
                payloads: snapshot,
            };
            if let Err(e) = handle.emit(events::JSONL_BATCH, &payload) {
                tracing::warn!("replay single-chunk emit failed: {e}");
            }
            tracing::info!(
                "[perf] replayed {n} events to frontend (single chunk) in {}ms",
                started.elapsed().as_millis()
            );
            return;
        }

        // N ≥ 阈值 → 切块，末块先发（最新一段先到 → 用户立刻可见）
        let chunks = build_chunks(&snapshot);
        let chunk_total = chunks.len();
        tracing::info!(
            "[perf] replay切块: total={n}, chunks={chunk_total} (CHUNK_SIZE={CHUNK_SIZE}, 末块先发)"
        );

        for (idx, chunk) in chunks.into_iter().enumerate() {
            let chunk_started = std::time::Instant::now();
            let payload = JsonlBatchPayload {
                chunk_index: idx as u32,
                chunk_total: chunk_total as u32,
                payloads: chunk,
            };
            if let Err(e) = handle.emit(events::JSONL_BATCH, &payload) {
                tracing::warn!("replay chunk {idx} emit failed: {e}");
            }
            tracing::info!(
                "[perf] chunk {idx}/{chunk_total} emit in {}ms",
                chunk_started.elapsed().as_millis()
            );
            if idx + 1 < chunk_total {
                std::thread::sleep(std::time::Duration::from_millis(CHUNK_PAUSE_MS));
            }
        }

        tracing::info!(
            "[perf] replayed {n} events to frontend (chunked × {chunk_total}) in {}ms total",
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

/// 切块策略（P5.4 B 重构简化）：按 CHUNK_SIZE 切块，**末块先发**——最新一段
/// 先到达前端 → DOM stickToBottom 让用户立刻看到最新内容。后续块（更老内容）
/// 前端按 seq 自动排到正确位置。
///
/// **不再区分 head / older / per_session** —— 前端 RecordTimeline 按 seq 排序，
/// 块内顺序对 DOM 无影响（只影响"用户多快看到这一段"）。chunks[0] = 最新一段，
/// chunks[N-1] = 最老一段，跟 v2.3 head-first 视觉效果一致但代码大幅简化。
fn build_chunks(snapshot: &[JsonlLinePayload]) -> Vec<Vec<JsonlLinePayload>> {
    let mut chunks: Vec<Vec<JsonlLinePayload>> = Vec::new();
    let total = snapshot.len();
    let mut end = total;
    while end > 0 {
        let start = end.saturating_sub(CHUNK_SIZE);
        chunks.push(snapshot[start..end].to_vec());
        end = start;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::JsonlRecord;

    /// 用 path 字段携带 idx 给测试用（Unknown 变体是 unit struct 不能塞 metadata）。
    /// P5.1：seq 也用 idx，方便排序断言。
    fn payload(sid: &str, idx: usize) -> JsonlLinePayload {
        JsonlLinePayload {
            session_id: sid.to_string(),
            cwd: None,
            path: format!("/fake/{sid}/{idx}.jsonl"),
            seq: idx as u64,
            message: JsonlRecord::Unknown,
        }
    }

    fn idx_of(p: &JsonlLinePayload) -> usize {
        let path = &p.path;
        let last = path.rsplit('/').next().unwrap();
        last.trim_end_matches(".jsonl").parse().unwrap()
    }

    #[test]
    fn build_chunks_small_history_single_chunk() {
        // 50 条 < CHUNK_SIZE → 全进单块
        let payloads: Vec<_> = (0..50).map(|i| payload("s1", i)).collect();
        let chunks = build_chunks(&payloads);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 50);
    }

    #[test]
    fn build_chunks_large_history_splits_by_chunk_size() {
        let payloads: Vec<_> = (0..3000).map(|i| payload("s1", i)).collect();
        let chunks = build_chunks(&payloads);
        // 3000 / 600 = 5 块
        assert_eq!(chunks.len(), 5);
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, 3000);
    }

    #[test]
    fn build_chunks_emits_newest_first() {
        // P5.4 关键：chunks[0] 应当是 input 末段（最新），chunks[N-1] 是 input 头段（最老）。
        // 末块先发 → 前端 timeline 按 seq 自动放，UI 上用户立刻看到最新内容。
        let payloads: Vec<_> = (0..1500).map(|i| payload("s1", i)).collect();
        let chunks = build_chunks(&payloads);
        // chunks[0] 应该含 idx 900..1500（最新 600 条）
        assert_eq!(chunks[0].len(), 600);
        assert_eq!(idx_of(&chunks[0][0]), 900);
        assert_eq!(idx_of(&chunks[0][599]), 1499);
        // chunks 末块应该含 idx 0..300（最老 300 条）
        let last_chunk = chunks.last().unwrap();
        assert_eq!(idx_of(&last_chunk[0]), 0);
    }

    #[test]
    fn build_chunks_preserves_input_order_within_chunks() {
        // chunk 内顺序 = 输入顺序的连续片段，前端按 seq 排即可还原全局序。
        let payloads: Vec<_> = (0..500).map(|i| payload("s1", i)).collect();
        let chunks = build_chunks(&payloads);
        // 反向拼接（块倒序 + 块内升序）= 完整时间序
        let mut reordered: Vec<&JsonlLinePayload> = Vec::new();
        for c in chunks.iter().rev() {
            for p in c {
                reordered.push(p);
            }
        }
        for (i, p) in reordered.iter().enumerate() {
            assert_eq!(
                idx_of(p),
                i,
                "block-reversed order should match input at {i}"
            );
        }
    }
}
