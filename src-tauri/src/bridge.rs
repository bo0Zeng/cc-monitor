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
    /// **远端健康通道**（SS-F，issue #32 起）：远端数据源把「拥塞丢行 / 版本不符」等
    /// 非致命健康事件回传给用户。前端单一 listener（remote-health.ts）按 origin 节流后
    /// 弹 toast。`kind` 区分类别（"overflow" / "version" / …），payload 见
    /// [`RemoteHealthPayload`]。#33 版本协商复用同通道、只换 kind/message，不另造。
    pub const REMOTE_HEALTH: &str = "remote-health";
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
pub struct SessionEndedPayload {
    pub session_id: String,
}

/// 远端健康事件 payload（SS-F，issue #32 起）。`origin` = 出问题的远端机器 label
/// （`None` 理论不该出现——远端事件总带 origin；保留 Option 以与其它 payload 一致）；
/// `kind` = 类别（"overflow" / "version" / …）供前端节流键与图标选择；`message` =
/// 直接展示给用户的人读说明。
#[derive(Debug, Serialize, Clone)]
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
pub struct SessionActivityPayload {
    pub session_id: String,
    pub status: Option<String>,
    pub waiting_for: Option<String>,
}

/// v2.3.0 issue #11：单个 session 的最新 task 列表快照。
/// 每次发都是**完整重发**（而非 diff），前端 panel 直接整体 re-render，
/// 避免 diff 算法 + 防止漏掉删除事件。
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TasksUpdatePayload {
    pub session_id: String,
    pub tasks: Vec<crate::tasks::TaskEntry>,
}
