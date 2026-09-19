use super::*;
use serde_json::Value;

fn spec(sid: &str, candidates: Vec<Option<&str>>) -> ResumeSpec {
    ResumeSpec {
        session_id: sid.to_string(),
        launch_candidates: candidates
            .into_iter()
            .map(|o| o.map(str::to_string))
            .collect(),
        claude_dir: String::new(),
        fallback_cwd: String::new(),
        already_in_tmux: false,
        agent_kind: String::new(),
    }
}

/// 契约：ResumeSpec → CommandPlan 的 wire 形状 + camelCase + aterm 4 caps 名 + substitutedFrom。
/// DG3（#2D）：ResumeSpec 的 `agent_kind` 从 **camelCase** wire `agentKind` 反序列化（resolve I/O 面
/// 全 camelCase）；缺省 → `""`（= claude 兼容）。DG6 据此构 codex resume vs claude --resume。
#[test]
fn resume_spec_parses_camelcase_agent_kind_default_claude() {
    let spec: ResumeSpec =
        serde_json::from_str(r#"{"sessionId":"s","agentKind":"codex"}"#).expect("parse");
    assert_eq!(spec.agent_kind, "codex");
    // 缺 agentKind → default ""（缺=claude，向后兼容旧 monitor 不发此字段）。
    let spec2: ResumeSpec = serde_json::from_str(r#"{"sessionId":"s"}"#).expect("parse");
    assert_eq!(spec2.agent_kind, "");
}

#[test]
fn resolve_builds_plan_with_aterm_field_names() {
    let s = spec(
        "abcd1234-5678-90ab-cdef-1234567890ab",
        vec![None, Some("cct"), Some("cc")],
    );
    let plan = resolve(&s).expect("valid");
    // 序列化 → 校验 camelCase 键名（尤其 aterm 4 caps 名逐字对齐、免映射）。
    let v: Value = serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
    assert_eq!(v["mode"], "PtyInject");
    assert_eq!(
        v["command"],
        "cct --resume abcd1234-5678-90ab-cdef-1234567890ab"
    );
    // substitutedFrom：MVP 恒省略——daemon 不做候选消解、无「被替换的原命令」（对齐 aterm
    // 语义：仅解析 launch≠原意首候选时才有；见 resolve() 注 + aterm 2026-07-18 确认）。
    assert!(
        v.get("substitutedFrom").is_none(),
        "MVP 无候选消解 → substitutedFrom 省略（不再误设为被用候选）"
    );
    assert_eq!(v["sessionName"], "cc-abcd1234"); // cc-<sid8>
    let caps = &v["capabilities"];
    assert!(
        caps.get("supportsSendKeys").is_some(),
        "aterm caps 名 supportsSendKeys"
    );
    assert!(caps.get("supportsCapture").is_some());
    assert!(caps.get("supportsMultiClient").is_some());
    assert!(caps.get("supportsMultiWindow").is_some());
    // 短名不得出现（否则两端要映射）。
    assert!(caps.get("sendKeys").is_none(), "不得用短名 sendKeys");
}

/// DG6：agent_kind="codex" → `<base> resume <uuid>`（**子命令、无 --resume**）+ 会话名 `cx-`；
/// 默认基底 `codex`、自定义候选照用。golden-parity aterm CodexInvocation.resumeInvocation/resumeSessionName。
#[test]
fn resolve_codex_builds_resume_subcommand_and_cx_name() {
    let uuid = "019f75dd-875c-7c81-9eda-32f866b2c60f";
    // 无候选 → 默认 codex。
    let s = ResumeSpec {
        agent_kind: "codex".into(),
        ..spec(uuid, vec![])
    };
    let v: Value =
        serde_json::from_str(&serde_json::to_string(&resolve(&s).expect("valid")).unwrap())
            .unwrap();
    assert_eq!(v["command"], format!("codex resume {uuid}"));
    assert_eq!(v["sessionName"], "cx-019f75dd"); // cx-<sid8>（非 cc-）
    assert_eq!(v["mode"], "PtyInject");
    // 自定义 codex 启动候选 → `<base> resume <sid>`。
    let s2 = ResumeSpec {
        agent_kind: "codex".into(),
        ..spec(uuid, vec![Some("mycodex")])
    };
    let v2: Value =
        serde_json::from_str(&serde_json::to_string(&resolve(&s2).expect("valid")).unwrap())
            .unwrap();
    assert_eq!(v2["command"], format!("mycodex resume {uuid}"));
}

/// DG6 审计补：非规范 agent_kind（大小写/其它值）→ **落 Claude 路**（`--resume`/`cc-`），
/// 防 Codex 分支误吞。契约：wire 值定死小写 `"codex"`（大小写敏感）。
#[test]
fn resolve_non_codex_agent_kind_falls_back_to_claude() {
    for ak in ["", "claude", "Codex", "CODEX", "foo"] {
        let s = ResumeSpec {
            agent_kind: ak.to_string(),
            ..spec("sid_x", vec![Some("claude")])
        };
        let v: Value =
            serde_json::from_str(&serde_json::to_string(&resolve(&s).expect("valid")).unwrap())
                .unwrap();
        assert_eq!(
            v["command"], "claude --resume sid_x",
            "agent_kind={ak:?} 应落 Claude"
        );
        assert_eq!(v["sessionName"], "cc-sid_x");
    }
}

/// 无候选 → 默认 `claude`、substitutedFrom 省略（None → skip_serializing_if）。
#[test]
fn resolve_defaults_to_claude_when_no_candidates() {
    let s = spec("sid_123", vec![None, Some("   ")]); // 全空/空白 → 无可用
    let plan = resolve(&s).expect("valid");
    let v: Value = serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
    assert_eq!(v["command"], "claude --resume sid_123");
    assert!(
        v.get("substitutedFrom").is_none(),
        "无候选 → substitutedFrom 省略"
    );
}

/// B2：非法 sessionId（含 shell 元字符）→ 错误 {code,message}，不进 command（注入防线）。
#[test]
fn resolve_rejects_injection_in_session_id() {
    for bad in ["", "a b", "sid;rm -rf /", "$(whoami)", "a`b`", "x/y"] {
        let s = spec(bad, vec![Some("cc")]);
        let err = resolve(&s).expect_err("must reject");
        assert_eq!(err.0, "invalid_session_id", "拒 {bad:?}");
    }
    // 合法集通过。
    assert!(resolve(&spec("abc-DEF_123", vec![Some("cc")])).is_ok());
}

/// 审计 security①：B2 对称化——launchCandidate（base）含 shell 元字符 → unsafe_launch_candidate，
/// 不进 command（此前 base 零校验、注入透传：`["cc; rm -rf /"]`→`cc; rm -rf / --resume …`）。
#[test]
fn resolve_rejects_injection_in_launch_candidate() {
    for bad in [
        "cc; rm -rf /",
        "a$(id)",
        "x`whoami`",
        "a|b",
        "a>b",
        "a\nb",
        "a&b",
    ] {
        let s = spec("abc123", vec![Some(bad)]);
        let err = resolve(&s).expect_err("must reject base");
        assert_eq!(err.0, "unsafe_launch_candidate", "拒 base {bad:?}");
    }
    // 合法 launcher（带 flag/路径/等号）通过。
    assert!(resolve(&spec("abc123", vec![Some("/usr/bin/cc --foo=bar")])).is_ok());
}

/// 审计 quality-阻塞：`resolve_from_json`（= `run()` 的分发核）真覆盖 happy 路径——
/// stdin JSON 串 → CommandPlan JSON 串（含 aterm caps 名）。此前 `run()` 零覆盖。
#[test]
fn resolve_from_json_happy_returns_command_plan_json() {
    let out = resolve_from_json(
        r#"{"sessionId":"abcd1234-ef","launchCandidates":["cc"],"claudeDir":"/c","fallbackCwd":"/t","alreadyInTmux":false}"#,
    )
    .expect("ok");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["command"], "cc --resume abcd1234-ef");
    assert_eq!(v["capabilities"]["supportsSendKeys"], true);
}

/// 审计 quality-阻塞：`run()` 的 bad_request 分支真覆盖（此前只测 `serde_json::from_str` 本身、
/// 不走分发）。畸形 JSON 经 `resolve_from_json` → `(bad_request, _)`。
#[test]
fn resolve_from_json_malformed_is_bad_request() {
    let err = resolve_from_json("{not json").expect_err("must err");
    assert_eq!(err.0, "bad_request");
}

/// 分发核对非法 sid / 不安全 base 同样短路成对应错误码（端到端错误 taxonomy）。
#[test]
fn resolve_from_json_propagates_validation_errors() {
    assert_eq!(
        resolve_from_json(r#"{"sessionId":"a;b"}"#).unwrap_err().0,
        "invalid_session_id"
    );
    assert_eq!(
        resolve_from_json(r#"{"sessionId":"ok1","launchCandidates":["c;d"]}"#)
            .unwrap_err()
            .0,
        "unsafe_launch_candidate"
    );
}
