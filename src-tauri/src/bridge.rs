//! 前后端契约的单一来源：Tauri 事件名常量 + emit payload schema。
//!
//! `events` 子模块定义所有 `emit` 事件名（jsonl-line / jsonl-batch / session-ended /
//! task-update）；payload 结构体（如 `JsonlLinePayload`，携带 per-file 单调 `seq`，
//! 前端 RecordTimeline 据此排序）也在本文件。前端 `events.ts` 的 TS 接口须与此保持一致。
//!
//! 改任何事件名 / payload 字段都要同步前端订阅与类型；删事件名前 grep 确认无 emit/listen。

use serde::Serialize;

pub mod events {
    pub const JSONL_LINE: &str = "jsonl-line";
    /// v1.7.13：replay 时一次性发整个 history（Vec<JsonlLinePayload>）。
    /// 避免单条 emit N 次累计的 IPC 序列化 + 派发 overhead（3000 条曾经 ~400ms）。
    pub const JSONL_BATCH: &str = "jsonl-batch";
    pub const SESSION_ENDED: &str = "session-ended";
    /// v2.3.0 issue #11：tasks 目录监听到变更（含初次创建 / 文件改 / 删除）→
    /// 后端重读 `<claude_dir>/tasks/<sid>/` 整目录后 emit 该 sid 的完整 task 列表。
    /// 前端按 sid 路由到对应 Tab 的 tasks panel。
    pub const TASKS_UPDATE: &str = "task-update";
    /// issue #23：会话红绿灯。session_map 检测到 sessions/<PID>.json 的官方 status
    /// 字段变化时 emit（变化才发——CLI 仅在状态转换时重写文件，天然稀疏）。
    /// 前端启动/F5 用 `list_session_activity` IPC 拉快照收敛（本事件不进 replay buffer）。
    pub const SESSION_ACTIVITY: &str = "session-activity";
    /// 会话（重新）变活：session_map 重扫发现 sessions/<PID>.json 新增、**且 PID 探活
    /// 通过** 时 emit（lib.rs 在 `change.added` 分支用 `is_session_active` 门控）。
    /// session-ended 的对称补全 —— 「结束有信号、复活也有信号」。
    /// 前端复活对应的**已归档本地 Tab**（resume 场景：崩溃→灰显→`/resume` 后免 F5 回 live）。
    /// liveness 门必不可少：崩溃残留的旧 PID.json 被后续文件事件重扫也会进 `added`
    /// （心跳已从 by_id 删过它），但 PID 已死、不发本事件，避免误复活刚归档的死会话 Tab。
    /// 不进 replay buffer（同 session-activity）——F5 靠 list_session_activity 快照收敛。
    pub const SESSION_STARTED: &str = "session-started";
    /// audit-fixes F03.2：远端 claude 退出但 tmux 会话尚在（idle-tmux 第三态）→ 前端渲**灰灯**、
    /// **不归档**。**唯一由 remote-session-emitter emit**（emitter 收 daemon-removed 时，若 sid 的
    /// `@ccm_sid` 仍出现在某 origin 的 `TmuxSessions` 帧里→判 idle）。不进 replay buffer（同
    /// session-activity/started）——F5 由 emitter 对账重发。idle 是 `remote_active` **之外**的态。
    pub const SESSION_IDLE: &str = "session-idle";
    /// 远端会话宣告（Batch5-F18）：daemon session_added 帧透传，前端建骨架 Tab。
    /// 不进 replay buffer——F5 只重载 webview（SSH 连接不重建、daemon 不重发），
    /// 兜底是该会话的行仍在 buffer：重放行经 ensureTab 照建 Tab。已宣告但零行
    /// 的远端会话 F5 后骨架消失属可接受边角（首行到达即重建）。
    pub const REMOTE_SESSION_ADDED: &str = "remote-session-added";
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
    /// Batch9-F30：远端快照 inflight 计数变化（{count}）。前端 events.ts 据此
    /// 让 batch mode 事件驱动（回填在途不提前退出），替代纯 300ms 静默启发式。
    /// 不进 replay buffer。
    pub const SNAPSHOT_INFLIGHT: &str = "snapshot-inflight";
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
pub struct JsonlLinePayload {
    pub session_id: String,
    pub cwd: Option<String>,
    pub path: String,
    pub seq: u64,
    /// issue #15：数据来源标签。`None` = 本地（不序列化，前端视为本地，Tab 标题无前缀）；
    /// `Some(host)` = 远端 SSH 数据源的主机名，前端据此给 Tab 标题加 `[host]` 前缀。
    #[serde(rename = "origin", skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    pub message: crate::messages::JsonlRecord,
}

/// v2.3.1 issue #1 启动加速：jsonl-batch 把一次 replay 切成多块 emit（每块一次），
/// 避免一次性灌入 N 千行卡死前端渲染管线。
///
/// **v2.6 B 重构起**：前端**不再**用 `chunk_index`/`chunk_total` 做 prepend/append 排序
/// ——每个 payload 一律按自身 `seq` 二分插入 RecordTimeline（INVARIANTS § 5「seq 单调」/
/// § 9「禁止按到达顺序排序」）。这两个字段保留仅为兼容/诊断：前端读但不据此决定位置
/// （见 events.ts「chunkIndex/chunkTotal 元数据仍在 payload，但不再做 prepend/append 决策」）。
/// `chunk_total == 1` = 不切块（小数据单次 emit，无切块开销）。
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JsonlBatchPayload {
    /// 0-based 块序号
    pub chunk_index: u32,
    /// 总块数（前端判断"是不是最后一块"用）
    pub chunk_total: u32,
    pub payloads: Vec<JsonlLinePayload>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct SessionEndedPayload {
    pub session_id: String,
}

/// audit-fixes F03.2：idle-tmux 灰灯事件（SESSION_IDLE）payload。独立命名（非复用
/// `SessionEndedPayload`）便于 grep 与语义分离——idle ≠ ended。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct SessionIdlePayload {
    pub session_id: String,
}

/// 会话（重新）变活的 payload（SESSION_STARTED）。前端：已有 Tab → 复活；无 Tab →
/// 建骨架（Batch7-F24 修复：本地**运行中途**新出现的 bg 会话此前只能等首行经
/// ensureTab 建成无标注普通 tab——与远端 remote-session-added 对称补上元信息通道）。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct SessionStartedPayload {
    pub session_id: String,
    /// Batch7-F24：pidfile 元信息（lookup 不到时 None——纯 revive 场景照旧）。
    pub cwd: Option<String>,
    pub kind: Option<String>,
    pub name: Option<String>,
}

/// 远端会话宣告 payload（REMOTE_SESSION_ADDED，Batch5-F18）。daemon 的
/// session_added 帧透传前端——ssh_source 在 dispatch Added 时同步 emit，
/// **先于该会话的任何内容行**，前端据此建骨架 Tab 不等首行。
///
/// 已知跨通道竞序边角（Batch5 G 验收留档）：本事件由 ssh_source task 直发，
/// 而 SessionRemoved/断连归档经 session_changes 通道 + emitter 线程 emit——
/// 重连时旧连接的归档若晚于新连接的 Added 到达，骨架会被 archived；有行的
/// 会话靠 ensureTab 远端见行复活自愈，**零行 idle 会话会卡 archived 到下一行
/// 到达**。低频、可自愈补救（F5 对账），暂不为此引入统一 lifecycle 通道。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct RemoteSessionAddedPayload {
    pub session_id: String,
    /// 机器标签（`[label]` Tab 前缀）。
    pub origin: String,
    /// Batch7-F24：pidfile 元信息透传（p1e daemon 起有值；旧 daemon → None）。
    /// kind = "interactive"/"bg"（bg → ⚙ 标识 + 树状归属）。wire 帧侧因 enum tag
    /// 占用叫 `session_kind`，bridge 事件 payload 无此约束，与本地 payload 统一叫 `kind`。
    pub kind: Option<String>,
    /// 骨架标题不再等首行——cwd 直接可用（偿还 F18 backlog）。
    pub cwd: Option<String>,
    pub name: Option<String>,
}

/// `frontend-ready` 事件的 payload（Batch5-F19）。前端 emit 时携带 localStorage
/// 记忆的上次所在 tab；后端 replay 按 session 分组、该 tab 的块先发。缺省 /
/// 解析失败 → None（旧行为；viewer 窗口不发此事件）。
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct FrontendReadyPayload {
    #[serde(rename = "prioritySid")]
    pub priority_sid: Option<String>,
}

/// `list_active_sessions` IPC 返回项（Batch5-F18）：本地活跃会话清单（含 cwd），
/// 供前端启动时先建全部骨架 Tab。远端不走此 IPC——连接晚于前端启动，走
/// [`RemoteSessionAddedPayload`] 事件。
#[derive(Debug, Serialize, Clone)]
pub struct ActiveSessionPayload {
    pub session_id: String,
    pub cwd: String,
    /// Batch7-F24：kind/name（bg → ⚙ 标识 + 树状归属；name 作 bg 标题）。
    pub kind: Option<String>,
    pub name: Option<String>,
}

/// 远端健康事件 payload（SS-F，issue #32 起）。`origin` = 出问题的远端机器 label
/// （`None` 理论不该出现——远端事件总带 origin；保留 Option 以与其它 payload 一致）；
/// `kind` = 类别（"overflow" / "version" / …）供前端节流键与图标选择；`message` =
/// 直接展示给用户的人读说明。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct RemoteHealthPayload {
    pub origin: Option<String>,
    pub kind: String,
    pub message: String,
}

/// issue #23：会话红绿灯状态。`status` 直接透传 Claude Code 官方枚举
/// （"busy" / "idle" / "shell" / "waiting"，None=旧版 CC 无此字段，前端按未知处理）；
/// `waiting_for` 仅 status=="waiting" 时有（"permission prompt" / "dialog open" …）。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct SessionActivityPayload {
    pub session_id: String,
    pub status: Option<String>,
    pub waiting_for: Option<String>,
}

/// v2.3.0 issue #11：单个 session 的最新 task 列表快照。
/// 每次发都是**完整重发**（而非 diff），前端 panel 直接整体 re-render，
/// 避免 diff 算法 + 防止漏掉删除事件。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct TasksUpdatePayload {
    pub session_id: String,
    pub tasks: Vec<crate::tasks::TaskEntry>,
}
