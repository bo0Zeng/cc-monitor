//! Codex 适配层 —— daemon 里**所有** Codex 专属知识的唯一住址（`S2`）。
//!
//! | 子模块 | 装什么 | 从哪搬来的 |
//! |---|---|---|
//! | [`parse`] | rollout 记录的信封与字段抽取、会话目录定位 | `observe/codex.rs`（整体） |
//! | [`usage`] | 扫 rollout 树 + 按 (model, day) 聚合用量 | `observe/usage_query.rs` 的 `aggregate_codex`/`analyze_codex_session` |
//! | [`resume`] | resume 的**命令形状**与会话名前缀、默认命令名 | `control/resolve_query.rs` 的 `is_codex` 分支 |
//!
//! # 接口面：**四类能力里的三类**，第四类今天是空的
//!
//! `D4` 把适配层的接口定为四类能力（会话发现与判活 · 会话内容读 · 用量 · 起会话/resume）。
//! 本层今天真实覆盖 **② 的一半（抽取器有、读路未接）· ③ · ④**；
//! **① 会话发现与判活是空的** —— `main.rs` 里 `homes: Vec::new()` 逐字写着「DG1 未接线」
//! （`S4` 之前这里是 `codex_dir: None` / `kinds: []` 两个字段，已被通用的 `homes` 替掉）。
//! ⇒ Codex 会话今天**不会**出现在流式 watcher 里，只在 `--usage` 和
//! `--resolve` 两条一次性路上被看见。这条缺口归 `S5`（`hello` 声明看得见哪些 agent）。
//!
//! ⚠ 不要从"这里有个模块"推断"Codex 支持完整"。

pub(crate) mod parse;
pub(crate) mod resume;
pub(crate) mod usage;
