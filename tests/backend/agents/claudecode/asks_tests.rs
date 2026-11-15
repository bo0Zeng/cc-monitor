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
    // 适配层只交码 ＋ 原值，写成人话在核心（`AskSuggestion::said`）。
    assert_eq!(a.suggestions[0].scope, SuggestScope::Session);
    assert_eq!(a.suggestions[0].rule, SuggestRule::Dirs(vec!["/w".into()]));
    assert_eq!(a.suggestions[1].rule, SuggestRule::AcceptEdits);
    assert_eq!(
        a.suggestions[0].raw,
        bash_req()["permission_suggestions"][0]
    );
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

/// 命令里说得出用意的那一段：准备段（赋值 · cd · export · 注释）跳过，引号 · `$(…)` · 反引号里的分号与 `2>&1` 不切，段首环境赋值跳过；
/// 全是准备段 ⇒ 只摆第一个词加「…」（赋值词只留到 `=`），值不上钉条。
#[test]
fn the_command_shows_from_the_part_that_says_what_it_does() {
    use crate::agents::beat::command_intent as at;
    for (cmd, want) in [
        ("F=/a/b.txt; sed -n '1,9p' \"$F\"", "sed -n '1,9p' \"$F\""),
        (
            "cd /w && cargo test -q 2>&1 | tail",
            "cargo test -q 2>&1 | tail",
        ),
        ("A=1 B=\"x; y\" && export C=2; LC_ALL=C sort f", "sort f"),
        ("# 看一眼\nset -e\nls -la /w", "ls -la /w"),
        ("echo 'a; b' && cd x", "echo 'a; b' && cd x"),
        (
            "grep -n x f &> /dev/null || true",
            "grep -n x f &> /dev/null || true",
        ),
        ("F=1; cd /w", "F=…"),
        ("export GITHUB_TOKEN=ghp_x1", "export …"),
        ("# 准备\ncd /w && set -e", "cd …"),
        ("  TOKEN=\"a b\"  ", "TOKEN=…"),
        ("  touch a  ", "touch a"),
        // `$(…)` 与反引号当引号：里面的 `;` · `|` · `&` 不切，带着它们的赋值词整个跳过。
        ("X=$(cd /tmp; pwd) rm -rf \"$X\"", "rm -rf \"$X\""),
        ("X=`cd /tmp; pwd` rm -rf \"$X\"", "rm -rf \"$X\""),
        (
            "D=$(dirname \"$(pwd; true)\" | tr a b); ls \"$D\"",
            "ls \"$D\"",
        ),
        ("N=$((1 + 2)); echo $N", "echo $N"),
        ("echo \"$(date; id)\" && ls", "echo \"$(date; id)\" && ls"),
    ] {
        assert_eq!(at(cmd), want, "{cmd}");
    }
    let mut v = bash_req();
    v["tool_input"]["command"] = json!("cd /w\nmake all\nmake test");
    assert_eq!(
        read(&v).unwrap().what.as_deref(),
        Some("make all"),
        "从那一段起，仍只摆一行"
    );
    v["tool_input"]["command"] = json!("export GITHUB_TOKEN=ghp_x1\ncd /w");
    let a = read(&v).unwrap();
    assert_eq!(a.what.as_deref(), Some("export …"), "钉条不带值");
    assert_eq!(
        a.detail, "export GITHUB_TOKEN=ghp_x1\ncd /w",
        "答批准那一屏照摆全文"
    );
}

/// 后台子 agent 发的一次 Bash 批准，命令以变量赋值打头：需手动那一条写得出谁要批（派出时的标签 · 类别）、
/// 批的是什么（说得出用意的那一段，不以 `F=` 开头）。记录树是沙箱里搭的：父记录 · 子记录 · 派出那一刻落的那一份。
#[test]
fn a_background_agents_approval_says_who_asks_and_what_the_command_does() {
    let d = std::env::temp_dir().join(format!("cc-asks-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let parent = d.join("s-1.jsonl");
    let sub = d.join("s-1").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(&parent, "{}\n").unwrap();
    std::fs::write(sub.join("agent-a1.jsonl"), "{}\n").unwrap();
    std::fs::write(sub.join("agent-a2.jsonl"), "{}\n").unwrap();
    std::fs::write(
        sub.join("agent-a1.meta.json"),
        json!({"toolUseId": "t1", "description": "Audit: docs", "agentType": "Explore", "requestShape": "background"}).to_string(),
    )
    .unwrap();
    std::fs::write(
        sub.join("agent-a2.meta.json"),
        json!({"toolUseId": "t2", "description": "other", "agentType": "Plan"}).to_string(),
    )
    .unwrap();
    let mut v = bash_req();
    v["agent_id"] = json!("a1");
    v["agent_type"] = json!("Explore");
    v["tool_input"]["command"] =
        json!("F=/r/tool-results/b8v1.txt; sed -n '1,40p' \"$F\" | head -5\necho done");
    let a = read(&v).unwrap();
    let label = crate::agents::asks::run_label(&parent, "a1");
    let ask = crate::agents::asks::product_of(
        "ask-1",
        &a,
        FACE.stop_on_deny,
        FACE.answers_on_screen(&a),
        label.as_deref(),
    );
    let run = ask.run.as_ref().expect("子 agent 发的带 run");
    assert!(
        run.text.0.contains("Audit: docs") && run.text.0.contains("Explore"),
        "{}",
        run.text.0
    );
    // 需手动那一条的钉条与代码头照这一份的 `what` 摆（观测面那一侧的判据挂在 `observe` 底下）。
    let want = "sed -n '1,40p' \"$F\" | head -5";
    assert_eq!(ask.what.as_deref(), Some(want));
    assert_eq!(
        crate::agents::asks::run_label(&parent, "zz"),
        None,
        "对不上 ⇒ 不带名"
    );
    let _ = std::fs::remove_dir_all(&d);
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
    assert_eq!(a.kind, AskKind::Answer);
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
    for req in [auq_req(), bash_req()] {
        assert!(
            !(FACE.cannot)(&read(&req).unwrap()),
            "这一家提问 · 批准都经钩子答"
        );
    }
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
    let mut kinds: Vec<&str> = hooks
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    kinds.sort_unstable();
    assert_eq!(
        kinds,
        [
            "Notification",
            "PermissionRequest",
            "Stop",
            "UserPromptSubmit"
        ],
        "答题那一种 ＋ 一轮的那三种"
    );
    let h = &hooks["PermissionRequest"][0]["hooks"][0];
    assert_eq!(h["type"], "command");
    // 实测：exec 形只写 args 不写 command ⇒ 整份设置被跳过、开局还弹框。
    assert_eq!(h["command"], "/h/.cc-monitor/bin/ccm");
    assert_eq!(h["args"], json!(["--", "--answer-hook", "claude"]));
    assert_eq!(h["timeout"], HOOK_TIMEOUT_SECS);
    for k in ["UserPromptSubmit", "Stop", "Notification"] {
        let h = &hooks[k][0]["hooks"][0];
        assert_eq!(h["command"], "/h/.cc-monitor/bin/ccm", "{k}");
        assert_eq!(h["args"], json!(["--", "--answer-hook", "claude"]), "{k}");
        assert_eq!(h["timeout"], EVENT_HOOK_TIMEOUT_SECS, "{k}");
    }
    assert_eq!(
        hooks["Notification"][0]["matcher"], "idle_prompt",
        "通知只要闲着那一种"
    );
    assert!(s.get("permissions").is_none() && s.get("disableAllHooks").is_none());
}

#[test]
fn the_turn_hooks_read_as_the_shared_events() {
    use crate::agents::AgentEvent as E;
    let of = |name: &str| json!({"session_id": "s-9", "hook_event_name": name, "cwd": "/w"});
    assert_eq!(heard(&of("Stop")), Some(("s-9".into(), E::TurnEnded)));
    assert_eq!(
        heard(&of("UserPromptSubmit")),
        Some(("s-9".into(), E::PromptSubmitted))
    );
    let mut idle = of("Notification");
    idle["notification_type"] = json!("idle_prompt");
    assert_eq!(
        heard(&idle),
        Some(("s-9".into(), E::TurnEnded)),
        "Esc 打断不发 Stop，闲着的通知也算一轮结束"
    );
    let mut perm = of("Notification");
    perm["notification_type"] = json!("permission_prompt");
    assert_eq!(heard(&perm), None, "批准那一种走 PermissionRequest");
    assert_eq!(heard(&bash_req()), None, "要人答的那一种不是事件");
    assert_eq!(
        heard(&json!({"hook_event_name": "Stop"})),
        None,
        "缺会话 id"
    );
}

#[test]
fn a_users_own_settings_get_every_hook_appended() {
    let user = json!({"model": "x", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "mine"}]}]}});
    let v = merge(&user, "/ccm").unwrap();
    assert_eq!(v["model"], "x");
    assert_eq!(
        v["hooks"]["Stop"].as_array().unwrap().len(),
        2,
        "他那一组留着、我们的接在后面"
    );
    assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "mine");
    for k in ["PermissionRequest", "UserPromptSubmit", "Notification"] {
        assert_eq!(v["hooks"][k].as_array().unwrap().len(), 1, "{k}");
    }
    let odd = json!({"hooks": {"Stop": "x"}});
    let v = merge(&odd, "/ccm").expect("一轮那一种形状不对 ⇒ 只那一种不挂");
    assert_eq!(v["hooks"]["Stop"], "x", "他那一份原样");
    assert_eq!(v["hooks"]["PermissionRequest"].as_array().unwrap().len(), 1);
    assert!(merge(&json!({"hooks": {"PermissionRequest": "x"}}), "/ccm").is_none());
}

/// ★ **跨语言金样**：几种请求读成的成品（`needs.ask`）== `tests/__fixtures__/needs-ask.golden.json`；
/// 界面那一侧（`tests/frontend/ui/session-reads.vitest.ts`「答题口的成品」）解同一份 —— 两端对同一形，异源。
/// 四种：批准一步（带两条以后不再问）· 子 agent 的批准（挡着终端、认得出框：只给允许 · 拒绝）· 批计划 · 回答两道题（单选带说明 · 多选）。
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
        let a = read(v).unwrap();
        serde_json::to_value(crate::agents::asks::product_of(
            id,
            &a,
            FACE.stop_on_deny,
            FACE.answers_on_screen(&a),
            None,
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

/// 注册表那一格插件那一路的版本门只声明：下限 2.1.280（`CLAUDE_CODE_PLUGIN_DIRS` 从这一版起认）· `PATH` 上的 `claude` · npm 包名
/// （读法是通用的那一个，判据在 `agents_tests`）。
#[test]
fn the_plugin_gate_declares_the_floor_the_command_and_the_package() {
    let gate = FACE.plugin.expect("有插件那一路").gate;
    assert_eq!(gate.command, "claude");
    assert_eq!(gate.package, "@anthropic-ai/claude-code");
    assert_eq!(gate.admits(Some([2, 1, 280])), Ok(()));
    for v in [Some([2, 1, 279]), None] {
        assert_eq!(gate.admits(v), Err(crate::agents::asks::HookOff::Version));
    }
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

// ── 读屏认框：挡着终端的那一条（后台子 agent 的 Bash 批准框）在屏上是不是正显示着 ──────────────────
// 屏是真屏夹具（夹具 claude-ask-box-* `.screen.txt`）：替身 `fake-claude-ask-box.sh` 照 11-12 实测的排法在私有 tmux 里画框、
// 照生产抓屏那一条 argv 截下；每趟重截来比的那一条住读屏驱动的判据（`control::screen_drive` 的
// `the_ask_box_fixtures_match_what_the_stand_in_draws`，那里有抓屏的 argv）。命令与说明都是编的。

/// 一份夹具那一屏（照文件名去掉 `.screen.txt` 那一截点；每一份照字面 `include_str!`，夹具守卫认得出谁在用）。
macro_rules! screen {
    ("wide-160") => {
        lines(include_str!(
            "../../../__fixtures__/claude-ask-box-wide-160.screen.txt"
        ))
    };
    ("main-160") => {
        lines(include_str!(
            "../../../__fixtures__/claude-ask-box-main-160.screen.txt"
        ))
    };
    ("other-tool-160") => {
        lines(include_str!(
            "../../../__fixtures__/claude-ask-box-other-tool-160.screen.txt"
        ))
    };
    ("queue-first") => {
        lines(include_str!(
            "../../../__fixtures__/claude-ask-box-queue-first.screen.txt"
        ))
    };
    ("queue-next") => {
        lines(include_str!(
            "../../../__fixtures__/claude-ask-box-queue-next.screen.txt"
        ))
    };
    ("queue-done") => {
        lines(include_str!(
            "../../../__fixtures__/claude-ask-box-queue-done.screen.txt"
        ))
    };
    ("wrap-60") => {
        lines(include_str!(
            "../../../__fixtures__/claude-ask-box-wrap-60.screen.txt"
        ))
    };
    ("split-40") => {
        lines(include_str!(
            "../../../__fixtures__/claude-ask-box-split-40.screen.txt"
        ))
    };
    ("short-80x11") => {
        lines(include_str!(
            "../../../__fixtures__/claude-ask-box-short-80x11.screen.txt"
        ))
    };
}

/// 要批的那一条：`agent_type` 给了 ⇒ 后台子 agent 发的。
fn sub_ask(command: &str, agent_type: Option<&str>) -> AskIn {
    let mut v = bash_req();
    v["tool_input"] = json!({"command": command, "description": "make plum"});
    if let Some(t) = agent_type {
        v["agent_id"] = json!("a-9");
        v["agent_type"] = json!(t);
    }
    read(&v).expect("读得出")
}

fn lines(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

const GP: Option<&str> = Some("general-purpose");
const LONG_CMD: &str = "touch ./a_very_long_file_name_number_one.txt ./a_very_long_file_name_number_two.txt ./a_very_long_file_name_number_three.txt";

#[test]
fn the_box_of_this_request_is_seen_in_a_wide_a_narrow_and_a_split_window() {
    let a = sub_ask("touch ./plum.txt", GP);
    assert!((SCREEN.covers)(&a), "Bash 批准认得");
    assert!(!(SCREEN.covers)(&read(&auq_req()).unwrap()), "提问框不认");
    assert!((SCREEN.seen)(&screen!("wide-160"), &a), "宽 160");
    assert!(
        (SCREEN.seen)(&screen!("queue-first"), &sub_ask("touch ./pear.txt", GP)),
        "右边带 k of n"
    );
    assert!(
        (SCREEN.seen)(&screen!("queue-next"), &a),
        "答掉一条后顶上来的那一条（2 of 2）"
    );
    let long = sub_ask(LONG_CMD, GP);
    assert!(
        (SCREEN.seen)(&screen!("wrap-60"), &long),
        "60 宽按词折行、前面加 │"
    );
    assert!(
        (SCREEN.seen)(&screen!("split-40"), &long),
        "40 宽拆栏 · 硬折"
    );
    assert!(
        (SCREEN.seen)(&screen!("main-160"), &sub_ask("touch ./plum.txt", None)),
        "主会话的框"
    );
    assert_eq!(
        SCREEN.allow,
        crate::agents::Keystroke::Char("1"),
        "允许按 1（头一项恒是 Yes）"
    );
    assert_eq!(
        SCREEN.deny,
        crate::agents::Keystroke::Key("esc"),
        "拒绝按 Esc（只拒这一条）"
    );
}

#[test]
fn a_box_that_is_not_this_request_is_not_seen() {
    let a = sub_ask("touch ./plum.txt", GP);
    let no = |screen: &[String], why: &str| assert!(!(SCREEN.seen)(screen, &a), "{why}");
    no(&screen!("main-160"), "主会话的框（不带 from）");
    assert!(
        !(SCREEN.seen)(
            &screen!("wide-160"),
            &sub_ask("touch ./plum.txt", Some("scout"))
        ),
        "别的 agent"
    );
    assert!(
        !(SCREEN.seen)(&screen!("wide-160"), &sub_ask("touch ./pear.txt", GP)),
        "别的命令"
    );
    no(&screen!("queue-first"), "1 of 2：正显示的是排着的另一条");
    no(&screen!("other-tool-160"), "别的工具");
    // 矮窗：框比窗高，标题 · Tip · 说明都被顶出屏外，只看得见从第一道虚线往下那一截。
    let short = screen!("short-80x11");
    assert!(
        short.first().is_some_and(|l| l.starts_with('╌')),
        "矮窗夹具从虚线起"
    );
    assert!(
        !(SCREEN.seen)(&short, &sub_ask(LONG_CMD, GP)),
        "矮窗：看不见标题"
    );
    // 问句下面没有 N. Yes … N. No。
    let mut bare = screen!("wide-160");
    bare.retain(|l| !l.trim_start().starts_with("❯ 1.") && !l.trim_start().starts_with("4. No"));
    no(&bare, "没有 Yes / No 那几项");
    no(&screen!("queue-done"), "屏上没有框");
    // 主会话那一条（不带 agent_type）对上的是不带 from 的那一形。
    let main = sub_ask("touch ./plum.txt", None);
    assert!(
        !(SCREEN.seen)(&screen!("wide-160"), &main),
        "主会话那条不认子 agent 的框"
    );
}

/// ★ 命令只差空白的两条不算同一条（屏上折行处之外的空白要对得上）；子 agent 发的却没带种类 ⇒ 认不准是谁的，不认。
#[test]
fn a_command_that_differs_only_in_spaces_or_an_agent_without_a_kind_is_not_seen() {
    let wide = screen!("wide-160");
    assert!(
        !(SCREEN.seen)(&wide, &sub_ask("touch  ./plum.txt", GP)),
        "多一个空格"
    );
    assert!(
        !(SCREEN.seen)(&wide, &sub_ask("touch./plum.txt", GP)),
        "少一个空格"
    );
    let wrap = screen!("wrap-60");
    assert!(
        wrap.iter()
            .any(|l| l.trim() == "│ ./a_very_long_file_name_number_two.txt"),
        "60 宽夹具按词折了行"
    );
    assert!(
        (SCREEN.seen)(&wrap, &sub_ask(LONG_CMD, GP)),
        "按词折行处丢掉的那个空格不算差"
    );
    let mut v = bash_req();
    v["tool_input"] = json!({"command": "touch ./plum.txt"});
    v["agent_id"] = json!("a-9");
    let kindless = read(&v).unwrap();
    assert!(kindless.blocks_terminal);
    assert!(!(SCREEN.seen)(&screen!("main-160"), &kindless), "没带种类");
}

/// ★ 选项折行：续行照缩进认（比编号那一格更靠右），不照开头数字 —— 续行恰好以「3. 」打头也不算一项。
#[test]
fn an_option_wrapped_onto_a_line_that_starts_with_a_number_is_still_one_option() {
    let mut s = screen!("wrap-60");
    let at = s
        .iter()
        .position(|l| l.trim() == "project")
        .expect("60 宽夹具里选项折了行");
    s[at] = s[at].replace("project", "3. project");
    assert!(
        (SCREEN.seen)(&s, &sub_ask(LONG_CMD, GP)),
        "续行被当成了一项"
    );
}

/// ★ 屏上说还排着几条（连正显示的这一条）：`k of n` ⇒ n − k + 1；不画（只一条 · 窄窗）⇒ 不知道。
#[test]
fn the_box_says_how_many_are_still_queued() {
    assert_eq!((SCREEN.queued)(&screen!("queue-first")), Some(2));
    assert_eq!((SCREEN.queued)(&screen!("queue-next")), Some(1));
    assert_eq!((SCREEN.queued)(&screen!("wide-160")), None);
    assert_eq!((SCREEN.queued)(&screen!("queue-done")), None);
}

/// ★ 排着几条只读正显示的这个框的标题：40 列拆栏（标题不以 `Bash command` 起头）时，回滚区里更早那个框的 `k of n` 不算。
#[test]
fn the_queue_count_is_read_only_from_the_box_now_shown() {
    let mut s = screen!("queue-first");
    s.extend(screen!("split-40"));
    assert_eq!((SCREEN.queued)(&s), None);
}
