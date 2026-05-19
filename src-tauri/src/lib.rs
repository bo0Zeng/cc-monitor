mod bridge;
mod config;
mod focus;
mod hook_installer;
mod messages;
mod parser;
mod session_map;
mod watcher;

use parking_lot::Mutex;
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
            let session_map = session_map::SessionMap::load(sessions_dir);

            // Watcher: 只对活跃 session 的 jsonl emit
            let active_filter: watcher::ActiveFilter = {
                let map = session_map.clone();
                Arc::new(move |sid: &str| map.is_session_active(sid))
            };
            let mut rx = watcher::spawn_watcher(projects_dir, active_filter);

            // Handshake: 前端 listener 注册前后端就开始 emit 会丢，所以先缓冲。
            // 前端 invoke "frontend-ready" 后清空缓冲，之后改为直接 emit。
            let pending: Arc<Mutex<Option<Vec<bridge::JsonlLinePayload>>>> =
                Arc::new(Mutex::new(Some(Vec::new())));

            // 前端 ready 事件 → drain
            {
                let pending = pending.clone();
                let handle = app.handle().clone();
                app.listen("frontend-ready", move |_event| {
                    let buf = pending.lock().take();
                    if let Some(buf) = buf {
                        let n = buf.len();
                        for p in buf {
                            let _ = handle.emit(bridge::events::JSONL_LINE, &p);
                        }
                        tracing::info!("flushed {n} buffered events to frontend");
                    }
                });
            }

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut emit_count = 0usize;
                let mut skip_count = 0usize;
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
                            // 缓冲存在 = 前端未 ready；否则直接 emit
                            let mut guard = pending.lock();
                            if let Some(buf) = guard.as_mut() {
                                buf.push(payload);
                            } else {
                                drop(guard);
                                match handle.emit(bridge::events::JSONL_LINE, &payload) {
                                    Ok(()) => {
                                        emit_count += 1;
                                        if emit_count % 50 == 0 {
                                            tracing::info!("emitted {emit_count} jsonl-line events (skipped {skip_count})");
                                        }
                                    }
                                    Err(e) => tracing::warn!("emit jsonl-line failed: {e}"),
                                }
                            }
                        }
                        Ok(_) => {
                            skip_count += 1;
                        }
                        Err(e) => {
                            tracing::warn!("parse line failed in {}: {e}", line.path.display());
                        }
                    }
                }
                tracing::info!("watcher loop ended; total emit={emit_count} skip={skip_count}");
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
