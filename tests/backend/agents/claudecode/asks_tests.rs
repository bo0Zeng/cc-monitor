//! Claude 答题口：hook 输入读成通用形、通用答案写回那一家的决定、起会话垫的那两个词。
//! 夹具照官方文档 hooks#permissionrequest 的字段与 10-10 / 10-11 实测记下的键名造，正文都是编的。

use super::*;
use serde_json::json;

fn bash_req() -> Value {
    json!({
        "session_id": "s-1",
        "transcript_path": "/x/s-1.jsonl",
        "cwd": "/w",
        "permission_mode": "default",
        "hook_event_name": "PermissionRequest",
        "tool_name": "Bash",
        "tool_input": {"command": "touch a.txt\necho ok", "description": "make a file"},
        "permission_suggestions": [
            {"type": "addDirectories", "directories": ["/w"], "destination": "session"},
            {"type": "setMode", "mode": "acceptEdits", "destination": "session"},
            {"type": "removeRules", "rules": [], "behavior": "allow", "destination": "session"}
        ]
    })
}

#[test]
fn a_bash_request_reads_as_an_approval_with_full_command_and_human_rules() {
    let a = read(&bash_req()).expect("PermissionRequest 读得出");
    assert_eq!(a.sid, "s-1");
    assert_eq!(a.kind, AskKind::Approve);
    assert_eq!(a.tool, "Bash");
    assert_eq!(a.what.as_deref(), Some("touch a.txt"));
    assert_eq!(a.detail, "touch a.txt\necho ok", "全文不截");
    assert_eq!(a.suggestions.len(), 2, "删规则那一种不是放宽，不给");
    assert!(a.suggestions[0].text.contains("/w"));
    assert!(!a.blocks_terminal);
    assert_eq!(a.agent_id, None);
}

#[test]
fn other_events_and_shapes_are_not_ours() {
    assert!(read(&json!({"hook_event_name": "SessionStart", "session_id": "s"})).is_none());
    let mut v = bash_req();
    v["tool_name"] = json!("");
    assert!(read(&v).is_none());
    let mut v = bash_req();
    v.as_object_mut().unwrap().remove("session_id");
    assert!(read(&v).is_none());
}

#[test]
fn a_sub_agent_request_carries_its_id_and_blocks_the_terminal() {
    let mut v = bash_req();
    v["agent_id"] = json!("a1");
    v["agent_type"] = json!("general-purpose");
    let a = read(&v).unwrap();
    assert_eq!(a.agent_id.as_deref(), Some("a1"));
    assert_eq!(a.agent_type.as_deref(), Some("general-purpose"));
    assert!(a.blocks_terminal, "后台子 agent：hook 先跑完框才出");
}

#[test]
fn allow_with_a_rule_hands_back_that_suggestion_verbatim() {
    let a = read(&bash_req()).unwrap();
    let out = decide(
        &a,
        &Verdict::Allow {
            rule: Some(1),
            mode: None,
            answers: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        out,
        json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {
            "behavior": "allow",
            "updatedPermissions": [{"type": "setMode", "mode": "acceptEdits", "destination": "session"}]
        }}})
    );
    let plain = decide(
        &a,
        &Verdict::Allow {
            rule: None,
            mode: None,
            answers: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        plain["hookSpecificOutput"]["decision"],
        json!({"behavior": "allow"})
    );
}

#[test]
fn deny_carries_the_note_and_stop() {
    let a = read(&bash_req()).unwrap();
    let out = decide(
        &a,
        &Verdict::Deny {
            note: Some("use b.txt".into()),
            stop: true,
        },
    )
    .unwrap();
    assert_eq!(
        out["hookSpecificOutput"]["decision"],
        json!({"behavior": "deny", "message": "use b.txt", "interrupt": true})
    );
    let bare = decide(
        &a,
        &Verdict::Deny {
            note: Some("  ".into()),
            stop: false,
        },
    )
    .unwrap();
    assert_eq!(
        bare["hookSpecificOutput"]["decision"],
        json!({"behavior": "deny"})
    );
}

fn auq_req() -> Value {
    json!({
        "session_id": "s-2",
        "hook_event_name": "PermissionRequest",
        "tool_name": "AskUserQuestion",
        "tool_input": {"questions": [
            {"question": "Which color?", "header": "Color", "multiSelect": false,
             "options": [{"label": "Red", "description": "warm"}, {"label": "Blue"}]},
            {"question": "Which fruits?", "header": "Fruit", "multiSelect": true,
             "options": [{"label": "Apple"}, {"label": "Pear"}]}
        ]}
    })
}

#[test]
fn a_question_reads_its_questions_and_answers_go_into_updated_input() {
    let a = read(&auq_req()).unwrap();
    assert_eq!(a.kind, AskKind::Question);
    assert_eq!(a.questions.len(), 2);
    assert!(a.questions[1].multi);
    assert_eq!(a.questions[0].options[0].label, "Red");
    assert_eq!(
        a.questions[0].options[0].description.as_deref(),
        Some("warm")
    );
    let out = decide(
        &a,
        &Verdict::Allow {
            rule: None,
            mode: None,
            answers: vec![
                ("Which color?".into(), "PURPLE".into()),
                ("Which fruits?".into(), "Apple, Pear".into()),
            ],
        },
    )
    .unwrap();
    let d = &out["hookSpecificOutput"]["decision"];
    assert_eq!(d["behavior"], "allow");
    assert_eq!(
        d["updatedInput"]["questions"],
        auq_req()["tool_input"]["questions"],
        "题目原样"
    );
    assert_eq!(
        d["updatedInput"]["answers"],
        json!({"Which color?": "PURPLE", "Which fruits?": "Apple, Pear"})
    );
}

#[test]
fn a_plan_is_approved_with_the_plan_verbatim_and_a_mode() {
    let v = json!({
        "session_id": "s-3",
        "hook_event_name": "PermissionRequest",
        "permission_mode": "plan",
        "tool_name": "ExitPlanMode",
        "tool_input": {"plan": "1) a\n2) b\n", "planFilePath": "/p/plan.md"}
    });
    let a = read(&v).unwrap();
    assert_eq!(a.kind, AskKind::Plan);
    assert_eq!(a.detail, "1) a\n2) b\n");
    assert!(a.suggestions.is_empty());
    for (mode, word) in [
        (PlanMode::Auto, "auto"),
        (PlanMode::AcceptEdits, "acceptEdits"),
        (PlanMode::Manual, "default"),
    ] {
        let out = decide(
            &a,
            &Verdict::Allow {
                rule: None,
                mode: Some(mode),
                answers: vec![],
            },
        )
        .unwrap();
        let d = &out["hookSpecificOutput"]["decision"];
        assert_eq!(d["updatedInput"], v["tool_input"]);
        assert_eq!(
            d["updatedPermissions"],
            json!([{"type": "setMode", "mode": word, "destination": "session"}])
        );
    }
}

#[test]
fn the_launch_words_are_settings_with_one_exec_form_hook_that_names_command() {
    let w = launch_args("/h/.cc-monitor/bin/ccm");
    assert_eq!(w[0], "--settings");
    let s: Value = serde_json::from_str(&w[1]).unwrap();
    let hooks = &s["hooks"];
    assert_eq!(
        hooks.as_object().unwrap().len(),
        1,
        "只挂 PermissionRequest"
    );
    let h = &hooks["PermissionRequest"][0]["hooks"][0];
    assert_eq!(h["type"], "command");
    // 实测：exec 形只写 args 不写 command ⇒ 整份设置被跳过、开局还弹框。
    assert_eq!(h["command"], "/h/.cc-monitor/bin/ccm");
    assert_eq!(h["args"], json!(["--", "--answer-hook", "claude"]));
    assert_eq!(h["timeout"], HOOK_TIMEOUT_SECS);
    assert!(s.get("permissions").is_none() && s.get("disableAllHooks").is_none());
}

/// ★ **跨语言金样**：几种请求读成的成品（`needs.ask`）== `tests/__fixtures__/needs-ask.golden.json`；
/// 界面那一侧（`tests/frontend/ui/session-reads.vitest.ts`「答题口的成品」）解同一份 —— 两端对同一形，异源。
/// 四种：批准一步（带两条以后不再问）· 子 agent 的批准（多一项改到终端里答）· 批计划 · 回答两道题（单选带说明 · 多选）。
#[test]
fn the_products_match_the_cross_language_golden() {
    let mut sub = bash_req();
    sub["agent_id"] = json!("a1");
    sub["agent_type"] = json!("general-purpose");
    let plan = json!({
        "session_id": "s-3",
        "hook_event_name": "PermissionRequest",
        "tool_name": "ExitPlanMode",
        "tool_input": {"plan": "1. a\n2. b\n"}
    });
    let product = |id: &str, v: &Value| {
        serde_json::to_value(crate::agents::asks::product_of(
            id,
            &read(v).unwrap(),
            1_000,
        ))
        .unwrap()
    };
    let got = json!({
        "approve": product("ask-1", &bash_req()),
        "subAgent": product("ask-2", &sub),
        "plan": product("ask-3", &plan),
        "question": product("ask-4", &auq_req()),
    });
    let want: Value =
        serde_json::from_str(include_str!("../../../__fixtures__/needs-ask.golden.json"))
            .expect("金样不是合法 JSON");
    assert_eq!(
        got,
        want,
        "成品与跨语言金样不一致。现打：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}

/// 换了启动器那一支交下去的小插件：`hooks/hooks.json` 与默认启动器垫的 `--settings` 那一串是同一份（同一处生成），
/// 插件清单只说名字；两份文件都在插件目录里（相对路径）。
#[test]
fn the_plugin_hooks_file_and_the_settings_words_come_from_one_source() {
    let entry = "/h/.cc-monitor/bin/ccm";
    let files = plugin_files(entry);
    let hooks = files
        .iter()
        .find(|(p, _)| *p == "hooks/hooks.json")
        .expect("插件里有 hooks/hooks.json");
    let manifest = files
        .iter()
        .find(|(p, _)| *p == ".claude-plugin/plugin.json")
        .expect("插件里有 .claude-plugin/plugin.json");
    assert_eq!(files.len(), 2, "只放这两份");
    let from_plugin: Value = serde_json::from_str(&hooks.1).unwrap();
    let from_settings: Value = serde_json::from_str(&launch_args(entry)[1]).unwrap();
    assert_eq!(from_plugin, from_settings, "两组 hook 逐字一样");
    let m: Value = serde_json::from_str(&manifest.1).unwrap();
    assert_eq!(m["name"], PLUGIN_NAME);
}

/// 本机 managed 策略：`disableSideloadFlags`（设了这个变量 claude 当场起不来）或 `allowManagedHooksOnly`（插件里的 hook 被拦）为真 ⇒ 不许。
#[test]
fn managed_policy_that_bars_session_plugins_is_recognised() {
    assert!(policy_blocks(&json!({"disableSideloadFlags": true})));
    assert!(policy_blocks(&json!({"allowManagedHooksOnly": true})));
    assert!(!policy_blocks(&json!({"disableSideloadFlags": false})));
    assert!(!policy_blocks(&json!({"model": "x"})));
    assert!(!policy_blocks(&json!([1])));
}

/// 装着的那一份是哪一版：原生安装（`…/versions/<版本>` 那个文件）· npm 安装（往上找那一家的 `package.json`）；认不出 ⇒ `None`。
#[test]
fn the_installed_version_reads_from_the_install_layout() {
    assert_eq!(
        version_in_name("/home/u/.local/share/claude/versions/2.1.296"),
        Some([2, 1, 296])
    );
    assert_eq!(version_in_name("/usr/bin/claude"), None);
    assert_eq!(
        version_in_package(r#"{"name":"@anthropic-ai/claude-code","version":"2.1.279"}"#),
        Some([2, 1, 279])
    );
    assert_eq!(
        version_in_package(r#"{"name":"other","version":"9.9.9"}"#),
        None,
        "别人家的 package.json 不算"
    );
    assert!(PLUGIN_MIN_VERSION > [2, 1, 279] && PLUGIN_MIN_VERSION <= [2, 1, 280]);
}

/// 钉条那一段与消息流那一行认同一张「工具 → 主参数」表；表外的工具才按那串常见键猜。
#[test]
fn the_pin_names_the_main_argument_of_every_tool_the_step_line_knows() {
    for (tool, input, want) in [
        ("WebSearch", json!({"query": "rust 2024"}), "rust 2024"),
        ("Task", json!({"description": "查一下"}), "查一下"),
        ("Skill", json!({"skill": "pdf"}), "pdf"),
        ("KillShell", json!({"shell_id": "s-1"}), "s-1"),
        ("SomeNewTool", json!({"path": "/w/x"}), "/w/x"),
    ] {
        let mut req = bash_req();
        req["tool_name"] = json!(tool);
        req["tool_input"] = input;
        assert_eq!(read(&req).unwrap().what.as_deref(), Some(want), "{tool}");
    }
}

/// 提问 / 计划的工具名只由 `asks` 一处定义，`cards.rs` 与 `steps.rs` 不再出现这两个串字面量。
#[test]
fn ask_and_plan_tool_literals_live_only_in_asks() {
    let cards = include_str!("../../../../src/backend/agents/claudecode/cards.rs");
    let steps = include_str!("../../../../src/backend/agents/claudecode/steps.rs");
    let asks = include_str!("../../../../src/backend/agents/claudecode/asks.rs");

    // 量的是串字面量（带引号那一形）：头注里点名工具说明形状不算。
    for tool in [ASK_TOOL, PLAN_TOOL] {
        let lit = format!("\"{tool}\"");
        assert!(asks.contains(&lit), "正控：asks 必含 {lit}");
        assert!(!cards.contains(&lit), "cards.rs 不应出现 {lit} 字面量");
        assert!(!steps.contains(&lit), "steps.rs 不应出现 {lit} 字面量");
    }
}
