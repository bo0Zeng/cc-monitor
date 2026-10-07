//! Claude 的**判活信号**：一个进程的 cmdline 看起来像不像它。
//!
//! # 这条判定是"最后一道"，方向是**放行**
//!
//! `sessions/<PID>.json` 这份 pidfile 只记 PID，PID 会**被复用**。判活的主线是
//! 「进程启动时刻 vs pidfile mtime」；cmdline 只在时刻证据缺席时兜底，
//! 而且它的形状是「**明显不像**才判冒名」—— 不是「像才判活」。
//!
//! ⚠ 词表里有 `node` 是因为 Claude Code 常以 `node .../claude` 的形式起来，
//! cmdline 里可能只看得见解释器。收紧它 = 把真会话误判成冒名 = 用户的会话凭空消失。

/// cmdline（已转小写）明显不像 Claude ⇒ `false`；空串或像 ⇒ `true`（放行）。
/// 词表与 tmux 那一格（批量停 / 起认窗格的 `agent`，精确比）同一张 [`super::cards::PROCESS_NAMES`]；两处判法各自不变（这里子串 ＋ 空串放行，那里精确），词只有一份。
pub(crate) fn cmdline_may_be_agent(lower: &str) -> bool {
    lower.trim().is_empty()
        || super::cards::PROCESS_NAMES
            .iter()
            .any(|name| lower.contains(name))
}

// 判据在 `observe/watcher_tests.rs`：经生产调用点 `add_time_verdict` 判。
