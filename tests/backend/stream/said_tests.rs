//! 那几条命令被拒时的那一句（从界面搬来的表）：金样里的每个码各一句、两两不同、句子里没有原话；
//! 后端知道的对象（会话名 · 收件人）在句子里（处理器原来那句进复制详情的原话：`inbound_tests.rs` 那条）。

use super::*;

fn golden(file: &str) -> serde_json::Value {
    let p = crate::guard_support::repo_root()
        .join("tests/__fixtures__")
        .join(file);
    serde_json::from_str(
        &std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}")),
    )
    .unwrap()
}

fn codes(g: &serde_json::Value, cmd: &str) -> Vec<String> {
    g[cmd]["codes"]
        .as_array()
        .unwrap_or_else(|| panic!("金样里 {cmd} 没有 codes"))
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn every_golden_code_has_its_own_sentence_without_raw_words() {
    let bus = golden("cc-bus-control.golden.json");
    let tmux = golden("tmux-control.golden.json");
    let term = golden("terminals.golden.json");
    let cases: Vec<(&str, serde_json::Value, Vec<String>, Option<&str>)> = vec![
        (
            "kill",
            serde_json::json!({ "name": "demo-cc" }),
            codes(&tmux, "kill"),
            Some("demo-cc"),
        ),
        (
            "terminal-preview",
            serde_json::json!({}),
            codes(&term, "terminal-preview"),
            None,
        ),
        (
            "bus-send",
            serde_json::json!({ "to": "agent-a" }),
            codes(&bus, "bus-send"),
            Some("agent-a"),
        ),
        (
            "bus-kill",
            serde_json::json!({ "id": "agent-a" }),
            codes(&bus, "bus-kill"),
            None,
        ),
        (
            "bus-spawn",
            serde_json::json!({}),
            codes(&bus, "bus-spawn"),
            None,
        ),
        (
            "bus-broadcast",
            serde_json::json!({}),
            codes(&bus, "bus-broadcast"),
            None,
        ),
    ];
    for (cmd, args, codes, object) in cases {
        assert!(!codes.is_empty(), "{cmd}：金样里一个码都没有");
        let said: Vec<String> = codes
            .iter()
            .map(|c| reword(cmd, &args, c).unwrap_or_else(|| panic!("{cmd} 没有表")))
            .collect();
        for (c, s) in codes.iter().zip(&said) {
            assert!(!s.trim().is_empty() && !s.contains('{'), "{cmd}/{c}：{s:?}");
            assert!(!s.contains(c.as_str()), "{cmd}/{c}：码上了句子：{s}");
            // `bad_id` 说的是 id 的形状不对（可能是发件人那一格），不点名收件人。
            if let (Some(o), false) = (object, c == "bad_id") {
                assert!(s.contains(o), "{cmd}/{c}：句子里没有对象 {o}：{s}");
            }
        }
        let distinct: std::collections::BTreeSet<&String> = said.iter().collect();
        // `bad_args` 与 `bad_target` 在读画面那一条是同一句（都是目标不对）；别的两两不同。
        let expect = if cmd == "terminal-preview"
            && codes.iter().any(|c| c == "bad_args")
            && codes.iter().any(|c| c == "bad_target")
        {
            said.len() - 1
        } else {
            said.len()
        };
        assert_eq!(
            distinct.len(),
            expect,
            "{cmd}：有两档被压成了同一句：{said:?}"
        );
    }
    assert!(
        reword("ping", &serde_json::json!({}), "x").is_none(),
        "没有表的命令该照原句"
    );
    // 账号库那几条没有表：契约对不上照处理器那句（「请求格式不对」），不猜成两端版本。
    assert!(reword("accounts-add", &serde_json::json!({}), "bad_args").is_none());
    assert!(reword("accounts-add", &serde_json::json!({}), "name_taken").is_none());
}
