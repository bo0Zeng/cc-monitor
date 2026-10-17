//! 旁路快照：排队、分发、拉取，以及撤销与两段编号。

use super::*;
use crate::copy_table::copy_text;
use crate::event_replay::EventReplay;
use std::sync::Arc;

//
// === 旁路快照拉取（每管道一个对话，完就断） ===
//
// 后端不重放历史；每个已宣告会话的**尾段**（最后 `SNAPSHOT_TAIL_LINES` 个可计行）由这里经已开着的长连接取回，按行号编 seq 灌进与 tail 行
// 完全相同的管线（flush_lines → on_line_batch_awaited）。两路 seq 同处行号空间：重叠区是精确重复的 (sid,seq)，被前端既有去重吸收。
// 尾段之下的头段不预拉：界面往上翻时按行号 / 按偏移取回（`live-window.ts` 的 `BelowState`）。
// 并发 ≤SNAPSHOT_CONCURRENCY（不抢 tail 通道带宽）；priority sid 优先出队。

const SNAPSHOT_CONCURRENCY: usize = 2;
/// 单会话快照体量上限（防御：远端超巨文件不无界拉取；超限截断 warn——
/// 历史浏览器按需查询不受此限）。
const SNAPSHOT_MAX_BYTES: u64 = 512 * 1024 * 1024;
const SNAPSHOT_CHUNK_LINES: usize = 500;
/// 快照只取最后这么多个可计行：**就是壳留存每会话留的那么多**（`event_replay::REPLAY_TAIL_KEEP`，一处口径）。
///
/// 为什么不对齐界面账本（`live-window.ts` 的 `PENDING_KEEP` 2000）：多拉的那 1400 行进了留存当场被修回 600（F5 那一屏看不到），
/// 只在 tab 账本里多占着，往上翻七八次之后才用得上 —— 而往上翻本来就按行号一批 200 行取回（一个往返）。
/// 600 正是界面开 tab 一次最多物化的条数（`materializeUntilFilled`，等式由 `replay-tail-keep.vitest.ts` 钉着），首屏不用等取回。
/// 沙箱读数（6 个活会话、原文 32 MB）：600 行 3.4 MB / 15 个往返；2000 行 7.8 MB / 22 个往返；原先整份 16.5 MB / 42 个往返。
const SNAPSHOT_TAIL_LINES: u64 = crate::event_replay::REPLAY_TAIL_KEEP as u64;

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

/// 这条连接的收口（`LineIntake`）被丢掉（断了 · 收工，任何路径）时关队列。
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

    pub(super) fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// 出队：priority sid（若在队中）优先，否则 FIFO。**closed 即 None**（未开
    /// 拉的排队项作废，重连重拉）。
    ///
    /// 丢失唤醒防护（审计 D-I1）：`notify_waiters` 不给未注册者存 permit——
    /// 必须**先注册**（`enable`）再检查状态，close/push 发生在注册后必被捕获、
    /// 发生在注册前则状态检查看得到。
    pub(super) async fn pop(&self, priority: Option<String>) -> Option<SnapshotItem> {
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
        // 没在看的会话不拉（它的行也不上流）：进名单那一刻由 `stream-watch` 的 `from[]` 叫界面自己补。
        let origin = crate::origin::Origin(host_label.clone());
        if !replay.watched(&origin).contains(&item.sid) {
            tracing::debug!("snapshot [{host_label}] {}: 没在看，不拉", item.sid);
            drop(permit);
            continue;
        }
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
    // 持锁交：保证到达序 == 计数变化序（会话流里的一格，没 credit 也照交、不丢）。
    replay.on_snapshot_inflight(*n as u32);
    drop(n);
}

// F5 电平同步（审计 D）：inflight 是变化沿，重载后前端初值 0 ⇒ 就绪点在重放最前面补一格当前电平 ——
// 电平由 `EventReplay::on_snapshot_inflight` 在会话流自己的锁里记一份，重放读那一份（`event_replay::lifecycle_replay`），
// **不回头读本锁**：本锁持着交电平（本锁 → 会话流的锁），反过来那一向会两条线程互等。

/// fetch 的三态结果：完成（行数）/ 被取消（不重试）。错误走 Err。
enum FetchOutcome {
    Done(u64),
    Cancelled,
}

// 快照只含可计行：口径只住后端 `observe::lines::counts`。

/// 拉取单个会话的尾段快照（手上有那台上一次给的 `held` 就交回去、只续读断线期间新到的那一截）并灌进既有管线。
///
/// 走已有长连接：一问 `history-tail`（交 `held`）拿回那张图 · 怎么接（后端判：续传 · 尾段 · 变短 / 被改写之后的尾段 · 缺得多不读）·
/// 第一页正文；一页装不下的照 `next` 用 `history-read` 分页接着拉到 `end`。头段 `[0, split_at)` 不读，界面往上翻时按行号 / 按偏移取回。
///
/// 每个 chunk 边界查取消（会话 removed / 连接断）——中止并**补偿 emit 一次
/// ended 格**：若某个已 flush 的 chunk 恰把归档 tab"见行复活"，这里把它
/// 压回 archived（审计 D-B1 僵尸复活的封口；archiveTab 幂等，重复无害）。
///
/// 完整性校验（审计 D-I2）：到达的可计行数必须**恰好等于** `total - from`。
async fn fetch_snapshot(
    q: &std::sync::Arc<SnapshotQueue>,
    item: &SnapshotItem,
    host_label: &str,
    replay: &Arc<EventReplay>,
) -> Result<FetchOutcome, String> {
    use crate::frame_query;
    use cmd_wire::session::TailHow;
    let sid = &item.sid;
    let path = &item.path;
    let origin = crate::origin::Origin(host_label.to_string());
    let held = crate::snapshot_resume::cursor_of(&origin, sid);
    // 快照是两件事、各一个期限：先问（一问，`PAGE_BUDGET`）；剩下的正文（分页）在知道还差多少字节之后再造、按大小给。
    let tail = frame_query::tail(
        &origin,
        path,
        SNAPSHOT_TAIL_LINES,
        held.as_ref(),
        frame_query::Deadline::within(frame_query::PAGE_BUDGET),
    )
    .await?;
    let plan = tail.plan;
    // 断线期间文件变短了 / 被改写了 ⇒ 这一次读出来的是另一代的行号：先交那个会话一格「已从头重读」（前端据它整份重来、留存丢旧的一代），再读尾段。
    let changed = match tail.how {
        TailHow::Truncated => Some(FileChange::Truncated),
        TailHow::Rewritten => Some(FileChange::Rewritten),
        TailHow::Tail | TailHow::Resume | TailHow::Handoff => None,
    };
    if let Some(change) = changed {
        tracing::warn!(
            "snapshot [{host_label}] {sid}: 记录文件在断线期间变了（{}），从尾段重读",
            change.as_wire()
        );
        replay
            .on_session_notice(crate::ui_contract::SessionFileNoticePayload {
                session_id: sid.to_string(),
                origin: host_label.to_string(),
                path: path.to_string(),
                change: change.as_wire().to_string(),
            })
            .await;
    }
    match tail.how {
        TailHow::Resume => tracing::info!(
            "snapshot [{host_label}] {sid}: 续传 —— 从第 {} 行读到第 {} 行",
            tail.from,
            plan.total
        ),
        // 缺得比留存还多 ⇒ 断线那一截不读：交会话流一格「从第 `from` 行起接着来」（界面那一路自己取最后那一截）。
        TailHow::Handoff => {
            tracing::info!(
                "snapshot [{host_label}] {sid}: 断线期间缺得比留存多 —— 不读那一截，从第 {} 行起接着来（界面自己取最后那一截）",
                tail.from
            );
            replay.on_watch_from(
                &origin,
                vec![cmd_wire::watch::WatchFrom {
                    sid: sid.to_string(),
                    path: path.to_string(),
                    seq: tail.from,
                }],
            );
        }
        TailHow::Tail | TailHow::Truncated | TailHow::Rewritten => {}
    }
    let mut seq = tail.from;
    let mut total_bytes: u64 = 0;
    let mut chunk: Vec<JsonlLine> = Vec::with_capacity(SNAPSHOT_CHUNK_LINES);
    let mut runs = crate::SkipRuns::default(); // 这一次快照自己一份（与实时那一路不交错）
    let mut cancelled = false;
    let mut page = tail.page;
    let body = frame_query::Deadline::within(frame_query::read_budget(
        plan.end.saturating_sub(page.next).min(SNAPSHOT_MAX_BYTES),
    ));
    'read: loop {
        // 后端只交可计行、每行带成品（进不进界面 · `cwd` · 末端都是它给的）；这里只编号、攒批。
        for row in std::mem::take(&mut page.rows) {
            chunk.push(JsonlLine {
                session_id: sid.to_string(),
                path: std::path::PathBuf::from(path),
                seq,
                record: row.record,
                cwd: row.cwd,
                end: row.end,
                rid: None,
                catch_up: catch_up_at(item.expected_lines, seq),
            });
            seq += 1;
            if chunk.len() >= SNAPSHOT_CHUNK_LINES {
                if q.is_cancelled(sid) {
                    cancelled = true;
                    break 'read;
                }
                flush_lines(replay, host_label, std::mem::take(&mut chunk), &mut runs).await;
            }
        }
        if page.eof || page.next >= plan.end {
            break;
        }
        let offset = page.next;
        page = frame_query::read_page(&origin, path, offset, Some(plan.end), body).await?;
        total_bytes += page.next - offset;
        if total_bytes > SNAPSHOT_MAX_BYTES {
            // 防御上限：不再继续拉（完整性校验会把截断判为失败 → toast）。
            tracing::warn!(
                "snapshot [{host_label}] {sid}: 超过 {SNAPSHOT_MAX_BYTES} 字节上限，截断"
            );
            break;
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
                replay.on_lifecycle(out.origin(), vec![out.frame()]);
            }
        }
        return Ok(FetchOutcome::Cancelled);
    }
    if !chunk.is_empty() {
        flush_lines(replay, host_label, chunk, &mut runs).await;
    }
    // 完整性校验：精确对账（F30）—— `[from, total)` 每一行都到了。
    let (arrived, want) = (seq - tail.from, plan.total - tail.from);
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
    // `[from, total)` 全到了 ⇒ 存后端给的那一份（下一次原样交回）。尾段之下的前端按需取回。
    crate::snapshot_resume::note_done(&origin, sid, tail.held);
    Ok(FetchOutcome::Done(arrived))
}

/// 这个会话的第 `seq` 行是不是补历史：宣告那一刻那台说的行数 `announced`（`session_added.lines`）以前的是；
/// 快照读到它以后的是宣告之后才写进来的（实时那条流也交它，谁先到算谁）⇒ 不是。行数不知道 ⇒ 快照里的都算补历史。
pub(crate) fn catch_up_at(announced: Option<u64>, seq: u64) -> bool {
    announced.is_none_or(|l| seq < l)
}

/// 快照中途被撤时要不要补一个 `ended` 格：**远端补、本机不补。**
///
/// 补偿治的是「已 flush 的那一块把刚归档的 tab 见行复活」—— 而**只有远端的行会复活 tab**
/// （前端 `tabs.ts` 只有 `remote-line` 那一格；本机 tab 的活与死只由 PID 探活那一路翻）。
/// 本机再补一个 `ended` 格，没有要治的病，反倒会把本机那边刚判成「可重连」的会话压成「已结束」。
pub(crate) fn compensates_on_cancel(origin: &crate::origin::Origin) -> bool {
    !origin.is_local()
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/snapshot_tests.rs"]
mod snapshot_tests;
