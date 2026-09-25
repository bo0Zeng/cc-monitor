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
//! 〔U3b · `设计/10` 步 8〕**两档**，以「前端能不能按偏移把正文要回来」为界：
//!
//! | 会话 | 留多少 | 丢掉的正文从哪回来 |
//! |---|---|---|
//! | 前端**已接上骨架**并调过 [`EventReplay::keep_tail_only`] | 尾巴 [`REPLAY_TAIL_KEEP`] 条（修剪有 [`TRIM_SLACK`] 的摊还余量 ⇒ 上界 `KEEP + SLACK`） | 骨架滚到那里时 `read_session_range`（`--read-session-from-offset --until`） |
//! | 其余（没索引：本机后端不在 / 老后端 / Codex） | **不设上限**（原样） | 无处可回 ⇒ 不许丢 |
//!
//! 🔴 **第二档仍然无上限，这是刻意的**：没有骨架的会话，丢掉的正文前端再也拿不回来
//! （上翻到头就没了），而「内存省一点」换「历史少一截」是回归。极端情况重启 monitor 即清（原话照旧）。
//! 读数（长度 / 修剪次数）见 [`EventReplay::stats`] 与每次修剪的 `[replay]` 日志行。

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
    /// 〔U3b〕前端已接上骨架的会话 → 它在 `history` 里此刻有几条（只给这些会话计数，
    /// 未登记的会话一条都不数 ⇒ 第二档零额外开销）。
    tail_only: std::collections::HashMap<String, usize>,
    /// 〔U3b〕累计修剪掉的条数（读数口，[`EventReplay::stats`]）。
    trimmed_total: u64,
}

/// 〔U3b · `设计/10` 步 8〕**接上骨架的会话，history 里只留尾巴这么多条可显示记录。**
///
/// # 依据（量出来的，不是拍的）
///
/// ① **前端开一个 tab 时最多建多少条不用滚动**：`tabs.ts` 的 `materializeUntilFilled` 是
///    `MATERIALIZE_TAIL_K`（150）× 最多 4 轮 = **600**。尾巴少于它，F5 之后那一屏就要等
///    按偏移取正文（多一次 IPC ＋ 一次后端进程）；多于它，多出来的那段首屏根本用不上。
///    ⇒ 取 600。这条等式由 `tests/replay-tail-keep.vitest.ts` 对着两边源码钉着（改一边会红）。
/// ② **它够不够一屏**（2026-09-24，laptop 本机 39 份会话，按骨架第一级粗估、1080 px 视口）：
///    被截的 7 份里，尾巴 600 条覆盖 **11.8–75.3 屏**（p50 28.6）⇒ 首屏 ＋ 头几次上翻都不用去取。
///    对照：150 条最坏只有 1.6 屏（离「一屏」只差一点），300 条最坏 6.6 屏。
/// ③ **它省多少**：39 份会话可显示记录的字节 188.8 MB → 26.3 MB（13.9%）；
///    单会话留下的字节 p50 1.50 MB、最大 2.45 MB（全留时最大的那份 95 MB）。
///
/// ⚠ 这些数是**记录行的字节**，不是 `JsonlLinePayload` 在堆上的真大小（解析后的结构体
///    另有开销，未量）；量具与读数在 `设计/10 §10`。
pub const REPLAY_TAIL_KEEP: usize = 600;

/// 修剪的摊还余量：一个会话超过 `KEEP + SLACK` 才修剪回 `KEEP`。
/// 修剪一次是 O(history)（按 seq 找第 KEEP 大、再 retain）⇒ 每来一行都修会让 live 路付 O(history)；
/// 攒 `KEEP/4` 条修一次，摊到每行是 O(history)/150。**代价**：接了骨架的会话上界是 750 条不是 600。
pub const TRIM_SLACK: usize = REPLAY_TAIL_KEEP / 4;

/// 〔U3b〕读数：`history` 总长 · 接了骨架的会话数 · 累计修剪条数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayStats {
    pub history_len: usize,
    pub tail_only_sessions: usize,
    pub trimmed_total: u64,
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
/// 一次攒出来的批 >= 此值时（典型场景：用户
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
                tail_only: std::collections::HashMap::new(),
                trimmed_total: 0,
            }),
        }
    }

    /// 行进重放缓冲的**唯一**入口：先进 `history`，ready 之后按批大小分流发出去 ——
    /// 小批（< [`INCREMENTAL_BATCH_THRESHOLD`]）逐条 `jsonl-line`，大批切块 `jsonl-batch`。
    /// 大 batch 的块序列**在调用方任务内发完才返回**（Batch5-F17 审计 R1）——`ssh_source` 的攒批 flush
    /// 用它，保证行 emit 严格先于随后的 SessionRemoved/断连归档（issue #20 / FIX 2 的顺序契约），
    /// 同时对后端帧流形成天然背压（emit 期间不再收帧）。
    ///
    /// 〔CF1 · 2026-09-24〕原来还有一份不 await、把块序列 spawn 出去的孪生（只供本机 watcher 那条
    /// std 线程用，`真相源/10 §7.2`「五段逻辑字面重复」）。本机内容改走后端的帧之后它零调用方，删了；
    /// 名字里的 `_awaited` 留着是为了不在十几路同时改的时候改一个到处被点名的符号。
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
            push_and_trim(&mut inner, &payloads);
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
    ///    （直接走 on_line_batch_awaited 的实时那一支，前端 timeline 自动排序，**无需 catch-up**）
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
        // 直接走 on_line_batch_awaited 的实时那一支（ready=true）→ emit jsonl-line → 前端
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
    /// **seq 与实时 `jsonl-line` 同空间**（都是后端给的行号），所以新窗口前端把
    /// 定向历史 + 实时增量混进同一个 RecordTimeline 时顺序天然正确（重叠由前端 seq 去重）。
    ///
    /// 仅活跃 session 的历史在 buffer 里（后端只宣告、只 tail 活跃会话）；archived session
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
        inner.tail_only.remove(session_id);
        let before = inner.history.len();
        inner.history.retain(|p| p.session_id != session_id);
        let removed = before - inner.history.len();
        if removed > 0 {
            tracing::info!("event_replay forget {session_id}: dropped {removed} entries");
        }
    }

    /// 〔U3b〕前端对这个会话**接上了骨架**（拿得到索引、按偏移取得回正文）⇒ 从此它在 history 里
    /// 只留尾巴 [`REPLAY_TAIL_KEEP`] 条。当场修剪一次；返回这次丢掉的条数。幂等。
    ///
    /// ⚠ 登记是**单向**的：之后若索引再拿不到（远端断线 / 后端被换成老版本），F5 之后那个 tab
    /// 只剩尾巴，上翻到头就没了 —— 要等索引恢复。如实登记在 `设计/10 §10` 的「买不到」。
    pub fn keep_tail_only(&self, session_id: &str) -> usize {
        let mut inner = self.inner.lock();
        let n = inner
            .history
            .iter()
            .filter(|p| p.session_id == session_id)
            .count();
        inner.tail_only.insert(session_id.to_string(), n);
        trim_to_tail(&mut inner, session_id)
    }

    /// 〔U3b〕读数口（日志与判据用）。
    pub fn stats(&self) -> ReplayStats {
        let inner = self.inner.lock();
        ReplayStats {
            history_len: inner.history.len(),
            tail_only_sessions: inner.tail_only.len(),
            trimmed_total: inner.trimmed_total,
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
    /// （remote-session-emitter 随后端的 added/removed 增删）。**不区分 host**：
    /// 多机（#30）下仍依赖「sid 全局唯一」—— Claude sid 是 UUID v4，跨机碰撞概率 ≈ 0，
    /// 故按裸 sid 去重/对账安全。**若将来后端改用非 UUID sid（PID/自增），必须把
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

/// 〔U3b〕进账：push 进 history；登记过「只留尾巴」的会话计数，超过 `KEEP + SLACK` 就修回 `KEEP`。
fn push_and_trim(inner: &mut Inner, payloads: &[JsonlLinePayload]) {
    let mut over: Vec<String> = Vec::new();
    for p in payloads {
        inner.history.push_back(p.clone());
        if let Some(n) = inner.tail_only.get_mut(&p.session_id) {
            *n += 1;
            if *n > REPLAY_TAIL_KEEP + TRIM_SLACK && !over.contains(&p.session_id) {
                over.push(p.session_id.clone());
            }
        }
    }
    for sid in over {
        trim_to_tail(inner, &sid);
    }
}

/// 〔U3b〕把一个会话修回尾巴 `KEEP` 条 —— **按 seq 取最大的那些，不按到达序**：
/// 远端快照走 `--read-session-tail`（尾部优先），到达序是「尾块在前、头块在后」，按到达序丢会把尾巴丢掉。
/// 返回丢掉的条数。
fn trim_to_tail(inner: &mut Inner, sid: &str) -> usize {
    let mut seqs: Vec<u64> = inner
        .history
        .iter()
        .filter(|p| p.session_id == sid)
        .map(|p| p.seq)
        .collect();
    if seqs.len() <= REPLAY_TAIL_KEEP {
        return 0;
    }
    seqs.sort_unstable_by(|a, b| b.cmp(a));
    let floor = seqs[REPLAY_TAIL_KEEP - 1];
    let before = inner.history.len();
    inner
        .history
        .retain(|p| p.session_id != sid || p.seq >= floor);
    let dropped = before - inner.history.len();
    let kept = seqs.len() - dropped;
    inner.tail_only.insert(sid.to_string(), kept);
    inner.trimmed_total += dropped as u64;
    tracing::info!(
        "[replay] {sid} 修剪到尾巴 {kept} 条（丢 {dropped}，seq < {floor}）；history 总长 {}",
        inner.history.len()
    );
    dropped
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
#[path = "../../../tests/bridge/event_replay_tests.rs"]
mod tests;
