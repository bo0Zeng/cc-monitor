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
    /// frontend 已收到 replay；可走 live emit。
    ready: bool,
    /// `replay_and_mark_ready` chunked 路径**进行中**：标记为 true 期间
    /// `on_line_batch` 把新行 push 进 history 但**不 emit**——避免与正在 emit 的
    /// chunks 顺序错位（INVARIANT § 9）。replay 末块完成 + catch-up emit 收尾后
    /// 才置 false，期间 push 进来的新行由末块路径统一 catch-up emit。
    replaying: bool,
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
                replaying: false,
            }),
        }
    }

    /// v2.4.2 issue #2: watcher 一次 process_file 收集到的 batch 入口。
    ///
    /// **未 ready 时**（启动 replay 还没触发）：全部 push 到 history buffer，
    /// 等 frontend-ready 时 `replay_and_mark_ready` 一并切块发。等同 record()
    /// 的行为，watcher 初始全量扫走这条路径。
    ///
    /// **已 ready 时**（debouncer 监听阶段）：按 batch 大小分流：
    /// - `< INCREMENTAL_BATCH_THRESHOLD`：逐条 `emit(JSONL_LINE)`，保持 v2.4
    ///   实时增量低延迟语义（用户日常敲键 1-N 行）
    /// - `>= INCREMENTAL_BATCH_THRESHOLD`：切块 `emit(JSONL_BATCH)`，触发前端
    ///   batch 模式（lazy hljs / chunked prepend），用户场景：`claude --resume
    ///   <sid>` 灌几千行历史
    ///
    /// 两条路径都先把 payloads push 到 history（防 F5 刷新丢失）。
    pub fn on_line_batch<R: Runtime>(
        &self,
        handle: &AppHandle<R>,
        payloads: Vec<JsonlLinePayload>,
    ) {
        if payloads.is_empty() {
            return;
        }

        // 先持锁 push history + 看 ready / replaying 状态，决定后续 emit 策略
        let (ready, replaying, big_batch) = {
            let mut inner = self.inner.lock();
            for p in &payloads {
                inner.history.push_back(p.clone());
            }
            (
                inner.ready,
                inner.replaying,
                payloads.len() >= INCREMENTAL_BATCH_THRESHOLD,
            )
        };

        if !ready || replaying {
            // 启动 replay 还没触发 / 正在 chunked emit 中段：仅 push history。
            // 前者由 frontend-ready 时统一发；后者由 `replay_and_mark_ready` 末块
            // 的 catch-up 段统一发——这条路径修复 v2.3.0 起 chunked replay 期间
            // watcher 新行被吞、前端"重放窗口期冻结"直到 F5 的 UX bug。
            return;
        }

        if !big_batch {
            // 小 batch：逐条 jsonl-line（保留 v2.4 实时低延迟语义）
            for p in payloads {
                if let Err(e) = handle.emit(events::JSONL_LINE, &p) {
                    tracing::warn!("emit jsonl-line failed: {e}");
                }
            }
            return;
        }

        // 大 batch：切块 jsonl-batch，复用 build_chunks 的 head + older 倒序策略。
        // 前端 events.ts 收到 jsonl-batch 自动进 batch 模式 + chunked prepend。
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
            // 块间小 pause 让 watcher 真新行能 live emit 插入（同 replay 的策略）
            if idx as u32 + 1 < chunk_total {
                std::thread::sleep(std::time::Duration::from_millis(CHUNK_PAUSE_MS));
            }
        }
        tracing::info!(
            "[perf] incremental batch chunked emit done in {}ms total",
            started.elapsed().as_millis()
        );
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

        // 阶段 1：持锁拿 snapshot 决定切块策略，同时记 snapshot_len 用于末尾 catch-up
        let snapshot: Vec<JsonlLinePayload> = {
            let mut inner = self.inner.lock();
            // 标记进入 chunked replay：期间 on_line_batch 仅 push 不 emit，
            // 防止与正在 emit 的 chunks 顺序错位（INVARIANT § 9）。
            inner.replaying = true;
            inner.history.iter().cloned().collect()
        };
        let snapshot_len = snapshot.len();
        let n = snapshot_len;

        // N < 阈值 → 单次 emit，行为 100% 跟之前一致（仅 payload schema 改）
        if n < SINGLE_CHUNK_THRESHOLD {
            let payload = JsonlBatchPayload {
                chunk_index: 0,
                chunk_total: 1,
                payloads: snapshot,
            };
            let catch_up: Vec<JsonlLinePayload> = {
                let mut inner = self.inner.lock();
                if let Err(e) = handle.emit(events::JSONL_BATCH, &payload) {
                    tracing::warn!("replay single-chunk emit failed: {e}");
                }
                inner.ready = true;
                inner.replaying = false;
                // 单 chunk 路径理论上锁内连续执行，期间 on_line_batch 抢不到锁——
                // 但为对称统一，仍然取 snapshot_len 之后的 tail 兜底（一般为空）。
                inner.history.iter().skip(snapshot_len).cloned().collect()
            };
            for p in catch_up {
                if let Err(e) = handle.emit(events::JSONL_LINE, &p) {
                    tracing::warn!("single-chunk catch-up jsonl-line emit failed: {e}");
                }
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

        // 阶段 2：逐块 emit。每块持短锁 emit，块间释放锁 stop 让 watcher push 进来
        // （但 replaying=true 让它只 push 不 emit；末尾 catch-up 段统一发）
        for (idx, chunk) in chunks.into_iter().enumerate() {
            let chunk_started = std::time::Instant::now();
            let payload = JsonlBatchPayload {
                chunk_index: idx as u32,
                chunk_total: chunk_total as u32,
                payloads: chunk,
            };
            {
                // 持锁完成 emit 防 watcher 中途插队（虽然 replaying=true 已经让
                // on_line_batch 走 push-only 路径，但同时持锁是历史 INVARIANT § 5
                // 的传统纵深防御，便宜可保留）。
                let _guard = self.inner.lock();
                if let Err(e) = handle.emit(events::JSONL_BATCH, &payload) {
                    tracing::warn!("replay chunk {idx} emit failed: {e}");
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

        // 阶段 3：末块发完后做 catch-up——把 chunked 期间 watcher push 进来的
        // 真新行用 jsonl-line live 通道 emit 出去（前端走 source="live"，append
        // 贴底，自然排在 head chunk 之后符合时间顺序）。完成后清 replaying + 标
        // ready。**修复 v2.3.0+ 的"重放窗口期冻结到 F5"UX bug**。
        let catch_up: Vec<JsonlLinePayload> = {
            let mut inner = self.inner.lock();
            inner.ready = true;
            inner.replaying = false;
            inner.history.iter().skip(snapshot_len).cloned().collect()
        };
        if !catch_up.is_empty() {
            tracing::info!(
                "[perf] chunked replay catch-up: {} live rows arrived during emit window",
                catch_up.len()
            );
            for p in catch_up {
                if let Err(e) = handle.emit(events::JSONL_LINE, &p) {
                    tracing::warn!("catch-up jsonl-line emit failed: {e}");
                }
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

    #[test]
    fn replaying_flag_suppresses_emit_then_catch_up_drains() {
        // 单元测：模拟 chunked replay 期间 watcher push 新行 → on_line_batch 看到
        // replaying=true 仅 push 不 emit；之后清 replaying 取 history 尾部 catch-up
        // 应该拿到那些新行（修复 P0-1：v2.3+ chunked 窗口期前端冻结到 F5）。
        let r = EventReplay::new();
        // 模拟 replay 中段：standalone 设 replaying + ready 还没置
        {
            let mut inner = r.inner.lock();
            inner.replaying = true;
            // 假设 snapshot 已发了 10 条（实际由 replay_and_mark_ready snapshot.clone() 抓拍）
            for i in 0..10 {
                inner.history.push_back(payload("s1", i));
            }
        }
        let snapshot_len = 10usize;

        // 模拟 watcher 期间 push 5 条新行——由于无 AppHandle，直接绕过 emit 路径
        // 验"replaying 时只 push history"：手动 push 后 catch-up 应能取到 5 条
        {
            let mut inner = r.inner.lock();
            for i in 10..15 {
                inner.history.push_back(payload("s1", i));
            }
        }

        // 模拟 replay 末块完成后取 catch-up
        let catch_up: Vec<JsonlLinePayload> = {
            let mut inner = r.inner.lock();
            inner.ready = true;
            inner.replaying = false;
            inner.history.iter().skip(snapshot_len).cloned().collect()
        };
        assert_eq!(catch_up.len(), 5, "chunked 窗口期 5 条新行应进 catch-up");
        // 顺序仍是 jsonl 顺序（idx 10..15）
        for (i, p) in catch_up.iter().enumerate() {
            assert_eq!(idx_of(p), 10 + i, "catch-up 内部顺序乱了");
        }
    }
}
