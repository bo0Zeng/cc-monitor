//! U3（2026-08-01）：**观测面** —— 读，不改变世界。
//!
//! §1.1 第二条解耦线的一半。判据不是「模块名里有没有 query」，是**它会不会改变世界**：
//! 流式 watcher、四类一次性查询、以及供它们用的读正文核（`record_page`；轮次判词随记录解释进了 `agents/claudecode/turn.rs`）。
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
// cc-bus 钩子诊断（`hooks-diag` 帧命令的本体，只读；帧面宿主在顶层 `feature_face`）。
pub(crate) mod cc_bus_hooks;
// 会话事实（分叉血缘 · 改动文件集 · agent 列表 · 最新 usage）的本体；帧面宿主在顶层 `read_face`。
pub(crate) mod facts_query;
// 中转看见的请求标记（每个会话的请求带没带扩展上下文那一项）：中转的 tap 口写、会话事实读。
pub(crate) mod relay_marks;
// 〔审计 F 🔴-6〕读路径越界围栏的唯一住址：`history_query` 与 `search_query` 都经它。
pub(crate) mod fence;
pub(crate) mod fs;
pub mod history_query;
pub mod search_query;
// 搜索的通用口径（原共享 crate `search-core` 通用那一半：常量 · snippet 预算 · 最近优先 · 片段 / 截断 · 标题）。
pub(crate) mod search_rules;
// 会话账本：可重连 / 已结束由这台后端裁、发成品帧（挂在 watcher 发帧的出口上）。
pub(crate) mod session_ledger;
pub(crate) mod session_terminals; // `session-terminals`：此刻是哪个终端在显示这个会话（点 ↗ 时问一次）
                                  // 会话的任务列表（`tasks-list` 帧命令的本体；帧面宿主在顶层 `feature_face`）。
pub(crate) mod tasks_query;
// 「你说过的话」清单的纯核（四条口径的唯一住址）；argv 与分派在 `history_query`。
// 运行簿：会话 ＝ 主运行 ＋ 子运行；只认「运行」，每家的形状问适配层（watcher 写、流归位读）。
pub(crate) mod record_page; // 按路径读正文出成品（切行 · 编号 · 挑哪一家解释）
pub mod runs;
pub(crate) mod tmux_observe; // tmux 观测（原 `watcher.rs` A 块）
pub(crate) mod user_inputs;
pub mod watcher;
