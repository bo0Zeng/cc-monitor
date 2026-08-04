//! U3（2026-08-01）：**控制面** —— 会改变世界，或产出「要怎么改变世界」的计划。
//!
//! §1.1 第二条解耦线的另一半。三个模块各自改变的东西不同：
//!
//! - [`fork_write`]：**写文件系统**（`O_EXCL` 新建一个 `<new-sid>.jsonl`）。
//!   全 crate **唯一**的写盘白名单模块，红线 I7 的那个洞口。
//! - [`tmux_hook`]：**改 tmux server 状态**（`tmux set-hook -g`）+ **发信号**（`SIGUSR1`）。
//! - [`gate`]（F03）：**§34 Gate 2（identity）在本侧的承载** —— 探一次 tmux 拿回
//!   `@ccm_sid` 与 `#{session_id}` 句柄，判定本身在共享的 `gate-core`（定框 C1）。
//!   它**只读** tmux，但归 control/ —— 因为它是「能不能改这个会话」这个**决策**的一部分
//!   （定框 C13：区别不在进程在哪，在它有没有决策权）。
//! - [`kill`]（F04a）：**杀一个 tmux 会话**。过 §34 三道门（Gate 3 = `windows==1` 只给它），
//!   对 `#{session_id}` 句柄下手而不是名字。⚠ 本模块落地 ≠ monitor 那条路已切过来（那是 F04b，C6 顺序）。
//! - [`launch`]（U8a-2b）：**起 tmux 会话 / 往已有会话键入载荷**（U8a 分解里的「平面 ②」）。
//!   起进程（`tmux`，argv 直传不过 shell），已登记进 `readonly_guard::spawn_registry`。
//!   **不 attach** —— 那是平面 ③，daemon 在远端开不了你面前的窗。
//! - [`resolve_query`]：产出 `CommandPlan`（「这个会话该怎么起」）。
//!   名字里有 `query` 但它不是观测 —— 账本 S14 明写它是 backend 的**计划面**。
//!   按「读 / 改变世界」这条线分，产计划属于控制的前半。
//!
//! # 这一层**不许**引用 [`crate::observe`]
//!
//! 由 `crate::layering_guard` 机检。U3 摸底时真有过一条反向边
//! （`fork_write` → `accounts_query::read_regular_capped`），**没有给它开例外** ——
//! 那个函数根本不是 observe 的域逻辑，是通用安全读文件，搬进 `common/fs.rs` 之后
//! 反向边自然消失。铁律 6：改结构让问题不存在。

pub(crate) mod fork_write;
pub(crate) mod gate;
pub(crate) mod kill;
pub(crate) mod launch;
pub(crate) mod resolve_query;
pub(crate) mod tmux_hook;
