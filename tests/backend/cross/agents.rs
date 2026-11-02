//! agent 适配层判据里引到上层的那几条：那一层是独立的包（`backend-agents`）、够不到上层 ⇒ 挂在主 crate。

/// 最小假 agent 走全流程的那几条（经 ccm_line · control · history · observe）。
#[path = "../agents/fake_tests.rs"]
mod fake_tests;

/// 后端那几份线上形里的 agent 格（会话事实 · 换号重启 · 问答 hook）也是 `AgentKind`：哪一格还是串就编不过。
#[test]
fn backend_wire_agent_cells_are_agent_kinds() {
    use crate::agents::AgentKind;
    fn maybe(_: &Option<AgentKind>) {}
    fn kind(_: &AgentKind) {}
    let _ = |f: &crate::observe::recognize::facts::SessionFacts,
             r: &crate::control::session_restart::RestartReq,
             a: &crate::control::asks::AskReq| {
        maybe(&f.agent);
        kind(&r.agent);
        kind(&a.agent);
    };
}

/// 跨语言金样：夹具经**生产**读法（`read_at`）＋ **生产**构造器（`feature_face::mcp_reply`）现算 == 手写的 reply。
/// 要求：「MCP 列表改由后端出成品」· 「成品两侧对拍」。
#[test]
fn the_mcp_product_matches_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/mcp-read.golden.json"))
            .expect("金样读不出来");
    let root = std::env::temp_dir().join(format!("sh1-mcp-golden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let cj = root.join(".claude.json");
    let d = dir.display().to_string();
    std::fs::write(&cj, g["claudeJson"].as_str().unwrap().replace("<DIR>", &d)).unwrap();
    std::fs::write(dir.join(".mcp.json"), g["mcpJson"].as_str().unwrap()).unwrap();
    let got = serde_json::to_value(crate::faces::feature_face::mcp_reply(
        &crate::agents::claudecode::mcp::read_at(
            Some(&cj),
            Some(&dir),
            &crate::agents::McpLook::default(),
        ),
        &Default::default(),
    ))
    .unwrap();
    let want: serde_json::Value = serde_json::from_str(
        &g["reply"]
            .to_string()
            .replace("<DIR>", &d)
            .replace("<CJ>", &cj.display().to_string()),
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(got, want, "`mcp-read` 成品与金样不相等");
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == "mcp-read")
        .expect("登记表里没有 `mcp-read`");
    let mut codes: Vec<&str> = spec.codes.to_vec();
    codes.sort_unstable();
    let mut golden: Vec<&str> = g["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    golden.sort_unstable();
    assert_eq!(codes, golden, "金样的拒绝码与后端登记的不相等");
}

/// Claude 换模型的确认框按的那个键，终端认得（`control::terminals::KEYS` 里有）。
#[test]
fn the_switch_model_confirm_key_is_a_terminal_key() {
    let face = crate::agents::claudecode::LAUNCH.model_switch.unwrap();
    let bx = face.confirm.expect("Claude 换模型声明了确认框");
    assert!(crate::control::terminals::KEYS
        .iter()
        .any(|(k, _)| *k == bx.key));
}
