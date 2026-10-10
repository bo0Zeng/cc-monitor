//! 旁路快照：排队、分发、拉取，以及撤销与两段编号。

use super::*;
use crate::copy_table::copy_text;
use crate::event_replay::EventReplay;
use std::sync::Arc;

//
// === 旁路快照拉取（每管道一个对话，完就断） ===
//
// tail-only 下后端不重放历史；每个已宣告会话的完整历史由这里经独立 SSH 连接跑 `--read-session` 一次性查询拉回，按行号编 seq 灌进与 tail 行
// 完全相同的管线（flush_lines → on_line_batch_awaited）。两路 seq 同处行号空间：重叠区是精确重复的 (sid,seq)，被前端既有去重吸收。
// 并发 ≤SNAPSHOT_CONCURRENCY（不抢 tail 通道带宽）；priority sid 优先出队。

const SNAPSHOT_CONCURRENCY: usize = 2;
/// 单会话快照体量上限（防御：远端超巨文件不无界拉取；超限截断 warn——
/// 历史浏览器按需查询不受此限）。
const SNAPSHOT_MAX_BYTES: u64 = 512 * 1024 * 1024;
const SNAPSHOT_CHUNK_LINES: usize = 500;
/// 尾部优先 —— 最新 N 行先到（第一批 emit 即最新内容），旧历史回填。
const SNAPSHOT_TAIL_LINES: usize = 500;

/// 每连接一个：待拉快照队列。sid 幂等（重复宣告不重拉）；`cancel(sid)`
/// （SessionRemoved 时调）摘除排队项 + 给 inflight 打取消标记 + 从 seen 摘除
/// （同连接内 removed→re-added 可重拉，审计 D-S4）；close（断连）**立即作废**
/// 未开拉的排队项——重连会重建队列重拉，断连后继续拉只会把行灌在归档清算
/// 之后（审计 D-B1 僵尸复活）。
pub(super) struct SnapshotQueue {
    pending: std::sync::Mutex<SnapshotPending>,
    notify: tokio::sync::Notify,
    closed: std::sync::atomic::AtomicBool,
}

struct SnapshotPending {
    queue: std::collections::VecDeque<SnapshotItem>,
    seen: std::collections::HashSet<String>,
    /// 已取消（会话已 removed）的 sid——inflight fetch 每个 chunk 边界查它中止。
    cancelled: std::collections::HashSet<String>,
}

// 会话 / tmux 的裁决住那台后端（`src/backend/observe/session_ledger.rs`，成品帧 `session_state`），本机远端同一份；
//   monitor 只有一本成品缓存（`session_book.rs`：转交 ＋ F5 重放 ＋ 断连说「说不清」）。

#[derive(Clone)]
pub(super) struct SnapshotItem {
    pub(super) sid: String,
    pub(super) path: String,
    /// backend prime 时的完整行数 L（p1f 帧 `lines`）——完整性校验：快照行数
    /// < L = 中途断/backend 报错 → 判失败重试（审计 D-I2：exit status 拿不到）。
    pub(super) expected_lines: Option<u64>,
}

/// stream_loop 退出（重连/EOF/错误任何路径）时关队列。
pub(super) struct SnapshotQueueCloser(pub(super) std::sync::Arc<SnapshotQueue>);
impl Drop for SnapshotQueueCloser {
    fn drop(&mut self) {
        self.0.close();
    }
}

impl SnapshotQueue {
    pub(super) fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(SnapshotQueue {
            pending: std::sync::Mutex::new(SnapshotPending {
                queue: std::collections::VecDeque::new(),
                seen: std::collections::HashSet::new(),
                cancelled: std::collections::HashSet::new(),
            }),
            notify: tokio::sync::Notify::new(),
            closed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub(super) fn push(&self, item: SnapshotItem) {
        {
            let mut p = self.pending.lock().unwrap();
            // 重新宣告 = 会话回来了：解除既往取消标记（cancel 时 seen 已摘，
            // 这里 insert 成功才入队）。
            p.cancelled.remove(&item.sid);
            if !p.seen.insert(item.sid.clone()) {
                return; // 本连接内已拉/在拉
            }
            p.queue.push_back(item);
        }
        self.notify.notify_one();
    }

    /// SessionRemoved：摘排队项 + 标记 inflight 取消 + 允许 re-added 重拉。
    pub(super) fn cancel(&self, sid: &str) {
        let mut p = self.pending.lock().unwrap();
        p.queue.retain(|it| it.sid != sid);
        p.seen.remove(sid);
        p.cancelled.insert(sid.to_string());
    }

    /// inflight fetch 的取消/作废检查（chunk 边界调）：会话已 removed 或连接
    /// 已断（断连后继续灌行会落在归档清算之后——B1）。
    fn is_cancelled(&self, sid: &str) -> bool {
        self.closed.load(std::sync::atomic::Ordering::SeqCst)
            || self.pending.lock().unwrap().cancelled.contains(sid)
    }

    fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// 出队：priority sid（若在队中）优先，否则 FIFO。**closed 即 None**（未开
    /// 拉的排队项作废，重连重拉）。
    ///
    /// 丢失唤醒防护（审计 D-I1）：`notify_waiters` 不给未注册者存 permit——
    /// 必须**先注册**（`enable`）再检查状态，close/push 发生在注册后必被捕获、
    /// 发生在注册前则状态检查看得到。
    async fn pop(&self, priority: Option<String>) -> Option<SnapshotItem> {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
                return None;
            }
            {
                let mut p = self.pending.lock().unwrap();
                if let Some(pri) = priority.as_deref() {
                    if let Some(i) = p.queue.iter().position(|it| it.sid == pri) {
                        return p.queue.remove(i);
                    }
                }
                if let Some(item) = p.queue.pop_front() {
                    return Some(item);
                }
            }
            notified.await;
        }
    }
}

/// 分发器：每连接一个 task。并发 ≤SNAPSHOT_CONCURRENCY 地把队列里的会话交给
/// [`fetch_snapshot`]；每项失败重试 1 次（间隔 1s），仍败 → remote-health toast
/// （该 tab 只有实时行，历史浏览器兜底可看全量）。取消（会话 removed/断连）
/// 不算失败、不重试不 toast。
pub(super) async fn snapshot_dispatcher(
    q: std::sync::Arc<SnapshotQueue>,
    replay: Arc<EventReplay>,
    health: HealthOut,
    host_label: String,
) {
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(SNAPSHOT_CONCURRENCY));
    loop {
        let Ok(permit) = sem.clone().acquire_owned().await else {
            return; // semaphore closed（不可达，防御）
        };
        let Some(item) = q.pop(replay.priority_sid()).await else {
            return; // 队列已关（排队项作废，重连重拉）
        };
        let q = q.clone();
        let replay = replay.clone();
        let health = health.clone();
        let host_label = host_label.clone();
        // incr 在 spawn 之前（审计 D：上一 task 归零与下一 task 起跑之间的
        // 瞬时 0 窗口会让 300ms 定时器恰好放行 batch）
        snapshot_inflight_change(&replay, 1);
        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            struct InflightGuard(Arc<EventReplay>);
            impl Drop for InflightGuard {
                fn drop(&mut self) {
                    snapshot_inflight_change(&self.0, -1);
                }
            }
            let _inflight = InflightGuard(replay.clone());
            let sid_short: String = item.sid.chars().take(8).collect();
            let mut last_err = String::new();
            for attempt in 1..=2 {
                match fetch_snapshot(&q, &item, &host_label, &replay).await {
                    Ok(FetchOutcome::Done(lines)) => {
                        tracing::info!(
                            "snapshot [{host_label}] {sid_short}: {lines} 行历史就位（attempt {attempt}）"
                        );
                        return;
                    }
                    Ok(FetchOutcome::Cancelled) => {
                        // 会话已 removed / 连接已断：静默中止（补偿归档已在
                        // fetch 内 emit），不重试不 toast。
                        tracing::info!(
                            "snapshot [{host_label}] {sid_short}: 取消（会话结束/断连）"
                        );
                        return;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "snapshot [{host_label}] {sid_short} attempt {attempt} 失败: {e}"
                        );
                        last_err = e;
                        if attempt == 1 {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                }
            }
            // 那一句不夹下层原话；最后一次的原话进复制详情（remote-health 的 detail 格）。
            let said = crate::detail::Said::with_raw(
                copy_text(
                    "rsSshSource.snapshot.failed",
                    &[("sid", &sid_short.to_string())],
                ),
                &last_err,
            );
            let payload = crate::ui_contract::RemoteHealthPayload {
                origin: host_label.clone(),
                kind: "snapshot".to_string(),
                message: said.said,
                detail: said.detail,
            };
            if let Err(e) = health(payload) {
                tracing::warn!("snapshot remote-health emit failed: {e}");
            }
        });
    }
}

/// Batch9-F30：全局快照 inflight 计数——前端 batch mode 的事件驱动信号
/// （回填在途时不提前退出 batch 模式，见 events.ts）。
/// 计数 + emit 在同一把锁下串行（审计 D：原子操作与 emit 分离时，两个并发
/// task 收尾的 emit 可乱序——{count:0} 先到、{count:1} 后到 → 前端计数粘在
/// 非零、batch 被压满 5min 防呆）。低频（每快照 2 次），锁开销可忽略。
static SNAPSHOT_INFLIGHT: std::sync::Mutex<usize> = std::sync::Mutex::new(0);

fn snapshot_inflight_change(replay: &EventReplay, delta: isize) {
    let mut n = SNAPSHOT_INFLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    *n = if delta > 0 {
        *n + 1
    } else {
        n.saturating_sub(1)
    };
    // 持锁交：保证到达序 == 计数变化序（会话流里的一格，不吃 credit、不丢）。
    replay.on_snapshot_inflight(*n as u32);
    drop(n);
}

/// F5 电平同步（审计 D）：inflight 是变化沿，重载后前端初值 0 ⇒ 就绪点在重放最前面补一格当前电平（`event_replay::lifecycle_replay`）。
pub fn snapshot_inflight_level() -> u32 {
    *SNAPSHOT_INFLIGHT.lock().unwrap_or_else(|e| e.into_inner()) as u32
}

/// fetch 的三态结果：完成（行数）/ 被取消（不重试）。错误走 Err。
enum FetchOutcome {
    Done(u64),
    Cancelled,
}

// 快照只含可计行：口径只住后端 `history_query::line_counts`。

/// 拉取单个会话的完整历史快照并灌进既有管线。
///
/// 🔴 **不再为每份快照单拨一条 SSH。** 此前这里 exec 一次
/// `<backend> --read-session-tail <p> 500`，读它一口气印出来的「meta ＋ 尾段 ＋ 头段」；
/// 现在走已有长连接：先 `history-tail` 问那张图（`total` / `tail_from` / 两段的字节边界），
/// 再按 `[split_at, end)`、`[0, split_at)` 两段用 `history-read` 分页取正文 ——
/// 与那条子命令印出的两段**逐字节相同**（后端扫的是同一个函数），行号映射（[`tail_seq`]）一个字没动。
///
/// 每个 chunk 边界查取消（会话 removed / 连接断）——中止并**补偿 emit 一次
/// ended 格**：若某个已 flush 的 chunk 恰把归档 tab"见行复活"，这里把它
/// 压回 archived（审计 D-B1 僵尸复活的封口；archiveTab 幂等，重复无害）。
///
/// 完整性校验（审计 D-I2）：到达的可计行数必须**恰好等于** `total`。
async fn fetch_snapshot(
    q: &std::sync::Arc<SnapshotQueue>,
    item: &SnapshotItem,
    host_label: &str,
    replay: &Arc<EventReplay>,
) -> Result<FetchOutcome, String> {
    use crate::frame_query;
    let sid = &item.sid;
    let path = &item.path;
    let origin = crate::origin::Origin(host_label.to_string());
    // 快照是两件事、各一个期限：先问图（一问，`PAGE_BUDGET`）；
    //   读正文（分页）在知道要读多少字节之后再造、按大小给（`frame_query::read_budget`），每一页都拿同一个时刻去等。
    let plan = frame_query::tail(
        &origin,
        path,
        SNAPSHOT_TAIL_LINES as u64,
        frame_query::Deadline::within(frame_query::PAGE_BUDGET),
    )
    .await?;
    // 断线重连后从续点接着拉（`snapshot_resume` 头注），续点对不上才整份。
    let cursor = crate::snapshot_resume::cursor_of(&origin, sid);
    let mut how = crate::snapshot_resume::plan_read(cursor.as_ref(), path, &plan);
    // 断线期间文件变短了（续点比这一次的图长）⇒ 这一次整份读出来的是另一代的行号：
    //   先交那个会话一格「变短了、已从头重读」（前端据它整份重来、留存丢旧的一代），再整份读。
    //   续点不必另丢：这一次整份读完立的新锚盖掉它。
    if crate::snapshot_resume::shrank(cursor.as_ref(), path, &plan) {
        replay
            .on_session_notice(crate::ui_contract::SessionFileNoticePayload {
                session_id: sid.to_string(),
                origin: host_label.to_string(),
                path: path.to_string(),
                change: FileChange::Truncated.as_wire().to_string(),
            })
            .await;
    }
    // 续传之前先核锚那一行还是不是那一行（`snapshot_resume` 头注「截断 / 改写检测」）：
    //   断线期间被整份改写而且变长的文件，上面那道「文件没变短」拦不住。对不上 ⇒ 续点作废、整份重读、交那个会话一格「被改过」。
    if let (crate::snapshot_resume::Read::Resume { .. }, Some(w)) =
        (&how, cursor.as_ref().and_then(|c| c.witness.clone()))
    {
        let page = frame_query::read_page(
            &origin,
            path,
            w.start,
            Some(w.end),
            frame_query::Deadline::within(frame_query::PAGE_BUDGET),
        )
        .await?;
        // 一页没读满那一行（`next < end` 且没到头）⇒ 核不了，照续传（不许把「没读全」说成「被改过」）。
        let whole = page.eof || page.next >= w.end;
        if whole && !crate::snapshot_resume::witness_holds(&w, &page.rows) {
            tracing::warn!(
                "snapshot [{host_label}] {sid}: 续点那一行（字节 {}–{}）与上次不是同一行 —— 记录文件在断线期间被改写过，整份重读",
                w.start,
                w.end
            );
            crate::snapshot_resume::forget(&origin, sid);
            replay
                .on_session_notice(crate::ui_contract::SessionFileNoticePayload {
                    session_id: sid.to_string(),
                    origin: host_label.to_string(),
                    path: path.to_string(),
                    change: FileChange::Rewritten.as_wire().to_string(),
                })
                .await;
            how = crate::snapshot_resume::Read::Full;
        }
    }
    if let crate::snapshot_resume::Read::Resume {
        from_byte,
        upto,
        first_seq,
        skip_below,
    } = &how
    {
        tracing::info!(
            "snapshot [{host_label}] {sid}: 续传 —— 从字节 {from_byte}（第 {first_seq} 行）读到 {upto}，\
             第 {skip_below} 行之前的已发过、不再发"
        );
    }
    let mut walk = crate::snapshot_resume::Walk::new(&how, &plan);
    // 走读时顺手挑下一次续传要核的那一行（文件最后一个可计行）。
    let mut pick = crate::snapshot_resume::WitnessPick::default();
    let mut total_bytes: u64 = 0;
    let mut chunk: Vec<JsonlLine> = Vec::with_capacity(SNAPSHOT_CHUNK_LINES);
    let mut runs = crate::SkipRuns::default(); // 这一次快照自己一份（与实时那一路不交错）
    let mut cancelled = false;
    let segments = walk.segments().to_vec();
    let body_bytes: u64 = segments
        .iter()
        .map(|(from, upto)| upto.saturating_sub(*from))
        .sum();
    let body =
        frame_query::Deadline::within(frame_query::read_budget(body_bytes.min(SNAPSHOT_MAX_BYTES)));
    'read: for (from, upto) in segments {
        let mut offset = from;
        while offset < upto {
            let page = frame_query::read_page(&origin, path, offset, Some(upto), body).await?;
            total_bytes += page.next - offset;
            if total_bytes > SNAPSHOT_MAX_BYTES {
                // 防御上限：不再继续拉（完整性校验会把截断判为失败 → toast）。
                tracing::warn!(
                    "snapshot [{host_label}] {sid}: 超过 {SNAPSHOT_MAX_BYTES} 字节上限，截断"
                );
                break 'read;
            }
            let spans = crate::snapshot_resume::row_spans(offset, &page.rows);
            // 后端只交可计行、每行带成品（进不进界面 · `cwd` · 摘要都是它给的）；这里只编号、挑见证、攒批。
            for (row, span) in page.rows.into_iter().zip(spans) {
                pick.see(upto, plan.end, row.hash, span);
                let Some(seq) = walk.step() else {
                    continue; // 续传：锚到续点之间的行前端已有，数掉不发
                };
                chunk.push(JsonlLine {
                    session_id: sid.to_string(),
                    path: std::path::PathBuf::from(path),
                    seq,
                    message: row.message,
                    cwd: row.cwd,
                    end: span.map(|(_, e)| e),
                    rid: None,
                });
                if chunk.len() >= SNAPSHOT_CHUNK_LINES {
                    if q.is_cancelled(sid) {
                        cancelled = true;
                        break 'read;
                    }
                    flush_lines(replay, host_label, std::mem::take(&mut chunk), &mut runs).await;
                }
            }
            offset = page.next;
            if page.eof {
                break;
            }
        }
    }
    if cancelled || q.is_cancelled(sid) {
        // 补偿（见 doc comment）；丢弃未 flush 的 chunk。只对远端补（[`compensates_on_cancel`]）。
        // 补的是那条会话**此刻**的终局（成品缓存里后端说过的 · 连接断了 ⇒ 说不清），不再由 monitor 恒判「已结束」。
        if compensates_on_cancel(&origin) {
            let again = crate::session_book::book()
                .read()
                .settle_again(origin.as_wire_str(), sid);
            for out in again {
                replay.on_lifecycle(out.origin(), out.frames());
            }
        }
        return Ok(FetchOutcome::Cancelled);
    }
    if !chunk.is_empty() {
        flush_lines(replay, host_label, chunk, &mut runs).await;
    }
    // 完整性校验：`total` 精确对账（F30）—— 续传时对的是「锚之后那一截」。
    let (arrived, want) = (walk.arrived(), walk.want());
    if arrived != want {
        return Err(copy_text(
            "rsSshSource.snapshot.incomplete",
            &[
                ("arrived", &arrived.to_string()),
                ("want", &want.to_string()),
            ],
        ));
    }
    // 下界：宣告时 prime 的行数 L（`session_added.lines`）—— 文件在宣告之后被截短才会撞上。
    if let Some(expected) = item.expected_lines {
        if plan.total < expected {
            return Err(copy_text(
                "rsSshSource.snapshot.incompletePlan",
                &[
                    ("total", &plan.total.to_string()),
                    ("expected", &expected.to_string()),
                ],
            ));
        }
    }
    // `[0, total)` 全到了（整份：刚发完；续传：锚之前的早有、之后的刚发完）⇒ 立锚。
    crate::snapshot_resume::note_snapshot_done(&origin, sid, path, &plan);
    crate::snapshot_resume::note_witness(&origin, sid, pick.done());
    Ok(FetchOutcome::Done(arrived))
}

/// 快照中途被撤时要不要补一个 `ended` 格：**远端补、本机不补。**
///
/// 补偿治的是「已 flush 的那一块把刚归档的 tab 见行复活」—— 而**只有远端的行会复活 tab**
/// （前端 `tabs.ts` 只有 `remote-line` 那一格；本机 tab 的活与死只由 PID 探活那一路翻）。
/// 本机再补一个 `ended` 格，没有要治的病，反倒会把本机那边刚判成「可重连」的会话压成「已结束」。
pub(crate) fn compensates_on_cancel(origin: &crate::origin::Origin) -> bool {
    !origin.is_local()
}

/// 两段编号映射（纯函数，与测试共用——审计 D：原测试在测试体内重实现映射，
/// 锤不到生产代码）：到达序 → 行号。前 total-tail_from 行是尾段（最新），
/// 其余是头段回填。调用方保证 tail_from <= total（`frame_query::tail` 校验）。
pub(crate) fn tail_seq(arrived: u64, total: u64, tail_from: u64) -> u64 {
    let seg1 = total.saturating_sub(tail_from);
    if arrived < seg1 {
        tail_from + arrived
    } else {
        arrived - seg1
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/snapshot_tail_tests.rs"]
mod snapshot_tail_tests;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/snapshot_tests.rs"]
mod snapshot_tests;
