//! 观测面 —— 读，不改变世界：流式 watcher、几类一次性查询、以及供它们用的读正文核（`query::body`）。判据是它会不会改变世界，不是模块名。
//!
//! 本层仍是 Claude 专属的：`watcher`/`accounts_query`/`query::locate` 都直接认识 Claude 的目录布局与文件格式（Codex 那半在 [`crate::agents::codex`]）。
//!
//! 读一份记录分四层，只许往下引：读行 [`lines`] → 认行 [`recognize`] → 扫描图与骨架（[`record_scan`] · [`skeleton`]）→ 查询 [`query`]；
//! 跟读（[`watcher`]）在查询之上。方向由 `crate::layering_guard` 的 `ONE_WAY` 钉住。
//!
//! # 与 [`crate::control`] 的关系：互不引用
//!
//! 时机在这边、做的那一半在控制面的三件（装 tmux hook · 认起会话便条 · 打身份标）watcher 只报一声，
//! 由 `main.rs` 起 watcher 时接上控制面的函数（`watcher::Reactions`）；打标的回值（容器）照样随 `session_added` 报出去。
//! 两边都不许引对方，由 `crate::layering_guard` 钉住。

pub mod accounts_query;
// cc-bus 钩子诊断（`hooks-diag` 帧命令的本体，只读；帧面宿主在顶层 `feature_face`）。
pub(crate) mod cc_bus_hooks;
// 续算会话事实的那份累加器表（`history-facts` 的续点令牌）。
// 读行：一行行读到最后一个完整行 · 算不算一行 · 一行怎么解析（读记录的各处都经它）。
pub(crate) mod lines;
// 起好的会话报到没有（`session-arrive` 用）：挂在 watcher 发帧的出口上，认 `session_added`。
pub(crate) mod arrivals;
// 一次性等待（记录里长出某一条 · 会话由新进程报出）：换号重启那条命令用，期限由发起方给。
pub(crate) mod one_wait;
// 查询：每条帧命令的本体，只读扫描图切页、拼成品；找记录与过围栏 · 尾段 · 正文。
pub(crate) mod query;
// 认行：给一行认出用户输入 · 轮次 · 事实 · 骨架索引那一行（纯函数 ＋ 小状态，不碰文件）。
pub(crate) mod recognize;
// 冷开一条会话那几问（尾段 · 骨架索引 · 用户输入 · 轮次 · 事实）一遍扫出、按（路径 · 长度 · 修改时刻）留着共用。
pub(crate) mod record_scan;
// 中转看见的请求标记（每个会话的请求带没带扩展上下文那一项）：中转的 tap 口写、会话事实读。
pub(crate) mod relay_marks;
// 运行簿：会话 ＝ 主运行 ＋ 子运行；只认「运行」，每家的形状问适配层（watcher 写、流归位读）。
pub mod runs;
pub mod search_query;
// 会话账本：可重连 / 已结束由这台后端裁、发成品帧（挂在 watcher 发帧的出口上）。
pub(crate) mod record_hook; // 问过记录在不在之后交给会话账本的那一口（不经 watcher）
pub(crate) mod session_ledger;
// `session-terminals`：此刻是哪个终端在显示这个会话（点 ↗ 时问一次）。
pub(crate) mod session_terminals;
// 大记录的骨架落盘（冷开扫描图落、实时跟读起步照它）。
pub mod skeleton;
// 会话的任务列表（`tasks-list` 帧命令的本体；帧面宿主在顶层 `feature_face`）。
pub(crate) mod tasks_query;
// tmux 观测。
pub(crate) mod tmux_observe;
pub mod watcher;

// agent 适配层判据里经观测面的那几条（适配层是独立的包、够不到这一层；本层拆成包时跟着走）。
#[cfg(test)]
#[path = "../../../tests/backend/agents/claudecode/record_of_tests.rs"]
mod agents_claudecode_record_of_tests;
#[cfg(test)]
#[path = "../../../tests/backend/agents/on_observe_tests.rs"]
mod agents_tests;
