//! **哪些路径是 Claude 自己的数据，不许我们写。**
//! 〔`设计/99 §2 Q2`，用户 2026-09-21 逐字裁「拆」〕
//!
//! # 本模块只有这一件事
//!
//! 一个判定（[`is_protected_claude_data_path`]）＋ 它的拒绝（[`guard_write`]）。
//! 没有别的。它的语料是**一个路径字符串**，没有连接、没有 IO、没有 `async`
//! ⇒ 「这条路径被挡住」这件事在一台**没有任何连接**的机器上判得动，
//! 而 `guard_write` 的 `Err` 是在任何一次往返**之前**就出来的。
//!
//! 那个「之前」不是散文：`remote_write_registry_tests` 的
//! `a_fenced_write_refuses_before_it_touches_the_wire` 逐条要求池子里那七条写命令
//! 的函数体里 `guard_write` 出现在拿连接（`pool_for` / 借通道）**之前**；
//! 本模块「够不着线」这件事由 `claude_data_fence_tests` 的
//! `the_fence_cannot_reach_the_wire` 从另一侧钉着（本文件生产段里零传输符号）。
//!
//! # 它是 `src/doc/INVARIANTS.md` `§1` 那两段澄清的**共同**执行体
//!
//! `§1` 是「monitor 零侵入 Claude Code 数据源」。它底下**两段澄清**各自逐字点名本判定：
//!
//! | 澄清段 | 谁在用 | 用法 |
//! |---|---|---|
//! | **F47**（SFTP 文件面板） | `sftp_pool` 那七条写命令 · `filewin::writeops::fenced_path` | 面板/窗口写**任意用户选的路径** ⇒ 拒碰 Claude 的 jsonl/pidfile |
//! | **F03b**（收件箱编辑） | `skill_host::resolve_editable` 的第②道 | **纵深**：即使声明表写歪了，也不许碰 Claude 的数据 |
//!
//! ⇒ 两段澄清共用**一个**判定，这是它有独立住址的第一个理由：在它搬出来之前，
//! 一个写**本机** `INBOX.txt` 的模块要伸手到一个名叫「SFTP 连接池」的模块里，
//! 去问一个关于**本机路径**的问题。
//!
//! ⚠ `§1` 的**内容**本轮一个字都没动（那要用户拍）——
//! 动的只有它写的**住址**：原先写 `sftp_pool::is_protected_claude_data_path`，
//! 现在写本模块。
//!
//! # 🔴 判定本身一个字节都没改
//!
//! [`is_protected_claude_data_path`] 的函数体是从 `sftp_pool` **原样**搬过来的
//! （结构判定：`<任意>/projects/<proj>/<sid>.jsonl` 恰 2 段 · `<任意>/sessions/<x>.json`
//! 恰 1 段；batch20 那次审计修闭的 `CLAUDE_CONFIG_DIR` 重定位缺口也原样在内）。
//! [`guard_write`] 的拒绝原话同样原样。⇒ **本轮变的是它住哪、谁能看见它、谁在数它**，
//! 不是它判什么。想改判定的射程，那是另一件、要用户拍。
//!
//! # ⚠ 它**不**管什么（只登记，不扩射程）
//!
//! - **方向相反的那一道不在这儿**：`sftp::is_safe_remote_jsonl` 的正题恰恰是
//!   「**只许**删 `projects/**/*.jsonl`」（`§1` 例外 3，历史浏览器删远端会话）。
//!   两道都读 Claude 的目录结构、方向相反，**别互相替代、也别合并**。
//!   「全仓还有谁在读这个结构」由 `claude_data_fence_tests` 的
//!   `the_protected_path_judgement_has_exactly_one_home` 钉成相等断言。
//! - **本判定只看路径的形状**，不看那场会话是不是真的活着（要那个得问
//!   `session_map`，而面板上那一问会变成一次网络往返 ⇒ 刻意不要）。
//! - 它管不着的那几类路径**如实登记在**
//!   `claude_data_fence_tests::THE_SHAPES_THIS_FENCE_DOES_NOT_COVER` 里
//!   —— 那张表是**读数**，不是待办：往里加一类就是扩射程，要用户拍。

// 🔴〔2026-09-22〕**判定与拒绝搬进了共享 crate `claude-fence-core`。**
//
// 搬家的理由：用户裁定「允许」后端在用户显式操作下写用户选的路径
//（`设计/60 §8.3`）⇒ 那道围栏要由**拥有那份数据的那台机器**执行，
// 而远端的 Claude 数据只有远端那个后端够得着。
// 两棵树各写一份就是两个家 —— 而上面那一节逐字立着「判定与拒绝这一对不许再分」。
//
// ⚠ **本模块的头注一个字都没删**：它写的是「为什么有这道围栏、它管不着什么」，
//   那是给 monitor 这一侧的读者看的，而读者会先找到这里。
//   搬走的只有**函数体**，而它在那边一个字节都没改。
// ⚠ 桥这一侧的消费者**一处都不用改**（`skill_host` · `sftp_pool` 那九处 `guard_write`）——
//   它们引的仍是 `crate::claude_data_fence::…`。
pub use claude_fence_core::{guard_write, is_protected_claude_data_path};

#[cfg(test)]
#[path = "../../../tests/bridge/claude_data_fence_tests.rs"]
mod tests;
