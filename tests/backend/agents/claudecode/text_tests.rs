//! 要求住址：`INVARIANTS §20`（CLI 注入的非真用户输入不算用户说的话）· `设计/10 §2.2b ⑤`（注入噪声那条规则只有一份）。
//!
//! 〔P1〕随 Claude 记录文本那一半从共享 crate `search-core` 搬来（期望一字未改）。

use super::*;
use crate::agents::claudecode::schema::UserText;
use serde_json::Value;

#[test]
fn extract_text_blocks_string_and_array() {
    assert_eq!(extract_text_blocks(&Value::String("hi".into())), "hi");
    let arr = serde_json::json!([
        {"type":"text","text":"line1"},
        {"type":"tool_use","name":"Bash","input":{}},
        {"type":"text","text":"line2"}
    ]);
    assert_eq!(extract_text_blocks(&arr), "line1\nline2");
}

#[test]
fn extract_tool_text_assistant_and_user() {
    let asst = serde_json::json!([{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}]);
    let t = extract_tool_text(&asst, true);
    assert!(t.contains("Bash") && t.contains("ls -la"));
    let user =
        serde_json::json!([{"type":"tool_result","content":[{"type":"text","text":"file out"}]}]);
    assert!(extract_tool_text(&user, false).contains("file out"));
}

#[test]
fn clean_user_text_strips_wrappers_and_interrupt() {
    assert_eq!(
        clean_user_text("<system-reminder>noise</system-reminder>真内容"),
        "真内容"
    );
    assert_eq!(clean_user_text("[Request interrupted by user]"), "");
}

#[test]
fn the_one_user_text_rule_takes_the_better_half_of_each_side() {
    let cases: [(&str, &str, bool); 8] = [
        (
            "<local-command-stderr>boom</local-command-stderr>真话",
            "真话",
            false,
        ),
        ("Continue from where you left off.", "", false),
        ("no response requested", "", false),
        (
            "先说一句\nNo response requested.\n再说一句",
            "先说一句\n\n再说一句",
            false,
        ),
        (
            "please continue from where you left off.",
            "please continue from where you left off.",
            false,
        ),
        ("[Request interrupted by user for tool use]", "", true),
        (
            "[Request interrupted by user]\n接着说的真话",
            "[Request interrupted by user]\n接着说的真话",
            false,
        ),
        (
            "<system-reminder>x</system-reminder>  [Request interrupted by user]  ",
            "",
            true,
        ),
    ];
    for (input, clean, interrupt) in cases {
        assert_eq!(
            user_text(input),
            UserText {
                clean: clean.to_string(),
                interrupt
            },
            "{input:?}"
        );
    }
}
