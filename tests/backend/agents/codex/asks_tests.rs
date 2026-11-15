//! Codex 钩子档：起会话垫的那一个 `-c`（四种钩子 ＋ 各自的信任哈希）· 钩子入参翻成统一事件。
//! 键 · 哈希 · 版本门各自的判据在 `hook_trust_tests.rs`；这里只钉挂出去的那一整张。
//! 哈希夹具是照 codex-rs `rust-v0.167.0` 的规则（钩子的规范化身份 · 配置指纹 · 信任键的拼法）用 Python `hashlib` 异源算的，
//! 不是本实现自己吐的；入参照那一版钩子入参的字段造，值都是编的。

use super::*;
use serde_json::json;

/// 起会话垫的那一个 `-c`：四种钩子都在，信任那几格逐字等于异源算的夹具（ccm 入口 `/h/ccm`）。
#[cfg(not(windows))]
#[test]
fn launch_args_carry_the_four_hooks_with_their_trusted_hashes() {
    const FIXTURE: &str = concat!(
        r#"hooks={"PermissionRequest"=[{"hooks"=[{"command"="'/h/ccm' -- --answer-hook codex","timeout"=3600,"type"="command"}]}],"#,
        r#""SessionStart"=[{"hooks"=[{"command"="'/h/ccm' -- --answer-hook codex","timeout"=10,"type"="command"}]}],"#,
        r#""Stop"=[{"hooks"=[{"command"="'/h/ccm' -- --answer-hook codex","timeout"=10,"type"="command"}]}],"#,
        r#""UserPromptSubmit"=[{"hooks"=[{"command"="'/h/ccm' -- --answer-hook codex","timeout"=10,"type"="command"}]}],"#,
        r#""state"={"/<session-flags>/config.toml:permission_request:0:0"={"trusted_hash"="sha256:aa6b28cf0802574c967fce5bf982c0adeccbc7b3617f17a58cb6880ecc53189f"},"#,
        r#""/<session-flags>/config.toml:session_start:0:0"={"trusted_hash"="sha256:630a699fa6078e01f0dd4403480450148ee674dd21168197549a1de40a8a9412"},"#,
        r#""/<session-flags>/config.toml:stop:0:0"={"trusted_hash"="sha256:9877c679a2e0a155c55d114fb0fe1e68367b4ecb05d14abcc5b7315d755941e2"},"#,
        r#""/<session-flags>/config.toml:user_prompt_submit:0:0"={"trusted_hash"="sha256:23cbc90c719a30230920b21aa6d8d93a89e42bb466a59dc26d6d8bb181142fb6"}}}"#,
    );
    assert_eq!(
        launch_args("/h/ccm"),
        vec!["-c".to_string(), FIXTURE.to_string()]
    );
}

/// 入口路径里带单引号也照 POSIX 单引号拆开写（钩子命令经 `sh -lc` 跑）。
#[cfg(not(windows))]
#[test]
fn the_hook_command_quotes_the_entry_for_the_shell() {
    assert_eq!(
        command_of("/h/it's/ccm"),
        r#"'/h/it'\''s/ccm' -- --answer-hook codex"#
    );
}

fn hook(event: &str, more: Value) -> Value {
    let mut v = json!({
        "session_id": "019a-thread",
        "transcript_path": null,
        "cwd": "/w",
        "hook_event_name": event,
        "model": "m",
        "permission_mode": "default",
    });
    v.as_object_mut()
        .unwrap()
        .extend(more.as_object().unwrap().clone());
    v
}

/// 一轮结束 · 提交了一句 · 会话起来闲着 ⇒ 统一事件；答题那一种 · 中途压缩那次起来 · 缺会话 id ⇒ 不是。
#[test]
fn hook_inputs_turn_into_the_unified_events() {
    let stop = hook(
        "Stop",
        json!({"turn_id": "t1", "stop_hook_active": false, "last_assistant_message": "ok"}),
    );
    assert_eq!(
        heard(&stop),
        Some(("019a-thread".into(), AgentEvent::TurnEnded))
    );
    let prompt = hook("UserPromptSubmit", json!({"turn_id": "t1", "prompt": "hi"}));
    assert_eq!(
        heard(&prompt),
        Some(("019a-thread".into(), AgentEvent::PromptSubmitted))
    );
    for source in ["startup", "resume", "clear", "fork"] {
        let start = hook("SessionStart", json!({"source": source}));
        assert_eq!(
            heard(&start),
            Some(("019a-thread".into(), AgentEvent::TurnEnded)),
            "{source}"
        );
    }
    assert_eq!(
        heard(&hook("SessionStart", json!({"source": "compact"}))),
        None
    );
    let ask = hook(
        "PermissionRequest",
        json!({"turn_id": "t1", "tool_name": "Bash", "tool_input": {"command": "ls"}}),
    );
    assert_eq!(heard(&ask), None);
    let mut nosid = stop.clone();
    nosid["session_id"] = json!(" ");
    assert_eq!(heard(&nosid), None);
}

/// 注册表里 Codex 那一格有答题口：钩子档（一轮结束交得进活会话表那一道）；起会话垫的就是本模块那两个词。
#[test]
fn the_registry_gives_codex_the_hook_tier() {
    let f = crate::agents::ask_face(super::super::AGENT_KIND).expect("Codex 有答题口");
    let stop = hook(
        "Stop",
        json!({"turn_id": "t1", "stop_hook_active": false, "last_assistant_message": null}),
    );
    assert_eq!(
        (f.heard)(&stop),
        Some(("019a-thread".into(), AgentEvent::TurnEnded))
    );
    assert_eq!((f.launch_args)("/h/ccm"), launch_args("/h/ccm"));
    assert_eq!(f.launch_flag, CONFIG_FLAG);
    assert!(f.merge.is_none(), "`-c` 可重复，不读用户那几条");
    let g = f.gate.expect("有版本门");
    assert_eq!(g.min_version, super::super::hook_trust::MIN_VERSION);
    // 拒绝写不出「并停下」：回包带 `interrupt` Codex 当这道钩子失败 ⇒ 待答成品里不出「拒绝并停下」。
    assert!(!f.stop_on_deny);
}

/// 经钩子答得了的只有批准那一种（`PermissionRequest`）；提问 · 批计划（Codex 问用户的那个工具）没有钩子能答 ⇒ 认作答不了、照常在终端里问。
#[test]
fn only_approvals_are_answered_through_the_hook() {
    use crate::agents::AskKind;
    let of = |kind| AskIn {
        sid: "s1".into(),
        kind,
        tool: "Tool".into(),
        what: None,
        detail: String::new(),
        suggestions: Vec::new(),
        questions: Vec::new(),
        agent_id: None,
        agent_type: None,
        blocks_terminal: false,
        input: Value::Null,
    };
    assert!(!(FACE.cannot)(&of(AskKind::Approve)));
    assert!((FACE.cannot)(&of(AskKind::Answer)));
    assert!((FACE.cannot)(&of(AskKind::Plan)));
}

/// 一份 `PermissionRequest` 入参（形照 codex-rs `rust-v0.167.0` 的 permission-request 入参 schema，值是编的）。
fn ask(tool: &str, input: Value) -> Value {
    hook(
        "PermissionRequest",
        json!({"turn_id": "t1", "tool_name": tool, "tool_input": input}),
    )
}

/// 批一条命令：种类是批准；钉条是命令头一行、全文不截；钩子跑在批准框之前 ⇒ 挡着终端；Codex 不给「以后不再问」的建议。
#[test]
fn a_command_approval_is_read_into_one_pending_request() {
    let a = read(&ask(
        "Bash",
        json!({"command": "rm -rf build\necho done", "description": "clean"}),
    ))
    .expect("读得出");
    assert_eq!(a.sid, "019a-thread");
    assert_eq!(a.kind, crate::agents::AskKind::Approve);
    assert_eq!(a.tool, "Bash");
    assert_eq!(a.what.as_deref(), Some("rm -rf build"));
    assert_eq!(a.detail, "rm -rf build\necho done");
    assert!(a.blocks_terminal);
    assert!(a.suggestions.is_empty() && a.questions.is_empty());
    assert_eq!((a.agent_id, a.agent_type), (None, None));
    assert_eq!(a.input["description"], "clean");
}

/// 改文件（`apply_patch`）：钉条是补丁里头一个文件，全文是整份补丁；MCP 工具等认不出的 ⇒ 参数原样排开、钉条说不出。
#[test]
fn patches_and_other_tools_carry_their_full_input() {
    let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-x\n+y\n*** Add File: b.txt\n+hi\n*** End Patch";
    let a = read(&ask("apply_patch", json!({"command": patch}))).expect("读得出");
    assert_eq!(a.what.as_deref(), Some("src/a.rs"));
    assert_eq!(a.detail, patch);
    let m = read(&ask("mcp__docs__search", json!({"q": "x"}))).expect("读得出");
    assert_eq!(m.kind, crate::agents::AskKind::Approve);
    assert_eq!(m.what, None);
    assert_eq!(
        m.detail,
        serde_json::to_string_pretty(&json!({"q": "x"})).unwrap()
    );
}

/// 子 agent 发的带着它的 id 与种类；别的事件 · 缺会话 id · 缺工具名 ⇒ 不是待答请求。
#[test]
fn subagents_are_named_and_odd_inputs_are_not_requests() {
    let mut v = ask("Bash", json!({"command": "ls"}));
    v["agent_id"] = json!("ag1");
    v["agent_type"] = json!("worker");
    let a = read(&v).expect("读得出");
    assert_eq!(
        (a.agent_id.as_deref(), a.agent_type.as_deref()),
        (Some("ag1"), Some("worker"))
    );
    assert_eq!(read(&hook("Stop", json!({"turn_id": "t1"}))), None);
    let mut nosid = ask("Bash", json!({"command": "ls"}));
    nosid["session_id"] = json!("");
    assert_eq!(read(&nosid), None);
    let mut notool = ask("Bash", json!({"command": "ls"}));
    notool.as_object_mut().unwrap().remove("tool_name");
    assert_eq!(read(&notool), None);
}

/// 答的那一份逐字是 permission-request 输出 schema 那一形：只有 `behavior` 与可缺的 `message`；
/// `interrupt` · `updatedPermissions` · `updatedInput` 三格 Codex 一见就当失败 ⇒ 一格都不带（「拒绝并停下」也只拒绝）。
#[test]
fn the_answer_is_allow_or_deny_with_an_optional_message() {
    let a = read(&ask("Bash", json!({"command": "ls"}))).expect("读得出");
    let out = |v: &Verdict| decide(&a, v).expect("给得出").to_string();
    let allow = Verdict::Allow {
        rule: Some(0),
        mode: None,
        answers: Vec::new(),
    };
    assert_eq!(
        out(&allow),
        r#"{"hookSpecificOutput":{"decision":{"behavior":"allow"},"hookEventName":"PermissionRequest"}}"#
    );
    let deny = Verdict::Deny {
        note: Some("not now".into()),
        stop: true,
    };
    assert_eq!(
        out(&deny),
        r#"{"hookSpecificOutput":{"decision":{"behavior":"deny","message":"not now"},"hookEventName":"PermissionRequest"}}"#
    );
    let bare = Verdict::Deny {
        note: Some("  ".into()),
        stop: false,
    };
    assert_eq!(
        out(&bare),
        r#"{"hookSpecificOutput":{"decision":{"behavior":"deny"},"hookEventName":"PermissionRequest"}}"#
    );
}

/// 用户自己写的 `-c hooks=…` 整格盖掉我们那一张 `-c hooks={…}`（`-c` 同一层、后写的胜；键按 `.` 切，`hooks` 这一格整个换掉）：
/// `-c hooks=…` · `--config hooks=…` · `--config=hooks=…` · `-chooks=…` · `-c=hooks=…` 都算；`-c hooks.state.x=…`（只动里面一格）·
/// `-c model=…` 不算；`--` 之后是第一句，不看。
#[test]
fn a_users_own_hooks_table_overrides_ours() {
    let f = FACE.overridden.expect("Codex 声明了盖表那一判");
    let v = |a: &[&str]| f(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    for over in [
        &["-c", "hooks={}"][..],
        &["--config", "hooks = {}"],
        &["--config=hooks={}"],
        &["-chooks={}"],
        &["-c=hooks={}"],
        &["-m", "o3", "-c", " hooks ={Stop=[]}"],
    ] {
        assert!(v(over), "{over:?}");
    }
    for not in [
        &["-c", "hooks.state.x={}"][..],
        &["-c", "model=\"o3\""],
        &["-c", "hookser=1"],
        &["--", "-c", "hooks={}"],
        &[],
    ] {
        assert!(!v(not), "{not:?}");
    }
}
