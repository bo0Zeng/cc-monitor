//! Issue #11: Claude Code CLI 的 task 列表读取 + 实时 watcher。
//!
//! ## 数据源
//!
//! Claude Code CLI 持久化 task 到 `<claude_dir>/tasks/<session_id>/<id>.json`。
//! 附 `.highwatermark`（下一个 id 计数）+ `.lock`（写锁），都忽略。
//!
//! ## 〔RM1b · 第四波〕读这件事归后端 —— 本机与远端**同一条路**
//!
//! 「走目录、认 `<数字>.json`、剥 BOM、解成对象、按数字排」住后端（`src/backend/observe/tasks_query.rs`，帧命令 `tasks-list`）。
//! 〔LOC1a · 第四波 4D · C4e 批 4〕**字段语义也搬过去了**：后端出成品 `{tasks: [...]}`（`tasks_query.rs::task_entry`），
//! 界面经通道直接问（`src/tasks-panel.ts::fetchSessionTasks` / `decodeTasks`）；本模块那条 Tauri 命令（`get_session_tasks`〔散文墓碑〕）
//! 与行解释（`parse_task_lines`〔散文墓碑〕）一起删了（`设计/05 §14.3`「业务解释只有一个家」）。本模块只剩：
//!
//! 1. [`spawn_task_watcher`] —— **只在本机**：notify 监听 `tasks/` 递归，反推 session_id，
//!    然后**经本机后端**（`<local>` 长连接）问那个 sid 的成品再 emit `task-update`。watcher 不读任务文件、不解释字段。
//! 2. [`TaskEntry`] —— 成品一格的**线上形状**（ts-rs 类型的来源；推送载荷用它）。按形状严格收（`deny_unknown_fields`），
//!    收不下 ⇒ 这一次不推并说一句「两端契约对不上」，不跳过、不猜。

//! ## 边界
//!
//! - `tasks_root` 不存在（用户从没用过 Claude task tracker）→ watcher 不 spawn。
//! - 同一 100ms 窗口内同 sid 多文件变更 → 用 HashSet dedup 只重读一次。
//! - ⚠ 远端**没有推送**：后端出方向加一种帧 = 动 `wire.rs`（第四波 SR1a 独占）⇒ 本件不做。
//!   远端的新鲜度靠前端在「切到这个 tab / 展开任务面板」那一刻现问（`tasks-panel.ts`）。

use crate::bridge;
use crate::origin::Origin;
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// 单个 task 记录。`status` 保留 String（不强类型 enum）以容纳 CLI 未来可能新增
/// 的 status 值（如 `cancelled` 等），前端做 icon 映射时兜底显示原文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskEntry {
    pub id: String,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    // C02：**显式 `ts(optional)` 而不是靠 `ts-rs` 的 `has_default` 兜底**。
    // 兜底产出 `description?: string | null`（可缺席**且**可为 null），而 `skip_serializing_if`
    // 意味着运行时**永不为 null**（缺席就是缺席）⇒ 那个 `| null` 是过度宽松。
    // 显式属性产出 `description?: string`，与运行时一致，也与手写版（`tasks-panel.ts`）一致。
    #[cfg_attr(test, ts(optional))]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    // C02：**显式 `ts(optional)` 而不是靠 `ts-rs` 的 `has_default` 兜底**。
    // 兜底产出 `active_form?: string | null`（可缺席**且**可为 null），而 `skip_serializing_if`
    // 意味着运行时**永不为 null**（缺席就是缺席）⇒ 那个 `| null` 是过度宽松。
    // 显式属性产出 **`activeForm?: string`**（容器上有 `rename_all = "camelCase"`）。
    // 本注释初版写成 `active_form?: string` —— **写错了字段名，而这段的整个论点就是
    // 「字段名由 serde 决定」，写错等于自相矛盾**（C02 Phase D 审计 I6 点出）。
    #[cfg_attr(test, ts(optional))]
    pub active_form: Option<String>,
    pub status: String,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

/// 问 `origin` 那台机器的后端：这个会话的任务**成品**，按形状严格收成 [`TaskEntry`]。
///
/// 〔LOC1a〕只剩本机 watcher 一个调用方（界面经通道直接问）。收不下 ⇒ `Err`（不跳过某一条：
/// 跳过是字段语义，那一份只住后端）。
pub(crate) async fn fetch_session_tasks(
    origin: &Origin,
    session_id: &str,
) -> Result<Vec<TaskEntry>, String> {
    let who = crate::backend::control::frame_query::who(origin);
    let data = crate::backend::control::frame_query::call(
        origin,
        "tasks-list",
        serde_json::json!({ "sid": session_id }),
        crate::backend::control::frame_query::Deadline::within(TASKS_BUDGET),
    )
    .await?;
    decode_tasks(&who, data)
}

/// 这一问的期限（与已删的按行那一档同值）。
const TASKS_BUDGET: Duration = Duration::from_secs(30);

/// `tasks-list` 的 `data` → `Vec<TaskEntry>`。**纯函数**：顶层恰好一格 `tasks`，每格按 [`TaskEntry`] 严格收。
pub(crate) fn decode_tasks(who: &str, data: serde_json::Value) -> Result<Vec<TaskEntry>, String> {
    let bad = |e: String| format!("{who}的后端回的任务清单认不出来（{e}），多半是两边版本不一样");
    let serde_json::Value::Object(mut o) = data else {
        return Err(bad("not an object".into()));
    };
    if o.len() != 1 {
        return Err(bad(format!(
            "top-level keys other than `tasks`: {:?}",
            o.keys().collect::<Vec<_>>()
        )));
    }
    let tasks = o
        .remove("tasks")
        .ok_or_else(|| bad("missing `tasks`".into()))?;
    serde_json::from_value::<Vec<TaskEntry>>(tasks).map_err(|e| bad(e.to_string()))
}

/// 启动 task watcher 线程。监听 `tasks_root` 递归变更，dedup by session_id 后
/// 重读整个 session 目录并 emit `task-update`。
///
/// `tasks_root` 不存在时不 spawn（initial 时机 user 还没用过 task tracker，
/// 但目录可能后续被创建——本设计权衡：第一次启动 monitor 时该目录还没建则
/// 这次会话拿不到实时 task 更新，下次启动 monitor 才接上。简单 > 复杂热重建）。
pub fn spawn_task_watcher(tasks_root: PathBuf, app: AppHandle) {
    if !tasks_root.exists() {
        tracing::info!(
            "tasks_root not present yet ({}); task watcher will not start this session",
            tasks_root.display()
        );
        return;
    }

    if let Err(e) = std::thread::Builder::new()
        .name("task-watcher".into())
        .spawn(move || run_watcher(tasks_root, app))
    {
        tracing::error!(
            "spawn task-watcher thread failed: {e}; task panels won't get realtime updates"
        );
    }
}

fn run_watcher(tasks_root: PathBuf, app: AppHandle) {
    let (notify_tx, notify_rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = match new_debouncer(Duration::from_millis(100), notify_tx) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("task watcher debouncer init failed: {e}");
            return;
        }
    };
    if let Err(e) = debouncer
        .watcher()
        .watch(&tasks_root, RecursiveMode::Recursive)
    {
        tracing::error!("watch failed for {}: {e}", tasks_root.display());
        return;
    }

    while let Ok(evt) = notify_rx.recv() {
        let Ok(events) = evt else { continue };

        // 同一 debounce 批次里同一 sid 可能多文件变更，dedup 后只 emit 一次。
        let mut touched: HashSet<String> = HashSet::new();
        for ev in events {
            if let Some(sid) = session_id_from_change(&ev.path, &tasks_root) {
                touched.insert(sid);
            }
        }

        for sid in touched {
            // 〔RM1b → LOC1a〕经本机后端问成品（watcher 只知道「哪个 sid 变了」，不读内容、不解释字段）。
            // 本线程是 notify 的 std 线程、不在 async 运行时里 ⇒ `block_on` 安全。
            let tasks =
                match tauri::async_runtime::block_on(fetch_session_tasks(&Origin::local(), &sid)) {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!("task-update：经本机后端重读 {sid} 失败，这一次不推：{e}");
                        continue;
                    }
                };
            let payload = bridge::TasksUpdatePayload {
                session_id: sid.clone(),
                tasks,
            };
            if let Err(e) = app.emit(bridge::events::TASKS_UPDATE, &payload) {
                tracing::warn!("emit task-update for {sid} failed: {e}");
            }
        }
    }
}

/// 从变更路径反推 session_id。期望路径形如 `<tasks_root>/<sid>/<id>.json`
/// 或 `<tasks_root>/<sid>/.lock`（后者 stripped 仍能拿到 sid）。
///
/// 如果变更发生在 `<tasks_root>/<sid>/` 本身（如新建/删除目录）→ session_id =
/// 路径自己的 file_name；本设计不区分，统一处理。
fn session_id_from_change(changed: &Path, root: &Path) -> Option<String> {
    let rel = changed.strip_prefix(root).ok()?;
    let first = rel.components().next()?;
    let s = first.as_os_str().to_str()?;
    if s.is_empty() {
        return None;
    }
    Some(s.to_string())
}

// 〔LOC1a · 第四波 4D · C4e 批 4〕IPC 快照那条命令（`get_session_tasks`〔散文墓碑〕）退役：界面经通道直接问 `tasks-list`。

#[cfg(test)]
#[path = "../../../tests/bridge/tasks_tests.rs"]
mod tests;
