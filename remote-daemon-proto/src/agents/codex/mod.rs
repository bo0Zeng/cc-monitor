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
//! **① 会话发现与判活**〔`S5` 08-14 订正〕：**"这台机器上有没有这个 agent"那一格已经有了**
//! （[`home`] + `agents::visible_homes`），**"有哪些会话、活没活"那一格仍然是空的**。
//! 两格别混：前者只看 home 目录在不在，后者要扫会话树 + 判活（DG1，仍未接线）。
//! ⇒ Codex 会话今天**不会**出现在流式 watcher 里，只在 `--usage` 和
//! `--resolve` 两条一次性路上被看见。
//!
//! ⚠ 而且**发现出来的东西今天不上线**：`main.rs` 的 `homes: Vec::new()` 一行未动
//! （`S5` 的口径是「**能填不真填**」——填 `homes` 是一次跨仓契约变更，留成一次纯发布决策）。
//!
//! ⚠ 不要从"这里有个模块"推断"Codex 支持完整"。

pub(crate) mod parse;
pub(crate) mod resume;
pub(crate) mod usage;

/// 本 agent 在 wire 上的 **`agent_kind` 值**〔`S5`〕。
///
/// 值域由既有契约定死（`ResumeSpec.agentKind` 逐字「`"codex"`=codex，**大小写敏感**」·
/// `session_added.agent_kind` 发 `"codex"`），改它 = 改跨仓契约。
///
/// 它住在**适配层**而不是通用层，是为了让 `agents::REGISTRY` 那张注册表里
/// 一个 agent 名的字面量都没有（顺带：`agent_locality_guard` 判据②数的正是通用层里
/// 带引号的 `"codex"`，把它写在注册表里会**当场**多一条 kind 派发登记 —— 而注册表
/// 根本不在做派发）。
pub(crate) const AGENT_KIND: &str = "codex";

/// 本 agent 在这台机器上的 home 目录 —— **只答"它该在哪"，不答"在不在"**〔`S5`〕。
///
/// 与 Claude 那家的**真实差别**：这里可能答不出来（`None`）——
/// [`parse::resolve_codex_dir`] 认 `$CODEX_HOME`，否则 `$HOME/.codex`；
/// **两个环境变量都没有**时它没有第三条退路（Claude 那边有 cwd 兜底）。
/// `None` = 连一个候选路径都说不出 ⇒ 一定看不见。
pub(crate) fn home() -> Option<std::path::PathBuf> {
    parse::resolve_codex_dir()
}
