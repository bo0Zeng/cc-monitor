//! 观测面 —— 读，不改变世界：流式 watcher、几类一次性查询、以及供它们用的读正文核（`record_page`）。判据是它会不会改变世界，不是模块名。
//!
//! 本层仍是 Claude 专属的：`watcher`/`accounts_query`/`history_query` 都直接认识 Claude 的目录布局与文件格式（Codex 那半在 [`crate::agents::codex`]）。
//!
//! # 与 [`crate::control`] 的关系：一条窄接口，方向固定
//!
//! 允许 `observe → control`，反向不许。`watcher` 调 `control::identity_tag::tag`：`(pid, sid)` 只在 pidfile 事件那一刻同时在手，
//! 打标的回值（容器）随 `session_added` 报出去 —— 反过来只能靠轮询。只是时机在这边、回值不用的两件（装 tmux hook · 认起会话便条）
//! watcher 只报一声，做的那一半由 `main.rs` 起 watcher 时接上控制面（`watcher::Reactions`）。
//! 跨层边的条数由 `crate::layering_guard` 钉住 —— 多一个就红，逼人回答「这条也该跨层吗」。

pub mod accounts_query;
// cc-bus 钩子诊断（`hooks-diag` 帧命令的本体，只读；帧面宿主在顶层 `feature_face`）。
pub(crate) mod cc_bus_hooks;
// 会话事实（分叉血缘 · 改动文件集 · agent 列表 · 最新 usage）的本体；帧面宿主在顶层 `read_face`。
pub(crate) mod facts_cache;
pub(crate) mod facts_query;
// 中转看见的请求标记（每个会话的请求带没带扩展上下文那一项）：中转的 tap 口写、会话事实读。
pub mod history_query;
pub(crate) mod relay_marks;
// 一次性等待（记录里长出某一条 · 会话由新进程报出）：换号重启那条命令用，期限由发起方给。
pub(crate) mod one_wait;
pub mod search_query;
// 会话账本：可重连 / 已结束由这台后端裁、发成品帧（挂在 watcher 发帧的出口上）。
pub(crate) mod session_ledger;
pub(crate) mod session_terminals; // `session-terminals`：此刻是哪个终端在显示这个会话（点 ↗ 时问一次）
                                  // 会话的任务列表（`tasks-list` 帧命令的本体；帧面宿主在顶层 `feature_face`）。
pub(crate) mod tasks_query;
// 「你说过的话」清单的纯核（四条口径的唯一住址）；argv 与分派在 `history_query`。
// 运行簿：会话 ＝ 主运行 ＋ 子运行；只认「运行」，每家的形状问适配层（watcher 写、流归位读）。
// 冷开一条会话那几问（尾段 · 骨架索引 · 用户输入 · 轮次 · 事实）一遍扫出、按（路径 · 长度 · 修改时刻）留着共用。
// 读行：一行行读到最后一个完整行 · 算不算一行 · 一行怎么解析（读记录的各处都经它）。
pub(crate) mod lines;
pub(crate) mod record_page; // 按路径读正文出成品（切行 · 编号 · 挑哪一家解释）
pub(crate) mod record_scan;
pub mod runs;
pub mod skeleton; // 大记录的骨架落盘（预读 · 冷开扫描图从它接着读）
pub(crate) mod tmux_observe; // tmux 观测（原 `watcher.rs` A 块）
pub(crate) mod turns; // `history-turns`：一轮的摘要（过程行 · 刻度 · 大纲同源）
pub(crate) mod user_inputs;
pub mod watcher;
