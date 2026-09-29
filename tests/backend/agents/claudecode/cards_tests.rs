//! 要求住址：`设计/00 §2.1`（加一个 agent 只改 `agents/`）· `设计/01 §5` D1「一个判定只有一个家」（J19 那一格从共享 crate 收进这里）。

use super::*;

/// 每个工具名恰落一种卡、别的名字（含大小写不同）是普通卡；期望手写自原先两处（monitor 画像表 · `agent-tools-core`）的值。
#[test]
fn each_tool_name_maps_to_exactly_its_card() {
    for (name, want) in [
        ("Agent", Some(ToolCard::Agent)),
        ("Task", Some(ToolCard::Agent)),
        ("AskUserQuestion", Some(ToolCard::Interactive)),
        ("ExitPlanMode", Some(ToolCard::Interactive)),
        ("Edit", Some(ToolCard::Diff)),
        ("Write", Some(ToolCard::Diff)),
        ("MultiEdit", Some(ToolCard::Diff)),
        ("Read", Some(ToolCard::Md)),
        ("Grep", Some(ToolCard::Md)),
        ("WebFetch", Some(ToolCard::Md)),
        ("NotebookRead", Some(ToolCard::Md)),
        ("TodoWrite", Some(ToolCard::Md)),
        ("Bash", None),
        ("NotebookEdit", None),
        ("task", None),
        ("", None),
    ] {
        assert_eq!(tool_card(name), want, "{name:?}");
    }
    assert_eq!(PROCESS_NAMES, &["claude", "node"]);
}
