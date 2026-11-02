//! 前后端契约的单一来源：Tauri 事件名常量 + emit payload schema。
//!
//! `events` 子模块定义所有 `emit` 事件名；payload 结构体
//! （如 `JsonlLinePayload`，携带 per-file 单调 `seq`，前端 RecordTimeline 据此排序）也在本文件。
//! 前端 `events.ts` 的 TS 接口须与此保持一致。
//!
//! 改任何事件名 / payload 字段都要同步前端订阅与类型；删事件名前 grep 确认无 emit/listen。

use serde::Serialize;

pub mod events {
    // 会话起停 / 状态那 9 个事件（`session-started` / `-ended` / `-idle` / `-container` /
    //   `-activity` · `remote-session-added` · `origin-sessions-listed` · `snapshot-inflight`）并进了会话流 `subscribe(origin, "session-lines")`：
    //   流里的一格（[`super::SessionStreamFrame`] 的起停那几种），没 credit 也照交、不丢（[`super::SessionStreamFrame::never_dropped`]）。
    // `task-update` 事件退役：任务变更经通道 `subscribe(origin, "changed/tasks")`（后端 `changed {tasks}` 帧）。
    /// **方向相反的那一个**（前端 emit、Rust `app.listen` 收）：前端注册完 listener 后
    /// 通知后端开始 replay 历史，payload 见 [`FrontendReadyPayload`]。
    ///
    /// **C02 Phase D 审计 I3 补上这个常量**：C02 恰好是「给 `frontend-ready` 首次上类型」
    /// 的那一次，而它的**名字**当时两侧都是裸字面量、`events` 里没有常量，
    /// 于是「10 个事件名钉死」那条守卫**不含它** —— 给它上了类型却把名字漏在门禁外。
    /// 加常量本身零行为变化（同一个字面量，只是有了名字）。
    pub const FRONTEND_READY: &str = "frontend-ready";
    /// **本机后端起没起来变了**：[`LocalBackend`](super::LocalBackend)（与 `backend_status` 的 `local` 那一格同形，
    /// `local_backend_host::local_backend_now`）。通道接上 / 断了 · 正在起 · 口上换了人 · 起的时候被拒就推一帧；
    /// 各台（含本机）的状态成品走本机后端的 `changed {machine}`。
    pub const LOCAL_BACKEND: &str = "local-backend";
    /// **要让用户知道的出错**（`ui_error::UiErrorPayload`：照文案键取出的那句话 ＋ 复制详情 ＋ 能重新连接的那一台）。
    /// 壳推给界面的出错只这一种；日志行不上屏。
    pub const UI_ERROR: &str = "monitor-error";
    // 窗到窗的 `open-account-panel`（设置窗账号页指路框 ⇒ 主窗口账号面板）随指路框退役：默认轮换与规则在设置「轮换」栏里管。
    // 「某台远端的长连接握手完成、能问话了」那个事件（`remote-backend-ready`）退役：
    //   前端经通道 `subscribe(origin, "changed/accounts")` 收同一件事（`Seen` ＝ 能问了 · `Frame` ＝ 那台账号清单变了），
    //   句柄是 `event_replay`（头注那张 kind 表）。「前端只有两个动作」。
    // FOCUS_SWITCH 已删除：Win11 默认终端 (WindowsTerminal.exe) 是单进程多窗口架构，
    // OS GetForegroundWindow 只能拿到 WT 主进程 PID，无法区分 tab/window 内跑哪个
    // claude session。在 WT 默认环境下永远不工作；非 WT 终端可工作但不值为少数场景维护。
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
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct JsonlLinePayload {
    pub session_id: String,
    pub cwd: Option<String>,
    pub path: String,
    // **C03 大整数策略**：量纲是**per-file 行号**，`Number.MAX_SAFE_INTEGER` = 2^53-1 行
    // ——按每行约 1 KB 算是 9 EB 的单个 jsonl 文件 ⇒ f64 精度足够。
    // 而 `bigint` 是错的：Tauri IPC 走 `serde_json::to_string` ⇒ 线上是 JSON 文本
    // ⇒ `JSON.parse` 永远给不出 BigInt。
    #[cfg_attr(test, ts(type = "number"))]
    pub seq: u64,
    /// 这一行出自哪台机器（本机逐字 `"<local>"`，远端是那台的名字，总带）：界面据它分流，远端 Tab 标题加 `[host]` 前缀。
    pub origin: crate::origin::Origin,
    /// 这一行的通用记录 —— 那台后端的成品（`cmd-wire/src/record.rs::Record`，ts-rs 从共享 crate 导出），
    /// monitor **原样转交、一个字段都不读**。
    #[cfg_attr(test, ts(type = "import(\"./LineRecord\").LineRecord"))]
    pub record: RecordBody,
    /// `[skipped_from, seq)` 这些行号 monitor **连着见过、都不可显示**（照占号、不出 payload）⇒
    /// 前端可以把它们记成见过，去重集合成区间、段数不再随会话长度涨。缺 = 没有这一段或不确知（不猜）。
    #[cfg_attr(test, ts(optional, type = "number"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_from: Option<u64>,
    /// 这一行（含 `\n`）之后那个字节（后端 `line.byte_offset` · 快照的行区间末端，原样转交）：界面从它要尾段那一截的骨架索引。
    /// 说不准 ⇒ 省略。
    #[cfg_attr(test, ts(optional, type = "number"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
    /// 这一行的对账键（那台后端给，原样转交）：活卡按它把同一次应答的那段流整轮撤掉。没有 ⇒ 省略。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rid: Option<String>,
    /// 这一行是补历史带来的（开页时 / 那台晚连上时把宣告那一刻已有的记录补过来，monitor 那一侧的旁路快照），不是刚写进来的。
    /// 界面据它只把**刚写进来的**人话当成「用户刚敲的」（自动跟到那个 tab），不按时刻猜。刚写进来的 ⇒ 省略。
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub catch_up: bool,
}

/// 后端给的一条记录成品（JSON 原文）。monitor 只搬：不解析、不读字段，序列化时原样嵌进去。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
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

/// 会话内容流（`subscribe(origin, "session-lines"[/<sid>])`）里**一格的体**。
///
/// 通道只搬不透明字节；读它的是两端的业务那一侧（这里造、`src/frontend/ui/events.ts` 读）。
///
/// - `{"line": JsonlLinePayload}`：一行记录（seq = 行号，与实时 / 快照 / 按行号取回同一个空间）。
/// - `{"batch": "start" | "end"}`：一段**成批**的行（F5 重放 · 一次攒出 ≥ 50 行的大增量）的边界。
///   前端据此进 / 出批模式（原来由 `jsonl-batch` 事件本身表达；那个事件退役之后，边界就是流里的一格）。
///   ⚠ 它们也占 credit、占位置：没 credit 时同样可能被丢（前端有「队列清空就补排结束」的兜底）。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum SessionStreamFrame {
    Line(JsonlLinePayload),
    Batch(BatchEdge),
    /// 这个会话的记录文件不见了 / 被改过已从头重读（后端 `session_file_gone` /
    /// `session_file_reread`）。与行同一条流、同序（行先冲出去再交它）⇒ 前端落到那个 tab 上说一句话。
    /// ⚠ 不进留存：F5 之后那句话没了（已知缺口）。
    FileNotice(SessionFileNoticePayload),
    /// **会话起停 / 状态的成品**（那台后端裁、`session_book` 原样转，形状就是那台的帧）—— 与行同一条流、同一个顺序
    /// （行与起停的先后就是流的先后，那条「ended 抢在行前面 ⇒ 僵尸」由构造排除）。
    /// ⚠ 这几种**不许丢**（[`Self::never_dropped`]）：丢一格起停别处补不回来。哪台由订阅说（每条订阅按台订）。
    /// 活会话（`session_added`：骨架 · 此刻的灯与名字 · 容器）。
    Added(cmd_wire::frame::SessionAdded),
    /// 灯与名字变了（`session_status`）。
    Status(cmd_wire::frame::SessionStatus),
    /// 离开「活」（`session_left`：去向在 `face.life`）。
    Left(cmd_wire::frame::SessionLeft),
    /// 那台的活会话清单报完了。
    Listed(OriginSessionsListedPayload),
    /// 旁路快照在途几份（全局电平；批模式据此不提前收尾）。
    SnapshotInflight(SnapshotInflightPayload),
    /// 一个会话的运行表（`session_runs`）：列在 agent 面板里，状态标到派出它的那张工具卡上。
    Runs(cmd_wire::frame::SessionRuns),
    /// 一个会话的主线外清单（账拼好的整份）：清单里的那几条卡折成一段（ESC 回退掉的）。
    Branch(SessionBranchPayload),
    /// 这个会话刚进了那台那条流的「在看」名单（`stream-watch` 应答的一格）：它的这份记录从第 `seq` 行起上流，
    /// 之前没上流的那段由界面自己补（按行号 / 按骨架）。不进留存：留存在这一刻按它修成「连着的」（见 `event_replay::on_watch_from`）。
    Watch(cmd_wire::watch::WatchFrom),
}

impl SessionStreamFrame {
    /// 没 credit 时这一格丢不丢：行与批边界 · 记录文件出声丢（丢了按行号补）；起停那几种**照交、照扣**（credit 可欠，
    /// 丢了别处补不回来）。界面不分格种，每格还一格。穷尽 `match`：新长一种格编译期就要表态。
    pub fn never_dropped(&self) -> bool {
        match self {
            SessionStreamFrame::Line(_)
            | SessionStreamFrame::Batch(_)
            | SessionStreamFrame::FileNotice(_) => false,
            SessionStreamFrame::Added(_)
            | SessionStreamFrame::Status(_)
            | SessionStreamFrame::Left(_)
            | SessionStreamFrame::Listed(_)
            | SessionStreamFrame::SnapshotInflight(_)
            | SessionStreamFrame::Runs(_)
            | SessionStreamFrame::Branch(_)
            | SessionStreamFrame::Watch(_) => true,
        }
    }

    /// 一条订阅（`only` = 只跟某一个会话的那一形）收不收这一格：说某个会话的 ⇒ 是它才收；说整台的 ⇒ 只有整台订阅收
    /// （说不清也是逐会话的 `session_left`）。
    pub fn reaches(&self, only: Option<&str>) -> bool {
        match (only, self.session_id()) {
            (None, _) => true,
            (Some(want), Some(sid)) => want == sid,
            (Some(_), None) => false,
        }
    }

    /// 这一格说的是哪个会话（`session-lines/<sid>` 那一形据此分流）；说的是整台 / 全局的 ⇒ `None`。
    pub fn session_id(&self) -> Option<&str> {
        match self {
            SessionStreamFrame::Line(p) => Some(&p.session_id),
            SessionStreamFrame::FileNotice(p) => Some(&p.session_id),
            SessionStreamFrame::Added(p) => Some(&p.sid),
            SessionStreamFrame::Status(p) => Some(&p.sid),
            SessionStreamFrame::Left(p) => Some(&p.sid),
            SessionStreamFrame::Runs(p) => Some(&p.sid),
            SessionStreamFrame::Branch(p) => Some(&p.sid),
            SessionStreamFrame::Watch(p) => Some(&p.sid),
            SessionStreamFrame::Batch(_)
            | SessionStreamFrame::Listed(_)
            | SessionStreamFrame::SnapshotInflight(_) => None,
        }
    }
}

/// [`SessionStreamFrame::Branch`] 的体：一个会话的主线外清单（那台给的一截一截，账拼成整份）。
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct SessionBranchPayload {
    pub sid: String,
    /// 清单里那几条记录的 `id`（整份）。
    pub off: Vec<String>,
}

/// 界面报给壳的一格（Tauri 命令 `watch_sessions` · `kept_sessions` 的入参）：哪台的哪个会话。
#[derive(Debug, serde::Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct TabSession {
    pub origin: crate::origin::Origin,
    pub session_id: String,
}

/// [`SessionStreamFrame::SnapshotInflight`] 的体。
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct SnapshotInflightPayload {
    pub count: u32,
}

/// [`SessionStreamFrame::FileNotice`] 的体。
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct SessionFileNoticePayload {
    pub session_id: String,
    /// 那台机器（本机 `<local>`）—— 与行的 `origin` 同一格语义，订阅按它分流。
    pub origin: String,
    pub path: String,
    /// `"gone"` / `"truncated"` / `"rewritten"`（[`crate::stream_source::FileChange::as_wire`]）。
    pub change: String,
}

/// 成批那一段的哪一头。
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum BatchEdge {
    Start,
    End,
}

/// 会话流 `session-live-card`（通道 `subscribe`）里一格的体：哪台机器 ＋ 那台某会话某运行此刻整份的活卡（壳照端核心的活卡板折好，
/// `resident_client::live`；界面照画、不认 `op`）。空 ⇒ 那里此刻没有卡。
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct SessionLiveCardsPayload {
    pub origin: crate::origin::Origin,
    #[cfg_attr(test, ts(type = "import(\"./LiveCards\").LiveCards"))]
    pub live: resident_client::live::LiveCards,
}

/// `listed` 格 的 payload：哪台机器的清单报完了。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct OriginSessionsListedPayload {
    /// 用 `Origin`，不用裸字符串（`origin_tests::no_new_raw_string_origin_parameters` 那条棘轮）；线上逐字同一个字符串。
    pub origin: crate::origin::Origin,
    /// 这一刻机器表里的每一台都报完了（「各台都报完」那一刻；F5 重放同一份）。
    pub all: bool,
}

/// `frontend-ready` 事件的 payload（Batch5-F19）。前端 emit 时携带 localStorage
/// 记忆的上次所在 tab；后端 replay 按 session 分组、该 tab 的块先发。缺省 /
/// 解析失败 → None（旧行为；viewer 窗口不发此事件）。
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct FrontendReadyPayload {
    #[serde(rename = "prioritySid")]
    pub priority_sid: Option<String>,
}

/// 本机后端此刻起没起来（壳自己的事实：这台电脑上的进程）。[`events::LOCAL_BACKEND`] 那一帧与 `backend_status` 的 `local` 那一格。
/// 连着 ⇒ 本机那一台的状态成品由本机后端出，这里只说连着；没连着的几样各带名字旁那个词 · 问题行那一句 · 修法
/// （壳写好的；起不来那几样只有［重试］＝ 再起一次本机后端），被拒时再带一句下一步。
/// 没连着的几样还各带画成机器的哪一态（`look`：点照机器那一张表挑）与问题行的语气（`tone`），界面不按 `state` 另判。
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LocalBackend {
    /// 通道在：本机那一行看本机后端的成品。
    Up,
    /// 起本机后端那一趟正在走。
    Starting {
        look: cmd_wire::machine::MachineStateKind,
        tone: cmd_wire::cells::Tone,
        word: String,
        said: String,
    },
    /// 没起来、接不上。
    Down {
        look: cmd_wire::machine::MachineStateKind,
        tone: cmd_wire::cells::Tone,
        word: String,
        said: String,
        fixes: Vec<cmd_wire::machine::FixButton>,
        next: Option<String>,
    },
    /// 口上占着的那一份比这一版旧（终端里先起的旧版）。
    Older {
        look: cmd_wire::machine::MachineStateKind,
        tone: cmd_wire::cells::Tone,
        word: String,
        said: String,
        fixes: Vec<cmd_wire::machine::FixButton>,
        next: Option<String>,
    },
    /// 口上占着的那一份比这一版新。
    Newer {
        look: cmd_wire::machine::MachineStateKind,
        tone: cmd_wire::cells::Tone,
        word: String,
        said: String,
        fixes: Vec<cmd_wire::machine::FixButton>,
        next: Option<String>,
    },
}
