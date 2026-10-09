//! 观测面 —— 读，不改变世界：流式 watcher、几类一次性查询、以及供它们用的读正文核（`record_page`）。判据是它会不会改变世界，不是模块名。
//!
//! 本层仍是 Claude 专属的：`watcher`/`accounts_query`/`history_query` 都直接认识 Claude 的目录布局与文件格式（Codex 那半在 [`crate::agents::codex`]）。
//!
//! # 与 [`crate::control`] 的关系：一条窄接口，方向固定
//!
//! 允许 `observe → control`，反向不许。`watcher` 调 `control::tmux_hook::install_hooks`：tmux hook 活在 server 进程的内存里、
//! server 每次重起都要重装，而「server 起来了」只有 observe 知道（socket 目录 inotify）—— 反过来只能靠轮询。
//! 跨层边的条数由 `crate::layering_guard` 钉住 —— 多一个就红，逼人回答「这条也该跨层吗」。

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
// 一次性等待（记录里长出某一条 · 会话由新进程报出）：换号重启那条命令用，期限由发起方给。
pub mod history_query;
pub(crate) mod listing_scan; // 历史清单那一行的逐行扫描（累计的那几格 · 读每一行的窄探针）
pub(crate) mod one_wait;
pub mod search_query;
// 搜索的通用口径（原共享 crate `search-core` 通用那一半：常量 · snippet 预算 · 最近优先 · 片段 / 截断 · 标题）。
pub(crate) mod search_rules;
// 会话账本：可重连 / 已结束由这台后端裁、发成品帧（挂在 watcher 发帧的出口上）。
pub(crate) mod session_ledger;
pub(crate) mod session_terminals; // `session-terminals`：此刻是哪个终端在显示这个会话（点 ↗ 时问一次）
                                  // 会话的任务列表（`tasks-list` 帧命令的本体；帧面宿主在顶层 `feature_face`）。
pub(crate) mod interrupts_query; // `session-interrupts`：重启切换 / 结束之前，会打断什么（按族）
pub(crate) mod tasks_query;
// 「你说过的话」清单的纯核（四条口径的唯一住址）；argv 与分派在 `history_query`。
// 运行簿：会话 ＝ 主运行 ＋ 子运行；只认「运行」，每家的形状问适配层（watcher 写、流归位读）。
// 冷开一条会话那几问（尾段 · 骨架索引 · 用户输入 · 轮次 · 事实）一遍扫出、按（路径 · 长度 · 修改时刻）留着共用。
pub(crate) mod record_page; // 按路径读正文出成品（切行 · 编号 · 挑哪一家解释）
pub(crate) mod record_scan;
pub mod runs;
pub(crate) mod tmux_observe; // tmux 观测（原 `watcher.rs` A 块）
pub(crate) mod turns; // `history-turns`：一轮的摘要（过程行 · 刻度 · 大纲同源）
pub(crate) mod user_inputs;
pub mod watcher;
