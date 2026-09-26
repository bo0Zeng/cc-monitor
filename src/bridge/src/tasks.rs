//! Issue #11: Claude Code CLI 的 task 列表读取 + 实时 watcher。
//!
//! ## 数据源
//!
//! Claude Code CLI 持久化 task 到 `<claude_dir>/tasks/<session_id>/<id>.json`。
//! 附 `.highwatermark`（下一个 id 计数）+ `.lock`（写锁），都忽略。
//!
//! ## 〔RM1b · 第四波〕读这件事归后端 —— 本机与远端**同一条路**
//!
//! 此前本模块自己 `read_dir` 本机那个目录（`local_read_surface_registry` 里一条 `reader`），
//! 远端会话的任务在远端机器上 ⇒ 远端 tab 永远拿不到（`parity_ledger` `session.tasks` 那笔欠账）。
//! 现在「走目录、认 `<数字>.json`、剥 BOM、解成对象、按数字排」住后端
//! （`src/backend/observe/tasks_query.rs`，帧命令 `tasks-list`），本模块只剩三件事：
//!
//! 1. [`get_session_tasks`] —— 按 `origin` 问那台机器的后端（本机逐字 `"<local>"`，
//!    走的是同一个 `frame_query::lines`），拿回一行一个的原样 JSON 对象。
//! 2. [`parse_task_lines`] —— **字段语义的唯一住址**：一行解不成 [`TaskEntry`] 就跳过那一行
//!    （与搬家前「半截 JSON 单条跳过」同一口径；后端不认字段，只保证每行是一个对象）。
//! 3. [`spawn_task_watcher`] —— **只在本机**：notify 监听 `tasks/` 递归，反推 session_id，
//!    然后**经本机后端**重读那个 sid 再 emit `task-update`。watcher 自己不读任务文件的内容。
//!
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
#[serde(rename_all = "camelCase")]
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

/// 后端回来的那几行 → [`TaskEntry`]。**字段语义的唯一住址**。
///
/// 一行解不成 `TaskEntry` ⇒ 跳过那一行（`trace!`）：后端只保证「每行是一个 JSON 对象、按任务号升序」，
/// 缺 `id` / `subject` / `status` 的对象在这里被挡下 —— 与搬家前 `serde_json::from_str::<TaskEntry>`
/// 失败即跳过是同一口径，顺序原样保留。
pub fn parse_task_lines(lines: &[String]) -> Vec<TaskEntry> {
    lines
        .iter()
        .filter_map(|l| match serde_json::from_str::<TaskEntry>(l) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::trace!("任务行解不成 TaskEntry：{e}");
                None
            }
        })
        .collect()
}

/// 问 `origin` 那台机器的后端：这个会话的任务。本机与远端同一条路。
pub(crate) async fn fetch_session_tasks(
    origin: &Origin,
    session_id: &str,
) -> Result<Vec<TaskEntry>, String> {
    use crate::backend::control::frame_query::{self, Deadline};
    let lines = frame_query::lines(
        origin,
        "tasks-list",
        serde_json::json!({ "sid": session_id }),
        Deadline::within(frame_query::LINES_BUDGET),
    )
    .await?;
    Ok(parse_task_lines(&lines))
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
            // 〔RM1b〕经本机后端重读（watcher 只知道「哪个 sid 变了」，不读内容）。
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

/// IPC：前端 Tab 创建时（以及远端 tab 被切到 / 面板展开时）拿一次快照。
///
/// 〔RM1b〕收 `origin`：本机逐字 `"<local>"`，远端是那台的 label —— 两侧同一条路
/// （那台机器的后端 `tasks-list`）。`route` 只用来拦空白名（「没给名字」不是本机）。
#[tauri::command]
pub async fn get_session_tasks(
    origin: Origin,
    session_id: String,
) -> Result<Vec<TaskEntry>, String> {
    let _ = origin.route("get_session_tasks")?;
    fetch_session_tasks(&origin, &session_id).await
}

#[cfg(test)]
#[path = "../../../tests/bridge/tasks_tests.rs"]
mod tests;
