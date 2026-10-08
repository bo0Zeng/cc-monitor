//! 「现在重启」：设置窗重启条与行内那一句点了之后，壳重起 cc-monitor 自己。
//!
//! 走 Tauri 的 `request_restart`：经事件循环发出退出，`lib.rs` 那一臂照常跑（本机后端照它自己那份退出行为去留、单实例锁随退出放掉），
//! 跑完再起一个新的 cc-monitor。会打断什么由界面先问后端（`machine-interrupts` 带 `appExit`），这里不判。

/// IPC：重起 cc-monitor（不回来）。
#[tauri::command]
pub fn restart_app(handle: tauri::AppHandle) {
    tracing::info!("设置窗点了「现在重启」：重起 cc-monitor");
    handle.request_restart();
}
