//! 系统通知的平台那一半：壳里发系统通知只有这一个家（界面经 `desktop_notify::notify_desktop`，壳自己经 [`show`]）。
//!
//! Linux：直调 notify-rust，发完把那条会话总线连接留到这条通知被关掉（[`on_close`] 在一条线程里等）——
//! GNOME 见发信人的总线名没了、而它又认得出是哪个有窗口的程序，就当场把通知关掉（L2 · 10-08 真窗口：
//! notification 插件每条新开一条连接、发完就丢 ⇒ `Notify` 之后十几毫秒 `NotificationClosed`）。
//! 带 `desktop-entry` 提示（安装包装的是 `cc-monitor.desktop`），桌面据它认出是谁、点通知时拉起谁。
//! 别处：照旧经 notification 插件。
//!
//! [`on_close`]: notify_rust::NotificationHandle::on_close

/// 安装包里那份 .desktop 的名字（不带后缀）：通知的 `desktop-entry` 提示。
#[cfg(target_os = "linux")]
const DESKTOP_ENTRY: &str = "cc-monitor";

/// 发一条系统通知。发不出去 ⇒ 一句原话（调用方决定说不说）。
pub fn show(app: &tauri::AppHandle, title: &str, body: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        let handle = notify_rust::Notification::new()
            .appname(DESKTOP_ENTRY)
            .summary(title)
            .body(body)
            .auto_icon()
            .hint(notify_rust::Hint::DesktopEntry(DESKTOP_ENTRY.to_string()))
            .show()
            .map_err(|e| e.to_string())?;
        // 等它被关掉（用户点掉 · 过期 · 桌面收走）才放连接；线程随之结束。
        std::thread::spawn(move || handle.on_close(|_: notify_rust::CloseReason| {}));
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| e.to_string())
    }
}
