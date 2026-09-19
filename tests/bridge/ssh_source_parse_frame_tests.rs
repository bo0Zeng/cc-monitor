use super::*;

/// hello 帧解析：取 v / build_id / host_arch / claude_dir（#33 起捕获 build_id）。
#[test]
fn parses_hello_and_captures_build_id() {
    let line = r#"{"kind":"hello","v":1,"build_id":"abc123","host_arch":"aarch64","claude_dir":"/home/pi/.claude"}"#;
    let frame = parse_frame(line).expect("hello must parse");
    assert_eq!(
        frame,
        InboundFrame::Hello {
            v: 1,
            build_id: "abc123".to_string(),
            host_arch: "aarch64".to_string(),
            claude_dir: "/home/pi/.claude".to_string(),
            // `S4`：本样本无 `homes` 字段（= 今天所有已部署的 daemon）→ 空表 ⇒ 回退 `claude_dir`。
            homes: Vec::new(),
            // F66：旧 daemon（本样本无 capabilities 字段）→ 空集（保守缺省）
            capabilities: Vec::new(),
            // U8a-2a：同理，无 commands 字段 → 空集 ⇒ 一条入方向命令都不发。
            commands: Vec::new(),
        }
    );
}

/// ★ `S4`：`hello.homes` 的**解析 + 回退**必须有判据 —— 四种形态一次钉住。
///
/// 它防的是 `commands` 那次同款的病（见下一条的 D 审计变异 B1）：
/// 字段名漂一个字母 ⇒ `homes` 永远空 ⇒ 永远走回退 ⇒ **一切照常绿**，
/// 而 additive 迁移实际上没发生。所以这里逐形态断言，不只断言"不 panic"。
#[test]
fn parses_hello_homes_and_falls_back_to_claude_dir() {
    // ① 无 `homes`（= 今天所有已部署的 daemon）⇒ 空表 ⇒ 回退 `claude_dir`。
    let old = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/old/.claude"}"#;
    match parse_frame(old).expect("hello must parse") {
        InboundFrame::Hello {
            homes, claude_dir, ..
        } => {
            assert!(homes.is_empty(), "旧 daemon 不该凭空长出 homes");
            assert_eq!(
                claude_home_from_hello(&homes, &claude_dir),
                "/old/.claude",
                "无 homes 时必须回退 claude_dir —— 这条一坏，所有已部署的 daemon 当场失去 home"
            );
        }
        other => panic!("expected Hello, got {other:?}"),
    }

    // ② 有 `homes` 且含 claude 项 ⇒ **homes 优先**（`claude_dir` 故意给个不同的值，
    //    这样"优先"是真的被验到了，而不是两边碰巧相等）。
    let new = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/legacy/.claude","homes":[{"agent_kind":"claude","path":"/new/.claude"},{"agent_kind":"codex","path":"/new/.codex"}]}"#;
    match parse_frame(new).expect("hello must parse") {
        InboundFrame::Hello {
            homes, claude_dir, ..
        } => {
            assert_eq!(homes.len(), 2, "两项都该解析出来：{homes:?}");
            assert_eq!(homes[1].agent_kind, "codex");
            assert_eq!(homes[1].path, "/new/.codex");
            assert_eq!(
                claude_home_from_hello(&homes, &claude_dir),
                "/new/.claude",
                "有 homes 时必须**优先**读它 —— 回退值是 /legacy/.claude，读到它就说明优先级反了"
            );
        }
        other => panic!("expected Hello, got {other:?}"),
    }

    // ③ 有 `homes` 但**没有 claude 那一项**（只服务别的 agent）⇒ 仍回退 `claude_dir`。
    let other_only = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/legacy/.claude","homes":[{"agent_kind":"codex","path":"/new/.codex"}]}"#;
    match parse_frame(other_only).expect("hello must parse") {
        InboundFrame::Hello {
            homes, claude_dir, ..
        } => assert_eq!(
            claude_home_from_hello(&homes, &claude_dir),
            "/legacy/.claude",
            "homes 非空但没有 claude 项时，不许把别的 agent 的 home 当成 claude 的"
        ),
        other => panic!("expected Hello, got {other:?}"),
    }

    // ④ 坏数据：非数组 / 元素不是对象 / 缺字段 / 字段类型不对
    //    ⇒ **逐项丢掉、整帧仍解析**（`homes` 不是必需字段，坏它不该让整条 hello 变 garbage）。
    let junk = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d","homes":"not-an-array"}"#;
    match parse_frame(junk).expect("非数组的 homes 不该让整帧变 None") {
        InboundFrame::Hello { homes, .. } => assert!(homes.is_empty()),
        other => panic!("expected Hello, got {other:?}"),
    }
    let mixed = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d","homes":[7,null,{"agent_kind":"claude"},{"path":"/p"},{"agent_kind":"codex","path":42},{"agent_kind":"codex","path":"/ok"}]}"#;
    match parse_frame(mixed).expect("坏项不该让整帧变 None") {
        InboundFrame::Hello { homes, .. } => {
            assert_eq!(
                homes,
                vec![AgentHome {
                    agent_kind: "codex".to_string(),
                    path: "/ok".to_string(),
                }],
                "只有完整且类型正确的那一项该留下：{homes:?}"
            );
        }
        other => panic!("expected Hello, got {other:?}"),
    }
}

/// ★ U8a-2a：`hello.commands` 的**解析**必须有判据。
///
/// D 审计变异 B1：把 `obj.get("commands")` 改成 `obj.get("commandz")`（两侧字段名漂移
/// 或一个笔误）⇒ `cargo test` 与 e2e **双双全绿**，而后果是 `commands` 永远空集 ⇒
/// `InboundClient::accepts()` 永远假 ⇒ **一条入方向命令都发不出去**，
/// 也就是 U8a-2a 要修的那个「通道不可达」原地复活。
///
/// e2e 挡不住的原因：它 `grep` 的是整行 hello，daemon 把 `ping` 放哪个键里都绿。
#[test]
fn parses_hello_commands_across_the_three_shapes() {
    let with = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d","commands":["cancel","ping","resolve"]}"#;
    match parse_frame(with).expect("hello must parse") {
        InboundFrame::Hello { commands, .. } => {
            assert_eq!(
                commands,
                vec!["cancel", "ping", "resolve"],
                "声明的命令集没解出来"
            );
        }
        other => panic!("不是 hello：{other:?}"),
    }
    // 旧 daemon：无该字段 → 空集（保守缺省，不发任何入方向命令）。
    let without =
        r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d"}"#;
    match parse_frame(without).expect("hello must parse") {
        InboundFrame::Hello { commands, .. } => assert!(commands.is_empty()),
        other => panic!("不是 hello：{other:?}"),
    }
    // 坏 daemon：非数组 / 元素非字符串 → 滤成空集，**绝不 panic**（同 capabilities 口径）。
    let junk = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d","commands":"ping"}"#;
    match parse_frame(junk).expect("hello must parse") {
        InboundFrame::Hello { commands, .. } => assert!(commands.is_empty()),
        other => panic!("不是 hello：{other:?}"),
    }
    let mixed = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d","commands":["ping",7,null]}"#;
    match parse_frame(mixed).expect("hello must parse") {
        InboundFrame::Hello { commands, .. } => assert_eq!(commands, vec!["ping"]),
        other => panic!("不是 hello：{other:?}"),
    }
}

/// ★ U8a-2a：`reply` 帧的**字段**解析必须有判据。
///
/// D 审计变异 I1：`ok` 改成 `unwrap_or(true)`（错误应答变成成功）+ `data` 从 `payload`
/// 键取 ⇒ 全绿。`known_kinds_matches_parse_frame` 只钉「有没有这条臂」，不看臂里取什么。
#[test]
fn parses_reply_and_cancelled_field_by_field() {
    let ok_line = r#"{"kind":"reply","id":"a-1","ok":true,"data":{"pong":1}}"#;
    assert_eq!(
        parse_frame(ok_line).expect("reply must parse"),
        InboundFrame::Reply {
            id: "a-1".into(),
            ok: true,
            code: None,
            message: None,
            data: Some(serde_json::json!({ "pong": 1 })),
        }
    );
    let err_line =
        r#"{"kind":"reply","id":"a-2","ok":false,"code":"bad_request","message":"缺 sid"}"#;
    assert_eq!(
        parse_frame(err_line).expect("reply must parse"),
        InboundFrame::Reply {
            id: "a-2".into(),
            ok: false,
            code: Some("bad_request".into()),
            message: Some("缺 sid".into()),
            data: None,
        }
    );
    // daemon 对**协议级**错误回空 id（它那时还不知道 id）——空串是合法值，不是坏帧。
    let proto_err =
        r#"{"kind":"reply","id":"","ok":false,"code":"line_too_long","message":"x"}"#;
    assert!(matches!(
        parse_frame(proto_err),
        Some(InboundFrame::Reply { ok: false, .. })
    ));
    // 必需字段缺失 / 类型不对 → 坏帧跳过（与其余帧同一口径），**绝不当成 ok**。
    for bad in [
        r#"{"kind":"reply","ok":true}"#,
        r#"{"kind":"reply","id":"a-3"}"#,
        r#"{"kind":"reply","id":"a-3","ok":"true"}"#,
        r#"{"kind":"reply","id":7,"ok":true}"#,
    ] {
        assert!(parse_frame(bad).is_none(), "坏 reply 却解出来了：{bad}");
    }
    assert_eq!(
        parse_frame(r#"{"kind":"cancelled","id":"a-4"}"#).expect("cancelled must parse"),
        InboundFrame::Cancelled { id: "a-4".into() }
    );
    assert!(parse_frame(r#"{"kind":"cancelled"}"#).is_none());
}

/// #33：hello 缺 build_id → None（按必需字段，坏帧跳过；既有 daemon 总在发它）。
#[test]
fn hello_missing_build_id_returns_none() {
    let line = r#"{"kind":"hello","v":1,"host_arch":"x86_64","claude_dir":"/c"}"#;
    assert_eq!(parse_frame(line), None);
}

/// F66（#58③）wire 契约：hello 的 `capabilities` 字段。
/// ① 缺字段（旧 daemon）→ 空集（向后兼容，保守缺省，同 §27 族）。
/// ② 声明数组 → 原样解析（monitor 按此决定发哪些 flag）。
/// ③ 非数组 / 元素非字符串 → 滤成空集，绝不 panic（宽容解析，§18）。
#[test]
fn hello_capabilities_backward_compat_and_declared() {
    // ① 旧 daemon：无 capabilities → 空集
    let old =
        r#"{"kind":"hello","v":1,"build_id":"p1e","host_arch":"x86_64","claude_dir":"/c"}"#;
    match parse_frame(old).unwrap() {
        InboundFrame::Hello { capabilities, .. } => {
            assert!(capabilities.is_empty(), "旧 daemon 无声明 → 空集");
        }
        _ => panic!("expected Hello"),
    }
    // ② 新 daemon：声明能力
    let new = r#"{"kind":"hello","v":1,"build_id":"p1h","host_arch":"x86_64","claude_dir":"/c","capabilities":["bg","tail-only"]}"#;
    match parse_frame(new).unwrap() {
        InboundFrame::Hello { capabilities, .. } => {
            assert_eq!(
                capabilities,
                vec!["bg".to_string(), "tail-only".to_string()]
            );
        }
        _ => panic!("expected Hello"),
    }
    // ③ 畸形 capabilities（非数组 / 混入非字符串）→ 不 panic，滤成空/仅字符串
    let bad = r#"{"kind":"hello","v":1,"build_id":"p1x","host_arch":"x86_64","claude_dir":"/c","capabilities":"not-array"}"#;
    match parse_frame(bad).unwrap() {
        InboundFrame::Hello { capabilities, .. } => {
            assert!(capabilities.is_empty(), "非数组 capabilities → 空集，不崩");
        }
        _ => panic!("expected Hello"),
    }
    let mixed = r#"{"kind":"hello","v":1,"build_id":"p1x","host_arch":"x86_64","claude_dir":"/c","capabilities":["bg",42,null,"tail-only"]}"#;
    match parse_frame(mixed).unwrap() {
        InboundFrame::Hello { capabilities, .. } => {
            assert_eq!(
                capabilities,
                vec!["bg".to_string(), "tail-only".to_string()],
                "非字符串元素被滤掉，字符串保留"
            );
        }
        _ => panic!("expected Hello"),
    }
}

/// #33：版本协商真值表。协议不符优先于 build 差异。
#[test]
fn negotiate_version_truth_table() {
    // 全同 → Ok。
    assert_eq!(
        negotiate_version(EXPECTED_PROTO_V, EXPECTED_DAEMON_BUILD_ID),
        VersionVerdict::Ok
    );
    // 协议同、build 异 → StaleBuild（带上报值）。
    assert_eq!(
        negotiate_version(EXPECTED_PROTO_V, "p1a-history"),
        VersionVerdict::StaleBuild {
            reported: "p1a-history".to_string()
        }
    );
    // 协议异 → Incompatible，且即便 build 也不同，协议优先。
    assert_eq!(
        negotiate_version(999, "whatever"),
        VersionVerdict::Incompatible { reported_v: 999 }
    );
    assert_eq!(
        negotiate_version(999, EXPECTED_DAEMON_BUILD_ID),
        VersionVerdict::Incompatible { reported_v: 999 },
        "协议不符时即使 build 匹配也算不兼容"
    );
}

/// #33：version_warning 文案——Ok→None，其余→Some 且含 label。
#[test]
fn version_warning_messages() {
    assert_eq!(
        version_warning(EXPECTED_PROTO_V, EXPECTED_DAEMON_BUILD_ID, "pi"),
        None
    );
    let stale = version_warning(EXPECTED_PROTO_V, "p1a-history", "pi").expect("stale warns");
    assert!(stale.contains("pi") && stale.contains("p1a-history"));
    let incompat = version_warning(2, EXPECTED_DAEMON_BUILD_ID, "wsl").expect("incompat warns");
    assert!(incompat.contains("wsl") && incompat.contains("不兼容"));
}

/// 两条 line 帧：逐字段断言 session_id / path / seq / raw 都原样取出。
#[test]
fn parses_two_line_frames_with_all_fields() {
    let l0 = r#"{"kind":"line","session_id":"s-1","path":"/home/pi/.claude/projects/p/s-1.jsonl","seq":0,"raw":"{\"type\":\"user\"}"}"#;
    let l1 = r#"{"kind":"line","session_id":"s-1","path":"/home/pi/.claude/projects/p/s-1.jsonl","seq":1,"raw":"second"}"#;

    let f0 = parse_frame(l0).expect("line 0 must parse");
    assert_eq!(
        f0,
        InboundFrame::Line {
            session_id: "s-1".to_string(),
            path: "/home/pi/.claude/projects/p/s-1.jsonl".to_string(),
            seq: 0,
            raw: r#"{"type":"user"}"#.to_string(),
        }
    );

    let f1 = parse_frame(l1).expect("line 1 must parse");
    match f1 {
        InboundFrame::Line { seq, raw, .. } => {
            assert_eq!(seq, 1);
            assert_eq!(raw, "second");
        }
        other => panic!("expected Line, got {other:?}"),
    }
}

// ---------- P5（zero-poll-liveness）：正向死亡帧的解析 ----------

#[test]
fn tmux_session_closed_parses() {
    assert_eq!(
        parse_frame(r#"{"kind":"tmux_session_closed","name":"cc-abc123"}"#),
        Some(InboundFrame::TmuxSessionClosed {
            name: "cc-abc123".to_string()
        })
    );
}

/// 缺 `name` / 非字符串 ⇒ 坏帧跳过（`None`），**不 panic**，与其余帧同一口径。
#[test]
fn tmux_session_closed_bad_payload_is_skipped() {
    assert_eq!(parse_frame(r#"{"kind":"tmux_session_closed"}"#), None);
    assert_eq!(
        parse_frame(r#"{"kind":"tmux_session_closed","name":42}"#),
        None
    );
}

/// 未知 kind（协议向前演进新增的帧类型）→ None，调用方 warn+skip，绝不 panic。
#[test]
fn unknown_kind_returns_none() {
    let line = r#"{"kind":"future_thing","x":1}"#;
    assert_eq!(parse_frame(line), None);
}

/// 完全非 JSON 的 garbage 行 → None，绝不 panic。
#[test]
fn garbage_non_json_returns_none() {
    assert_eq!(parse_frame("not json"), None);
    assert_eq!(parse_frame(""), None);
    // 合法 JSON 但不是 object（数组 / 标量）也 → None
    assert_eq!(parse_frame("[1,2,3]"), None);
    assert_eq!(parse_frame("42"), None);
    // object 但缺 kind
    assert_eq!(parse_frame(r#"{"v":1}"#), None);
}

/// 已知 kind + 额外未知字段：仍正常解析，多余字段被忽略（不 fail）。
#[test]
fn known_kind_with_extra_fields_still_parses() {
    let line = r#"{"kind":"session_added","sid":"s-9","extra":"ignored","nested":{"a":1}}"#;
    let frame = parse_frame(line).expect("session_added with extras must parse");
    assert_eq!(
        frame,
        InboundFrame::SessionAdded {
            sid: "s-9".to_string(),
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        }
    );
}

/// Batch7-F24：p1e daemon 的 session_added 附加元信息正确解析；
/// 旧 daemon 缺字段 → None（上一测试已覆盖）。
#[test]
fn session_added_metadata_parses() {
    let line = r#"{"kind":"session_added","sid":"s-bg","session_kind":"bg","cwd":"/proj/x","name":"评估任务","path":"/home/u/.claude/projects/p/s-bg.jsonl","lines":42}"#;
    let frame = parse_frame(line).expect("must parse");
    assert_eq!(
        frame,
        InboundFrame::SessionAdded {
            sid: "s-bg".to_string(),
            session_kind: Some("bg".to_string()),
            attachable: None,
            cwd: Some("/proj/x".to_string()),
            name: Some("评估任务".to_string()),
            path: Some("/home/u/.claude/projects/p/s-bg.jsonl".to_string()),
            lines: Some(42),
            status: None,
            waiting_for: None,
        }
    );
}

/// Batch9-F27：session_status 帧解析 + session_added 初始 status。
#[test]
fn session_status_frame_parses() {
    let line = r#"{"kind":"session_status","sid":"s-1","status":"waiting","waiting_for":"permission prompt"}"#;
    assert_eq!(
        parse_frame(line),
        Some(InboundFrame::SessionStatus {
            sid: "s-1".to_string(),
            status: Some("waiting".to_string()),
            waiting_for: Some("permission prompt".to_string()),
        })
    );
    // 缺 waiting_for → None
    let line = r#"{"kind":"session_status","sid":"s-2","status":"busy"}"#;
    assert_eq!(
        parse_frame(line),
        Some(InboundFrame::SessionStatus {
            sid: "s-2".to_string(),
            status: Some("busy".to_string()),
            waiting_for: None,
        })
    );
}

/// session_removed 映射到对应 variant。
#[test]
fn parses_session_removed() {
    let line = r#"{"kind":"session_removed","sid":"s-dead"}"#;
    let frame = parse_frame(line).expect("session_removed must parse");
    assert_eq!(
        frame,
        InboundFrame::SessionRemoved {
            sid: "s-dead".to_string(),
            // ★ S0 向后兼容：**旧 daemon 不发 cause** ⇒ 必须解析成 Gone，
            // 即维持今天的行为（查快照判灰点）。
            cause: RemovalCause::Gone,
        }
    );
}

/// ★ S0：带 cause 的帧解析 + 未知取值的降级方向。
#[test]
fn parses_session_removed_cause() {
    assert_eq!(
        parse_frame(r#"{"kind":"session_removed","sid":"s","cause":"superseded"}"#),
        Some(InboundFrame::SessionRemoved {
            sid: "s".to_string(),
            cause: RemovalCause::Superseded,
        })
    );
    // 未知取值退回 Gone：宁可保守判活（可能多留一个灰点），也不能凭一个不认识的词
    // 直接归档掉一个其实还活着的会话——归档是**破坏性**的（forget 绑定 + 关 tab）。
    assert_eq!(
        parse_frame(r#"{"kind":"session_removed","sid":"s","cause":"从未见过的词"}"#),
        Some(InboundFrame::SessionRemoved {
            sid: "s".to_string(),
            cause: RemovalCause::Gone,
        })
    );
}

/// issue #32：overflow 帧解析出 dropped 计数；缺/错 dropped 当坏帧跳过（None）。
#[test]
fn parses_overflow_and_rejects_bad_dropped() {
    let frame = parse_frame(r#"{"kind":"overflow","dropped":12}"#).expect("overflow parses");
    assert_eq!(
        frame,
        InboundFrame::Overflow {
            dropped: 12,
            lost: Vec::new(),
            lost_truncated: false
        }
    );
    // 缺 dropped → None
    assert_eq!(parse_frame(r#"{"kind":"overflow"}"#), None);
    // dropped 类型错（字符串）→ None
    assert_eq!(parse_frame(r#"{"kind":"overflow","dropped":"12"}"#), None);
}

/// B2：tmux_sessions 帧解析出 raw（tmux ls 原文，含转义 TAB）；缺/错 raw 当坏帧跳过（None）。
#[test]
fn parses_tmux_sessions_and_rejects_bad_raw() {
    let frame = parse_frame(
        "{\"kind\":\"tmux_sessions\",\"raw\":\"s1\\t/p\\tclaude\\t1\\t2\\tsid-a\"}",
    )
    .expect("tmux_sessions parses");
    assert_eq!(
        frame,
        InboundFrame::TmuxSessions {
            raw: "s1\t/p\tclaude\t1\t2\tsid-a".to_string(),
            // P1：旧 daemon 无该字段 ⇒ None（**不是**坏帧）。
            observation: None,
        }
    );
    // NO_TMUX 哨兵也是合法 raw。
    assert!(matches!(
        parse_frame(r#"{"kind":"tmux_sessions","raw":"NO_TMUX"}"#),
        Some(InboundFrame::TmuxSessions { .. })
    ));
    // 缺 raw / raw 非字符串 → None（坏帧跳过）。
    assert_eq!(parse_frame(r#"{"kind":"tmux_sessions"}"#), None);
    assert_eq!(parse_frame(r#"{"kind":"tmux_sessions","raw":5}"#), None);
    // P1（additive 字段）：observation 存在则读出；**非字符串不是坏帧**、退化成 None
    // （坏 daemon 也只该让 monitor 退回保守判据，不该让整帧被丢）。
    assert_eq!(
        parse_frame(r#"{"kind":"tmux_sessions","raw":"","observation":"zero_sessions"}"#),
        Some(InboundFrame::TmuxSessions {
            raw: String::new(),
            observation: Some("zero_sessions".to_string()),
        })
    );
    assert_eq!(
        parse_frame(r#"{"kind":"tmux_sessions","raw":"","observation":7}"#),
        Some(InboundFrame::TmuxSessions {
            raw: String::new(),
            observation: None,
        }),
        "observation 类型错只该退化成 None，不该把整帧当坏帧丢掉"
    );
}

/// 已知 kind 但必需字段缺失 / 类型错 → None（坏帧当 garbage 跳过，不 panic）。
#[test]
fn known_kind_missing_or_wrong_field_returns_none() {
    // line 缺 seq
    assert_eq!(
        parse_frame(r#"{"kind":"line","session_id":"s","path":"/p","raw":"x"}"#),
        None
    );
    // seq 类型错（字符串而非数字）
    assert_eq!(
        parse_frame(r#"{"kind":"line","session_id":"s","path":"/p","seq":"0","raw":"x"}"#),
        None
    );
    // session_added 缺 sid
    assert_eq!(parse_frame(r#"{"kind":"session_added"}"#), None);
}

/// 模拟一段帧序列逐行喂入：hello → 两条 line → 未知 → garbage → session_removed。
/// 断言 dispatch 正确性 + 未知/garbage 为 None（不 panic）。
#[test]
fn dispatch_over_a_frame_sequence() {
    let lines = [
        r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/c"}"#,
        r#"{"kind":"line","session_id":"s","path":"/p","seq":0,"raw":"a"}"#,
        r#"{"kind":"line","session_id":"s","path":"/p","seq":1,"raw":"b"}"#,
        r#"{"kind":"future_thing","x":1}"#,
        "not json",
        r#"{"kind":"session_removed","sid":"s"}"#,
    ];
    let parsed: Vec<Option<InboundFrame>> = lines.iter().map(|l| parse_frame(l)).collect();

    assert!(matches!(parsed[0], Some(InboundFrame::Hello { v: 1, .. })));
    assert!(matches!(parsed[1], Some(InboundFrame::Line { seq: 0, .. })));
    assert!(matches!(parsed[2], Some(InboundFrame::Line { seq: 1, .. })));
    assert_eq!(parsed[3], None, "unknown kind → None");
    assert_eq!(parsed[4], None, "garbage → None");
    assert!(matches!(
        parsed[5],
        Some(InboundFrame::SessionRemoved { .. })
    ));
}
