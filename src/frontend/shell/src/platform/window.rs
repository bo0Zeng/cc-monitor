//! 窗口这一族的平台差异：壳里平台 cfg 的唯一住址（同 [`super::fs`]）：[`desktop_fixes`]（`run()` 里那段 `cfg(windows)` 块）· [`bring_to_front`]（两个平台臂）。

use tauri::Manager;

/// nudge skip 判定的纯函数对。`pack_nudge_state`：终态物理尺寸 + fullscreen 位打包成一个可比较状态值。
/// fullscreen 占 bit 63（F11 borderless 全屏与 maximize 在「自动隐藏任务栏」下 inner 尺寸可能相同 —— 状态位保证这类高危过渡不被 skip）；
/// 宽度截 31 位（物理像素远小于 2^31）。只有本文件 Windows 那一臂调它；`test` 也编，好在 Linux 上跑那几条纯函数单测。
#[cfg(any(windows, test))]
fn pack_nudge_state(w: u32, h: u32, fullscreen: bool) -> u64 {
    ((fullscreen as u64) << 63) | (((w as u64) & 0x7FFF_FFFF) << 32) | h as u64
}

/// skip 当且仅当：曾经 nudge 过（last != 0）且 (尺寸+全屏态) 与上次执行完的
/// nudge 完全一致——典型即"最小化→恢复"。0 是安全哨兵：真实窗口尺寸非零，
/// pack 结果不可能为 0（0×0 在事件入口与 settle 双重滤除）。
#[cfg(any(windows, test))]
fn nudge_should_skip(last_nudged: u64, packed: u64) -> bool {
    last_nudged != 0 && last_nudged == packed
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/lib_nudge_skip_tests.rs"]
mod nudge_skip_tests;

/// Windows：WebView2 最大化 / 全屏后内容错位的修复（resize 去抖后三板斧）。别处原样返回。
#[cfg(windows)]
pub fn desktop_fixes(mut builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    // WebView2 maximize / 全屏后内容错位修复。根因在 WebView2 Runtime 内部：maximize / restore / 全屏切换后丢失 / 挂起对宿主 bounds 更新的处理
    // （WebView2Feedback #4095 族）：宿主侧 put_Bounds 成功、容器 HWND 已是全尺寸，但合成层（「Intermediate D3D Window」）停在旧尺寸 → 内容不铺满、周围留白。
    // ±1px 抖动会被 Runtime 合并 / 丢弃，所以用 controller 级三板斧（with_webview 闭包内直接 COM 调用）：
    // 1. 双 rect SetBounds（h-1 → h）：让 Runtime 看到「变化后的 rect」重新 put_Bounds
    // 2. NotifyParentWindowPositionChanged：微软文档明示的宿主位置变化通知
    // 3. SetIsVisible(false→true) 翻转：强制重建 / 重挂合成 visual，对 #4095 族最有效；仅 maximize / fullscreen 时做（普通拖拽 resize 不翻，避免闪烁）
    //
    // 最小化守卫：tao 在 WM_SIZE(SIZE_MINIMIZED) 时发 Resized(0,0)，而 wry 自己的 subclass 跳过 SIZE_MINIMIZED —— 最小化时把 controller bounds 打成 0×0
    // 会让 renderer 视口归零、进入挂起态，restore 后输入 hit-test 层数秒才重建。入口按 0×0 早退 + 线程动作前二次守卫（去抖期间可能又被最小化）。
    //
    // 去抖：resize 期间每个事件 bump 一个 generation；后台线程等到连续 60ms 没有新事件再动手。nudge_pending 保证一个突发 resize 只有一个去抖线程在飞。
    // with_webview 的闭包由 tauri 派发到主线程执行 —— 闭包内只做 COM 调用、禁止 sleep。
    builder = builder.on_window_event({
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::Arc;
        use std::time::Duration;
        let resize_gen = Arc::new(AtomicU64::new(0));
        let nudge_pending = Arc::new(AtomicBool::new(false));
        // 上次 nudge 闭包执行完毕时的 (尺寸+全屏态) 打包值（pack_nudge_state；0 = 从未）。最小化→恢复回到同状态时合成层没有错位理由
        // （#4095 是过渡 bug），却会因 is_maximized() 为 true 走 SetIsVisible 翻转 → 瞬间露白底 ⇒ 同状态直接 skip 全部 COM 动作。
        // ① store 在 with_webview 闭包尾执行（单个 COM 调用失败仍记录；派发失败才不记录）；② 拖拽一圈回到原尺寸的 settle 也会被 skip（残余风险接受）。
        // 全屏与 maximize 同 inner 尺寸的角例由打包值里的 fullscreen 位区分。
        let last_nudged = Arc::new(AtomicU64::new(0));
        move |window, event| {
            let size = match event {
                tauri::WindowEvent::Resized(s) => *s,
                _ => return,
            };
            // 最小化：绝不动 webview bounds（见块头 ⚠），也不 bump gen
            if size.width == 0 && size.height == 0 {
                return;
            }
            resize_gen.fetch_add(1, Ordering::SeqCst);
            // 已有一个去抖线程在飞 → 它会读到新的 gen 自行续等，不再 spawn
            if nudge_pending.swap(true, Ordering::SeqCst) {
                return;
            }
            let window = window.clone();
            let resize_gen = resize_gen.clone();
            let nudge_pending = nudge_pending.clone();
            let last_nudged = last_nudged.clone();
            std::thread::spawn(move || {
                let mut last = resize_gen.load(Ordering::SeqCst);
                loop {
                    std::thread::sleep(Duration::from_millis(60));
                    let now = resize_gen.load(Ordering::SeqCst);
                    if now == last {
                        break;
                    }
                    last = now;
                }
                nudge_pending.store(false, Ordering::SeqCst);
                // 二次守卫：去抖期间窗口可能又被最小化 / 尺寸归零
                if window.is_minimized().unwrap_or(false) {
                    tracing::info!("nudge skip: window minimized during debounce");
                    return;
                }
                let target = match window.inner_size() {
                    Ok(t) if t.width > 0 && t.height > 0 => t,
                    _ => {
                        tracing::info!("nudge skip: zero/unknown inner_size");
                        return;
                    }
                };
                let maximized = window.is_maximized().unwrap_or(false);
                let fullscreen = window.is_fullscreen().unwrap_or(false);
                let flip = maximized || fullscreen;
                // Batch7-F23A：同(尺寸+全屏态) skip（典型 = 最小化恢复）。
                // 判定与打包是纯函数（单测见 nudge_skip_tests）。
                let packed =
                    pack_nudge_state(target.width, target.height, fullscreen);
                if nudge_should_skip(last_nudged.load(Ordering::SeqCst), packed) {
                    tracing::info!(
                        "nudge skip: size+state unchanged {}x{} fs={fullscreen} (restore-from-minimize path)",
                        target.width,
                        target.height
                    );
                    return;
                }
                let Some(webview) = window.webviews().into_iter().next() else {
                    tracing::warn!("nudge skip: no webview on window");
                    return;
                };
                tracing::info!(
                    "nudge settle: target={}x{} maximized={maximized} fullscreen={fullscreen} flip={flip}",
                    target.width,
                    target.height
                );
                let last_nudged_in = last_nudged.clone();
                let res = webview.with_webview(move |pw| {
                    // RECT 必须来自 webview2-com 0.38 配对的 windows 0.61
                    // （windows-wv2 rename，见 Cargo.toml），0.56 的类型不互通
                    use windows_wv2::Win32::Foundation::RECT;
                    let controller = pw.controller();
                    let full = RECT {
                        left: 0,
                        top: 0,
                        right: target.width as i32,
                        bottom: target.height as i32,
                    };
                    let shrunk = RECT {
                        bottom: target.height.saturating_sub(1) as i32,
                        ..full
                    };
                    // 每个 COM 调用的失败单独 warn（不再静默）：理论上存在不对称失败
                    // ——如 SetIsVisible(false) 成功而 (true) 失败会让 webview 停在隐藏态，
                    // 无日志就无从取证。失败不中断后续调用（终态尽量推向可见+正确 bounds）。
                    unsafe {
                        if let Err(e) = controller.SetBounds(shrunk) {
                            tracing::warn!("nudge SetBounds(shrunk) failed: {e}");
                        }
                        if let Err(e) = controller.SetBounds(full) {
                            tracing::warn!("nudge SetBounds(full) failed: {e}");
                        }
                        if let Err(e) = controller.NotifyParentWindowPositionChanged() {
                            tracing::warn!("nudge NotifyParentWindowPositionChanged failed: {e}");
                        }
                        if flip {
                            if let Err(e) = controller.SetIsVisible(false) {
                                tracing::warn!("nudge SetIsVisible(false) failed: {e}");
                            }
                            if let Err(e) = controller.SetIsVisible(true) {
                                tracing::warn!("nudge SetIsVisible(true) failed: {e}");
                            }
                        }
                    }
                    // 闭包执行完毕才记录（with_webview 的 Ok 只代表"已派发到主
                    // 线程"——审计 D 修订：在这里 store 才是"执行完"的语义）
                    last_nudged_in.store(packed, Ordering::SeqCst);
                });
                if let Err(e) = res {
                    tracing::warn!("nudge with_webview failed: {e}");
                }
            });
        }
    });
    builder
}

/// 非 Windows：没有这一件（WebView2 那个错位是 Windows 专属）。
#[cfg(not(windows))]
pub fn desktop_fixes(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
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
pub async fn bring_to_front(app: tauri::AppHandle) -> Result<(), String> {
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

    // HWND 必须在 webview 所属线程里取（Tauri 内部约束），随即 cast 成
    // isize 跨线程，INVARIANTS § 19 跨 windows crate 版本约定。
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let tauri_hwnd = win.hwnd().map_err(|e| format!("hwnd: {e}"))?;
    let hwnd_value = tauri_hwnd.0 as isize;

    // INVARIANTS § 10：Win32 同步调用必须 spawn_blocking，否则慢路径会让 Tauri
    // IPC 派发线程排队（v2.4 起 autoFollowUserActive 高频触发该 IPC）。
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        unsafe {
            let h = HWND(hwnd_value);
            tracing::info!("bring_monitor_to_front: monitor hwnd = {:#x}", hwnd_value);

            // === 三层 hack 突破 Win10/11 前台抢焦限制 ===
            // 详 ARCHITECTURE.md § 5「bring_monitor_to_front 三层 hack」。
            // attach/detach + Alt down/up 同闭包内必须配对，整段在同一 blocking
            // 线程内串行，安全。

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
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg(not(windows))]
pub async fn bring_to_front(app: tauri::AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let _ = win.unminimize();
    let _ = win.show();
    win.set_focus().map_err(|e| format!("set_focus: {e}"))
}

/// 把主窗口拉回来（第二次启动 · 点了通知）：最小化着就还原、藏着就显示；`focus` ⇒ 再拉到前面。
/// Linux 带上启动方给的激活令牌（GTK `set_startup_id` ＋ `present`）：Wayland 上没有令牌桌面不让抢前台，只弹「已就绪」。
pub fn raise_main(app: &tauri::AppHandle, token: Option<String>, focus: bool) {
    use tauri::Manager;
    let handle = app.clone();
    let run = app.run_on_main_thread(move || {
        let Some(win) = handle.get_webview_window(crate::MAIN_WINDOW_LABEL) else {
            return;
        };
        if let Err(e) = win.unminimize() {
            tracing::info!("主窗口没还原：{e}");
        }
        if let Err(e) = win.show() {
            tracing::info!("主窗口没显示：{e}");
        }
        if !focus {
            return;
        }
        #[cfg(target_os = "linux")]
        match win.gtk_window() {
            Ok(gw) => {
                use gtk::prelude::GtkWindowExt;
                if let Some(t) = token.as_deref() {
                    gw.set_startup_id(t);
                }
                gw.present();
            }
            Err(e) => tracing::info!("拿不到主窗口：{e}"),
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = &token;
            if let Err(e) = win.set_focus() {
                tracing::info!("主窗口没拿到焦点：{e}");
            }
        }
    });
    if let Err(e) = run {
        tracing::info!("主窗口没拉到前面：{e}");
    }
}
