//! 自动起算发那一趟：这台后端自己当 `ccm`，用那个号的配置目录在家里的工作目录起一次那一家的程序
//! （参数住适配层：不交互 · 最便宜的模型 · 一句 · 不落会话记录），有期限；之后按额度账出没出新数记成与不成。

use super::autostart::AutostartFail;
use crate::platform::child::{Child, ChildFail, Deadline};
use std::path::Path;

/// 那一趟的期限：最短一句、一轮，正常几秒；到点杀整组、记 `timedOut`。
pub(crate) const SEND_WITHIN: Deadline = Deadline::secs(120);

/// 交给 `ccm` 的 argv（不含程序名）：最后一个 `--` 左边原样交那一家的程序（适配层给的那一趟参数：不交互 · 最便宜的模型 ·
/// 一句 · 不落会话记录），右边是 ccm 自己的选项（用这个号的配置目录 · 在这个目录）。这一家没有这一形 ⇒ `None`。
/// 走不走中转由 `ccm` 照它对每一次起会话的同一套规矩定（这台中转在跑 ⇒ 注入；发之前先核过它在跑）。
pub(crate) fn ccm_argv(agent: &str, account_dir: &Path, dir: &Path) -> Option<Vec<String>> {
    use crate::control::ccm::argv::flag;
    let mut v: Vec<String> = crate::agents::open_window_args_of(agent)?
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    v.push(flag::END.to_string());
    v.push(flag::ACCOUNT_DIR.to_string());
    v.push(account_dir.to_string_lossy().into_owned());
    v.push(flag::CWD.to_string());
    v.push(dir.to_string_lossy().into_owned());
    Some(v)
}

/// 起那一趟：建好工作目录（只建那一层）、起 `program`（生产 = 本后端自己，当 `ccm` 用）、等它到 [`SEND_WITHIN`]。
/// 回退出码；起不来 / 过了期限 ⇒ 原语的失败。
pub(crate) fn run_ccm(
    program: &Path,
    agent: &str,
    account_dir: &Path,
    dir: &Path,
) -> Result<i32, ChildFail> {
    let argv = ccm_argv(agent, account_dir, dir)
        .ok_or_else(|| ChildFail::Io(std::io::Error::from(std::io::ErrorKind::Unsupported)))?;
    if let Some(parent) = dir.parent() {
        crate::common::own_dir::ensure_private_dir(parent).map_err(ChildFail::Io)?;
    }
    crate::common::own_dir::ensure_private_dir(dir).map_err(ChildFail::Io)?;
    let out = Child::new(program)
        .args(argv)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .run(SEND_WITHIN)?;
    Ok(out.status.code().unwrap_or(-1))
}

/// `ccm` 起不了它要起的程序时的退出码（`control/ccm` 最终那一跳）。
pub(crate) const CCM_COULD_NOT_EXEC: i32 = 4;

/// 那一趟之后怎么记：额度账上出了这个号的新数 ⇒ 成；否则按看得到的原因记。
pub(crate) fn verdict(
    ran: &Result<i32, ChildFail>,
    heard: bool,
    needs_login: bool,
) -> Result<(), AutostartFail> {
    if heard {
        return Ok(());
    }
    Err(match ran {
        Err(e) if e.is_timed_out() => AutostartFail::TimedOut,
        Err(ChildFail::NotFound(_)) | Ok(CCM_COULD_NOT_EXEC) => AutostartFail::NoClaude,
        _ if needs_login => AutostartFail::NeedsLogin,
        _ => AutostartFail::NoReading,
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/autostart_send_tests.rs"]
mod tests;
