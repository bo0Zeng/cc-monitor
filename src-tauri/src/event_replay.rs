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
use tauri::{AppHandle, Emitter, Runtime, WebviewWindow};

pub struct EventReplay {
    inner: Mutex<Inner>,
}

struct Inner {
    history: VecDeque<JsonlLinePayload>,
    /// frontend 已收到 replay；可走 live emit。
    /// P5.4 B 重构：删了 `replaying` flag —— chunked emit 期间 watcher push 直接
    /// emit，前端 timeline 按 seq 自动放到正确位置。
    ready: bool,
    /// Batch8-F26：frontend-ready 携带的"用户上次所在 tab"（F19 语义）。存下来
    /// 供远端快照拉取排队（当前 tab 的会话先拉）；None = 无记忆/未就绪。
    priority_sid: Option<String>,
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
                priority_sid: None,
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

        // 大 batch：切块 jsonl-batch（前端按 seq 自动排，无 head/older 区分）。
        //
        // Batch5-F17：块序列 spawn 到 async_runtime、块间 tokio::time::sleep
        // （原地 std::thread::sleep 会睡 tokio worker，INVARIANT § 10）。
        //
        // ⚠ spawn = 本函数返回≠emit 完成：与其他通道（如 session-ended）的相对
        // 顺序不保证。**顺序敏感的调用方必须用 `on_line_batch_awaited`**——
        // ssh_source 的边界/断连 flush 若走本入口，迟到的行会把刚归档的远端
        // Tab 复活成僵尸 live（F17 审计 R1）。本入口仅供本地 watcher std 线程。
        let n = payloads.len();
        let chunks = build_chunks(&payloads);
        let chunk_total = chunks.len() as u32;
        tracing::info!(
            "[perf] incremental batch chunked: total={n}, chunks={chunk_total} (likely /resume or large append)"
        );
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            emit_chunks(&handle, chunks, chunk_total).await;
        });
    }

    /// `on_line_batch` 的 await 变体（Batch5-F17 审计 R1）：大 batch 的块序列
    /// **在调用方任务内发完才返回**——ssh_source 的攒批 flush 用它，保证行 emit
    /// 严格先于随后的 SessionRemoved/断连归档（issue #20 / FIX 2 的顺序契约），
    /// 同时对 daemon 帧流形成天然背压（emit 期间不再收帧）。
    pub async fn on_line_batch_awaited<R: Runtime>(
        &self,
        handle: &AppHandle<R>,
        payloads: Vec<JsonlLinePayload>,
    ) {
        if payloads.is_empty() {
            return;
        }
        let (ready, big_batch) = {
            let mut inner = self.inner.lock();
            for p in &payloads {
                inner.history.push_back(p.clone());
            }
            (inner.ready, payloads.len() >= INCREMENTAL_BATCH_THRESHOLD)
        };
        if !ready {
            return;
        }
        if !big_batch {
            for p in payloads {
                if let Err(e) = handle.emit(events::JSONL_LINE, &p) {
                    tracing::warn!("emit jsonl-line failed: {e}");
                }
            }
            return;
        }
        let n = payloads.len();
        let chunks = build_chunks(&payloads);
        let chunk_total = chunks.len() as u32;
        tracing::info!("[perf] incremental batch chunked (awaited): total={n}, chunks={chunk_total} (remote snapshot)");
        emit_chunks(handle, chunks, chunk_total).await;
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
    /// async：块间 pause 用 `tokio::time::sleep`——本函数跑在 tauri::async_runtime
    /// 的 task 里（lib.rs frontend-ready），原 `std::thread::sleep` 会压住 tokio
    /// worker（INVARIANT § 10），issue #20 顺手清理。
    pub async fn replay_and_mark_ready<R: Runtime>(
        &self,
        handle: &AppHandle<R>,
        priority_sid: Option<&str>,
    ) {
        let started = std::time::Instant::now();
        // Batch8-F26：留存 priority（远端快照排队用）
        self.inner.lock().priority_sid = priority_sid.map(str::to_string);

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

        // N ≥ 阈值 → 切块。Batch5-F19：priority session（用户上次所在 tab）的
        // 块在前（组内仍末块先发）——当前 tab 最先可读；其余随后（同样末块先发）。
        // emit 重排对视觉正确性零影响（前端按 seq 排，INVARIANT § 5/§ 9）。
        let chunks = build_priority_chunks(snapshot, priority_sid);
        let chunk_total = chunks.len();
        tracing::info!(
            "[perf] replay切块: total={n}, chunks={chunk_total} (CHUNK_SIZE={CHUNK_SIZE}, 末块先发, priority={priority_sid:?})"
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
                tokio::time::sleep(std::time::Duration::from_millis(CHUNK_PAUSE_MS)).await;
            }
        }

        tracing::info!(
            "[perf] replayed {n} events to frontend (chunked × {chunk_total}) in {}ms total",
            started.elapsed().as_millis()
        );
    }

    /// issue #10：把指定 session 的历史**定向** emit 给某个独立 viewer 窗口（不广播）。
    ///
    /// 独立窗口（`viewer-<sid>`）打开后调用：主窗口的全局 replay 早已发过，新窗口错过了，
    /// 这里从 buffer 里挑该 sid 的历史，按 `build_chunks`（末块先发）只发给这一个窗口。
    /// **seq 与实时 `jsonl-line` 同空间**（都是 watcher 的 per-file seq），所以新窗口前端把
    /// 定向历史 + 实时增量混进同一个 RecordTimeline 时顺序天然正确（重叠由前端 seq 去重）。
    ///
    /// 仅活跃 session 的历史在 buffer 里（watcher 只 tail 活跃 jsonl）；archived session
    /// 走前端一次性文件读路径，不经此函数。
    pub fn replay_session_to_window<R: Runtime>(
        &self,
        window: &WebviewWindow<R>,
        session_id: &str,
    ) {
        let history: Vec<JsonlLinePayload> = {
            let inner = self.inner.lock();
            inner
                .history
                .iter()
                .filter(|p| p.session_id == session_id)
                .cloned()
                .collect()
        };
        if history.is_empty() {
            tracing::info!("replay_session_to_window({session_id}): no buffered history");
            return;
        }
        let n = history.len();
        let chunks = build_chunks(&history);
        let chunk_total = chunks.len() as u32;
        let label = window.label().to_string();
        // 显式 WebviewWindow 目标定向投递（不广播，避免污染主窗口 timeline）。
        // 前端 viewer 用 getCurrentWebviewWindow().listen 接（同 WebviewWindow{label} kind）。
        // 不能用 `&str` 目标（那会变 EventTarget::AnyLabel，命不中前端的窗口作用域监听）。
        let target = tauri::EventTarget::webview_window(label.clone());
        for (idx, chunk) in chunks.into_iter().enumerate() {
            let payload = JsonlBatchPayload {
                chunk_index: idx as u32,
                chunk_total,
                payloads: chunk,
            };
            if let Err(e) = window.emit_to(target.clone(), events::JSONL_BATCH, &payload) {
                tracing::warn!("replay_session_to_window emit chunk {idx} failed: {e}");
            }
        }
        tracing::info!(
            "replay_session_to_window({session_id}): {n} events in {chunk_total} chunks → {label}"
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

    /// Batch8-F26：远端快照排队的优先 sid（F19"上次所在 tab"）。
    pub fn priority_sid(&self) -> Option<String> {
        self.inner.lock().priority_sid.clone()
    }

    /// replay 后对账用：buffer 里所有**本地**（`origin == None`）session 的去重 sid。
    ///
    /// 前端是纯事件增量模型：Tab 见行即建 live，只有一次性的 `session-ended` 能归档。
    /// F5 / HMR 重载后 replay 把 buffer 里已结束会话的行也重放成 live Tab，但归档信号
    /// （session-ended）不在 buffer、不会重发 → 僵尸 live Tab（还因 closeTab 门控
    /// archived 而关不掉）。frontend-ready 重放后，用本集合 × session_map 当前活跃集
    /// 对账、对已结束的本地 sid 补发 session-ended（issue #19）。**仅本地**：session_map
    /// 只认本地，远端 sid 不在其中。远端版见 [`Self::buffered_remote_session_ids`]（issue #20）。
    pub fn buffered_local_session_ids(&self) -> Vec<String> {
        let inner = self.inner.lock();
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for p in inner.history.iter() {
            if p.origin.is_none() && seen.insert(p.session_id.clone()) {
                out.push(p.session_id.clone());
            }
        }
        out
    }

    /// `buffered_local_session_ids` 的远端版（issue #20）：buffer 里所有
    /// **远端**（`origin == Some(host)`）session 的去重 sid。
    ///
    /// 远端 sid 不在 session_map 里，对账要用 lib.rs 维护的远端活跃集
    /// （remote-session-emitter 随 daemon 的 added/removed 增删）。**不区分 host**：
    /// 多机（#30）下仍依赖「sid 全局唯一」—— Claude sid 是 UUID v4，跨机碰撞概率 ≈ 0，
    /// 故按裸 sid 去重/对账安全。**若将来 daemon 改用非 UUID sid（PID/自增），必须把
    /// remote_active / 前端 Tab key / RemoteHwndCache 升为 (origin, sid)**（见 #30 跟进）。
    pub fn buffered_remote_session_ids(&self) -> Vec<String> {
        let inner = self.inner.lock();
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for p in inner.history.iter() {
            if p.origin.is_some() && seen.insert(p.session_id.clone()) {
                out.push(p.session_id.clone());
            }
        }
        out
    }
}

impl Default for EventReplay {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch5-F19：分组切块——priority session（用户上次所在 tab）的块在前，其余
/// payload 保原序成组随后；两组内部均沿 [`build_chunks`] 的末块先发。所有
/// payload 不丢不重；`priority_sid` 为 None 或不命中任何 payload 时**逐字节
/// 等价** `build_chunks(snapshot)`（rest 即全量）。
fn build_priority_chunks(
    snapshot: Vec<JsonlLinePayload>,
    priority_sid: Option<&str>,
) -> Vec<Vec<JsonlLinePayload>> {
    let Some(sid) = priority_sid else {
        return build_chunks(&snapshot);
    };
    // 按值 partition：调用方本就拥有 snapshot，白拿这份拷贝（审计 S2）。
    let (pri, rest): (Vec<JsonlLinePayload>, Vec<JsonlLinePayload>) =
        snapshot.into_iter().partition(|p| p.session_id == sid);
    if pri.is_empty() {
        return build_chunks(&rest);
    }
    let mut chunks = build_chunks(&pri);
    chunks.extend(build_chunks(&rest));
    chunks
}

/// 增量大 batch 的块序列 emit（Batch5-F17 抽取，供 spawn 与 awaited 两个入口
/// 共用）：块间 `tokio::time::sleep` pacing，块内顺序由 for 循环保证。
async fn emit_chunks<R: Runtime>(
    handle: &AppHandle<R>,
    chunks: Vec<Vec<JsonlLinePayload>>,
    chunk_total: u32,
) {
    let started = std::time::Instant::now();
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
            tokio::time::sleep(std::time::Duration::from_millis(CHUNK_PAUSE_MS)).await;
        }
    }
    tracing::info!(
        "[perf] incremental batch chunked emit done in {}ms total",
        started.elapsed().as_millis()
    );
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
            origin: None,
            message: JsonlRecord::Unknown,
        }
    }

    fn idx_of(p: &JsonlLinePayload) -> usize {
        let path = &p.path;
        let last = path.rsplit('/').next().unwrap();
        last.trim_end_matches(".jsonl").parse().unwrap()
    }

    #[test]
    fn buffered_local_session_ids_dedups_and_skips_remote() {
        let replay = EventReplay::new();
        {
            // 子模块可直接访问私有 inner。
            let mut inner = replay.inner.lock();
            inner.history.push_back(payload("s1", 0));
            inner.history.push_back(payload("s1", 1)); // 同 sid 第二行 → 去重
            inner.history.push_back(payload("s2", 0));
            let mut remote = payload("r1", 0);
            remote.origin = Some("nanopi".to_string()); // 远端 → 跳过
            inner.history.push_back(remote);
        }
        let mut ids = replay.buffered_local_session_ids();
        ids.sort();
        assert_eq!(ids, vec!["s1".to_string(), "s2".to_string()]);
    }

    #[test]
    fn buffered_remote_session_ids_dedups_and_skips_local() {
        let replay = EventReplay::new();
        {
            let mut inner = replay.inner.lock();
            inner.history.push_back(payload("s1", 0)); // 本地 → 跳过
            let mut r1a = payload("r1", 0);
            r1a.origin = Some("nanopi".to_string());
            inner.history.push_back(r1a);
            let mut r1b = payload("r1", 1); // 同 sid 第二行 → 去重
            r1b.origin = Some("nanopi".to_string());
            inner.history.push_back(r1b);
            let mut r2 = payload("r2", 0);
            r2.origin = Some("rk3576".to_string()); // 不同 host 也收
            inner.history.push_back(r2);
        }
        let mut ids = replay.buffered_remote_session_ids();
        ids.sort();
        assert_eq!(ids, vec!["r1".to_string(), "r2".to_string()]);
    }

    // === Batch5-F19：build_priority_chunks ===

    #[test]
    fn priority_chunks_put_priority_session_first_no_loss_no_dup() {
        // s1 与 s2 交错各 700 条（> CHUNK_SIZE=600，两组都会切多块）
        let mut snapshot = Vec::new();
        for i in 0..700 {
            snapshot.push(payload("s1", i * 2));
            snapshot.push(payload("s2", i * 2 + 1));
        }
        let chunks = build_priority_chunks(snapshot.clone(), Some("s2"));
        let flat: Vec<&JsonlLinePayload> = chunks.iter().flatten().collect();
        assert_eq!(flat.len(), 1400, "no loss");
        // 前 700 条全是 s2（priority 组整体在前）
        assert!(flat[..700].iter().all(|p| p.session_id == "s2"));
        assert!(flat[700..].iter().all(|p| p.session_id == "s1"));
        // 组内末块先发：priority 组第一块的首元素 idx 大于最后一块的首元素 idx
        let first_chunk_first = idx_of(&chunks[0][0]);
        let pri_chunk_count = chunks
            .iter()
            .take_while(|c| c[0].session_id == "s2")
            .count();
        let last_pri_first = idx_of(&chunks[pri_chunk_count - 1][0]);
        assert!(
            first_chunk_first > last_pri_first,
            "newest-first within priority group"
        );
        // rest 组同样末块先发（审计 S3：只验 pri 组会漏掉 rest 组改正序的回归）
        let first_rest_first = idx_of(&chunks[pri_chunk_count][0]);
        let last_rest_first = idx_of(&chunks[chunks.len() - 1][0]);
        assert!(
            first_rest_first > last_rest_first,
            "newest-first within rest group"
        );
        // 去重校验：uuid 级不重复（用 (sid, seq) 对）
        let mut seen = std::collections::HashSet::new();
        for p in &flat {
            assert!(seen.insert((p.session_id.clone(), p.seq)), "no dup");
        }
    }

    #[test]
    fn priority_none_or_miss_equals_plain_build_chunks() {
        let snapshot: Vec<JsonlLinePayload> = (0..1500).map(|i| payload("s1", i)).collect();
        let plain = build_chunks(&snapshot);
        let none = build_priority_chunks(snapshot.clone(), None);
        let miss = build_priority_chunks(snapshot.clone(), Some("nope"));
        let key = |cs: &Vec<Vec<JsonlLinePayload>>| -> Vec<Vec<u64>> {
            cs.iter()
                .map(|c| c.iter().map(|p| p.seq).collect())
                .collect()
        };
        assert_eq!(key(&none), key(&plain), "None → 等价 build_chunks");
        assert_eq!(key(&miss), key(&plain), "sid 不命中 → 退化等价");
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
