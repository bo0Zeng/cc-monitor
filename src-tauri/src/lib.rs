mod bridge;
mod config;
mod event_replay;
mod history;
mod messages;
mod parser;
mod paths;
mod session_map;
mod subagent;
mod utils;
mod watcher;

use std::sync::Arc;
use tauri::{Emitter, Listener, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Debug build 自动开 DevTools
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            // Claude 数据目录走三级回退：用户配置 → CLAUDE_CONFIG_DIR → ~/.claude
            let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
            tracing::info!("monitor using claude_dir: {}", claude_dir.display());
            let projects_dir = claude_dir.join("projects");
            let sessions_dir = claude_dir.join("sessions");

            // SessionMap = Claude Code 自己维护的 ~/.claude/sessions/<PID>.json
            let (session_map, session_changes) =
                session_map::SessionMap::load_with_changes(sessions_dir);

            // Watcher: 只对活跃 session 的 jsonl emit
            let active_filter: watcher::ActiveFilter = {
                let map = session_map.clone();
                Arc::new(move |sid: &str| map.is_session_active(sid))
            };
            let watcher_handle = watcher::spawn_watcher(projects_dir, active_filter);
            let mut rx = watcher_handle.rx;
            let force_rescan_tx = watcher_handle.force_rescan_tx;

            // session 集合变化 emitter：
            //   - added：通知 jsonl-watcher 主动重扫该 session（修 Bug 2-A 竞态）
            //   - removed：透传 session-ended 给前端，Tab 灰显归档
            {
                let handle = app.handle().clone();
                let spawned = std::thread::Builder::new()
                    .name("session-changes-emitter".into())
                    .spawn(move || {
                        while let Ok(change) = session_changes.recv() {
                            for sid in &change.added {
                                tracing::info!("session added: {sid}, triggering jsonl rescan");
                                if let Err(e) = force_rescan_tx.send(sid.clone()) {
                                    tracing::warn!("force_rescan send failed for {sid}: {e}");
                                }
                            }
                            for sid in change.removed {
                                let payload = bridge::SessionEndedPayload {
                                    session_id: sid.clone(),
                                };
                                if let Err(e) = handle.emit(bridge::events::SESSION_ENDED, &payload)
                                {
                                    tracing::warn!("emit session-ended failed: {e}");
                                } else {
                                    tracing::info!("session ended: {sid}");
                                }
                            }
                        }
                    });
                if let Err(e) = spawned {
                    tracing::error!(
                        "failed to spawn session-changes-emitter thread: {e}; \
                         session 增减事件将丢失，Tab 不会自动归档 / 新会话可能丢首屏"
                    );
                }
            }

            // 焦点同步功能已移除：Windows 11 默认 WT 是单进程多窗口架构，
            // GetForegroundWindow 永远返回 WT 主进程 PID，OS 无法区分 tab/window。
            // 旧 focus.rs / lookup_by_foreground_pid / focus-switch IPC 都已删。
            // Tab 切换走手动点击或 Ctrl+Tab 快捷键。

            // 持久化重播：F5 刷新后整个 history 重新 emit，前端状态完整恢复。
            let replay = Arc::new(event_replay::EventReplay::new());

            // 前端 ready 事件 → replay all（每次刷新都会重新触发）
            {
                let replay = replay.clone();
                let handle = app.handle().clone();
                app.listen("frontend-ready", move |_event| {
                    replay.replay_and_mark_ready(&handle);
                });
            }

            let handle = app.handle().clone();
            let replay_loop = replay.clone();
            tauri::async_runtime::spawn(async move {
                let mut total = 0usize;
                let mut skip = 0usize;
                while let Some(line) = rx.recv().await {
                    match parser::parse_line(&line.raw) {
                        Ok(Some(record)) if record.is_displayable() => {
                            let cwd = extract_cwd(&record);
                            let payload = bridge::JsonlLinePayload {
                                session_id: line.session_id.clone(),
                                cwd,
                                path: line.path.to_string_lossy().into_owned(),
                                message: record,
                            };
                            replay_loop.record(&handle, payload);
                            total += 1;
                            if total % 200 == 0 {
                                tracing::info!("recorded {total} jsonl events (skipped {skip})");
                            }
                        }
                        Ok(_) => {
                            skip += 1;
                        }
                        Err(e) => {
                            tracing::warn!("parse line failed in {}: {e}", line.path.display());
                        }
                    }
                }
                tracing::info!("watcher loop ended; total={total} skip={skip}");
            });

            // 让 forget_session 命令能拿到 replay，bring_terminal_to_front 拿到 session_map
            app.manage(replay.clone());
            app.manage(session_map.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::load_config,
            config::save_config,
            subagent::load_subagent,
            forget_session,
            bring_terminal_to_front,
            history::list_history_projects,
            history::list_history_sessions_in_project,
            history::read_session_jsonl,
            history::delete_history_session,
            history::update_history_metadata,
            history::resume_history_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn extract_cwd(rec: &messages::JsonlRecord) -> Option<String> {
    match rec {
        messages::JsonlRecord::User { cwd, .. } => cwd.clone(),
        _ => None,
    }
}

/// 前端关闭 archived Tab 时调用：从 event_replay 历史里抹掉这个 session，
/// 防止下次 F5 刷新它原地复活。
#[tauri::command]
fn forget_session(
    session_id: String,
    replay: tauri::State<'_, Arc<event_replay::EventReplay>>,
) -> Result<(), String> {
    replay.forget(&session_id);
    Ok(())
}

/// 把 session 对应的终端窗口调到前台。前端 Tab 上的 ↗ 按钮 / Ctrl+\` 触发。
#[tauri::command]
fn bring_terminal_to_front(
    session_id: String,
    map: tauri::State<'_, Arc<session_map::SessionMap>>,
) -> Result<(), String> {
    map.bring_terminal_to_front(&session_id)
}
