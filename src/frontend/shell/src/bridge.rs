//! 前后端契约的单一来源：Tauri 事件名常量 + emit payload schema。
//!
//! `events` 子模块定义所有 `emit` 事件名（task-update / remote-health …；〔MIG-1〕会话起停并进了会话流的格）；payload 结构体
//! （如 `JsonlLinePayload`，携带 per-file 单调 `seq`，前端 RecordTimeline 据此排序）也在本文件。
//! 前端 `events.ts` 的 TS 接口须与此保持一致。
//!
//! 〔CF2 · 第四波 4B〕**会话内容不再是 Tauri 事件**：原来的 `jsonl-line` / `jsonl-batch` 两个事件
//! （与载荷 `JsonlBatchPayload`〔散文墓碑〕）退役，会话内容改走通道的 `subscribe`
//! （`chan/webview.rs` · `event_replay·rs` 头注「订阅」）；流里每一格的体是 [`SessionStreamFrame`]。
//!
//! 改任何事件名 / payload 字段都要同步前端订阅与类型；删事件名前 grep 确认无 emit/listen。

use serde::Serialize;

pub mod events {
    // 〔MIG-1 · `设计/99 §2.1 ⑬`〕会话起停 / 状态那 9 个事件（`session-started` / `-ended` / `-idle` / `-container` / `-unseen` /
    //   `-activity` · `remote-session-added` · `origin-sessions-listed` · `snapshot-inflight`）并进了会话流 `subscribe(origin, "session-lines")`：
    //   流里的一格（[`super::SessionStreamFrame`] 的起停那几种），不吃 credit、不丢（[`super::SessionStreamFrame::takes_credit`]）。
    // 〔MIG-3b · ㉓②〕`task-update` 事件退役：任务变更经通道 `subscribe(origin, "session-tasks")`（后端 `tasks_changed` 帧）。
    /// **方向相反的那一个**（前端 emit、Rust `app.listen` 收）：前端注册完 listener 后
    /// 通知后端开始 replay 历史，payload 见 [`FrontendReadyPayload`]。
    ///
    /// **C02 Phase D 审计 I3 补上这个常量**：C02 恰好是「给 `frontend-ready` 首次上类型」
    /// 的那一次，而它的**名字**当时两侧都是裸字面量、`events` 里没有常量，
    /// 于是「10 个事件名钉死」那条守卫**不含它** —— 给它上了类型却把名字漏在门禁外。
    /// 加常量本身零行为变化（同一个字面量，只是有了名字）。
    pub const FRONTEND_READY: &str = "frontend-ready";
    /// **远端健康通道**（SS-F，issue #32 起）：远端数据源把「拥塞丢行 / 版本不符」等
    /// 非致命健康事件回传给用户。前端单一 listener（remote-health.ts）按 origin 节流后
    /// 弹 toast。`kind` 区分类别（"overflow" / "version" / …），payload 见
    /// [`RemoteHealthPayload`]。#33 版本协商复用同通道、只换 kind/message，不另造。
    pub const REMOTE_HEALTH: &str = "remote-health";
    // 〔DL1 · 第五波〕「某台远端的长连接握手完成、能问话了」那个事件（`remote-backend-ready`）退役：
    //   前端经通道 `subscribe(origin, "accounts-changed")` 收同一件事（`Seen` ＝ 能问了 · `Frame` ＝ 那台账号清单变了），
    //   句柄是 `event_replay`（头注那张 kind 表）。`设计/01 §2.2`「前端只有两个动作」。
    // FOCUS_SWITCH 已删除：Win11 默认终端 (WindowsTerminal.exe) 是单进程多窗口架构，
    // OS GetForegroundWindow 只能拿到 WT 主进程 PID，无法区分 tab/window 内跑哪个
    // claude session。在 WT 默认环境下永远不工作；非 WT 终端可工作但不值为少数场景维护。
    //
    // SUBAGENT_LINE 已废弃：subagent 不走实时 watcher，由前端 invoke
    // `load_subagent` 在用户展开 Task 折叠卡时按需加载。
}

/// P5.1：`seq` 字段是 same-session 内单调递增的行号（watcher 给每文件维护
/// `next_seq` 计数器）。前端 RecordTimeline 按 seq 排序到 DOM —— 后端 emit 顺序
/// 不再影响视觉顺序，watcher 任意时机 push 进来都能放到正确位置。
///
/// 重要约束：
/// - 同一 jsonl 文件内 seq 单调（process_file 顺序读，单调）
/// - 同一 session 内 seq 单调（同 session 通常单文件；多文件 fork 场景见 § Notes）
/// - 跨 session 不可比（每个 tab 独立 timeline）
/// - 不跨 monitor 进程持久（每次启动从 0 开始；F5 重新拉 history 顺序仍正确）
///
/// Notes: session fork (`/branch`) 创建新 jsonl 文件 → 新 session_id，timeline 独立。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct JsonlLinePayload {
    pub session_id: String,
    pub cwd: Option<String>,
    pub path: String,
    // **C03 大整数策略**：量纲是**per-file 行号**，`Number.MAX_SAFE_INTEGER` = 2^53-1 行
    // ——按每行约 1 KB 算是 9 EB 的单个 jsonl 文件 ⇒ f64 精度足够。
    // 而 `bigint` 是错的：Tauri IPC 走 `serde_json::to_string` ⇒ 线上是 JSON 文本
    // ⇒ `JSON.parse` 永远给不出 BigInt（`tauri-2.11.2/src/ipc/mod.rs:181-183`）。
    #[cfg_attr(test, ts(type = "number"))]
    pub seq: u64,
    /// issue #15：数据来源标签。`None` = 本地（不序列化，前端视为本地，Tab 标题无前缀）；
    /// `Some(host)` = 远端 SSH 数据源的主机名，前端据此给 Tab 标题加 `[host]` 前缀。
    // **C01/C02 立的规则**：有 `skip_serializing_if` 就必须配 `ts(optional)`。
    // 不配的话 ts-rs 生成 `origin: string | null` —— 两处都错：线上是**省略**而不是
    // 必填，而且**永远不会是 null**（Rust 侧 None 直接不序列化）。
    // 这一条本轮由守卫当场抓到（原话：「会多一个永不出现的 `| null`」）。
    #[cfg_attr(test, ts(optional))]
    #[serde(rename = "origin", skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// 〔MOD · `设计/90 §3` 判据 3〕这一行在渲染模型里的样子 —— 那台后端的成品（`agents/claudecode/schema.rs::JsonlRecord`，
    /// ts-rs 从后端导出），monitor **原样转交、一个字段都不读**。
    #[cfg_attr(test, ts(type = "import(\"./JsonlRecord\").JsonlRecord"))]
    pub message: RecordBody,
    /// 〔RENDER2 · `设计/10 §3.2`〕`[skipped_from, seq)` 这些行号 monitor **连着见过、都不可显示**（照占号、不出 payload）⇒
    /// 前端可以把它们记成见过，去重集合成区间、段数不再随会话长度涨（`真相源/130 §3`）。缺 = 没有这一段或不确知（不猜）。
    #[cfg_attr(test, ts(optional, type = "number"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_from: Option<u64>,
}

/// 〔MOD · `设计/90 §3` 判据 3〕后端给的一条记录成品（JSON 原文）。monitor 只搬：不解析、不读字段，序列化时原样嵌进去。
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct RecordBody(pub Box<serde_json::value::RawValue>);

impl RecordBody {
    /// 从一段 JSON 原文造（原文不是合法 JSON ⇒ `None`）。
    pub fn from_json(text: String) -> Option<Self> {
        serde_json::value::RawValue::from_string(text)
            .ok()
            .map(Self)
    }
}

impl PartialEq for RecordBody {
    fn eq(&self, other: &Self) -> bool {
        self.0.get() == other.0.get()
    }
}
impl Eq for RecordBody {}

/// 〔CF2 · 第四波 4B〕会话内容流（`subscribe(origin, "session-lines"[/<sid>])`）里**一格的体**。
///
/// 通道只搬不透明字节（`设计/05 §3.3.0`）；读它的是两端的业务那一侧（这里造、`src/events.ts` 读）。
///
/// - `{"line": JsonlLinePayload}`：一行记录（seq = 行号，与实时 / 快照 / 按行号取回同一个空间）。
/// - `{"batch": "start" | "end"}`：一段**成批**的行（F5 重放 · 一次攒出 ≥ 50 行的大增量）的边界。
///   前端据此进 / 出批模式（原来由 `jsonl-batch` 事件本身表达；那个事件退役之后，边界就是流里的一格）。
///   ⚠ 它们也占 credit、占位置：没 credit 时同样可能被丢（前端有「队列清空就补排结束」的兜底）。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
#[serde(rename_all = "snake_case")]
pub enum SessionStreamFrame {
    Line(JsonlLinePayload),
    Batch(BatchEdge),
    /// 〔FW1 · 第四波 4D · 主会话裁 D-d〕这个会话的记录文件不见了 / 被改过已从头重读（后端 `session_file_gone` /
    /// `session_file_reread`）。与行同一条流、同序（行先冲出去再交它）⇒ 前端落到那个 tab 上说一句话。
    /// ⚠ 不进留存：F5 之后那句话没了（已知缺口，主会话 09-25 认）。
    FileNotice(SessionFileNoticePayload),
    /// 〔MIG-1 · `设计/99 §2.1 ⑬`〕**会话起停 / 状态的成品**（那台后端裁、`session_book` 原样转）—— 与行同一条流、同一个顺序
    /// （行与起停的先后就是流的先后，`05 §15.3` 那条「ended 抢在行前面 ⇒ 僵尸」由构造排除）。
    /// ⚠ 这几种**不吃 credit、不许丢**（[`Self::takes_credit`]；登记的唯一例外）：丢一格起停别处补不回来。
    /// 活会话（本机远端同一形；`origin` 说哪台）。
    Live(SessionLivePayload),
    /// 红绿灯。
    Activity(SessionActivityPayload),
    /// 活会话住在什么容器里。
    Container(SessionContainerPayload),
    /// 可重连（claude 退了、tmux 会话还在）。
    Idle(SessionIdlePayload),
    /// 已结束。
    Ended(SessionEndedPayload),
    /// 说不清（那台看不见了 / F5 时那台还没报完清单）。
    Unseen(SessionUnseenPayload),
    /// 那台的活会话清单报完了。
    Listed(OriginSessionsListedPayload),
    /// 旁路快照在途几份（全局电平；批模式据此不提前收尾）。
    SnapshotInflight(SnapshotInflightPayload),
}

impl SessionStreamFrame {
    /// 〔MIG-1 · ⑬ 登记的例外〕这一格吃不吃 credit：行与批边界 · 记录文件出声吃（可丢、丢了按行号补）；
    /// 起停那几种**不吃、不丢**（丢了别处补不回来）。穷尽 `match`：新长一种格编译期就要表态。
    /// TS 那一侧同一张表住 `src/events.ts::CREDIT_EXEMPT_FRAMES`，两侧对金样 `tests/__fixtures__/session-stream-credit.golden.json`。
    pub fn takes_credit(&self) -> bool {
        match self {
            SessionStreamFrame::Line(_)
            | SessionStreamFrame::Batch(_)
            | SessionStreamFrame::FileNotice(_) => true,
            SessionStreamFrame::Live(_)
            | SessionStreamFrame::Activity(_)
            | SessionStreamFrame::Container(_)
            | SessionStreamFrame::Idle(_)
            | SessionStreamFrame::Ended(_)
            | SessionStreamFrame::Unseen(_)
            | SessionStreamFrame::Listed(_)
            | SessionStreamFrame::SnapshotInflight(_) => false,
        }
    }

    /// 一条订阅（`only` = 只跟某一个会话的那一形）收不收这一格：说某个会话的 ⇒ 是它才收；说整台的 ⇒ 整台订阅都收，
    /// 只跟一个会话的只收〔MIG-1 续〕机器级「说不清」（那台看不见了，它跟的那一条也说不清了）。
    pub fn reaches(&self, only: Option<&str>) -> bool {
        match (only, self.session_id()) {
            (None, _) => true,
            (Some(want), Some(sid)) => want == sid,
            (Some(_), None) => matches!(self, SessionStreamFrame::Unseen(_)),
        }
    }

    /// 这一格说的是哪个会话（`session-lines/<sid>` 那一形据此分流）；说的是整台 / 全局的 ⇒ `None`。
    pub fn session_id(&self) -> Option<&str> {
        match self {
            SessionStreamFrame::Line(p) => Some(&p.session_id),
            SessionStreamFrame::FileNotice(p) => Some(&p.session_id),
            SessionStreamFrame::Live(p) => Some(&p.session_id),
            SessionStreamFrame::Activity(p) => Some(&p.session_id),
            SessionStreamFrame::Container(p) => Some(&p.session_id),
            SessionStreamFrame::Idle(p) => Some(&p.session_id),
            SessionStreamFrame::Ended(p) => Some(&p.session_id),
            SessionStreamFrame::Batch(_)
            | SessionStreamFrame::Unseen(_)
            | SessionStreamFrame::Listed(_)
            | SessionStreamFrame::SnapshotInflight(_) => None,
        }
    }
}

/// 〔MIG-1〕[`SessionStreamFrame::SnapshotInflight`] 的体。
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct SnapshotInflightPayload {
    pub count: u32,
}

/// 〔FW1〕[`SessionStreamFrame::FileNotice`] 的体。
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct SessionFileNoticePayload {
    pub session_id: String,
    /// 那台机器（本机 `<local>`）—— 与行的 `origin` 同一格语义，订阅按它分流。
    pub origin: String,
    pub path: String,
    /// `"gone"` / `"truncated"` / `"rewritten"`（[`crate::ssh_source::FileChange::as_wire`]）。
    pub change: String,
}

/// 〔CF2〕成批那一段的哪一头。
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
#[serde(rename_all = "snake_case")]
pub enum BatchEdge {
    Start,
    End,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct SessionEndedPayload {
    pub session_id: String,
}

/// audit-fixes F03.2：可重连（idle-tmux 灰灯）的 payload（〔MIG-1〕会话流 `idle` 那一格）。独立命名（非复用
/// `SessionEndedPayload`）便于 grep 与语义分离——idle ≠ ended。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct SessionIdlePayload {
    pub session_id: String,
}

/// 〔GP1 · 第四波〕「说不清」的 payload（〔MIG-1〕会话流 `unseen` 那一格）。独立命名，理由同 [`SessionIdlePayload`]：unseen ≠ ended。
/// 〔MIG-1 续 · 主会话裁〕**机器级**：说的是「那台看不见了 / 那台还没报完清单」，前端对那台上活的 · 可重连的 tab 一并落说不清
/// （原先逐会话发一格 `session_id`）。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct SessionUnseenPayload {
    pub origin: crate::origin::Origin,
}

/// 〔MIG-1〕活会话的成品（会话流里的 [`SessionStreamFrame::Live`]；本机远端同一形，`origin` 说哪台）。
/// 前端：本机 ⇒ 复活已有 tab / 建骨架（原 `session-started`）；远端 ⇒ 建骨架（原 `remote-session-added`）。先于该会话的行。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct SessionLivePayload {
    pub session_id: String,
    /// 哪台机器（本机 `<local>`；远端是 `[label]` Tab 前缀）。
    pub origin: String,
    /// Batch7-F24：pidfile 元信息透传（p1e backend 起有值；旧 backend → None）。
    /// kind = "interactive"/"bg"（bg → ⚙ 标识；〔V125〕bg 平铺为普通 tab，不再挂宿主排成树）。wire 帧侧因 enum tag
    /// 占用叫 `session_kind`，bridge 事件 payload 无此约束，与本地 payload 统一叫 `kind`。
    pub kind: Option<String>,
    /// **E73（additive）：attach 进去对人有没有意义。**
    ///
    /// `kind` 此前把两件事压在一个轴上：①「该不该在 UI 出现」②「是不是一个人坐在终端里
    /// 跟它对话」。SDK / 脚本驱动的会话正好「①要②不要」—— 它有 tmux、`@ccm_sid` 也对，
    /// 但 `stdin=DEVNULL`。`false` ⇒ 前端不给 attach / ↗ / 「杀死空 tmux」。
    /// **None = true**（存量会话与旧后端一律照旧）。
    pub attachable: Option<bool>,
    /// 骨架标题不再等首行——cwd 直接可用（偿还 F18 backlog）。
    pub cwd: Option<String>,
    pub name: Option<String>,
    /// 〔FIX3 · `99 §2.2 ②`〕启动期令牌（`设计/80 §8.2`）：前端起新会话时铸的那一个，据它认出「我刚起的那条起来了」。
    /// 那台后端读不到 / 没索要 ⇒ 缺席。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rbind_token: Option<String>,
}

/// 〔U4b · 第四波〕`container` 格 的 payload。`container` 只有两个值：`"tmux"` / `"none"`
/// （判不了的不发这个事件）。
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct SessionContainerPayload {
    pub session_id: String,
    pub container: String,
}

/// 〔TAP · V124〕会话流 `session-tap`（通道 `subscribe`，`设计/05 §15`）里一格的体：后端 `tap` 帧的字段原样 ＋ 哪台机器。
///
/// `stream` = claude 请求头里自带的会话标识（〔V141〕== 它的 sid；前端拿它对 tab 的 sid，对不上 / 空 ⇒ 匿名流、不显示）；`resp` · `n` 见后端 `wire::Frame::Tap`；
/// `data`（SSE 事件原文，一个 JSON 串）与 `end`（`"done"` / `"broken"`）恰有一个。
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct SessionTapPayload {
    pub origin: crate::origin::Origin,
    pub stream: String,
    // C03 大整数策略：本进程第几个响应 / 响应里第几个事件，远在 2^53 之内；线上是 JSON 文本，`bigint` 是错的。
    #[cfg_attr(test, ts(type = "number"))]
    pub resp: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub n: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub end: Option<String>,
}

/// 〔U4b · 第四波〕`listed` 格 的 payload：哪台机器的清单报完了。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct OriginSessionsListedPayload {
    /// 用 `Origin`，不用裸字符串（`origin_tests::no_new_raw_string_origin_parameters` 那条棘轮）；线上逐字同一个字符串。
    pub origin: crate::origin::Origin,
}

/// `frontend-ready` 事件的 payload（Batch5-F19）。前端 emit 时携带 localStorage
/// 记忆的上次所在 tab；后端 replay 按 session 分组、该 tab 的块先发。缺省 /
/// 解析失败 → None（旧行为；viewer 窗口不发此事件）。
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct FrontendReadyPayload {
    #[serde(rename = "prioritySid")]
    pub priority_sid: Option<String>,
}

// 〔MIG-1〕`list_active_sessions` 的返回项 `ActiveSessionPayload`〔散文墓碑〕删了：本机骨架从会话流里的 `live` 成品来
//   （F5 就绪点按成品缓存重放），那条命令随之退役。

/// 远端健康事件 payload（SS-F，issue #32 起）。`origin` = 出问题的远端机器 label
/// （〔C4b · 第四波〕从 `Option<String>` 改成 `String`：五个发射点全在 `ssh_source.rs`、全都带着那台的 label，
/// 「没有 origin」从来不是一个会发生的值 —— 类型里就不给它留格子，TS 那侧随之不再装得下 `null`）；
/// `kind` = 类别（"overflow" / "version" / …）供前端节流键与图标选择；`message` =
/// 直接展示给用户的人读说明。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
#[serde(rename_all = "camelCase")]
pub struct RemoteHealthPayload {
    pub origin: String,
    pub kind: String,
    pub message: String,
}

/// issue #23：会话红绿灯状态。`status` 直接透传 Claude Code 官方枚举
/// （"busy" / "idle" / "shell" / "waiting"，None=旧版 CC 无此字段，前端按未知处理）；
/// `waiting_for` 仅 status=="waiting" 时有（"permission prompt" / "dialog open" …）。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
pub struct SessionActivityPayload {
    pub session_id: String,
    pub status: Option<String>,
    pub waiting_for: Option<String>,
}
