//! U3（2026-08-01）：**观测面** —— 读，不改变世界。
//!
//! §1.1 第二条解耦线的一半。判据不是「模块名里有没有 query」，是**它会不会改变世界**：
//! 流式 watcher、四类一次性查询、以及供它们用的纯解析核（`turn_detect`）。
//!
//! ⚠ **本层今天仍是 Claude 专属的**（`S1` 的判据红着这份清单）：`watcher`/`accounts_query`/
//! `history_query` 都直接认识 Claude 的目录布局与文件格式。Codex 那半 `S2` 已经搬去
//! [`crate::agents::codex`]；Claude 这半归 `S3`。
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

pub mod accounts_query;
// 〔STC · `设计/90 §4` 阶段 C〕会话事实（分叉血缘 · 改动文件集 · agent 列表 · 最新 usage）的本体；帧面宿主在顶层 `read_face`。
pub(crate) mod facts_query;
// 〔TL3 · 审计 F 🔴-6〕读路径越界围栏的唯一住址（`设计/15 §4.2` · `§5.3 C5`）：`history_query` 与 `search_query` 都经它。
pub(crate) mod fence;
pub(crate) mod fs;
pub mod history_query;
pub mod search_query;
// 〔MIG-1 · `99 §2.1 ⑬`〕会话账本：可重连 / 已结束由这台后端裁、发成品帧（挂在 watcher 发帧的出口上）。
pub(crate) mod session_ledger;
// 〔RM1b · 第四波〕插件市场只读枚举（`plugins-marketplaces` 帧命令的本体；从 monitor `plugins.rs` 原样搬来）。
pub(crate) mod plugins_query;
// 〔RM1b · 第四波〕会话的任务列表（`tasks-list` 帧命令的本体；帧面宿主在顶层 `feature_face`）。
pub(crate) mod tasks_query;
// 〔SE1〕「你说过的话」清单的纯核（四条口径的唯一住址）；argv 与分派在 `history_query`。
pub(crate) mod turn_detect;
pub(crate) mod user_inputs;
pub mod watcher;
