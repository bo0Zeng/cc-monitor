//! U3（2026-08-01）：**观测面** —— 读，不改变世界。
//!
//! §1.1 第二条解耦线的一半。判据不是「模块名里有没有 query」，是**它会不会改变世界**：
//! 流式 watcher、四类一次性查询、以及供它们用的两个纯解析核（`turn_detect` / `codex`）。
//!
//! # 与 [`crate::control`] 的关系：**一条窄接口，方向固定**
//!
//! 允许 `observe → control`，**反向不许**（§1.1-2）。今天这条窄接口**恰好一个符号**：
//! `watcher` 调 `control::tmux_hook::install_hooks`。
//!
//! 那不是设计失误 —— tmux hook 活在 **server 进程的内存里**，server 每次重起都要重装，
//! 而「server 起来了」这个事实**只有 observe 知道**（socket 目录 inotify）。
//! 信息流的方向就是这样，硬要反过来只能靠轮询，那与 §41 的零定时器铁律正面冲突。
//! 计划自审 §0.5-7 预言过它，这里如实兑现。
//!
//! 条数由 `crate::layering_guard` 钉住 —— **多一个就红**，逼人回答「这条也该跨层吗」。

pub(crate) mod accounts_query;
pub(crate) mod codex;
pub(crate) mod fs;
pub(crate) mod history_query;
pub(crate) mod search_query;
pub(crate) mod turn_detect;
pub(crate) mod usage_query;
pub(crate) mod watcher;
