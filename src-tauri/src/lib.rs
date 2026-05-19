mod bridge;
mod config;
mod event_replay;
mod focus;
mod hook_installer;
mod messages;
mod parser;
mod session_map;
mod subagent;
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
        .setup(|app| {
            // Debug build 自动开 DevTools
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            let home = dirs::home_dir().ok_or("home dir not found")?;
            let projects_dir = home.join(".claude").join("projects");
            let sessions_dir = home.join(".claude").join("sessions");

            // SessionMap = Claude Code 自己维护的 ~/.claude/sessions/<PID>.json
            let (session_map, session_changes) =
                session_map::SessionMap::load_with_changes(sessions_dir);

            // session 退出（PID.json 被删）→ 透传 session-ended 给前端，让 Tab 灰显归档
            {
                let handle = app.handle().clone();
                std::thread::Builder::new()
                    .name("session-ended-emitter".into())
                    .spawn(move || {
                        while let Ok(change) = session_changes.recv() {
                            for sid in change.removed {
                                let payload = bridge::SessionEndedPayload {
                                    session_id: sid.clone(),
                                };
                                if let Err(e) =
                                    handle.emit(bridge::events::SESSION_ENDED, &payload)
                                {
                                    tracing::warn!("emit session-ended failed: {e}");
                                } else {
                                    tracing::info!("session ended: {sid}");
                                }
                            }
                        }
                    })
                    .ok();
            }

            // Watcher: 只对活跃 session 的 jsonl emit
            let active_filter: watcher::ActiveFilter = {
                let map = session_map.clone();
                Arc::new(move |sid: &str| map.is_session_active(sid))
            };
            let mut rx = watcher::spawn_watcher(projects_dir, active_filter);

            // 焦点同步：SetWinEventHook 推前台 PID → 查 session_map 得 session_id → emit
            {
                let mut fg_rx = focus::spawn_focus_thread();
                let handle = app.handle().clone();
                let map = session_map.clone();
                tauri::async_runtime::spawn(async move {
                    let mut last_sid: Option<String> = None;
                    while let Some(fg_pid) = fg_rx.recv().await {
                        let Some(sid) = map.lookup_by_foreground_pid(fg_pid) else {
                            continue;
                        };
                        if last_sid.as_deref() == Some(&sid) {
                            continue;
                        }
                        last_sid = Some(sid.clone());
                        let payload = bridge::FocusSwitchPayload {
                            session_id: sid.clone(),
                        };
                        if let Err(e) = handle.emit(bridge::events::FOCUS_SWITCH, &payload) {
                            tracing::warn!("emit focus-switch failed: {e}");
                        } else {
                            tracing::debug!("focus → {sid} (fg_pid={fg_pid})");
                        }
                    }
                });
            }

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

            // 让 forget_session 命令能拿到 replay
            app.manage(replay.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::load_config,
            config::save_config,
            hook_installer::install_hook,
            subagent::load_subagent,
            forget_session,
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
