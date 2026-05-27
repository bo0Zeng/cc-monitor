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
    pub message: crate::messages::JsonlRecord,
}

/// v2.3.1 issue #1 启动加速：jsonl-batch 加 chunk 元数据。
///
/// 切块策略下，单次 replay 会触发多次 emit（每块一次）。前端按 `chunk_index` 区分：
/// - `chunk_index == 0`（head）：append 到 stream 底部，记 firstChunkAnchor
/// - `chunk_index > 0`（older）：prepend (insertBefore firstChunkAnchor)
///
/// `chunk_total == 1` 时退化到 v2.2 单次 emit 行为（小数据走这条路径无切块开销）。
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

/// v2.3.0 issue #11：单个 session 的最新 task 列表快照。
/// 每次发都是**完整重发**（而非 diff），前端 panel 直接整体 re-render，
/// 避免 diff 算法 + 防止漏掉删除事件。
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TasksUpdatePayload {
    pub session_id: String,
    pub tasks: Vec<crate::tasks::TaskEntry>,
}
