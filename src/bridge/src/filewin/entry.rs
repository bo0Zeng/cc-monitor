//! `24e` 第二刀：**用户可达的入口** —— 那唯一一条 `#[tauri::command]`。
//!
//! 第一刀落地之后这个窗口只能由代码调 [`super::shell::open_detached`]
//! ⇒ 界面上没有任何地方点得开它。本模块补的就是那一格。
//!
//! # 🔴 一、为什么先列一趟目录、再开窗（这一条是承重的）
//!
//! 最省事的写法是「`spawn` 一条线程就返回 `Ok(())`」。**那是一条静默成功**：
//! 开窗失败（没有图形会话 / DISPLAY 不对）与远端连不上，在 webview 那侧
//! 长得跟成功一模一样 —— 用户点了按钮，什么都没发生，也没有任何一句话。
//!
//! ⇒ 本命令**先真的把那个目录列出来**（远端走 [`super::source::list_remote`]，
//! 也就是共用那条池），列不出来就**带着原文报错返回**，webview 那侧照旧弹它的失败提示；
//! 列出来了才开窗，而且把**那一屏直接交给窗口**（[`super::shell::open_detached_seeded`]）
//! ⇒ 窗口一出来就是有内容的，也不会对同一个目录连打两次往返。
//!
//! ⚠ **它买不到「窗口真的出现在屏幕上」**。那一格要一个图形会话，本机
//! `XDG_SESSION_TYPE=tty`（`真相源/99 §一`）⇒ 本机永远量不到。
//! 能确定地量到的是「开窗请求发出去了」（[`super::shell::open_requested`]），
//! 与「这个目录此刻列得出来」（回值那个行数）。**两件都不是「窗口在屏幕上」，别读宽。**
//!
//! # 二、为什么回一个行数，而不是 `()`
//!
//! 回 `()` 的话前端拿不到任何可说的东西，那条「点了之后出声」的链就断在包装层。
//! 回**这一趟列到的行数** ⇒ 前端那句提示里的数是**真读数**，不是文案。
//! ⚠ 刻意不回一个结构体：`设计` 那条纪律是「返回类型只在 TS 侧真消费字段时才生成」
//! （`tests/ipc/commands.vitest.ts` 头注逐字），一个 `usize` 不需要 `ts-rs`。
//!
//! # 🔴 三、签名为什么只吃 `RemoteConfig`（而窗口自己两侧都会）
//!
//! 窗口的数据面两侧都通（`Source::Local` / `Source::Remote`），
//! 但**这条命令只开远端那一侧**，因为界面上点得到它的地方只有一个 ——
//! 旧 SFTP 面板的表头（见 `src/sftp/panel.ts`），而那块面板本来就是远端专用的。
//!
//! ⇒ 于是 `parity_ledger` 那一行签的是 `Side::Remote`，**签的是实况不是愿望**。
//! 本机那一侧今天的可达路径是**窗口自己那颗「本机」按钮**
//! （[`super::shell::FileWindow::go_local`]）—— 它不是第二条 Tauri 命令，
//! 所以它不进那张表；如实记在这儿，别以为本机侧没人走得到。
//!
//! ⚠ 顶栏那个 SFTP 入口（`src/main.ts::openSftpFromTopbar`，0 台提示 / 1 台直开 / 多台选单）
//! **这一刀没碰** —— `src/main.ts` 不在本刀写区。要把原生窗口接到顶栏上，
//! 得连那颗按钮一起改，那是下一刀的事（而且那一刀正好是「旧面板退役」那一刀）。

use crate::ssh_source::RemoteConfig;

use super::shell::open_detached_seeded;
use super::source::{list_remote, Source};

/// 在**原生窗口**里打开远端 `path` 这个目录。
///
/// 回值 = 这一趟列到的行数。⚠ 它是「开窗那一刻那个目录有多少项」，
/// 不是「窗口里现在有多少行」（窗口自己会刷新、会换目录）。
///
/// # Errors
///
/// - 路径是空的 ⇒ 立刻回错（**不拿 `.` 兜底**：远端的 `.` 归谁解释是 `sftp_realpath`
///   的事，在这里猜一个默认值就是把两处的规矩写成两份）。
/// - 目录列不出来（连不上 / 没权限 / 不是目录）⇒ 把 `sftp_pool` 那边的原文带回去。
#[tauri::command]
pub async fn open_file_window(cfg: RemoteConfig, path: String) -> Result<usize, String> {
    if path.trim().is_empty() {
        return Err("要打开哪个目录？路径是空的".into());
    }
    // ① 先真的列一趟 —— 走共用那条池（同进程、无 IPC）。列不出来就别开窗。
    let rows = list_remote(&cfg, &path).await?;
    let n = rows.len();
    // ② 拿着这一屏开窗。⚠ 不 `join` 那个句柄：`run_native` 要阻塞到窗口关闭，
    //    等它就等于把 Tauri 的 async 运行时挂在一个窗口的寿命上。
    let _ = open_detached_seeded(
        Source::Remote(Box::new(cfg)),
        path,
        tokio::runtime::Handle::try_current().ok(),
        rows,
    );
    Ok(n)
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/entry_tests.rs"]
mod tests;
