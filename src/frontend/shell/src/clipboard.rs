//! 写系统剪贴板：全产品的复制（复制详情 · 复制命令 · 代码块复制 …）只这一个家（界面经 [`clipboard_write`]，`src/frontend/ui/clipboard.ts`）。
//!
//! 为什么不让网页自己写（`navigator.clipboard.writeText`）：WebView2 在系统剪贴板被别的程序占着（打开不关）时照样回成功，
//! 界面就说「已复制」而剪贴板里还是旧的（WIN5 · 10-08 虚拟机现打）。这里走系统接口（arboard，各平台的做法在它里面），回的是真成败：
//! - Windows：`OpenClipboard` 被占着 ⇒ 重试几次仍打不开回错（`ClipboardOccupied`）。
//! - Linux：Wayland 合成器支持 data-control 那一档 ⇒ 经它写；不支持 ⇒ 经 X11（含 XWayland）写，X 服务器确认拿到了所有权才算成；
//!   两样都没有 ⇒ 回错（界面走「就地展开全选」）。X11 上内容由持有者那一侧供给，所以那份句柄常驻进程，不随一次写丢掉。

use crate::copy_table::copy_text;
use crate::detail::Said;
use std::sync::Mutex;

/// 常驻的那份句柄（第一次写时建；写失败就丢掉，下一次重建 —— 合成器 / X 服务器换了也接得上）。
static BOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// 把 `text` 写进系统剪贴板。写不进 ⇒ 下层原话。
fn write_text(text: &str) -> Result<(), String> {
    let mut slot = BOARD.lock().unwrap_or_else(|p| p.into_inner());
    let mut board = match slot.take() {
        Some(b) => b,
        None => arboard::Clipboard::new().map_err(|e| e.to_string())?,
    };
    board.set_text(text).map_err(|e| e.to_string())?;
    *slot = Some(board);
    Ok(())
}

/// IPC：把 `text` 写进系统剪贴板；写不进 ⇒「写不进剪贴板」＋ 原话（界面据此走就地展开全选，不说「已复制」）。
#[tauri::command]
pub async fn clipboard_write(text: String) -> Result<(), Said> {
    let r: Result<(), Said> = async move {
        tokio::task::spawn_blocking(move || write_text(&text))
            .await
            .map_err(Said::crashed)?
            .map_err(|raw| Said::with_raw(copy_text("rsShellCmd.clipboard.failed", &[]), raw))
    }
    .await;
    r.map_err(|s| s.named("clipboard_write"))
}
