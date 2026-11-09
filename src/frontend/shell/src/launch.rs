//! Batch14-F41：终端拉起。〔「待迁」最后一行〕monitor 这里**只开终端**。
//!
//! 两块：
//! 1. [`open_local`](crate::platform::terminal::open_local) —— 通用「新终端窗口跑一条命令」：Windows 是 PowerShell 窗口
//!    （wt.exe Plan A → CREATE_NEW_CONSOLE Plan B，`-NoExit -EncodedCommand`），POSIX 是挑到的终端里 `bash -lic`。本地与远端共用此单一入口。
//! 2. [`open_terminal_window`] —— 开窗那一条 Tauri 命令：**只开窗**。交来的是本机后端 `terminal-ssh` 的成品 ——
//!    远端是 `& ssh -t[ -J …] … -- '<bash -lic ''…''>'`，本机是那一串本身；本机远端同一问。
//!
//! 那一问只交那台的名字（界面直接问本机后端 `terminal-ssh`），monitor 不交机器事实。
//!
//! ## 引号与注入
//! - **远端命令**：渲染侧（后端载荷渲染器 / `terminal-ssh`）判 —— 控制字符 · 长度 · 双引号（PowerShell 5.1 向 native 程序传参对内嵌
//!   `"` 有历史畸变，拒绝后前端走剪贴板回退）· 裸词白名单。
//! - **PowerShell 层**（本模块）：全命令体经 `-EncodedCommand`（base64）穿 wt.exe（`;` 分 tab 不会切碎）。

use crate::detail::Shell as _;
use copy_core::copy_text;
use copy_core::said::Said;

// 这里原来一个只双写 ASCII `'` 的 PowerShell 引号器（本机数据目录带弯引号时握手前奏会断）：前奏整段搬进本机后端，
// PowerShell 字面量只经后端 `platform/shell/dialect.rs::ps_literal` ⇒ 删了。monitor 生产段零 PowerShell 引号器（`quote_singleton_guard_tests`）。

// 开窗的两个平台臂（POSIX 规范化终端出口 · Windows `wt.exe` / `powershell.exe`）与 POSIX 那一臂「跑什么」的校验 · argv
//   （`build_local_posix_argv`，只那一臂读）住 `platform/terminal.rs`；本文件只剩开窗 · 设置页那两条 Tauri 命令。

/// 开窗命令交出去的结局：开了 · 这台开不了终端窗口（壳写好的那几句，界面照摆）。调用方按 `outcome` 判，不按哪句话里的字判。
/// 远端窗口那一问（`chan/host.rs`）开不了窗时回的也是这里的 `said`。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum TerminalOpened {
    /// 开了一个终端窗口。
    Opened,
    /// 这台开不了终端窗口（一个都没探到 · 设置里指定的那个不在）：照实说、给设置入口。
    NoWindow {
        /// 一句（当错的原话：远端窗口那一问的拒绝 · 抛出去的那一句）。
        said: String,
        /// 提示那一条的标题。
        title: String,
        /// 提示那一条的说明（说去设置里哪一格）。
        detail: String,
    },
}

/// 开窗那一下的结局 ⇒ 交出去的那一形（句子在这里写好，只此一处）。
pub(crate) fn opened_of(o: crate::platform::terminal::TerminalOpen) -> TerminalOpened {
    use crate::platform::terminal::TerminalOpen;
    match o {
        TerminalOpen::Opened => TerminalOpened::Opened,
        TerminalOpen::NoWindow => TerminalOpened::NoWindow {
            said: copy_text("rsLaunch.posix.noTerminalWindow", &[]),
            title: copy_text("terminalOpen.noTerminal.title", &[]),
            detail: copy_text("terminalOpen.noTerminal.detail", &[]),
        },
        TerminalOpen::SetMissing => TerminalOpened::NoWindow {
            said: copy_text("rsLaunch.posix.setTerminalMissing", &[]),
            title: copy_text("terminalOpen.setMissing.title", &[]),
            detail: copy_text("terminalOpen.setMissing.detail", &[]),
        },
    }
}

/// 〔「待迁」最后一行〕**开一个终端窗口跑 `command`**（起始目录 `cwd`，缺 ⇒ 不设）—— monitor 在「开终端」这件事上只剩这一下。
///
/// `command` 是**成品**：本机远端一样由本机后端 `terminal-ssh` 按本机终端方言渲好（远端是 `ssh -t …` 外壳，本机是那一串本身），
/// 这里不判、不拼。`cwd` 是这台上的起始目录（本机起会话的直路那一行不带 `--cwd`，目录由开窗时给），远端不给。
///
/// 能不能开、用哪个只问 [`open_local`](crate::platform::terminal::open_local)：开了回 `opened`；
/// 这台开不了终端窗口回 `noWindow` 带写好的那几句（[`TerminalOpened`]），前端照摆并给设置入口
/// （账号登录另给「在 tmux 里登录」）。真失败回 `Err`（一句人话）。
#[tauri::command]
pub async fn open_terminal_window(
    command: String,
    cwd: Option<String>,
) -> Result<TerminalOpened, cmd_wire::failed::Failed> {
    let r: Result<TerminalOpened, Said> = async move {
        // 进程 spawn 挪到阻塞线程池，不堵 IPC 派发线程。
        Ok(tokio::task::spawn_blocking(move || {
            let opened = crate::platform::terminal::open_local(&command, cwd.as_deref())?;
            tracing::info!("launch: terminal window {opened:?}");
            Ok::<_, Said>(opened_of(opened))
        })
        .await
        .map_err(|e| Said::with_raw(copy_text("rsLaunch.remote.taskFailed", &[]), e))??)
    }
    .await;
    r.map_err(|s| s.named("open_terminal_window"))
}

/// 设置页「终端」那一行要的事实（自动会挑谁 · 本机探到哪些 · 现在设的是什么）；判定在平台层，界面只画。
#[tauri::command]
pub async fn terminal_choices() -> crate::platform::terminal::TerminalChoices {
    tokio::task::spawn_blocking(crate::platform::terminal::terminal_choices)
        .await
        .unwrap_or_else(|_| crate::platform::terminal::terminal_choices())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/launch_tests.rs"]
mod tests;
