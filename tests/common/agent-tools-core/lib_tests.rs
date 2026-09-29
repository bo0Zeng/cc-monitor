//! 〔DUP2 · J19〕`agent-tools-core` 的判据。
//!
//! 要求住址：`设计/01 §5` D1「一个判定只有一个家」· `设计/90 §3` 判据 2；登记表 `tests/frontend/ui/judgment-single-home.vitest.ts` 的 J19。

use super::*;

/// 两个名字都认（新版 `Agent` · 旧名 `Task`），别的不认；大小写敏感（正反各一格）。
#[test]
fn both_agent_tool_names_are_recognised_and_nothing_else() {
    for t in ["Agent", "Task"] {
        assert!(is_claude_agent_tool(t), "{t} 该算 agent 工具");
    }
    for t in ["Bash", "task", "agent", "", "Tasks"] {
        assert!(!is_claude_agent_tool(t), "{t:?} 不该算 agent 工具");
    }
    assert_eq!(CLAUDE_AGENT_TOOLS.len(), 2);
}
