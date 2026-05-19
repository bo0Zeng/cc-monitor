//! 焦点切换 hook。
//!
//! 单独线程跑 `GetMessageW` 消息循环 + `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`。
//! 系统每次切换前台窗口时，hook 拿到 hwnd → `GetWindowThreadProcessId` 得 PID，
//! 通过 mpsc 发到 tokio 端。再由调用方（lib.rs）查 SessionMap 把 PID 映射到
//! session_id 后发 focus-switch 事件给前端。
//!
//! 解耦：本模块不引 SessionMap、不知道 session 概念，只负责把"当前前台窗口的
//! 进程 PID"实时推出来。

use std::sync::OnceLock;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// hook 回调是 C-ABI 函数指针，无法捕获闭包。用 OnceLock 暴露 Sender 给它。
static SENDER: OnceLock<UnboundedSender<u32>> = OnceLock::new();

/// 启动 hook 线程，返回前台 PID 接收端。
///
/// 只允许调用一次；二次调用会得到一个新的 channel 但 hook 线程仍持有第一个 Sender，
/// 实际不可用。lib.rs setup() 保证只调一次。
pub fn spawn_focus_thread() -> UnboundedReceiver<u32> {
    let (tx, rx) = mpsc::unbounded_channel::<u32>();
    if SENDER.set(tx).is_err() {
        tracing::warn!("focus thread already spawned");
        return rx;
    }

    #[cfg(windows)]
    {
        std::thread::Builder::new()
            .name("focus-hook".into())
            .spawn(focus_thread_main)
            .ok();
    }
    rx
}

#[cfg(windows)]
fn focus_thread_main() {
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMessageW, EVENT_SYSTEM_FOREGROUND, MSG, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    };

    unsafe {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        if hook.is_invalid() {
            tracing::error!("SetWinEventHook failed");
            return;
        }
        tracing::info!("focus hook installed");

        // OUTOFCONTEXT 模式下 hook 在本线程被回调，但仍需消息循环让线程活着。
        // PostQuitMessage 才会让 GetMessageW 返回 0 退出。我们不主动退，进程结束随之回收。
        let mut msg = MSG::default();
        loop {
            let r = GetMessageW(&mut msg, None, 0, 0);
            if r.0 == 0 || r.0 == -1 {
                break;
            }
        }

        let _ = UnhookWinEvent(hook);
    }
}

#[cfg(windows)]
unsafe extern "system" fn win_event_proc(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    hwnd: windows::Win32::Foundation::HWND,
    _id_object: i32,
    _id_child: i32,
    _id_thread: u32,
    _time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    if hwnd.0 == 0 {
        return;
    }
    let mut pid: u32 = 0;
    let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return;
    }
    if let Some(tx) = SENDER.get() {
        // 日志放在 send 前；CALLBACK 频率高（每次窗口切焦点），用 debug 级别
        tracing::debug!("focus hwnd → pid={pid}");
        let _ = tx.send(pid);
    }
}
