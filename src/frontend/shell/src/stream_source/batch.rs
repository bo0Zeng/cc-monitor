//! 攒批与收行：远端与本机两条流共用的内容收口。

use super::*;
use crate::event_replay::EventReplay;
use std::sync::Arc;
use std::time::Duration;

/// Line 帧攒批缓冲（Batch5-F17）。
///
/// backend 线协议没有批量帧（一行一帧），首连 snapshot 的几千行历史若逐帧调
/// 一批一条地交重放缓冲，恒 1 < INCREMENTAL_BATCH_THRESHOLD → 全部走
/// 逐条 jsonl-line live 渲染管线（v2.4.2 给本地修掉的逐行刷屏在远端重现）。
/// 客户端把**连续到达**的 Line 帧聚合成批再交重放缓冲：snapshot 密集
/// 连发天然聚成大批 → 自动跨过阈值复用 chunked 回放路径；日常单行增量
/// 只多一个静默窗口（~30ms）的延迟。时序判定（静默窗口）留在 [`LineIntake::recv_or_flush`]
/// 的 `tokio::time::timeout` 里；本结构只管容量与顺序，纯逻辑可直测。
struct Batcher {
    pending: Vec<JsonlLine>,
    cap: usize,
    /// 首行入缓冲的时刻——批龄上限用（F17 审计 R3：帧间隔持续 < 静默窗口时
    /// 永不静默，首行可见延迟无界；批龄到点强制 flush 双保险）。
    born: Option<std::time::Instant>,
}

impl Batcher {
    fn new(cap: usize) -> Self {
        Self {
            pending: Vec::new(),
            cap,
            born: None,
        }
    }

    /// 收一行；达容量上限或批龄超限则返回整批待发（防无界内存/无界延迟）。
    fn push(&mut self, line: JsonlLine) -> Option<Vec<JsonlLine>> {
        if self.pending.is_empty() {
            self.born = Some(std::time::Instant::now());
        }
        self.pending.push(line);
        let over_age = self
            .born
            .is_some_and(|b| b.elapsed().as_millis() as u64 >= BATCH_MAX_AGE_MS);
        if self.pending.len() >= self.cap || over_age {
            return self.take();
        }
        None
    }

    /// 取走全部待发行（空则 None）。到达顺序 = 发出顺序（backend per-file seq
    /// 单调，前端按 seq 排序，跨 session 混流不需要拆分）。
    fn take(&mut self) -> Option<Vec<JsonlLine>> {
        self.born = None;
        if self.pending.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }
}

/// 攒批静默窗口：一行到达后最多再等这么久看有没有后续行。首连 snapshot 帧间
/// 隔远小于此值 → 聚合；live 单行只付一次窗口的延迟（对流式渲染无感）。
const BATCH_QUIET_MS: u64 = 30;
/// 单批行数上限（与本地 replay CHUNK_SIZE 同量级，控制单次 IPC 体积）。
const BATCH_CAP: usize = 600;
/// 批龄上限：无论帧流多密集，首行入缓冲后最迟这么久必 flush（见 Batcher.born）。
const BATCH_MAX_AGE_MS: u64 = 200;

/// 攒批出口（Batch5-F17）：远端流、本机流、旁路快照三路的行**都**从这里出去
/// （`batch_to_payloads` → `on_line_batch_awaited`），用 **awaited 变体**——大批的块序列发完才返回，保证行
/// emit 严格先于随后的 SessionRemoved/断连归档（审计 R1：spawn 化的行若晚于
/// ended 格 到达前端，会把刚归档的远端 Tab 复活成僵尸 live），同时对
/// backend 帧流形成天然背压。
pub(super) async fn flush_lines(
    replay: &Arc<EventReplay>,
    host_label: &str,
    lines: Vec<JsonlLine>,
    runs: &mut crate::SkipRuns,
) {
    let flushed: Vec<(String, u64, Option<u64>)> = lines
        .iter()
        .map(|l| (l.session_id.clone(), l.seq, l.end))
        .collect();
    // 同一个 origin 既是载荷上的机器名、也是看不懂的行记账的那台。
    let origin = crate::origin::Origin(host_label.to_string());
    let payloads = crate::batch_to_payloads(lines, &origin, runs);
    // 交给订了它的那些会话流（`event_replay` 头注「订阅」）；出口在它手里，不再经 `app` 广播。
    replay.on_line_batch_awaited(payloads).await;
    // 发出去了才推续点（连续才推，见 `snapshot_resume::note_flushed`）。
    crate::snapshot_resume::note_flushed(
        &origin,
        flushed.iter().map(|(s, q, e)| (s.as_str(), *q, *e)),
    );
}

/// **内容那一半的唯一收口** —— 远端每条连接一个、本机每条流一个。
///
/// # 为什么要它
///
/// 「一条流，一个来源；本机与远端走同一条帧路」。本机那条流改走后端的 `line` 帧之后，
/// 「行怎么攒批、什么时候冲、历史怎么旁路补、会话走了撤什么」这几件事若在本机再写一份，
/// 就又是两份实现。⇒ 收成一个结构，**两个帧源各构造一次**：
/// [`stream_loop`]（远端）与 [`consume_local`]（本机）。判据钉的就是「构造点恰好这两处」。
///
/// # 它管什么、不管什么
///
/// 管：[`Batcher`] ＋ 带静默窗的收（[`LineIntake::recv_or_flush`]）· 旁路快照队列与分发器（`tail_only` 时起）·
/// 续点（`snapshot_resume`）。**不管会话的起停**：那是后端出的成品，两条流各自交 `session_book`。
///
/// 丢掉它 ⇒ 快照队列当场关（与原来 `stream_loop` 里那个 `SnapshotQueueCloser` 同一个时机）。
pub(crate) struct LineIntake {
    origin_label: String,
    replay: Arc<EventReplay>,
    batcher: Batcher,
    snapshots: std::sync::Arc<SnapshotQueue>,
    tail_only: bool,
    /// 实时那一路的「连着的不可显示那一段」（`SkipRuns`）。
    runs: crate::SkipRuns,
    _closer: SnapshotQueueCloser,
}

impl LineIntake {
    /// `tail_only`：这条流的后端是不是按「不重放历史」起的 —— 是 ⇒ 历史走旁路快照（起分发器）。
    pub(super) fn open(
        origin_label: String,
        tail_only: bool,
        replay: &Arc<EventReplay>,
        health: &HealthOut,
    ) -> Self {
        let snapshots = SnapshotQueue::new();
        if tail_only {
            tauri::async_runtime::spawn(snapshot_dispatcher(
                snapshots.clone(),
                replay.clone(),
                health.clone(),
                origin_label.clone(),
            ));
        }
        LineIntake {
            origin_label,
            replay: replay.clone(),
            batcher: Batcher::new(BATCH_CAP),
            _closer: SnapshotQueueCloser(snapshots.clone()),
            snapshots,
            tail_only,
            runs: crate::SkipRuns::default(),
        }
    }

    /// 带静默窗地收下一件：手里攒着行时最多等 [`BATCH_QUIET_MS`]，窗内没来新东西就先把攒的冲掉再回去等。
    ///
    /// ⚠ 超时打在 `recv` 上（cancel-safe），不打在读行上 —— 理由见 `stream_loop` 里那个读帧任务的注释。
    pub(super) async fn recv_or_flush<T>(
        &mut self,
        rx: &mut tokio::sync::mpsc::Receiver<T>,
    ) -> Option<T> {
        loop {
            if self.batcher.pending.is_empty() {
                return rx.recv().await;
            }
            match tokio::time::timeout(Duration::from_millis(BATCH_QUIET_MS), rx.recv()).await {
                Ok(m) => return m,
                Err(_) => self.flush().await,
            }
        }
    }

    /// 收一行（达容量 / 批龄就整批冲出去）。
    pub(super) async fn line(&mut self, line: JsonlLine) {
        if let Some(full) = self.batcher.push(line) {
            flush_lines(&self.replay, &self.origin_label, full, &mut self.runs).await;
        }
    }

    /// 把攒着的行冲出去（攒批边界：会话走了 / 流断了 / 静默窗到了）。
    pub(super) async fn flush(&mut self) {
        if let Some(lines) = self.batcher.take() {
            flush_lines(&self.replay, &self.origin_label, lines, &mut self.runs).await;
        }
    }

    /// 一个会话被宣告了：tail-only 下带 `path` 就排一份旁路快照（无 path = 会话刚起还没写 jsonl ⇒ 无历史可拉）。
    pub(super) fn announced(&self, sid: &str, path: Option<String>, lines: Option<u64>) {
        if !self.tail_only {
            return;
        }
        if let Some(p) = path {
            self.snapshots.push(SnapshotItem {
                sid: sid.to_string(),
                path: p,
                expected_lines: lines,
            });
        }
    }

    /// 一个会话的记录文件不见了 / 被改过已从头重读：残批先冲（出声那一格排在它之前的行后面、
    /// 重读出来的行前面 —— 后端发它就在重读的行之前），再交那个会话的内容流一格。
    pub(super) async fn notice(&mut self, sid: &str, path: &str, change: FileChange) {
        self.flush().await;
        // 从头重读 ⇒ 后端的行号从 0 重数：在飞 / 排队的快照（旧的一代）撤掉、续点作废。
        //   （与「会话走了」同一件事：`removed`）。留存里旧的一代由 `on_session_notice` 同一拍丢。
        if change != FileChange::Gone {
            self.removed(sid);
        }
        self.replay
            .on_session_notice(crate::ui_contract::SessionFileNoticePayload {
                session_id: sid.to_string(),
                origin: self.origin_label.clone(),
                path: path.to_string(),
                change: change.as_wire().to_string(),
            })
            .await;
    }

    /// 这条流上有一行丢了、说不出是哪个会话的哪一行（超长整行丢弃）：残批先冲，
    /// 再给订了这台的每条订阅原位一格「丢了、不知道丢到哪」（`Item::Gap` 的 `to_seq` 缺）——前端照往后补。
    pub(super) async fn lost(&mut self) {
        self.flush().await;
        self.replay.on_lost_somewhere(&self.origin_label);
    }

    /// 一个会话走了：撤它的快照（排队的摘掉、在飞的打取消标记）、续点作废（再宣告时整份拉）。
    pub(super) fn removed(&mut self, sid: &str) {
        self.runs.forget(sid);
        self.snapshots.cancel(sid);
        crate::snapshot_resume::forget(&crate::origin::Origin(self.origin_label.clone()), sid);
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/batcher_tests.rs"]
mod batcher_tests;
