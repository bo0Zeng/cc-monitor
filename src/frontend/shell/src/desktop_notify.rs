//! 系统通知那一条 Tauri 命令（「一轮完成」「需要你」）：界面判要不要发，壳只发 —— 平台那一半在 [`crate::platform::notify`]。

use crate::detail::Said;

/// IPC：发一条系统通知。
#[tauri::command]
pub async fn notify_desktop(
    app: tauri::AppHandle,
    title: String,
    body: String,
) -> Result<(), Said> {
    let r: Result<(), Said> = async move {
        tokio::task::spawn_blocking(move || crate::platform::notify::show(&app, &title, &body))
            .await
            .map_err(Said::crashed)??;
        Ok(())
    }
    .await;
    r.map_err(|s| s.named("notify_desktop"))
}
