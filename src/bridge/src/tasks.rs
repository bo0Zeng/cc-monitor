//! Issue #11: Claude Code CLI 的 task 列表读取 + 实时 watcher。
//!
//! ## 数据源
//!
//! Claude Code CLI 持久化 task 到 `<claude_dir>/tasks/<session_id>/<id>.json`。
//! 附 `.highwatermark`（下一个 id 计数）+ `.lock`（写锁），都忽略。
//!
//! ## 实施
//!
//! 1. `read_session_tasks(tasks_root, sid)` — 扫单个 session 的 task 目录，
//!    跳过非 `<digits>.json` 文件，半截 JSON 单条 catch 跳过（`.lock` 期间）。
//! 2. `spawn_task_watcher(tasks_root, handle)` — notify-debouncer-mini 监听
//!    `tasks/` 递归；文件变更 → 反推 session_id → 重读整个 session 目录 → emit。
//! 3. `get_session_tasks` IPC — Tab 创建时拿初始快照（async + spawn_blocking）。
//!
//! ## 边界
//!
//! - `tasks_root` 不存在（用户从没用过 Claude task tracker）→ watcher 不 spawn，
//!   IPC 返空 vec，前端 panel 自然隐藏。
//! - 同一 100ms 窗口内同 sid 多文件变更 → 用 HashSet dedup 只重读一次。
//! - 路径中遇到非 UTF-8 → 跳过。

use crate::bridge;
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

/// 读单个 session 的 task 列表，按 id 数字升序（= 创建顺序 = 跟终端一致）。
///
/// session 目录不存在 → 返空 vec（**不报错**：用户未在该 session 跑 task 是正常态）。
pub fn read_session_tasks(tasks_root: &Path, session_id: &str) -> Vec<TaskEntry> {
    let session_dir = tasks_root.join(session_id);
    if !session_dir.is_dir() {
        return Vec::new();
    }

    let mut out: Vec<(u64, TaskEntry)> = Vec::new();
    let entries = match std::fs::read_dir(&session_dir) {
        Ok(it) => it,
        Err(e) => {
            tracing::warn!("read_dir {} failed: {e}", session_dir.display());
            return Vec::new();
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let ext_ok = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        // 跳过 .lock / .highwatermark / 非 json / 非数字 stem
        if !ext_ok {
            continue;
        }
        let Ok(id_num) = stem.parse::<u64>() else {
            continue;
        };

        // 写者持 .lock 时这里可能读到半截 JSON：catch + 跳过，notify 下次
        // debounce 会再次触发整目录重读，自然恢复。
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::trace!("read task file {} failed: {e}", path.display());
                continue;
            }
        };
        let trimmed = raw.trim_start_matches('\u{feff}');
        let task: TaskEntry = match serde_json::from_str(trimmed) {
            Ok(t) => t,
            Err(e) => {
                tracing::trace!("parse task file {} failed: {e}", path.display());
                continue;
            }
        };
        out.push((id_num, task));
    }

    out.sort_by_key(|(id, _)| *id);
    out.into_iter().map(|(_, t)| t).collect()
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
            let tasks = read_session_tasks(&tasks_root, &sid);
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

/// IPC：前端 Tab 创建时拿一次初始快照。
///
/// async + spawn_blocking：read_dir + read_to_string 是阻塞 IO，避免占主 IPC 线程。
#[tauri::command]
pub async fn get_session_tasks(session_id: String) -> Result<Vec<TaskEntry>, String> {
    let tasks_root =
        tasks_root_for_current_claude_dir().ok_or_else(|| "no claude dir".to_string())?;
    tokio::task::spawn_blocking(move || Ok(read_session_tasks(&tasks_root, &session_id)))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 解析 agent 任务追踪目录(CC = `<claude_dir>/tasks/`)——F-MA 走活跃适配器 layout,不硬编码子目录。
pub fn tasks_root_for_current_claude_dir() -> Option<PathBuf> {
    crate::adapter::tasks_dir(&crate::paths::resolve_claude_dir()?)
}

#[cfg(test)]
#[path = "../../../tests/bridge/tasks_tests.rs"]
mod tests;
