mod auto_launch;
mod bind;
mod bridge;
mod config;
mod event_replay;
mod history;
mod logging;
mod messages;
mod parser;
mod paths;
mod profile_installer;
mod session_map;
mod subagent;
mod utils;
mod watcher;

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Listener, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // v2.0.0 (issue #4)：tracing 初始化提前到 Builder 之前 —— 一旦 init 全局
    // dispatcher 锁死，且我们要捕获 setup() 期间的所有 log。
    //
    // logging 模块内部把所有复杂度（rolling file appender / non_blocking writer /
    // ErrorEmitterLayer / EnvFilter reload）封死，对外只暴露 init + state。
    //
    // **monitor_data_dir 必须能解析**：这里用 dirs::home_dir 兜底，不依赖任何
    // 配置（避免 log 初始化跟 config 初始化循环依赖）。
    let monitor_data_dir = paths::resolve_monitor_data_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("cc-monitor-fallback"));
    let logging_state = logging::init(&monitor_data_dir);
    tracing::info!(
        "cc-monitor starting (data_dir={}, log_dir={})",
        monitor_data_dir.display(),
        logging_state.log_dir().display()
    );

    // setup 闭包是 FnOnce + 'static，必须 move-capture。把 logging_state shadow 进闭包，
    // 闭包内同时 install_error_emitter（&self 借用）+ app.manage(clone)
    let logging_state = logging_state;
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
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

            // monitor 自己的数据目录：~/.claude/claudecode-frontend
            let monitor_data_dir = paths::resolve_monitor_data_dir().ok_or("no data dir")?;
            tracing::info!("monitor_data_dir: {}", monitor_data_dir.display());

            // v2.0.0：把 AppHandle 注入给 ErrorEmitterLayer（之前一直是 None，
            // setup 期间的 ERROR 只写 log；从这里开始 ERROR 才会弹前端 toast）
            logging_state.install_error_emitter(app.handle().clone());

            // v1.7.1：把当前 exe 路径记到 auto-launch.json，让 cc function 能在用户启用
            // auto-launch 时主动启动 monitor（不硬编码安装路径）
            auto_launch::update_monitor_path_on_startup(&monitor_data_dir);

            // v1.7：BindRegistry 监听 ps-await/ → EnumWindows → 写 ps-registry/。
            // SidHwndCache 持久化 sid → 拉前所需信息（含复合指纹）。
            let bind_registry = bind::BindRegistry::spawn(monitor_data_dir.clone());
            let sid_hwnd_cache =
                bind::SidHwndCache::load(monitor_data_dir.join("sid-hwnd-cache.json"));

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
            //             + 调 SidHwndCache.record 把 sid → hwnd 绑定持久化
            //   - removed：透传 session-ended 给前端，Tab 灰显归档
            //              + 调 SidHwndCache.forget 清理过期 sid
            {
                let handle = app.handle().clone();
                let session_map_for_emitter = session_map.clone();
                let bind_for_emitter = bind_registry.clone();
                let cache_for_emitter = sid_hwnd_cache.clone();
                let spawned = std::thread::Builder::new()
                    .name("session-changes-emitter".into())
                    .spawn(move || {
                        while let Ok(change) = session_changes.recv() {
                            for sid in &change.added {
                                tracing::info!("session added: {sid}, triggering jsonl rescan");
                                if let Err(e) = force_rescan_tx.send(sid.clone()) {
                                    tracing::warn!("force_rescan send failed for {sid}: {e}");
                                }
                                // 尝试绑定 sid → hwnd（通过 claude_pid 的 parent PS）
                                if let Some(info) = session_map_for_emitter.lookup(sid) {
                                    let _ =
                                        cache_for_emitter.record(sid, info.pid, &bind_for_emitter);
                                }
                            }
                            for sid in change.removed {
                                cache_for_emitter.forget(&sid);
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

            // 给 Tauri 命令暴露 state。
            //
            // **v1.7.4 修回归**：v1.6.7 撤 bring_terminal_to_front 时把
            // `app.manage(session_map.clone())` 也删了，但 history 命令
            // （list_history_projects / list_history_sessions_in_project）也接
            // `State<Arc<SessionMap>>`，导致历史浏览器打不开，报"state not managed
            // for field `map`"。这里补回去。
            app.manage(session_map.clone());
            app.manage(replay.clone());
            app.manage(bind_registry.clone());
            app.manage(sid_hwnd_cache.clone());
            // v2.0.0 (issue #4)：logging state 也要 manage，IPC handler 才能拿到
            app.manage(logging_state.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::load_config,
            config::save_config,
            subagent::load_subagent,
            forget_session,
            bring_terminal_to_front,
            cc_integration_status,
            cc_integration_preview,
            cc_integration_scan_path,
            cc_integration_install,
            cc_integration_uninstall,
            cc_get_auto_launch,
            cc_set_auto_launch,
            // v2.0.0 (issue #4): 诊断 / log
            get_diagnostics_config,
            set_diagnostics_config,
            get_log_file_info,
            open_log_file,
            open_log_dir,
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

/// v1.7：拉对应终端窗口。
///
/// 流程：sid → 查 SidHwndCache → 校验复合指纹（IsWindow + owner_pid + procStart）
/// → activate_window。
///
/// **必须 async + spawn_blocking** 隔离 Win32 sync 调用（v1.6.5 的教训）。
#[tauri::command]
async fn bring_terminal_to_front(
    session_id: String,
    cache: tauri::State<'_, Arc<bind::SidHwndCache>>,
) -> Result<(), String> {
    let cache = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        let binding = cache.lookup(&session_id).ok_or_else(|| {
            format!(
                "session {session_id} 未绑定窗口。用 cc 命令启动 claude 才能拉前；\
                 cc 集成的安装见设置面板。"
            )
        })?;
        bind::verify_binding(&binding)?;
        bind::activate(binding.hwnd)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// === v1.7：PowerShell profile cc 集成 IPC ===

#[derive(serde::Serialize)]
struct CcStatusResponse {
    profiles: Vec<profile_installer::ProfileScan>,
    active_registrations: u32,
    default_command_name: &'static str,
    /// v1.7.0-1.7.1 错把 cc 块装到 profile.ps1（CurrentUserAllHosts，PS 不自动加载）
    /// 的遗留文件列表。v1.7.2 起改装到 Microsoft.PowerShell_profile.ps1（默认 $PROFILE）。
    /// UI 检测到非空时显示警告，引导用户清理。
    legacy_profile_paths_with_block: Vec<LegacyProfileEntry>,
}

#[derive(serde::Serialize)]
struct LegacyProfileEntry {
    kind: profile_installer::ProfileKind,
    path: String,
}

/// 扫描两个 PS profile + 报告当前活跃注册数。前端打开设置面板时调用。
#[tauri::command]
async fn cc_integration_status(
    command_name: Option<String>,
    bind_state: tauri::State<'_, Arc<bind::BindRegistry>>,
) -> Result<CcStatusResponse, String> {
    let cmd = command_name.unwrap_or_else(|| "cc".to_string());
    let bind_state = bind_state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let profiles: Vec<_> = profile_installer::discover_profiles()
            .into_iter()
            .map(|(kind, path)| profile_installer::scan_profile(kind, &path, &cmd))
            .collect();
        let legacy = profile_installer::scan_legacy_profiles()
            .into_iter()
            .map(|(kind, path)| LegacyProfileEntry { kind, path })
            .collect();
        Ok(CcStatusResponse {
            profiles,
            active_registrations: bind_state.registration_count() as u32,
            default_command_name: "cc",
            legacy_profile_paths_with_block: legacy,
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[derive(serde::Serialize)]
struct CcPreviewResponse {
    code: String,
}

/// 返回将要写入 profile 的代码（含 BEGIN/END marker）。前端预览 modal 显示。
///
/// `include_cc_function` 控制是否生成完整 cc function（true）还是只装 helper（false）。
#[tauri::command]
fn cc_integration_preview(
    command_name: String,
    include_cc_function: bool,
) -> Result<CcPreviewResponse, String> {
    Ok(CcPreviewResponse {
        code: profile_installer::render_cc_code(&command_name, include_cc_function),
    })
}

/// 安装 cc function 到指定 path（前端自己组装路径——版本下拉 + 可编辑覆盖）。
/// idempotent；已有 ccm 块则原地替换。
///
/// `include_cc_function = false` 时只装 `__ccm_bind` helper，避免覆盖用户已有的 cc。
#[tauri::command]
async fn cc_integration_install(
    path: String,
    command_name: String,
    include_cc_function: bool,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(path);
        profile_installer::install_to_profile(&p, &command_name, include_cc_function)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 扫单个 path 的安装状态（前端用户改了路径后调）。
#[tauri::command]
async fn cc_integration_scan_path(
    path: String,
    command_name: String,
) -> Result<profile_installer::ProfileScan, String> {
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(path);
        Ok(profile_installer::scan_path(&p, &command_name))
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 读 auto-launch.json：UI 显示当前 toggle 状态 + 记录的 exe 路径。
#[tauri::command]
fn cc_get_auto_launch() -> Result<auto_launch::AutoLaunchConfig, String> {
    let dir = auto_launch::data_dir().ok_or("no data dir")?;
    Ok(auto_launch::get_config(&dir))
}

/// UI toggle 改变时调：写 auto_launch_enabled。
#[tauri::command]
fn cc_set_auto_launch(enabled: bool) -> Result<(), String> {
    let dir = auto_launch::data_dir().ok_or("no data dir")?;
    auto_launch::set_enabled(&dir, enabled)
}

/// 卸载 cc function（删除 BEGIN/END 块；用户其他内容不动）。
#[tauri::command]
async fn cc_integration_uninstall(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(path);
        profile_installer::uninstall_from_profile(&p)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// ===== v2.0.0 (issue #4): 诊断 / log IPC =====

/// 读当前 diagnostics 配置。设置面板打开时调一次。
#[tauri::command]
fn get_diagnostics_config(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::DiagnosticsConfig, String> {
    Ok(state.config())
}

/// 应用新 diagnostics 配置。日志级别 + error_toast 立即生效；
/// log_enabled / max_files 改了返回 `NeedsRestart` 让前端提示用户重启。
#[tauri::command]
fn set_diagnostics_config(
    cfg: logging::DiagnosticsConfig,
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::RestartHint, String> {
    state.update_config(cfg)
}

/// 返回 log 目录 + 当前 log 文件 + 全部 .log 文件列表（path / size / mtime）。
/// 设置面板用来显示路径 + 文件大小，让用户一眼看到 log 状态。
#[tauri::command]
fn get_log_file_info(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::LogFileInfo, String> {
    Ok(state.log_file_info())
}

/// 用系统默认编辑器打开当前 log 文件（rolling::daily 写入的 mtime 最新那个）。
/// 失败常见原因：log_enabled=false 还没生成过 log 文件 → Err 让前端 alert 提示。
#[tauri::command]
async fn open_log_file(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<(), String> {
    let path = state
        .current_log_file()
        .ok_or_else(|| "没有 log 文件（log 文件未启用或还没产生）".to_string())?;
    let path_str = path.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || open_with_os(&path_str))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 用资源管理器打开 log 目录。
#[tauri::command]
async fn open_log_dir(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<(), String> {
    let dir = state.log_dir();
    // 目录可能还不存在（log_enabled=false 时不创建）
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("create log dir: {e}"))?;
    }
    let dir_str = dir.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || open_with_os(&dir_str))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 跨平台调系统默认 opener。Windows 用 `cmd /C start ""` 兜 path 中的空格。
/// 复用 tauri-plugin-opener 也行（前端就是走它），但这里在 Rust 端直接调更直接。
fn open_with_os(path_or_dir: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path_or_dir])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("start failed: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path_or_dir)
            .spawn()
            .map_err(|e| format!("open failed: {e}"))?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path_or_dir)
            .spawn()
            .map_err(|e| format!("xdg-open failed: {e}"))?;
        Ok(())
    }
}
