//! Batch14-F41：终端拉起。〔「待迁」最后一行〕monitor 这里**只开终端**。
//!
//! 三块：
//! 1. [`open_local`](crate::platform::terminal::open_local) —— 通用「新终端窗口跑一条命令」：Windows 是 PowerShell 窗口
//!    （wt.exe Plan A → CREATE_NEW_CONSOLE Plan B，`-NoExit -EncodedCommand`），POSIX 是挑到的终端里 `bash -lic`。本地与远端共用此单一入口。
//! 2. [`open_terminal_window`] —— 开窗那一条 Tauri 命令：**只开窗**。交来的是本机后端的成品 —— 远端那一行
//!    （`& ssh -t[ -J …] … -- '<bash -lic ''…''>'`）由帧命令 `terminal-ssh` 渲、本机那一串由本机后端起会话那一问交回。
//!    原先住这里的 `build_remote_ssh_ps_command`〔散文墓碑〕与它那几道白名单（用户名 · 地址 · 跳板 · 双引号）随之搬走。
//! 3. [`terminal_dial`] —— 给那一问交机器事实（`{machine, saved, jump, prefer}`：monitor 自己的机器表 ＋ 上次赢的那条，
//!    `dial_host::machine_facts`）。
//!
//! ## 引号与注入
//! - **远端命令**：渲染侧（后端载荷渲染器 / `terminal-ssh`）判 —— 控制字符 · 长度 · 双引号（PowerShell 5.1 向 native 程序传参对内嵌
//!   `"` 有历史畸变，拒绝后前端走剪贴板回退）· 裸词白名单。
//! - **PowerShell 层**（本模块）：全命令体经 `-EncodedCommand`（base64）穿 wt.exe（`;` 分 tab 不会切碎）。

use crate::copy_table::copy_text;
use crate::detail::Said;

/// 远端命令长度上限（防 IPC 侧异常输入；正常 resume 命令 <300 字节）。
const MAX_REMOTE_CMD: usize = 4096;

// 这里原来一个 `posix_quote`〔散文墓碑〕（`shell_quote_core::posix_quote` 的纯转发别名）：唯一的用户是远端那条
// ssh 外壳（`bash -lic '<命令>'`），随渲染搬进本机后端（`src/backend/dial/terminal.rs`，直调 `shell_quote_core`）⇒ 别名一起删。

// 这里原来一个只双写 ASCII `'` 的 PowerShell 引号器（本机数据目录带弯引号时握手前奏会断）：前奏整段搬进本机后端，
// PowerShell 字面量只经后端 `platform/shell/dialect.rs::ps_literal` ⇒ 删了。monitor 生产段零 PowerShell 引号器（`quote_singleton_guard_tests`）。

/// L1（local-as-remote）：**与传输无关**的那层命令校验 —— 三条送法一律适用。
///
/// 远端那条送法的同一组判据今天住本机后端（`dial/terminal.rs`，帧命令 `terminal-ssh`）；这里只剩 POSIX 本地那条路
/// （[`build_local_posix_argv`]）用。
///
/// **刻意不含「拒绝双引号」那条**：它的理由是 PowerShell 5.1 向 native 程序传参对内嵌 `"`
/// 有历史畸变（见调用处），是**那条送法的**约束，不是命令本身的性质。
/// 把它一并搬进来，等于把一个 Windows 怪癖套到 Linux 上 —— 判据要落在性质上。
fn validate_launch_cmd(cmd: &str, what: &str) -> Result<(), String> {
    if cmd.trim().is_empty() {
        return Err(copy_text(
            "rsLaunch.refuse.empty",
            &[("what", &what.to_string())],
        ));
    }
    if cmd.len() > MAX_REMOTE_CMD {
        return Err(copy_text(
            "rsLaunch.refuse.tooLong",
            &[
                ("what", &what.to_string()),
                ("len", &(cmd.len()).to_string()),
                ("max", &MAX_REMOTE_CMD.to_string()),
            ],
        ));
    }
    if cmd.chars().any(|c| c.is_control()) {
        return Err(copy_text(
            "rsLaunch.refuse.control",
            &[("what", &what.to_string())],
        ));
    }
    Ok(())
}

/// L1：**POSIX 本地**送法 —— 直接 exec，**不要 ssh 包**。
///
/// 返回 argv 而不是命令串：本地没有「要穿过一层 shell」的问题，拼成串再让别人拆是
/// 白白造一个注入面。`bash -lic` 这层**保留**——它和远端那条路是同一个语义
///（PATH / 别名 / 函数按「用户粘贴进交互终端」解析），`ccm` 正是靠它才被找到。
///
/// ⇒ 与远端那条送法的关系就是 §2「payload 共享、transport 只管送」：
/// 同一个 `cmd`，本地是 `bash -lic <cmd>`，远端是把这同一串再包进 ssh（那一层今天由本机后端 `terminal-ssh` 渲）。
/// 两侧各有测试逐字节钉住（本侧 `local_posix_sends_the_payload_itself_without_an_ssh_wrap`，
/// 远端那侧在 `tests/backend/dial_terminal_tests.rs`）。
pub fn build_local_posix_argv(cmd: &str) -> Result<Vec<String>, String> {
    validate_launch_cmd(cmd, &copy_text("rsLaunch.what.localCmd", &[]))?;
    Ok(vec!["bash".into(), "-lic".into(), cmd.into()])
}

// 开窗的两个平台臂（POSIX 规范化终端出口 · Windows `wt.exe` / `powershell.exe` · `ssh.exe` 预检）搬进 `platform/terminal.rs`，
//   头注逐字随之；本文件只剩「跑什么」的校验与两条 Tauri 命令。

/// 〔「待迁」最后一行〕**开一个终端窗口跑 `command`** —— monitor 在「开终端」这件事上只剩这一下。
///
/// `command` 是**成品**：远端那一行由本机后端 `terminal-ssh` 按本机终端方言渲好（`ssh -t …` 外壳），本机那一串由本机后端交回 ——
/// 这次拉起带启动期令牌时，两条都已在前面接好令牌握手前奏（本地半，后端渲）。这里不判、不拼。
/// `ssh = true` ⇒ 先查本机有没有 ssh 客户端（缺了窗口里只会报「找不到命令」，而 spawn 本身成功 ⇒ 前端误报成功）。
///
/// 能不能开、用哪个只问 [`open_local`](crate::platform::terminal::open_local)（与本机起会话同一处）：开了回 `"opened"`；
/// 这台找不到终端回 `"noWindow"`（[`TerminalOpen::NoWindow`](crate::platform::terminal::TerminalOpen)），前端照实说并给设置入口
/// （账号登录另给「在 tmux 里登录」）。真失败回 `Err`（一句人话）。
#[tauri::command]
pub async fn open_terminal_window(
    command: String,
    ssh: bool,
) -> Result<crate::platform::terminal::TerminalOpen, Said> {
    // 预检（阻塞）＋ 进程 spawn 挪到阻塞线程池，不堵 IPC 派发线程。
    Ok(tokio::task::spawn_blocking(move || {
        if ssh {
            if let Some(why) = crate::platform::terminal::ssh_client_missing() {
                return Err(why);
            }
        }
        let opened = crate::platform::terminal::open_local(&command, None)?;
        tracing::info!("launch: terminal window {opened:?}");
        Ok(opened)
    })
    .await
    .map_err(|e| copy_text("rsLaunch.remote.taskFailed", &[("e", &e.to_string())]))??)
}

/// 设置页「终端」那一行要的事实（自动会挑谁 · 本机探到哪些 · 现在设的是什么）；判定在平台层，界面只画。
#[tauri::command]
pub async fn terminal_choices() -> crate::platform::terminal::TerminalChoices {
    tokio::task::spawn_blocking(crate::platform::terminal::terminal_choices)
        .await
        .unwrap_or_else(|_| crate::platform::terminal::terminal_choices())
}

/// 开终端那一问（本机后端 `terminal-ssh`）要的**机器事实**：`{machine, saved, jump, prefer}`
/// —— monitor 自己的机器表 ＋ 上次赢的那条（[`crate::dial_host::machine_facts`]，与拨号请求同一份）。只读 monitor 自己的状态。
#[tauri::command]
pub async fn terminal_dial(origin: String) -> Result<serde_json::Value, Said> {
    // 本机那一支不经 ssh（前端 `terminal-open.ts` 原串直接开窗）⇒ 这里先分本机、说清，别掉进下面那句「未找到远端配置」。
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        return Err(copy_text("rsLaunch.terminalDial.local", &[]).into());
    }
    let cfg = crate::load_remote_config_by_label(&origin).ok_or_else(|| {
        copy_text(
            "rsLaunch.remote.noConfig",
            &[("machine", &format!("{:?}", origin))],
        )
    })?;
    Ok(crate::dial_host::machine_facts(&cfg))
}

/// **在本机开一个终端窗口跑 `cmd`**（工作目录 `cwd`，不在就不设）—— monitor 在起会话这件事上只剩这一下。
/// 那一串由本机后端出成品（帧命令 `launch-local`：计划 · 账号前缀 · 中转前缀 · 身份 token 全在那里），这里不判、不拼。
/// 开窗同 [`open_terminal_window`]（[`open_local`](crate::platform::terminal::open_local)，结局同形），多一个起始目录。阻塞那一截不占 IPC 线程。
#[tauri::command]
pub async fn open_local_terminal(
    cmd: String,
    cwd: Option<String>,
) -> Result<crate::platform::terminal::TerminalOpen, Said> {
    Ok(tokio::task::spawn_blocking(move || {
        crate::platform::terminal::open_local(&cmd, cwd.as_deref())
    })
    .await
    .map_err(|e| copy_text("rsLaunch.remote.taskFailed", &[("e", &e.to_string())]))??)
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/launch_tests.rs"]
mod tests;
