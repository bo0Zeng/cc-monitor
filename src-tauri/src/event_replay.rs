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

use crate::bridge::{events, JsonlBatchPayload, JsonlLinePayload};
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use tauri::{AppHandle, Emitter, Runtime};

pub struct EventReplay {
    inner: Mutex<Inner>,
}

struct Inner {
    history: VecDeque<JsonlLinePayload>,
    ready: bool,
}

/// 切块阈值 + 大小（v2.3.1 issue #1 启动加速）。
///
/// 数据驱动：
/// - history N < SINGLE_CHUNK_THRESHOLD → 单次 emit（无切块开销 + 简单路径）
/// - N ≥ SINGLE_CHUNK_THRESHOLD → 切块 emit，最新优先：
///   - head 块：HEAD_CHUNK_SIZE 条最新（用户立刻可见）
///   - mid 块：MID_CHUNK_SIZE 条
///   - 末块：剩余全部
///
/// 测得 N=3920 单次 emit 后前端 drain ~22s。切块后用户感知 < 1s 见首块。
const SINGLE_CHUNK_THRESHOLD: usize = 200;
const HEAD_CHUNK_SIZE: usize = 100;
const MID_CHUNK_SIZE: usize = 600;
/// chunk 之间释放锁停顿（让 watcher 真新消息能 live emit 插入）。
/// 太短：watcher record() 抢锁来不及；太长：用户感知延迟。10ms 是 ergonomic balance。
const CHUNK_PAUSE_MS: u64 = 10;

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

    /// 前端 (重) ready：按 session 倒序切块 emit 整个 history，最后设 ready。
    ///
    /// **顺序保证策略**（重要）：
    /// - **同一 session 内**：jsonl 顺序保留（不打乱时间线）
    /// - **chunk 内**：可以混 session，每条按原 jsonl 顺序
    /// - **chunk 之间**：head chunk 含每 session 最新 N 条；后续 chunk 装更老内容；
    ///   末块装最老剩余。前端按 chunk 顺序处理：第一块 append 到 stream，后续块
    ///   prepend 到前一块之前。这样 DOM 时间顺序最终是 [老 ... 新]。
    ///
    /// **切块阈值**：N < SINGLE_CHUNK_THRESHOLD → 单次 emit（避免小数据切块开销 +
    /// 行为完全跟 v2.2 一致）；N ≥ 200 → 按 HEAD/MID/MID/... 切块。
    ///
    /// **持锁策略**：每块 emit 时持锁；块之间**释放锁停顿** {@link CHUNK_PAUSE_MS}
    /// 让 watcher 推到 mpsc channel 的真新 record 能进 `record()` 走 live emit
    /// jsonl-line（用户输入新消息优先 / 并行）。
    ///
    /// v1.7.13: 之前对每条 history 单独 `emit(JSONL_LINE, p)` —— N=3000 时
    /// Tauri IPC 每次 emit 都有序列化 + 派发 overhead，实测 ~400ms 阻塞主线程。
    /// v2.2: 改成单次 `emit(JSONL_BATCH, Vec<...>)`，序列化只跑一次。
    /// v2.3.1 (issue #1): 切块 emit + payload 加 chunk_index/chunk_total 元数据，
    /// 用户感知 ~22s → ~2s（仅渲染最新 100 条立刻可交互，老内容后台 prepend）。
    pub fn replay_and_mark_ready<R: Runtime>(&self, handle: &AppHandle<R>) {
        let started = std::time::Instant::now();

        // 阶段 1：持锁拿 snapshot 决定切块策略
        let snapshot: Vec<JsonlLinePayload> = {
            let inner = self.inner.lock();
            inner.history.iter().cloned().collect()
        };
        let n = snapshot.len();

        // N < 阈值 → 单次 emit，行为 100% 跟之前一致（仅 payload schema 改）
        if n < SINGLE_CHUNK_THRESHOLD {
            let payload = JsonlBatchPayload {
                chunk_index: 0,
                chunk_total: 1,
                payloads: snapshot,
            };
            {
                let mut inner = self.inner.lock();
                if let Err(e) = handle.emit(events::JSONL_BATCH, &payload) {
                    tracing::warn!("replay single-chunk emit failed: {e}");
                }
                inner.ready = true;
            }
            tracing::info!(
                "[perf] replayed {n} events to frontend (single chunk) in {}ms",
                started.elapsed().as_millis()
            );
            return;
        }

        // N ≥ 阈值 → 切块。先按 session 分组确定每 session 最新 N 条
        let chunks = build_chunks(&snapshot);
        let chunk_total = chunks.len();
        tracing::info!(
            "[perf] replay切块: total={n}, chunks={chunk_total} (head={} + 后续 {}×{}≈{})",
            chunks.first().map(|c| c.len()).unwrap_or(0),
            chunk_total.saturating_sub(1),
            MID_CHUNK_SIZE,
            n.saturating_sub(chunks.first().map(|c| c.len()).unwrap_or(0)),
        );

        // 阶段 2：逐块 emit。每块持短锁 emit，块间释放锁 stop 让 live record 通过
        for (idx, chunk) in chunks.into_iter().enumerate() {
            let chunk_started = std::time::Instant::now();
            let payload = JsonlBatchPayload {
                chunk_index: idx as u32,
                chunk_total: chunk_total as u32,
                payloads: chunk,
            };
            {
                let mut inner = self.inner.lock();
                if let Err(e) = handle.emit(events::JSONL_BATCH, &payload) {
                    tracing::warn!("replay chunk {idx} emit failed: {e}");
                }
                // 最后一块 emit 完成后才标 ready；之前块 emit 都持锁（防 record() 看到
                // ready=true 走 live emit 同时块尚未到齐前端）
                if idx + 1 == chunk_total {
                    inner.ready = true;
                }
            }
            tracing::info!(
                "[perf] chunk {idx}/{chunk_total} emit in {}ms",
                chunk_started.elapsed().as_millis()
            );

            // 最后一块不停顿
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

/// 切块策略：head 装"每 session 最新 N 条"（保证用户立刻看到所有 session 最新动态），
/// 剩余按 MID_CHUNK_SIZE 切块，**block order = 老 → 新**（块 index 0 = head 最新，块
/// index N-1 = 最老剩余）。
///
/// 前端处理：
/// - chunk 0 (head) → append 到 stream 底部，记 firstChunkAnchor
/// - chunk 1..N (older content) → prepend (insertBefore firstChunkAnchor)
///
/// 这样 DOM 时间顺序仍是 [老 ... 新]，stickToBottom 自然让最新可见。
fn build_chunks(snapshot: &[JsonlLinePayload]) -> Vec<Vec<JsonlLinePayload>> {
    // 1. 按 session_id 分组，保留每 session 内的相对顺序（jsonl 顺序 = 时间顺序）
    let mut by_session: HashMap<String, Vec<&JsonlLinePayload>> = HashMap::new();
    let mut session_order: Vec<String> = Vec::new();
    for p in snapshot {
        if !by_session.contains_key(&p.session_id) {
            session_order.push(p.session_id.clone());
        }
        by_session.entry(p.session_id.clone()).or_default().push(p);
    }

    // 2. head chunk：每 session 最新 N 条（按 session 内尾部切）
    let per_session_head = HEAD_CHUNK_SIZE / by_session.len().max(1) + 1;
    let mut head: Vec<JsonlLinePayload> = Vec::new();
    let mut older: Vec<JsonlLinePayload> = Vec::new();
    for sid in &session_order {
        let list = by_session.get(sid).expect("session list present");
        let n = list.len();
        let head_n = per_session_head.min(n);
        let split_at = n - head_n;
        // older 段（按 session 内顺序，老的在前）
        for p in &list[..split_at] {
            older.push((*p).clone());
        }
        // head 段（同样按 session 内顺序）
        for p in &list[split_at..] {
            head.push((*p).clone());
        }
    }

    // 3. older 切块（每块 MID_CHUNK_SIZE 条）。**倒序切**让"次新"块先 emit，
    //    最老块最后 emit。前端 prepend 时 anchor 在 head 顶部 → 后续 chunk 越来
    //    越往上插入，最终 DOM 顺序 [最老 ... head 最新]。
    let mut chunks: Vec<Vec<JsonlLinePayload>> = Vec::new();
    chunks.push(head);

    // older 已按时间升序排（每 session 内段拼接），现在按 MID_CHUNK_SIZE **从尾部**
    // 往前切，让"较新的 older"先到达前端 prepend 紧贴 head 之前。
    let total_older = older.len();
    let mut end = total_older;
    while end > 0 {
        let start = end.saturating_sub(MID_CHUNK_SIZE);
        chunks.push(older[start..end].to_vec());
        end = start;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::JsonlRecord;

    /// 用 path 字段携带 idx 给测试用（Unknown 变体是 unit struct 不能塞 metadata）
    fn payload(sid: &str, idx: usize) -> JsonlLinePayload {
        JsonlLinePayload {
            session_id: sid.to_string(),
            cwd: None,
            path: format!("/fake/{sid}/{idx}.jsonl"),
            message: JsonlRecord::Unknown,
        }
    }

    fn idx_of(p: &JsonlLinePayload) -> usize {
        let path = &p.path;
        let last = path.rsplit('/').next().unwrap();
        last.trim_end_matches(".jsonl").parse().unwrap()
    }

    #[test]
    fn build_chunks_single_session_under_threshold_yields_one_chunk() {
        // 注：build_chunks 不做阈值判断（caller 做）；这里测块切分本身
        let payloads: Vec<_> = (0..50).map(|i| payload("s1", i)).collect();
        let chunks = build_chunks(&payloads);
        // 50 条 < HEAD_CHUNK_SIZE+per_session_head → 全进 head
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 50);
    }

    #[test]
    fn build_chunks_large_history_splits_correctly() {
        let payloads: Vec<_> = (0..3000).map(|i| payload("s1", i)).collect();
        let chunks = build_chunks(&payloads);
        assert!(chunks.len() >= 2);
        // head 至少 HEAD_CHUNK_SIZE / 2 量级
        assert!(chunks[0].len() >= HEAD_CHUNK_SIZE / 2);
        // 总和等于输入
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, 3000);
    }

    #[test]
    fn build_chunks_preserves_per_session_order() {
        // 每个 session 内 jsonl 顺序必须保留
        let payloads: Vec<_> = (0..500).map(|i| payload("s1", i)).collect();
        let chunks = build_chunks(&payloads);
        // chunks[0] = head（最新一段），chunks[1..] 是 older（倒序：最新 older 块先 emit）
        // 重组成全时间序列 idx 0,1,2,...,499 验证顺序保留
        let mut reordered: Vec<&JsonlLinePayload> = Vec::new();
        // older 块按"最老块在最后"的发送顺序排，倒着看就是时间升序
        for c in chunks.iter().skip(1).rev() {
            for p in c {
                reordered.push(p);
            }
        }
        // 最后追加 head（最新段）
        for p in &chunks[0] {
            reordered.push(p);
        }
        for (i, p) in reordered.iter().enumerate() {
            assert_eq!(idx_of(p), i, "order mismatch at position {i}");
        }
    }

    #[test]
    fn build_chunks_multi_session_distributes_head_across_sessions() {
        let mut payloads = Vec::new();
        for i in 0..400 {
            payloads.push(payload("s1", i));
        }
        for i in 0..400 {
            payloads.push(payload("s2", i));
        }
        let chunks = build_chunks(&payloads);
        // head 块应该含两个 session 的最新
        let head_sids: std::collections::HashSet<_> =
            chunks[0].iter().map(|p| p.session_id.as_str()).collect();
        assert_eq!(head_sids.len(), 2);
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, 800);
    }
}
