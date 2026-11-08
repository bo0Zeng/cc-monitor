//! Claude 的**判活信号**：一个进程的 cmdline 说它是不是一条 Claude 会话（三态 [`CmdlineSays`]）。
//!
//! # 「不像」是兜底，方向是**放行**；「不是会话」是总闸
//!
//! `sessions/<PID>.json` 这份 pidfile 只记 PID，PID 会**被复用**。判活的主线是启动时刻
//! （`history::live_table::add_time_verdict`）；「不像」只在启动时刻没核上时才判冒名，
//! 而且它的形状是「**明显不像**才算」—— 不是「像才判活」。
//! 「不是会话」（守护池停泊的备用进程 `bg-spare`）不一样：它是真 claude 进程、写合规 pidfile、启动时刻自洽，
//! 语义上却不是一条在跑的会话 ⇒ 启动时刻核上了也判不活。
//! 只在读得到 cmdline 的平台上起作用：Windows 上 `platform::proc::proc_cmdline` 恒为 `None`（要读 PEB），三态一样都给不出，
//! 备用进程在那里照时刻证据判活。
//!
//! ⚠ 词表里有 `node` 是因为 Claude Code 常以 `node .../claude` 的形式起来，
//! cmdline 里可能只看得见解释器。收紧它 = 把真会话误判成冒名 = 用户的会话凭空消失。

use super::super::CmdlineSays;

/// 守护池停泊的备用进程在 cmdline 里带的那个词。
const SPARE_WORD: &str = "bg-spare";

/// cmdline（已转小写）：带 [`SPARE_WORD`] ⇒ 不是会话；空串或像 ⇒ 是会话（放行）；明显不像 ⇒ 不像。
/// 词表与 tmux 那一格（批量停 / 起认窗格的 `agent`，精确比）同一张 [`super::cards::PROCESS_NAMES`]；两处判法各自不变（这里子串 ＋ 空串放行，那里精确），词只有一份。
pub fn cmdline_may_be_agent(lower: &str) -> CmdlineSays {
    if lower.contains(SPARE_WORD) {
        CmdlineSays::NotSession
    } else if lower.trim().is_empty()
        || super::cards::PROCESS_NAMES
            .iter()
            .any(|name| lower.contains(name))
    {
        CmdlineSays::Session
    } else {
        CmdlineSays::Unlike
    }
}

// 判据在 `history/live_table_tests.rs`：经生产调用点 `add_time_verdict` 判。
