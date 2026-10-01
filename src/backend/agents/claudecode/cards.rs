//! 〔THIN〕Claude 的**工具词表**：哪个工具名在界面上画成哪一种卡（[`tool_card`]）· tmux 前台命令哪几个算它的会话（[`PROCESS_NAMES`]）。
//!
//! 要求住址：`设计/00 §2.1`「加第三种 agent ＝ 在 `agents/` 下加一个目录 ＋ 在注册表里加一行」· `§1.2`「判定只在后端」。
//! 这几张表从前住 monitor `adapter.rs` 的画像表（生成 `agent-profile-table.ts` 给前端按工具名画卡）与共享 crate `agent-tools-core`
//! （agent 工具那一张，后端会话事实也判它）。界面不再认工具名：卡型随记录成品带出（`JsonlRecord::Assistant.toolCards`，
//! 由 [`super::schema`] 在解析时填），tmux 那一格随 `tmux-list` 成品带出（`agent`）。值逐字未改。

use crate::agents::ToolCard;

/// 「展开 ＝ 子会话」的工具名：`Task`（旧名）与 `Agent`（新版改名），两个都认。
const AGENT_TOOLS: &[&str] = &["Agent", "Task"];
/// 交互工具（agent 在等用户决定）。
const INTERACTIVE_TOOLS: &[&str] = &["AskUserQuestion", "ExitPlanMode"];
/// 写类工具（参数按行级 diff 画）。NotebookEdit **不在内**（会话事实的「改动文件」那张另算它，`observe/facts_query.rs`）。
const DIFF_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit"];
/// 结果默认按 Markdown 画的工具。
const MD_TOOLS: &[&str] = &["Read", "Grep", "WebFetch", "NotebookRead", "TodoWrite"];

/// tmux 前台命令算 Claude 的会话（Claude Code 是 Node CLI，视启动路径也可能报解释器 `node`）。注册表 `Adapter.processes` 那一格。
pub(crate) const PROCESS_NAMES: &[&str] = &["claude", "node"];

/// 这个工具名画成哪一种卡（**大小写敏感**，工具名原样比）；普通工具卡 ⇒ `None`。解析时填进记录成品（`schema.rs::with_tool_cards`）；派出子运行的那一类也由它认（`runs.rs::child_link`）。
pub(crate) fn tool_card(name: &str) -> Option<ToolCard> {
    [
        (AGENT_TOOLS, ToolCard::Agent),
        (INTERACTIVE_TOOLS, ToolCard::Interactive),
        (DIFF_TOOLS, ToolCard::Diff),
        (MD_TOOLS, ToolCard::Md),
    ]
    .into_iter()
    .find(|(tools, _)| tools.contains(&name))
    .map(|(_, card)| card)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/cards_tests.rs"]
mod tests;
