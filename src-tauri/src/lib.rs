mod auto_launch;
mod bind;
mod bridge;
mod config;
mod data_paths;
mod event_replay;
mod history;
mod logging;
mod messages;
mod parser;
mod paths;
mod profile_installer;
mod session_map;
mod subagent;
mod tasks;
mod utils;
mod watcher;

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Listener, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 启动 perf 测量起点
    let t0 = std::time::Instant::now();

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
        "[perf] T+{}ms cc-monitor starting (data_dir={}, log_dir={})",
        t0.elapsed().as_millis(),
        monitor_data_dir.display(),
        logging_state.log_dir().display()
    );

    // setup 闭包是 FnOnce + 'static，必须 move-capture。把 logging_state shadow 进闭包，
    // 闭包内同时 install_error_emitter（&self 借用）+ app.manage(clone)
    let logging_state = logging_state;

    // issue #9：single-instance lock。**必须是第一个 plugin**（Tauri 官方 plugin 要求）。
    // 第二个 cc-monitor 实例启动 → 触发本回调（在第一个实例里跑）→ 把主窗口
    // unminimize + show + set_focus → 第二个实例立即退出（plugin 内部处理）。
    // 详 doc/INVARIANTS.md § 16。
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    #[cfg(windows)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tracing::info!("second cc-monitor instance detected, bringing main window to front");
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                let _ = win.set_focus();
            }
        }));
    }

    builder
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
            // v2.3.0 issue #11：Claude Code CLI 的 task tracker 文件根
            let tasks_dir = claude_dir.join("tasks");

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

            // v2.4 (修首次启动乱序)：watcher 直接调 on_line 回调（取代之前的 mpsc
            // 中间层）。回调内同步 parse + record() —— history buffer 在 watcher
            // 线程内同步落盘，初始全量扫完成 = history 完整 = frontend-ready 触发
            // replay 时 snapshot 一定完整。
            //
            // 旧设计：watcher tx → mpsc → tauri::async_runtime::spawn drain → record。
            // async drain 跟 frontend-ready 是竞态：drain 没追上时 snapshot 不完整，
            // 部分历史漏到 live emit 路径 → 跟 chunked replay 错位 → 首次启动乱序。
            // F5 因 backend 已稳定看不到 bug。详 watcher.rs::spawn_watcher 注释。
            let replay = Arc::new(event_replay::EventReplay::new());
            // v2.4.2 issue #2: watcher 改成一次 process_file 给一批 lines，
            // lib.rs 这里 parse 整批后一次 on_line_batch 给 EventReplay。
            // EventReplay 按 batch 大小分流（详 event_replay::on_line_batch 注释）。
            let on_batch: watcher::BatchHandler = {
                let replay = replay.clone();
                let handle = app.handle().clone();
                Arc::new(move |lines: Vec<watcher::JsonlLine>| {
                    let mut payloads = Vec::with_capacity(lines.len());
                    for line in lines {
                        match parser::parse_line(&line.raw) {
                            Ok(Some(record)) if record.is_displayable() => {
                                let cwd = extract_cwd(&record);
                                payloads.push(bridge::JsonlLinePayload {
                                    session_id: line.session_id.clone(),
                                    cwd,
                                    path: line.path.to_string_lossy().into_owned(),
                                    message: record,
                                });
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!(
                                    "parse line failed in {}: {e}",
                                    line.path.display()
                                );
                            }
                        }
                    }
                    replay.on_line_batch(&handle, payloads);
                })
            };
            let watcher_handle = watcher::spawn_watcher(projects_dir, active_filter, on_batch);
            let force_rescan_tx = watcher_handle.force_rescan_tx;
            let initial_scan_done = watcher_handle.initial_scan_done;

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

            // v2.3.0 issue #11：监听 task 文件变更，per-session 重读后 emit 给前端。
            // 不依赖 SessionMap，独立 watcher。tasks_dir 不存在时函数内部 no-op。
            tasks::spawn_task_watcher(tasks_dir.clone(), app.handle().clone());

            // 前端 ready 事件 → 等 watcher 初始扫完成 → replay all。
            //
            // v2.4 修首次启动乱序：之前 listener 直接调 replay()，但 watcher 是
            // 异步全量扫，snapshot 时 history 不完整 → 部分历史漏到 live emit 路径
            // → 跟 chunked replay 错位。现在 listener 在 async task 里 spin-wait
            // `initial_scan_done`，扫完才 snapshot，保证 chunked replay 包含全部历史。
            //
            // 等待用 10ms 间隔 poll，整体 timeout 10s（防 watcher 死锁卡死整个 UI
            // 永远看不到内容）。timeout 到也会强行 replay，degraded but unblocked。
            {
                let replay = replay.clone();
                let handle = app.handle().clone();
                let initial_scan_done = initial_scan_done.clone();
                let t0_capture = t0;
                app.listen("frontend-ready", move |_event| {
                    let replay = replay.clone();
                    let handle = handle.clone();
                    let initial_scan_done = initial_scan_done.clone();
                    let listen_recv_at = t0_capture.elapsed().as_millis();
                    tauri::async_runtime::spawn(async move {
                        tracing::info!(
                            "[perf] T+{}ms frontend-ready received, waiting for watcher initial scan",
                            listen_recv_at
                        );
                        let wait_started = std::time::Instant::now();
                        const WAIT_TIMEOUT: std::time::Duration =
                            std::time::Duration::from_secs(10);
                        while !initial_scan_done
                            .load(std::sync::atomic::Ordering::Acquire)
                        {
                            if wait_started.elapsed() > WAIT_TIMEOUT {
                                tracing::warn!(
                                    "watcher initial scan timed out after 10s; replay with partial history"
                                );
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        }
                        tracing::info!(
                            "[perf] T+{}ms watcher initial scan done (+{}ms wait), starting replay",
                            t0_capture.elapsed().as_millis(),
                            wait_started.elapsed().as_millis()
                        );
                        replay.replay_and_mark_ready(&handle);
                    });
                });
            }

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

            tracing::info!(
                "[perf] T+{}ms setup() completed (watchers spawned, state managed)",
                t0.elapsed().as_millis()
            );

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::load_config,
            config::save_config,
            subagent::load_subagent,
            forget_session,
            bring_terminal_to_front,
            // v2.4 issue #2: 用户在终端输入时可选拉前 monitor 自身
            bring_monitor_to_front,
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
            history::stream_history_sessions_in_project,
            history::read_session_jsonl,
            history::stream_read_session_jsonl,
            history::delete_history_session,
            history::update_history_metadata,
            history::resume_history_session,
            // v2.3.0 issue #11: task 面板初次拉
            tasks::get_session_tasks,
            // v2.3.0 issue #3 (A 透明化): 设置面板「数据」区列出所有持久路径
            data_paths::get_data_paths,
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

/// v2.4 (issue #2)：把 monitor 自己的主窗口拉到最前 + unminimize + 抢焦点。
///
/// 用途：用户在终端敲键时，前端 user-active 信号路径下，若用户开了「拉前
/// monitor 窗口」toggle 就 invoke 这个 IPC 让 monitor 主动浮上来。
///
/// **核心问题**：用户敲终端时前台是 PS/WT，**monitor 不是前台进程** →
/// `SetForegroundWindow` 直接调被 OS 拒绝（只闪任务栏图标）。这是 Windows
/// 对前台抢焦的设计限制（防恶意软件偷焦点）。
///
/// **解法 = AttachThreadInput hack**：临时把当前线程附加到前台线程的输入
/// 队列，OS 把它俩视作"同输入上下文" → 借用前台线程的拉前权限 →
/// SetForegroundWindow 通过 → 立刻 detach。广泛使用的可靠 hack
/// （Visual Studio / 各 IDE 都用），不被 OS 视为恶意。
///
/// v2.4.0 直接用 win.set_focus()（内部就是 SetForegroundWindow）必败，
/// v2.4.1 hotfix 改这版。
///
/// Tauri 内部用 windows crate 0.61（HWND.0 = *mut c_void），我们 0.56
/// （HWND.0 = isize）；用 `as isize` cast 跨版本兼容。
#[cfg(windows)]
#[tauri::command]
fn bring_monitor_to_front(app: tauri::AppHandle) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_MENU,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
        SetForegroundWindow, SetWindowPos, ShowWindow, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE,
        SWP_NOSIZE, SW_RESTORE, SW_SHOW,
    };

    tracing::info!("bring_monitor_to_front: invoked");

    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let tauri_hwnd = win.hwnd().map_err(|e| format!("hwnd: {e}"))?;
    // 跨 windows crate 版本：tauri 0.61 *mut c_void → isize → 0.56 HWND
    let hwnd_value = tauri_hwnd.0 as isize;

    unsafe {
        let h = HWND(hwnd_value);
        tracing::info!("bring_monitor_to_front: monitor hwnd = {:#x}", hwnd_value);

        // === 三层 hack 突破 Win10/11 前台抢焦限制 ===
        //
        // Microsoft 在 Win10 1903+ 加固了 SetForegroundWindow：仅 AttachThreadInput
        // 不够，OS 会识别为绕过尝试拒绝。三层叠加才稳：
        //
        // 1. **keybd_event(Alt down)**：模拟用户级别 Alt 按键。OS 检测后将
        //    当前进程视为"刚有用户输入"，临时获得前台资格。
        //    配 down+up 配对几乎所有 Win32 应用都识别为 noop，副作用低。
        // 2. **AttachThreadInput**：附加到前台线程输入队列，共享其拉前权限。
        // 3. **SetWindowPos TOPMOST → NOTOPMOST**：触发 OS 重新计算 Z 序，
        //    强制窗口浮到栈顶（即使 SetForegroundWindow 失败也至少在视觉上覆盖）。
        //
        // Visual Studio / PowerToys / TranslucentTB 等都用此套组合。

        // 1. ShowWindow 先做：可能 minimize 状态
        if IsIconic(h).as_bool() {
            tracing::info!("bring_monitor_to_front: window iconic, SW_RESTORE");
            let _ = ShowWindow(h, SW_RESTORE);
        } else {
            let _ = ShowWindow(h, SW_SHOW);
        }

        // 2. 模拟 Alt 按键（down 阶段，up 在末尾）
        keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_EXTENDEDKEY, 0);

        // 3. AttachThreadInput
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let cur_thread = GetCurrentThreadId();
        tracing::info!(
            "bring_monitor_to_front: fg_hwnd={:#x} fg_thread={} cur_thread={}",
            fg.0,
            fg_thread,
            cur_thread
        );
        let attached = fg_thread != 0
            && fg_thread != cur_thread
            && AttachThreadInput(fg_thread, cur_thread, true).as_bool();

        // 4. TOPMOST 强制 Z 序拉顶 + BringWindowToTop
        let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        let _ = SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        let _ = BringWindowToTop(h);

        // 5. SetForegroundWindow 真正抢焦
        let ok = SetForegroundWindow(h).as_bool();
        tracing::info!(
            "bring_monitor_to_front: attached={} SetForegroundWindow={}",
            attached,
            ok
        );

        // 6. detach + Alt 释放
        if attached {
            let _ = AttachThreadInput(fg_thread, cur_thread, false);
        }
        keybd_event(
            VK_MENU.0 as u8,
            0,
            KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP,
            0,
        );

        if ok {
            Ok(())
        } else {
            // 拉前真失败也 Z 序已被推顶，视觉上窗口浮起来了
            // （只是焦点没抢到）。给前端 warn 但不视为 fatal。
            tracing::warn!("bring_monitor_to_front: SetForegroundWindow rejected (window Z-order raised but no focus)");
            Err("SetForegroundWindow rejected (window raised but not focused)".into())
        }
    }
}

#[cfg(not(windows))]
#[tauri::command]
fn bring_monitor_to_front(app: tauri::AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let _ = win.unminimize();
    let _ = win.show();
    win.set_focus().map_err(|e| format!("set_focus: {e}"))
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
async fn open_log_file(state: tauri::State<'_, Arc<logging::LoggingState>>) -> Result<(), String> {
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
async fn open_log_dir(state: tauri::State<'_, Arc<logging::LoggingState>>) -> Result<(), String> {
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
