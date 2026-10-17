//! **§34 杀会话 · 送键只在后端过门**：monitor 里没有第二条路。
//!
//! 门本身（身份门 `admit` · 窗口门 `admit_destructive`、谁过哪道门）由后端 `tests/backend/control/gate_tests.rs` 钉；
//! 这里只钉 monitor 这一侧：
//! [`tests::the_monitor_has_no_second_path_that_kills_a_session`]（monitor 里一处杀会话的 shell 串都没有）
//! ＋ [`tests::the_front_end_speaks_the_tmux_control_ops_only_through_one_module`]（界面只经一处说）
//! ＋ 远端 tmux 动词默认按有破坏性处理、只读表之外的都得过门。
//!
//! ⚠ **约定型守卫**：查的是符号名的源码形态，挡得住「顺手绕过门」，挡不住「换个名字继续错」。

#[cfg(test)]
#[path = "tmux_backend_gate_guard_tests.rs"]
mod tests;
