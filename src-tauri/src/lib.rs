mod bridge;
mod config;
mod event_replay;
mod focus;
mod hook_installer;
mod messages;
mod parser;
mod session_map;
mod watcher;

use std::sync::Arc;
use tauri::{Listener, Manager};

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
            let session_map = session_map::SessionMap::load(sessions_dir);

            // Watcher: 只对活跃 session 的 jsonl emit
            let active_filter: watcher::ActiveFilter = {
                let map = session_map.clone();
                Arc::new(move |sid: &str| map.is_session_active(sid))
            };
            let mut rx = watcher::spawn_watcher(projects_dir, active_filter);

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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::load_config,
            config::save_config,
            hook_installer::install_hook,
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
