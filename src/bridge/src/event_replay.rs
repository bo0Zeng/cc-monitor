//! 事件持久化重播：解决前端 F5 刷新后状态丢失的问题。〔CF2〕也是会话内容流的「句柄」那一侧：
//! 主界面经通道 `subscribe` 订的那条流，由本文件按 credit 交格（见下「订阅」）。
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
//! 〔CF2 · 第四波 4B〕**一档**：每个会话只留 seq 最高的 [`REPLAY_TAIL_KEEP`] 条（修剪有 [`TRIM_SLACK`] 的
//! 摊还余量 ⇒ 单会话上界 `KEEP + SLACK`）。丢掉的正文前端要得回来，两条路：
//!
//! | tab | 丢掉的正文从哪回来 |
//! |---|---|
//! | 接上了骨架 | 骨架滚到那里时按**字节**取（`read_session_range`，边界取自索引） |
//! | 没接骨架（后台 tab · 老后端 · seq 对不上的） | 往上翻过了账本最老那一条时按**行号**取（`read_session_lines`，后端 `history-lines`） |
//!
//! 原来这里是**两档**（〔U3b〕只有前端调过 `keep_tail_only`〔散文墓碑〕的会话才修剪，其余「无处可回 ⇒ 不许丢」，
//! 第二档不设上限 —— `设计/05 §3.3.4` 的级 3）。按行号取回之后「无处可回」不存在了，分档随之取消
//! （`调研/第四波记录/CF2.md §2`）。
//!
//! ⚠ **仍然没有上界的那一维是会话数**（`CF2.md §2.2`）：单会话 ≤ `KEEP + SLACK` 条，缓冲里有几个会话
//! 由「宣告过多少个 × 前端关没关（[`EventReplay::forget`]）× monitor 重启」决定。
//! 读数（长度 / 会话数 / 修剪次数）见 [`EventReplay::stats`] 与每次修剪的 `[replay]` 日志行。
//!
//! ## 订阅（〔CF2 · 第四波 4B〕`设计/05 §8` 步 6「流那半收口成 `subscribe`」）
//!
//! 会话内容到前端**不再是两个 Tauri 广播事件**（`jsonl-line` / `jsonl-batch`，已退役）：前端经通道说
//! `chan.subscribe(origin, kind, from, want)`（`src/ipc/chan.ts` → `chan/webview.rs::chan_subscribe`），
//! 本文件是那条流的**句柄**那一侧 —— 它认识会话，通道那两半不认识。
//!
//! | `kind` | 交什么 | 什么时候交留存（重放） |
//! |---|---|---|
//! | `session-lines` | `origin` 那台机器的全部会话 | **就绪点**（主界面的 `frontend-ready`）：宣告重发之后、对账之前 —— 顺序与原来 `replay_and_mark_ready`〔散文墓碑〕一致 |
//! | `session-lines/<sid>` | 只那一个会话（独立窗口） | 订阅当场 |
//! | `session-tap` | 〔TAP〕那台机器上中转抄出来的 SSE 事件（见 [`SESSION_TAP_KIND`]） | 没有留存（不重放）；订阅当场就收实时的 |
//! | `accounts-changed` | 〔DL1〕那台机器上「账号清单可能变了」：`Seen`（长连接又通了、能问了）· `Frame`（那台后端说账号清单变了）· `Unseen` · `Gap` | 没有留存（不重放）；订阅当场就收实时的 |
//!
//! 〔DL1 · `设计/01 §2.2`「前端只有两个动作」〕`accounts-changed` 顶掉的是最后一个裸 Tauri 事件 `remote-backend-ready`
//! （原常量 `REMOTE_BACKEND_READY`〔散文墓碑〕，住 `bridge.rs` 的 `events`）。它与 `session-lines` 住同一张订阅表，因为「那台看不看得见」
//! 只有一个家（下面的 `seen`）—— 另起一个句柄就得再养一份同样的表、在 `ssh_source` 同样的几处再喂一遍。
//!
//! 一格 = 一行（[`crate::bridge::SessionStreamFrame`]：`{"line": …}` 或成批那一段的边界 `{"batch": …}`）；
//! `Item::Frame.seq` 是这条订阅里的**位置**（0, 1, 2 …，连续），不是行号。
//!
//! 🔴 **credit 与「不许晚到」**（`调研/第四波记录/CF2.md §3.3`）：
//! - **实时那一份**（[`EventReplay::on_line_batch_awaited`]）：有 credit **当场**交（与原来同一个时刻 emit ⇒
//!   与其后的 `session-ended` 等起停事件的先后不变，issue #20）；没 credit 就**丢**、位置照占，
//!   下一次交出去之前原位先给 `Item::Gap`（`05 §3.3.4` 级 2）。**绝不攒着等 credit** —— 攒着的行会晚于
//!   其间发出的 `session-ended`（僵尸 tab）；也**绝不让管线等** —— 一个不给 credit 的窗口会卡住那台机器的整条流。
//! - **重放那一份**（就绪点 / 独立窗口开窗）按 credit **等**：它不在起停事件的顺序里（对账由同一个任务在它交完之后发）。
//! - `Gap` / `Unseen` / `Seen` 不占 credit；`Frame`（含两种边界）每格占一个。
//!
//! 丢了什么由前端自己补（「判可恢复归上层」）：它收到 `Gap` 就把那台机器上各 tab 的账本当成不可信、
//! 按行号往回取（`history-lines`）。

use crate::bridge::{BatchEdge, JsonlLinePayload, SessionStreamFrame};
use crate::chan::wire::{Body, By, Cursor, HopFault, HopId, Item};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// 〔CF2〕订阅流的出口：把一串格交给某个 webview 上的某条订阅。
///
/// 生产那一份是 `chan/webview.rs::WebviewSink`（`emit_to` 那个 webview 的 Tauri 事件 —— 与起停事件同一条
/// 投递队列，先后不乱，理由见 `CF2.md §3.2`）；判据用一个记录器替它。**调用时本文件不持锁。**
pub trait ItemSink: Send + Sync {
    fn deliver(&self, label: &str, sub: u64, items: Vec<Item>);
}

/// 〔CF2〕本文件认的流标签：一台机器的全部会话 / （带 `/<sid>`）只一个会话。
pub const SESSION_LINES_KIND: &str = "session-lines";

/// 〔TAP · V124〕本文件认的第二种流：一台机器上中转抄出来的 SSE 事件（体是 [`crate::bridge::SessionTapPayload`]）。
///
/// 与 `session-lines` 同一张订阅表、同一套 credit 与 `Gap`（`设计/05 §15`：一条帧路 ＋ `subscribe`），差别只有三处：
/// ① **没有留存**：tap 不进 `history`，订阅当场就是实时的（没有就绪点、没有重放）；
/// ② 只有整台机器那一形（前端按 `stream` 自己对 tab）；
/// ③ 实时那一份照「有 credit 当场交、没 credit 丢、位置照占、原位 `Gap`」（级 2）—— tap 本来就可丢（V24：SSE 只保快）。
pub const SESSION_TAP_KIND: &str = "session-tap";

/// 〔DL1〕本文件认的另一个流标签：那台机器上「账号清单可能变了」（见头注那张表）。
/// TS 那一侧的同一个串住 `src/session-accounts-poll.ts::ACCOUNTS_CHANGED_KIND`（两侧对拍在 `session-accounts-poll.vitest.ts`）。
pub const ACCOUNTS_CHANGED_KIND: &str = "accounts-changed";

/// 〔DL1〕`accounts-changed` 流里那一格 `Frame` 的体（不透明于通道；前端只认「来了一格」，体给日志看）。
const ACCOUNTS_CHANGED_BODY: &[u8] = br#"{"accounts_changed":true}"#;

pub struct EventReplay {
    inner: Mutex<Inner>,
    /// 〔MIG-1〕会话成品缓存（生产 = 进程里那一本 `session_book::book()`；判据各给一本自己的，不与并行的判据串味）。
    book: &'static parking_lot::RwLock<crate::session_book::Book>,
    /// 〔CF2〕有订阅拿到了 credit（或被撤了）—— 等 credit 的重放在这上面醒。
    credit_changed: tokio::sync::Notify,
}

struct Inner {
    history: VecDeque<JsonlLinePayload>,
    /// Batch8-F26：frontend-ready 携带的"用户上次所在 tab"（F19 语义）。存下来
    /// 供远端快照拉取排队（当前 tab 的会话先拉）；None = 无记忆/未就绪。
    priority_sid: Option<String>,
    /// 〔CF2〕每个会话此刻在 `history` 里有几条 ＋ 它最低留存的 seq（修剪过之后才有；
    /// 之后到达、seq 低于它的行不进缓冲 —— 见 [`push_and_trim`]）。
    sessions: HashMap<String, Held>,
    /// 〔U3b〕累计修剪掉的条数（读数口，[`EventReplay::stats`]）。
    trimmed_total: u64,
    /// 〔CF2〕订阅（按 `(webview, 编号)` 认）。
    subs: Vec<Sub>,
    /// 〔CF2〕订阅的代号：同一个 `(webview, 编号)` 被重订（页面重载）时新旧两份分得开。
    generation: u64,
    /// 〔CF2〕哪些 webview 已经过了就绪点（之后它再订整台机器，当场交留存）。
    ready_labels: HashSet<String>,
    /// 〔CF2〕每台机器的内容流此刻接没接着（`Unseen` / `Seen` 的来源）。没有的 = 没接着。
    seen: HashMap<String, bool>,
    /// 〔CF2〕出口（`lib.rs` 起步时装；没装之前一格都不交）。
    sink: Option<Arc<dyn ItemSink>>,
}

/// 〔CF2〕一条订阅。
struct Sub {
    label: String,
    id: u64,
    generation: u64,
    /// 订的是哪台机器（线上串：本机 `<local>`）。
    origin: String,
    /// 订的是哪一种流（〔TAP〕与〔DL1〕各加了一种；只有 `Lines` 收会话行、有留存）。
    kind: SubKind,
    /// `session-lines/<sid>` 那一形：只要这一个会话。
    only: Option<String>,
    /// 还能交几格 `Frame`（`want` 累加）。
    credit: u64,
    /// 下一格的位置。
    next: u64,
    /// 丢了还没说的那一段从哪个位置起（下一次交出去之前原位给 `Gap`）。
    gap_from: Option<u64>,
    /// 过了就绪点（实时的行交给它）。
    live: bool,
    /// 最后一次告诉它的「那台看不看得见」。
    told_seen: bool,
}

/// 〔合并 DL1 × TAP〕一条订阅订的是哪一种流 —— 两路各加了一个布尔（`tap` / `lines`），合并时收成这一个枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubKind {
    /// `session-lines` 一族：收会话行、有留存（就绪点重放）。
    Lines,
    /// 〔TAP · V124〕`session-tap`：中转抄出来的 SSE 事件，没有留存。
    Tap,
    /// 〔DL1〕`accounts-changed`：只收看得见 / 看不见与「那台账号清单变了」那一格，没有留存。
    AccountsChanged,
}

impl Sub {
    fn wants(&self, p: &JsonlLinePayload) -> bool {
        let origin = p.origin.as_deref().unwrap_or(crate::origin::LOCAL);
        self.kind == SubKind::Lines
            && origin == self.origin
            && self.only.as_deref().is_none_or(|s| s == p.session_id)
    }
}

/// 〔CF2〕一个会话在缓冲里的账：几条 ＋ 修剪过的话最低留存的 seq。
#[derive(Debug, Default, Clone, Copy)]
struct Held {
    count: usize,
    /// 修剪之后留下的最低 seq；`None` = 从没修剪过。
    floor: Option<u64>,
}

/// 〔U3b · `设计/10` 步 8〕→〔CF2〕**每个会话在 history 里只留尾巴这么多条可显示记录。**
///
/// # 依据（量出来的，不是拍的）
///
/// ① **前端开一个 tab 时最多建多少条不用滚动**：`tabs.ts` 的 `materializeUntilFilled` 是
///    `MATERIALIZE_TAIL_K`（150）× 最多 4 轮 = **600**。尾巴少于它，F5 之后那一屏就要等
///    按偏移取正文（多一次 IPC ＋ 一次后端进程）；多于它，多出来的那段首屏根本用不上。
///    ⇒ 取 600。这条等式由 `tests/replay-tail-keep.vitest.ts` 对着两边源码钉着（改一边会红）。
/// ② **它够不够一屏**（2026-09-24，gpd 本机 39 份会话，按骨架第一级粗估、1080 px 视口）：
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
/// 攒 `KEEP/4` 条修一次，摊到每行是 O(history)/150。**代价**：单会话上界是 750 条不是 600。
pub const TRIM_SLACK: usize = REPLAY_TAIL_KEEP / 4;

/// 〔U3b〕读数：`history` 总长 · 缓冲里的会话数 · 累计修剪条数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayStats {
    pub history_len: usize,
    /// 〔CF2〕缓冲里有几个会话（原 `tail_only_sessions`〔散文墓碑〕：只数接了骨架的那些；分档取消之后数全部）。
    pub sessions: usize,
    pub trimmed_total: u64,
}

/// 切块（v2.3.1 issue #1 启动加速 + P5.4 B 重构简化）：成批的那一段按 CHUNK_SIZE 行一块交，**末块先发**
/// （最新一段）；块与块之间停 CHUNK_PAUSE_MS。〔CF2〕一块 = 一次投递（一个 Tauri 事件里一串格）。
///
/// P5.4：不再区分 head / mid —— 前端 RecordTimeline 按 seq 自动排到正确位置，
/// 块内顺序对 DOM 无影响。chunks[0] = 最新一段，chunks[N-1] = 最老一段。
const CHUNK_SIZE: usize = 600;
/// chunk 之间停顿，让 IPC 派发线程喘息 + 实时的新行在缝隙间交出去。
const CHUNK_PAUSE_MS: u64 = 10;

/// v2.4.2 issue #2: 一次攒出来的批 >= 此值时（典型场景：用户 `claude --resume <sid>` 灌历史），
/// 这批按「成批」交（带 `batch` 边界、切块）；否则（用户日常敲键 1-N 行）逐行交保持 live 语义。
///
/// 经验值 50：日常增量绝对低于这个数（claude 流式回复一行一条 jsonl 也只有
/// 几条到十几条）；/resume 历史灌入轻松几百几千行。50 是清晰的分水岭。
const INCREMENTAL_BATCH_THRESHOLD: usize = 50;

/// 〔CF2〕一格的体（序列化失败 ⇒ 空体：两端契约的另一侧会按「解不出」处置，不猜）。
fn body_of(f: &SessionStreamFrame) -> Body {
    Body(serde_json::to_vec(f).unwrap_or_default())
}

/// 〔CF2〕成批那一段的若干块（末块先发），每块首尾加边界：第一块以 `start` 开头，每块以 `end` 收尾
/// （与原来「第 0 块触发进批、每块末尾排一次出批」同一个节奏）。
fn batch_chunks(chunks: Vec<Vec<JsonlLinePayload>>) -> Vec<Vec<Body>> {
    let start = body_of(&SessionStreamFrame::Batch(BatchEdge::Start));
    let end = body_of(&SessionStreamFrame::Batch(BatchEdge::End));
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let mut v = Vec::with_capacity(c.len() + 2);
            if i == 0 {
                v.push(start.clone());
            }
            v.extend(c.into_iter().map(|p| body_of(&SessionStreamFrame::Line(p))));
            v.push(end.clone());
            v
        })
        .collect()
}

/// 〔CF2〕**实时那一份**交给一条订阅的计划（纯函数）：有多少 credit 交多少，其余丢掉、位置照占；
/// 这一次交出去的第一格之前，若有没说的丢失 ⇒ 原位先给 `Gap`。返回要交的格（可能为空）。
fn plan_live(sub: &mut Sub, frames: Vec<Body>) -> Vec<Item> {
    let n = frames.len() as u64;
    let can = sub.credit.min(n);
    let mut out = Vec::new();
    if can > 0 {
        if let Some(from_seq) = sub.gap_from.take() {
            out.push(Item::Gap {
                from_seq,
                to_seq: Some(sub.next),
            });
        }
    }
    for body in frames.into_iter().take(can as usize) {
        out.push(Item::Frame {
            seq: sub.next,
            body,
        });
        sub.next += 1;
    }
    sub.credit -= can;
    if can < n {
        sub.gap_from.get_or_insert(sub.next);
        sub.next += n - can;
    }
    out
}

/// 〔CF2〕**重放那一份**的一步（纯函数）：只交手里 credit 够的那几格、**不丢**；返回 `(要交的格, 用掉了几格)`。
/// credit 为零 ⇒ `(空, 0)`，调用方去等。
fn plan_replay(sub: &mut Sub, rest: &[Body]) -> (Vec<Item>, usize) {
    let take = (sub.credit.min(rest.len() as u64)) as usize;
    if take == 0 {
        return (Vec::new(), 0);
    }
    let mut out = Vec::with_capacity(take + 1);
    if let Some(from_seq) = sub.gap_from.take() {
        out.push(Item::Gap {
            from_seq,
            to_seq: Some(sub.next),
        });
    }
    for body in &rest[..take] {
        out.push(Item::Frame {
            seq: sub.next,
            body: body.clone(),
        });
        sub.next += 1;
    }
    sub.credit -= take as u64;
    (out, take)
}

/// 〔MIG-1 · `99 §2.1 ⑬` 登记的例外〕**起停那几格**交给一条订阅的计划（纯函数）：不看 credit、**不丢**、照占位置；
/// 手里有没说的丢失 ⇒ 原位先给 `Gap`（位置号照样连得上）。只许交 [`SessionStreamFrame::takes_credit`] 为假的那几种。
fn plan_lifecycle(sub: &mut Sub, frames: &[Body]) -> Vec<Item> {
    let mut out = Vec::with_capacity(frames.len() + 1);
    if frames.is_empty() {
        return out;
    }
    if let Some(from_seq) = sub.gap_from.take() {
        out.push(Item::Gap {
            from_seq,
            to_seq: Some(sub.next),
        });
    }
    for body in frames {
        out.push(Item::Frame {
            seq: sub.next,
            body: body.clone(),
        });
        sub.next += 1;
    }
    out
}

/// 〔MIG-1〕一条订阅（整台 / 一个会话）的起停重放：骨架在行前（`before`）、终局在行后（`after`）—— 计划住 `session_book::Book::replay`，
/// 这里只按订阅的那台 / 那一个会话挑、换成格。`history` = 留存里这条订阅要的那些行。
fn lifecycle_replay(
    book: &parking_lot::RwLock<crate::session_book::Book>,
    origin: &str,
    only: Option<&str>,
    history: &[JsonlLinePayload],
) -> (Vec<Body>, Vec<Body>) {
    let mut buffered: Vec<(String, String)> = Vec::new();
    for p in history {
        let pair = (p.session_id.clone(), origin.to_string());
        if !buffered.contains(&pair) {
            buffered.push(pair);
        }
    }
    let plan = book.read().replay(&buffered);
    let pick = |outs: Vec<crate::session_book::Out>| -> Vec<Body> {
        outs.into_iter()
            .filter(|o| o.origin() == origin)
            .flat_map(|o| o.frames())
            .filter(|f| match (only, f.session_id()) {
                (None, _) => true,
                (Some(want), Some(sid)) => want == sid,
                (Some(_), None) => false,
            })
            .map(|f| body_of(&f))
            .collect()
    };
    // 旁路快照在途的电平：只在有在途时补一格（前端初值就是 0）。
    let mut before = Vec::new();
    let level = crate::ssh_source::snapshot_inflight_level();
    if level > 0 && only.is_none() {
        before.push(body_of(&SessionStreamFrame::SnapshotInflight(
            crate::bridge::SnapshotInflightPayload { count: level },
        )));
    }
    before.extend(pick(plan.before));
    (before, pick(plan.after))
}

/// 〔CF2〕「那台机器看得见 / 看不见」换成流里的一格（不占 credit）。
fn seen_item(seen: bool, opening: bool) -> Item {
    if seen {
        Item::Seen { from: None }
    } else {
        Item::Unseen {
            at: HopId {
                idx: 1,
                tag: if opening { "open" } else { "read" },
            },
            why: if opening {
                HopFault::Unreachable
            } else {
                HopFault::Dropped
            },
        }
    }
}

/// 〔CF2〕对端（本文件这个句柄）原位说「不行」：`Closed{Peer({"code","message"})}` —— 与后端命令的拒绝同一个信封。
fn refused(code: &str, message: String) -> Item {
    let body = serde_json::to_vec(&serde_json::json!({ "code": code, "message": message }))
        .unwrap_or_default();
    Item::Closed {
        by: By::Peer(Body(body)),
    }
}

/// 〔CF2〕一个流标签说的是哪一种流。
#[derive(Debug, PartialEq, Eq)]
enum Stream {
    /// `session-lines`：`None` = 整台机器 / `Some(sid)` = 一个会话。
    Lines(Option<String>),
    /// 〔TAP〕`session-tap`。
    Tap,
    /// 〔DL1〕`accounts-changed`。
    AccountsChanged,
}

/// 〔CF2〕`kind` ⇒ 哪一种流（〔TAP〕`session-tap` ⇒ [`Stream::Tap`]；〔DL1〕`accounts-changed` ⇒ [`Stream::AccountsChanged`]）。认不出 ⇒ `Err`。
fn parse_kind(kind: &str) -> Result<Stream, ()> {
    if kind == SESSION_LINES_KIND {
        return Ok(Stream::Lines(None));
    }
    if kind == SESSION_TAP_KIND {
        return Ok(Stream::Tap);
    }
    if kind == ACCOUNTS_CHANGED_KIND {
        return Ok(Stream::AccountsChanged);
    }
    match kind
        .strip_prefix(SESSION_LINES_KIND)
        .and_then(|r| r.strip_prefix('/'))
    {
        Some(sid) if !sid.is_empty() => Ok(Stream::Lines(Some(sid.to_string()))),
        _ => Err(()),
    }
}

impl EventReplay {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                history: VecDeque::new(),
                priority_sid: None,
                sessions: HashMap::new(),
                trimmed_total: 0,
                subs: Vec::new(),
                generation: 0,
                ready_labels: HashSet::new(),
                seen: HashMap::new(),
                sink: None,
            }),
            credit_changed: tokio::sync::Notify::new(),
            book: crate::session_book::book(),
        }
    }

    /// 〔CF2〕装出口（`lib.rs` 起步时一次）。
    pub fn attach_sink(&self, sink: Arc<dyn ItemSink>) {
        self.inner.lock().sink = Some(sink);
    }

    /// 行进重放缓冲的**唯一**入口：先进 `history`，再**当场**交给每一条已过就绪点、订了它的订阅 ——
    /// 小批（< [`INCREMENTAL_BATCH_THRESHOLD`]）逐行交，大批切块、带 `batch` 边界。
    /// 大 batch 的块序列**在调用方任务内交完才返回**（Batch5-F17 审计 R1）——`ssh_source` 的攒批 flush
    /// 用它，保证行先于随后的 SessionRemoved/断连归档交出去（issue #20 / FIX 2 的顺序契约），
    /// 同时对后端帧流形成天然背压（交的期间不再收帧）。
    ///
    /// 〔CF2〕没 credit 的订阅：丢、位置照占、下一次交之前原位给 `Gap`（头注「订阅」）。**不等 credit。**
    ///
    /// 〔CF1 · 2026-09-24〕原来还有一份不 await、把块序列 spawn 出去的孪生（只供本机 watcher 那条
    /// std 线程用，`真相源/10 §7.2`「五段逻辑字面重复」）。本机内容改走后端的帧之后它零调用方，删了；
    /// 名字里的 `_awaited` 留着是为了不在十几路同时改的时候改一个到处被点名的符号。
    /// 〔FW1 · 第四波 4D · D-d〕一个会话的记录文件不见了 / 被改过已从头重读：交给订了那台（或那一个会话）的实时订阅一格。
    ///
    /// 与行同一套记账（占 credit、占位置、没 credit 就原位记 `Gap`）；**不进留存**（F5 不重放这句话，主会话 09-25 认的已知缺口）。
    ///
    /// 〔RENDER2 · `设计/10 §3.2`〕「已从头重读」（截短 / 改写）⇒ 后端的行号从 0 重数：留存里这个会话旧的一代同一拍丢
    /// （F5 之后只重放新的一代，与前端收到这一格时整份重来对得上）。
    pub async fn on_session_notice(&self, notice: crate::bridge::SessionFileNoticePayload) {
        let body = body_of(&SessionStreamFrame::FileNotice(notice.clone()));
        let (sink, plans) = {
            let mut inner = self.inner.lock();
            if notice.change != "gone" {
                drop_session(&mut inner, &notice.session_id);
            }
            let Some(sink) = inner.sink.clone() else {
                return;
            };
            let mut plans: Vec<(String, u64, Vec<Item>)> = Vec::new();
            for sub in inner.subs.iter_mut().filter(|s| s.live) {
                if sub.origin != notice.origin
                    || sub.only.as_deref().is_some_and(|s| s != notice.session_id)
                {
                    continue;
                }
                let items = plan_live(sub, vec![body.clone()]);
                if !items.is_empty() {
                    plans.push((sub.label.clone(), sub.id, items));
                }
            }
            (sink, plans)
        };
        for (label, id, items) in plans {
            sink.deliver(&label, id, items);
        }
    }

    /// 〔RENDER2 · `99 §2.1` ㉓①〕这台机器的内容流上丢了一行、说不出丢在哪个会话（超长整行丢弃）：
    /// 订了这台 `session-lines` 的每条实时订阅原位收一格 `Gap { to_seq: None }`（不占位置、不占 credit）。
    pub fn on_lost_somewhere(&self, origin: &str) {
        let (sink, plans) = {
            let inner = self.inner.lock();
            let Some(sink) = inner.sink.clone() else {
                return;
            };
            let plans: Vec<(String, u64, Vec<Item>)> = inner
                .subs
                .iter()
                .filter(|s| s.live && s.kind == SubKind::Lines && s.origin == origin)
                .map(|s| {
                    let gap = Item::Gap {
                        from_seq: s.next,
                        to_seq: None,
                    };
                    (s.label.clone(), s.id, vec![gap])
                })
                .collect();
            (sink, plans)
        };
        for (label, id, items) in plans {
            sink.deliver(&label, id, items);
        }
    }

    pub async fn on_line_batch_awaited(&self, payloads: Vec<JsonlLinePayload>) {
        if payloads.is_empty() {
            return;
        }
        let bulk = payloads.len() >= INCREMENTAL_BATCH_THRESHOLD;
        let (sink, plans) = {
            let mut inner = self.inner.lock();
            push_and_trim(&mut inner, &payloads);
            let Some(sink) = inner.sink.clone() else {
                return;
            };
            let mut plans: Vec<(String, u64, Vec<Vec<Item>>)> = Vec::new();
            for sub in inner.subs.iter_mut().filter(|s| s.live) {
                let mine: Vec<JsonlLinePayload> =
                    payloads.iter().filter(|p| sub.wants(p)).cloned().collect();
                if mine.is_empty() {
                    continue;
                }
                let frames: Vec<Vec<Body>> = if bulk {
                    batch_chunks(build_chunks(&mine))
                } else {
                    vec![mine
                        .into_iter()
                        .map(|p| body_of(&SessionStreamFrame::Line(p)))
                        .collect()]
                };
                let chunks: Vec<Vec<Item>> = frames
                    .into_iter()
                    .map(|c| plan_live(sub, c))
                    .filter(|c| !c.is_empty())
                    .collect();
                if !chunks.is_empty() {
                    plans.push((sub.label.clone(), sub.id, chunks));
                }
            }
            (sink, plans)
        };
        let total: usize = plans.iter().map(|(_, _, c)| c.len()).sum();
        let mut sent = 0usize;
        for (label, id, chunks) in plans {
            for items in chunks {
                sink.deliver(&label, id, items);
                sent += 1;
                if sent < total {
                    tokio::time::sleep(std::time::Duration::from_millis(CHUNK_PAUSE_MS)).await;
                }
            }
        }
    }

    /// 〔CF2〕**就绪点**（主界面的 `frontend-ready` 那个任务里调，替掉原来的 `replay_and_mark_ready`〔散文墓碑〕）：
    /// 把登记了、还没过就绪点的订阅逐条按 credit 交它那台机器的留存，交完才返回。
    ///
    /// **顺序**（与原来同形）：调用方先重发宣告与容器（骨架 tab 先建）、再调本函数、再对账补发 `session-ended`
    /// —— 对账必须晚于重放（issue #19 / #20）。优先会话（上次所在 tab）的块在前（Batch5-F19）。
    ///
    /// 过就绪点的那一刻（拿快照的同一把锁里）订阅就收实时的行 —— 重放期间来的新行照样当场交，
    /// 前端按 seq 落位（P5.4：不需要 catch-up）。
    pub async fn ready_point(&self, priority_sid: Option<&str>) {
        let started = std::time::Instant::now();
        let (sink, jobs) = {
            let mut inner = self.inner.lock();
            inner.priority_sid = priority_sid.map(str::to_string);
            let Some(sink) = inner.sink.clone() else {
                return;
            };
            let Inner {
                history,
                subs,
                ready_labels,
                ..
            } = &mut *inner;
            let mut jobs: Vec<ReplayJob> = Vec::new();
            for sub in subs.iter_mut().filter(|s| !s.live) {
                sub.live = true;
                ready_labels.insert(sub.label.clone());
                let mine: Vec<JsonlLinePayload> =
                    history.iter().filter(|p| sub.wants(p)).cloned().collect();
                // 〔MIG-1〕会话行那一族才有起停（tap / 账号那两种没有）。
                let (before, after) = if sub.kind == SubKind::Lines {
                    lifecycle_replay(self.book, &sub.origin, sub.only.as_deref(), &mine)
                } else {
                    (Vec::new(), Vec::new())
                };
                jobs.push(ReplayJob {
                    label: sub.label.clone(),
                    id: sub.id,
                    generation: sub.generation,
                    lines: mine,
                    before,
                    after,
                });
            }
            (sink, jobs)
        };
        let n: usize = jobs.iter().map(|j| j.lines.len()).sum();
        for job in jobs {
            self.replay_into(&*sink, job, priority_sid).await;
        }
        tracing::info!(
            "[perf] ready point: replayed {n} lines in {}ms",
            started.elapsed().as_millis()
        );
    }

    /// 〔CF2〕把一份留存按 credit 交给一条订阅（不丢；credit 用完就等 `want`；订阅被撤 / 被重订就停）。
    async fn replay_into(&self, sink: &dyn ItemSink, job: ReplayJob, priority_sid: Option<&str>) {
        let ReplayJob {
            label,
            id,
            generation,
            lines: payloads,
            before,
            after,
        } = job;
        let label = label.as_str();
        // 〔MIG-1〕骨架（活会话 ＋ 灯 ＋ 容器）先于行，不吃 credit。
        if !self.deliver_lifecycle(sink, label, id, generation, &before) {
            return;
        }
        if !payloads.is_empty() {
            self.replay_lines(sink, label, id, generation, payloads, priority_sid)
                .await;
        }
        // 〔MIG-1〕终局（可重连 · 已结束 · 说不清 · 清单报完了）晚于行 —— 否则远端行把刚落定的 tab 翻活（issue #19 / #20）。
        self.deliver_lifecycle(sink, label, id, generation, &after);
    }

    /// 〔MIG-1〕按计划交一串起停格给一条订阅（不吃 credit、不丢）；订阅没了 ⇒ `false`。
    fn deliver_lifecycle(
        &self,
        sink: &dyn ItemSink,
        label: &str,
        id: u64,
        generation: u64,
        frames: &[Body],
    ) -> bool {
        let items = {
            let mut inner = self.inner.lock();
            let Some(sub) = inner
                .subs
                .iter_mut()
                .find(|s| s.label == label && s.id == id && s.generation == generation)
            else {
                return false;
            };
            plan_lifecycle(sub, frames)
        };
        if !items.is_empty() {
            sink.deliver(label, id, items);
        }
        true
    }

    async fn replay_lines(
        &self,
        sink: &dyn ItemSink,
        label: &str,
        id: u64,
        generation: u64,
        payloads: Vec<JsonlLinePayload>,
        priority_sid: Option<&str>,
    ) {
        let chunks = batch_chunks(build_priority_chunks(payloads, priority_sid));
        let total = chunks.len();
        for (ci, chunk) in chunks.into_iter().enumerate() {
            let mut rest: &[Body] = &chunk;
            while !rest.is_empty() {
                let notified = self.credit_changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let (items, took) = {
                    let mut inner = self.inner.lock();
                    let Some(sub) = inner
                        .subs
                        .iter_mut()
                        .find(|s| s.label == label && s.id == id && s.generation == generation)
                    else {
                        return; // 撤了 / 页面重载后被重订
                    };
                    plan_replay(sub, rest)
                };
                if took == 0 {
                    notified.await;
                    continue;
                }
                sink.deliver(label, id, items);
                rest = &rest[took..];
            }
            if ci + 1 < total {
                tokio::time::sleep(std::time::Duration::from_millis(CHUNK_PAUSE_MS)).await;
            }
        }
    }

    /// 〔CF2〕登记一条订阅（`chan/webview.rs::chan_subscribe` 调）。**不返回 `Result`**（`05 §3.3.5`）：
    /// 说不了的在流里原位说（`Closed{Peer}`）；那台机器此刻看不见 ⇒ 第一格 `Unseen`，订阅照样成立。
    ///
    /// - 同一个 `(webview, 编号)` 再订一次 ⇒ 旧的那条作废（页面重载后编号从头来；旧页面的订阅不留成孤儿）。
    /// - `from` 给了 ⇒ `Closed{Peer(bad_args)}`：webview 这一跳没有续传（`CF2.md §3.6`），不装作续上了。
    /// - 整台机器那一形：这个 webview 还没过就绪点 ⇒ 等就绪点；过了 ⇒ 当场交留存。一个会话那一形：当场交。
    pub fn subscribe(
        self: &Arc<Self>,
        label: &str,
        id: u64,
        origin: &crate::origin::Origin,
        kind: &str,
        from: Option<Cursor>,
        want: u32,
    ) {
        let sink = self.inner.lock().sink.clone();
        let Some(sink) = sink else {
            return;
        };
        let origin = origin.as_wire_str();
        if origin.trim().is_empty() {
            // 空白名：调用方没说哪台（同 `chan_call` 那道闸）⇒ 用法错，不当成「一台永远看不见的机器」。
            let item = Item::Closed {
                by: By::Ours(crate::chan::wire::OursFault::Misuse),
            };
            sink.deliver(label, id, vec![item]);
            return;
        }
        let (only, kind) = match parse_kind(kind) {
            Ok(Stream::Lines(o)) => (o, SubKind::Lines),
            Ok(Stream::Tap) => (None, SubKind::Tap),
            Ok(Stream::AccountsChanged) => (None, SubKind::AccountsChanged),
            Err(()) => {
                let item = refused("no-such-stream", format!("没有叫 `{kind}` 的流"));
                sink.deliver(label, id, vec![item]);
                return;
            }
        };
        if from.is_some() {
            let item = refused(
                "bad_args",
                "会话内容流不支持从某一格续看（页面重载就是一条新的订阅）".to_string(),
            );
            sink.deliver(label, id, vec![item]);
            return;
        }
        let (first, job) = {
            let mut inner = self.inner.lock();
            inner.subs.retain(|s| !(s.label == label && s.id == id));
            inner.generation += 1;
            let generation = inner.generation;
            let seen = inner.seen.get(origin).copied().unwrap_or(false);
            // 〔TAP〕tap 没有留存 ⇒ 订阅当场就是实时的（没有就绪点要等）；〔DL1〕`accounts-changed` 同理。
            let immediate =
                kind != SubKind::Lines || only.is_some() || inner.ready_labels.contains(label);
            let sub = Sub {
                label: label.to_string(),
                id,
                generation,
                origin: origin.to_string(),
                kind,
                only,
                credit: u64::from(want),
                next: 0,
                gap_from: None,
                live: immediate,
                told_seen: seen,
            };
            let job = (immediate && kind == SubKind::Lines).then(|| {
                let mine: Vec<JsonlLinePayload> = inner
                    .history
                    .iter()
                    .filter(|p| sub.wants(p))
                    .cloned()
                    .collect();
                let (before, after) =
                    lifecycle_replay(self.book, origin, sub.only.as_deref(), &mine);
                ReplayJob {
                    label: label.to_string(),
                    id,
                    generation,
                    lines: mine,
                    before,
                    after,
                }
            });
            inner.subs.push(sub);
            (
                if seen {
                    None
                } else {
                    Some(seen_item(false, true))
                },
                job,
            )
        };
        if let Some(item) = first {
            sink.deliver(label, id, vec![item]);
        }
        if let Some(job) = job {
            let this = Arc::clone(self);
            tauri::async_runtime::spawn(async move {
                this.replay_into(&*sink, job, None).await;
            });
        }
    }

    /// 〔MIG-1 · `99 §2.1 ⑬`〕那台机器的会话起停成品（`session_book` 的出口线程调）：交给订了那台 `session-lines` 的每条实时订阅
    /// （`session-lines/<sid>` 那一形只交它那一个会话的）—— **不吃 credit、不丢**、照占位置（登记的例外，[`plan_lifecycle`]）。
    /// 不进留存：F5 / 开窗的重放从成品缓存重算（[`lifecycle_replay`]）。
    pub fn on_lifecycle(&self, origin: &str, frames: Vec<SessionStreamFrame>) {
        debug_assert!(frames.iter().all(|f| !f.takes_credit()), "只许交起停那几格");
        if frames.is_empty() {
            return;
        }
        let bodies: Vec<(Option<String>, Body)> = frames
            .iter()
            .map(|f| (f.session_id().map(str::to_string), body_of(f)))
            .collect();
        let (sink, plans) = {
            let mut inner = self.inner.lock();
            let Some(sink) = inner.sink.clone() else {
                return;
            };
            let mut plans: Vec<(String, u64, Vec<Item>)> = Vec::new();
            for sub in inner
                .subs
                .iter_mut()
                .filter(|s| s.kind == SubKind::Lines && s.live && s.origin == origin)
            {
                let mine: Vec<Body> = bodies
                    .iter()
                    .filter(|(sid, _)| match (&sub.only, sid) {
                        (None, _) => true,
                        (Some(want), Some(sid)) => want == sid,
                        (Some(_), None) => false,
                    })
                    .map(|(_, b)| b.clone())
                    .collect();
                let items = plan_lifecycle(sub, &mine);
                if !items.is_empty() {
                    plans.push((sub.label.clone(), sub.id, items));
                }
            }
            (sink, plans)
        };
        for (label, id, items) in plans {
            sink.deliver(&label, id, items);
        }
    }

    /// 〔MIG-1〕旁路快照在途份数变了（全局电平）：交给每条实时的整台会话流订阅（不吃 credit、不丢）。
    pub fn on_snapshot_inflight(&self, count: u32) {
        let body = body_of(&SessionStreamFrame::SnapshotInflight(
            crate::bridge::SnapshotInflightPayload { count },
        ));
        let (sink, plans) = {
            let mut inner = self.inner.lock();
            let Some(sink) = inner.sink.clone() else {
                return;
            };
            let plans: Vec<(String, u64, Vec<Item>)> = inner
                .subs
                .iter_mut()
                .filter(|s| s.kind == SubKind::Lines && s.live && s.only.is_none())
                .map(|s| {
                    (
                        s.label.clone(),
                        s.id,
                        plan_lifecycle(s, std::slice::from_ref(&body)),
                    )
                })
                .collect();
            (sink, plans)
        };
        for (label, id, items) in plans {
            sink.deliver(&label, id, items);
        }
    }

    /// 〔TAP · V124〕中转抄出来的一个 SSE 事件（`session_tap::deliver` 经 `lib.rs` 装的出口调）：**不进 `history`**，
    /// 交给订了那台机器 `session-tap` 的每一条订阅 —— 有 credit 当场交；没有 ⇒ 丢、位置照占、下一次交之前原位 `Gap`
    /// （与会话行实时那一份同一个 [`plan_live`]）。**不等 credit、不攒**：tap 可丢（V24），攒着只会让活卡更晚。
    pub fn on_tap(&self, payload: crate::bridge::SessionTapPayload) {
        let origin = payload.origin.as_wire_str().to_string();
        let (sink, plans) = {
            let mut inner = self.inner.lock();
            let Some(sink) = inner.sink.clone() else {
                return;
            };
            let body = Body(serde_json::to_vec(&payload).unwrap_or_default());
            let mut plans: Vec<(String, u64, Vec<Item>)> = Vec::new();
            for sub in inner
                .subs
                .iter_mut()
                .filter(|s| s.kind == SubKind::Tap && s.live && s.origin == origin)
            {
                let items = plan_live(sub, vec![body.clone()]);
                if !items.is_empty() {
                    plans.push((sub.label.clone(), sub.id, items));
                }
            }
            (sink, plans)
        };
        for (label, id, items) in plans {
            sink.deliver(&label, id, items);
        }
    }

    /// 〔CF2〕订阅方报「我还能吃多少」（累加）。手里有没说的丢失、而此刻有 credit 了 ⇒ 当场原位给 `Gap`。
    pub fn want(&self, label: &str, id: u64, more: u32) {
        let (sink, gap) = {
            let mut inner = self.inner.lock();
            let sink = inner.sink.clone();
            let gap = inner
                .subs
                .iter_mut()
                .find(|s| s.label == label && s.id == id)
                .and_then(|sub| {
                    sub.credit += u64::from(more);
                    if sub.credit > 0 {
                        sub.gap_from.take().map(|from_seq| Item::Gap {
                            from_seq,
                            to_seq: Some(sub.next),
                        })
                    } else {
                        None
                    }
                });
            (sink, gap)
        };
        self.credit_changed.notify_waiters();
        if let (Some(sink), Some(gap)) = (sink, gap) {
            sink.deliver(label, id, vec![gap]);
        }
    }

    /// 〔CF2〕撤订阅（本地撤单，`05 §3.3.3`）：之后一格都不再交；在等 credit 的重放随之停。
    pub fn stop(&self, label: &str, id: u64) {
        self.inner
            .lock()
            .subs
            .retain(|s| !(s.label == label && s.id == id));
        self.credit_changed.notify_waiters();
    }

    /// 〔CF2〕那台机器的内容流接上了 / 断了（`ssh_source` 的连接与本机那条流的起落调它）⇒ 订了它的每条订阅
    /// 原位收一格 `Seen` / `Unseen`（状态没变就不重复说）。
    pub fn origin_seen(&self, origin: &crate::origin::Origin, seen: bool) {
        let origin = origin.as_wire_str();
        let (sink, told) = {
            let mut inner = self.inner.lock();
            inner.seen.insert(origin.to_string(), seen);
            let sink = inner.sink.clone();
            let told: Vec<(String, u64)> = inner
                .subs
                .iter_mut()
                .filter(|s| s.origin == origin && s.told_seen != seen)
                .map(|s| {
                    s.told_seen = seen;
                    (s.label.clone(), s.id)
                })
                .collect();
            (sink, told)
        };
        if let Some(sink) = sink {
            for (label, id) in told {
                sink.deliver(&label, id, vec![seen_item(seen, false)]);
            }
        }
    }

    /// 〔DL1〕那台机器的后端说「账号清单变了」（`accounts_changed` 帧，`设计/05 §13.6 ③`）⇒ 订了那台 `accounts-changed` 的
    /// 每条订阅收一格 `Frame`（有 credit 当场交；没有 ⇒ 丢、位置照占、下一次交之前原位 `Gap` —— 与实时行同一套）。
    pub fn accounts_changed(&self, origin: &crate::origin::Origin) {
        let origin = origin.as_wire_str();
        let (sink, plans) = {
            let mut inner = self.inner.lock();
            let Some(sink) = inner.sink.clone() else {
                return;
            };
            let plans: Vec<(String, u64, Vec<Item>)> = inner
                .subs
                .iter_mut()
                .filter(|s| s.kind == SubKind::AccountsChanged && s.origin == origin)
                .map(|s| {
                    let items = plan_live(s, vec![Body(ACCOUNTS_CHANGED_BODY.to_vec())]);
                    (s.label.clone(), s.id, items)
                })
                .filter(|(_, _, items)| !items.is_empty())
                .collect();
            (sink, plans)
        };
        for (label, id, items) in plans {
            sink.deliver(&label, id, items);
        }
    }

    /// 把指定 session_id 的全部历史从 buffer 移除。
    /// 用户主动关闭 archived Tab 时调用 —— 否则 F5 刷新 history 会重放出来"复活" Tab。
    pub fn forget(&self, session_id: &str) {
        drop_session(&mut self.inner.lock(), session_id);
    }

    /// 〔U3b〕读数口（日志与判据用）。
    pub fn stats(&self) -> ReplayStats {
        let inner = self.inner.lock();
        ReplayStats {
            history_len: inner.history.len(),
            sessions: inner.sessions.len(),
            trimmed_total: inner.trimmed_total,
        }
    }

    /// Batch8-F26：远端快照排队的优先 sid（F19"上次所在 tab"）。
    pub fn priority_sid(&self) -> Option<String> {
        self.inner.lock().priority_sid.clone()
    }

    // 〔MIG-1〕`buffered_local_session_ids`〔散文墓碑〕· `buffered_remote_sessions`〔散文墓碑〕（F5 对账拿留存里的 sid）删了：
    //   对账从成品缓存重算，就在各条订阅自己的重放里（[`lifecycle_replay`]）。
}

/// 〔MIG-1〕一条订阅的重放：起停骨架（行前）· 留存行（按 credit）· 起停终局（行后）。
struct ReplayJob {
    label: String,
    id: u64,
    generation: u64,
    lines: Vec<JsonlLinePayload>,
    before: Vec<Body>,
    after: Vec<Body>,
}

impl Default for EventReplay {
    fn default() -> Self {
        Self::new()
    }
}

/// 〔U3b〕→〔CF2〕进账：push 进 history、给**每个**会话计数，超过 `KEEP + SLACK` 就修回 `KEEP`。
///
/// 一个会话的留存整份丢（关掉已结束的 tab · 〔RENDER2〕记录文件从头重读、旧的一代作废）。
fn drop_session(inner: &mut Inner, session_id: &str) {
    inner.sessions.remove(session_id);
    let before = inner.history.len();
    inner.history.retain(|p| p.session_id != session_id);
    let removed = before - inner.history.len();
    if removed > 0 {
        tracing::info!("event_replay forget {session_id}: dropped {removed} entries");
    }
}

/// 修剪过的会话，之后到达、seq **低于**它最低留存那一条的行（尾部优先快照的头段回填）⇒ **不进缓冲**：
/// 它们进来也会在下一次修剪时被第一批丢掉，而每进 150 条就要付一次 O(history) 的修剪。
/// 这些行照样实时发给已就绪的前端（本函数只管缓冲）；F5 之后前端要，按行号取回。
fn push_and_trim(inner: &mut Inner, payloads: &[JsonlLinePayload]) {
    let mut over: Vec<String> = Vec::new();
    for p in payloads {
        let held = inner.sessions.entry(p.session_id.clone()).or_default();
        if held.floor.is_some_and(|f| p.seq < f) {
            continue;
        }
        held.count += 1;
        if held.count > REPLAY_TAIL_KEEP + TRIM_SLACK && !over.contains(&p.session_id) {
            over.push(p.session_id.clone());
        }
        inner.history.push_back(p.clone());
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
    inner.sessions.insert(
        sid.to_string(),
        Held {
            count: kept,
            floor: Some(floor),
        },
    );
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
