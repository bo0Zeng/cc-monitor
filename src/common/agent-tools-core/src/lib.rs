//! **agent 的工具词表** —— 哪些工具名算「展开 = 子会话」（agent 工具）。全仓唯一一份（`设计/01 §5` D1）。
//!
//! 〔DUP2 · 主会话 09-26 裁 J19〕此前两个 Rust 住址（`调研/第四波记录/STC.md §1.3`）：
//! - monitor `adapter.rs` 的 claude 画像那一格（经生成物 `src/generated/agent-profile-table.ts` 喂前端**渲染** agent 卡）；
//! - 后端 `observe/facts_query.rs` 自己一份（喂**会话事实**的 agent 列表）。
//!
//! 两份由一条异源对拍钉着（后端常量 == 生成物里 claude 那一行），因为两半不许在编译期互咬（`设计/90 §0` 的 `C2`）、
//! 当时没有能放它的共享 crate。今天两半都 `use` 这里 ⇒ 按构造同一份，那条对拍随之退役。
//!
//! 只放这一张：它是**两半都在判**的那一条。claude 的其余工具表（交互类 · 行级 diff · markdown 渲染 · 写类工具的路径键）
//! 各只有一个住址，不因为「都是工具词表」就搬过来（搬了就是为了整齐改被测物）。

/// Claude Code 里「展开 = 子会话」的工具名：`Task`（旧名）与 `Agent`（新版改名），两个都认。
pub const CLAUDE_AGENT_TOOLS: [&str; 2] = ["Agent", "Task"];

/// 这个工具名是不是 Claude Code 的 agent 工具（**大小写敏感**：工具名原样比对）。
pub fn is_claude_agent_tool(name: &str) -> bool {
    CLAUDE_AGENT_TOOLS.contains(&name)
}

#[cfg(test)]
#[path = "../../../../tests/common/agent-tools-core/lib_tests.rs"]
mod tests;
