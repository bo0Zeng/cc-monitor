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
            // `S4`：本样本无 `homes` 字段（= 今天所有已部署的后端）→ 空表 ⇒ 回退 `claude_dir`。
            homes: Vec::new(),
            // F66：旧后端（本样本无 capabilities 字段）→ 空集（保守缺省）
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
    // ① 无 `homes`（= 今天所有已部署的后端）⇒ 空表 ⇒ 回退 `claude_dir`。
    let old =
        r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/old/.claude"}"#;
    match parse_frame(old).expect("hello must parse") {
        InboundFrame::Hello {
            homes, claude_dir, ..
        } => {
            assert!(homes.is_empty(), "旧后端不该凭空长出 homes");
            assert_eq!(
                claude_home_from_hello(&homes, &claude_dir),
                "/old/.claude",
                "无 homes 时必须回退 claude_dir —— 这条一坏，所有已部署的后端当场失去 home"
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
/// e2e 挡不住的原因：它 `grep` 的是整行 hello，backend 把 `ping` 放哪个键里都绿。
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
    // 旧后端：无该字段 → 空集（保守缺省，不发任何入方向命令）。
    let without = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d"}"#;
    match parse_frame(without).expect("hello must parse") {
        InboundFrame::Hello { commands, .. } => assert!(commands.is_empty()),
        other => panic!("不是 hello：{other:?}"),
    }
    // 坏后端：非数组 / 元素非字符串 → 滤成空集，**绝不 panic**（同 capabilities 口径）。
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
    // backend 对**协议级**错误回空 id（它那时还不知道 id）——空串是合法值，不是坏帧。
    let proto_err = r#"{"kind":"reply","id":"","ok":false,"code":"line_too_long","message":"x"}"#;
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

/// #33：hello 缺 build_id → None（按必需字段，坏帧跳过；既有后端总在发它）。
#[test]
fn hello_missing_build_id_returns_none() {
    let line = r#"{"kind":"hello","v":1,"host_arch":"x86_64","claude_dir":"/c"}"#;
    assert_eq!(parse_frame(line), None);
}

/// F66（#58③）wire 契约：hello 的 `capabilities` 字段。
/// ① 缺字段（旧后端）→ 空集（向后兼容，保守缺省，同 §27 族）。
/// ② 声明数组 → 原样解析（monitor 按此决定发哪些 flag）。
/// ③ 非数组 / 元素非字符串 → 滤成空集，绝不 panic（宽容解析，§18）。
#[test]
fn hello_capabilities_backward_compat_and_declared() {
    // ① 旧后端：无 capabilities → 空集
    let old = r#"{"kind":"hello","v":1,"build_id":"p1e","host_arch":"x86_64","claude_dir":"/c"}"#;
    match parse_frame(old).unwrap() {
        InboundFrame::Hello { capabilities, .. } => {
            assert!(capabilities.is_empty(), "旧后端无声明 → 空集");
        }
        _ => panic!("expected Hello"),
    }
    // ② 新后端：声明能力
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
        negotiate_version(EXPECTED_PROTO_V, EXPECTED_BACKEND_BUILD_ID),
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
        negotiate_version(999, EXPECTED_BACKEND_BUILD_ID),
        VersionVerdict::Incompatible { reported_v: 999 },
        "协议不符时即使 build 匹配也算不兼容"
    );
}

/// #33：version_warning 文案——Ok→None，其余→Some 且含 label。
#[test]
fn version_warning_messages() {
    assert_eq!(
        version_warning(EXPECTED_PROTO_V, EXPECTED_BACKEND_BUILD_ID, "pi"),
        None
    );
    let stale = version_warning(EXPECTED_PROTO_V, "p1a-history", "pi").expect("stale warns");
    assert!(stale.contains("pi") && stale.contains("p1a-history"));
    let incompat = version_warning(2, EXPECTED_BACKEND_BUILD_ID, "wsl").expect("incompat warns");
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
            rbind_token: None,
            container: None,
            pid: None,
        }
    );
}

/// Batch7-F24：p1e backend 的 session_added 附加元信息正确解析；
/// 旧后端缺字段 → None（上一测试已覆盖）。
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
            rbind_token: None,
            container: None,
            pid: None,
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
            // ★ S0 向后兼容：**旧后端不发 cause** ⇒ 必须解析成 Gone，
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
    let frame =
        parse_frame("{\"kind\":\"tmux_sessions\",\"raw\":\"s1\\t/p\\tclaude\\t1\\t2\\tsid-a\"}")
            .expect("tmux_sessions parses");
    assert_eq!(
        frame,
        InboundFrame::TmuxSessions {
            raw: "s1\t/p\tclaude\t1\t2\tsid-a".to_string(),
            // P1：旧后端无该字段 ⇒ None（**不是**坏帧）。
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
    // （坏后端也只该让 monitor 退回保守判据，不该让整帧被丢）。
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

// ══════ `设计/80 §8.7` 步 4 的读侧：wire 上那个 `rbind_token` 读回来了 ══════
//
// 这一组守的是**方案 E 的远端那一半**：`sid ──wire──→ token`。
// 另一半（`token ──→ HWND`）在 `bind_tests.rs` 那一组。
// ⚠ **两半今天还没有被任何分派串起来** —— `↗` 改走 join 是 `§8.7` 的步 4，
//   而它逐字警告「不要先做 4」。本组买的是「键到手了」，不是「↗ 已经不依赖 tmux」。

/// 形状合法的令牌：恰好 32 个小写十六进制字符。
const PF_TOK: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";

fn added_token(line: &str) -> Option<String> {
    match parse_frame(line).expect("这一帧本身应当解析得出来") {
        InboundFrame::SessionAdded { rbind_token, .. } => rbind_token,
        other => panic!("解出来不是 session_added：{other:?}"),
    }
}

/// ★ 正题：帧上带令牌 ⇒ 读回来；**不带** ⇒ `None`（旧后端 / 没索要 / 真的没有）。
///
/// ⚠ 对照组（`None` 那一半）**必须在**：只验「带的时候读得到」的话，
///   「把这个字段读成一个常量」这种变异会照样绿。
#[test]
fn the_launch_token_rides_back_on_the_session_added_frame() {
    let with = format!(r#"{{"kind":"session_added","sid":"s-1","rbind_token":"{PF_TOK}"}}"#);
    assert_eq!(
        added_token(&with),
        Some(PF_TOK.to_string()),
        "帧上有令牌却读不回来 —— `sid → token` 这一半断了"
    );
    // 旧后端 / 客户端没索要 / 这条会话真的没令牌 —— wire 上三者同形，都是缺席。
    assert_eq!(added_token(r#"{"kind":"session_added","sid":"s-1"}"#), None);
}

/// ★ fail closed：形状不对**一律当没有**，不是「原样报出去」。
///
/// 为什么这一条非要有：`§8.5 ②` 买的是「这个会话有没有令牌」这**一个布尔**，
/// 用来取代今天那四档猜。而那个布尔只有在「有 ⇒ 形状确定对」时才说得准 ——
/// 放一个形状可疑的串进去，`↗` 会拿它去 join、命中不了，
/// 于是「拉错了/拉不到」这两件事又被压回同一个读数。
///
/// ⚠ 这一条与**后端读侧**（`identity_tag::rbind_token_of`）是**同向的两道闸**：
///   后端不该报出形状不对的值，而 monitor 也不许因此就信任上游。
#[test]
fn a_malformed_launch_token_on_the_wire_is_read_as_no_token() {
    let bad = [
        ("少一位", r#""0f1e2d3c4b5a69788796a5b4c3d2e1f""#),
        ("多一位", r#""0f1e2d3c4b5a69788796a5b4c3d2e1f00""#),
        ("有大写", r#""0F1E2D3C4B5A69788796A5B4C3D2E1F0""#),
        ("两侧空白", r#"" 0f1e2d3c4b5a69788796a5b4c3d2e1f0 ""#),
        ("空串", r#""""#),
        ("不是字符串", "12345"),
        ("null", "null"),
    ];
    for (why, raw) in bad {
        let line = format!(r#"{{"kind":"session_added","sid":"s-1","rbind_token":{raw}}}"#);
        assert_eq!(
            added_token(&line),
            None,
            "「{why}」这一形被读成了合法令牌：{line}"
        );
    }
    // ★ 反向自检：上面那些之所以 None，不是因为整帧解析挂了 —— 那一帧照样解得出来。
    let line = format!(r#"{{"kind":"session_added","sid":"s-1","rbind_token":" {PF_TOK} "}}"#);
    match parse_frame(&line).expect("整帧必须仍然解析得出来") {
        InboundFrame::SessionAdded { sid, .. } => assert_eq!(sid, "s-1"),
        other => panic!("{other:?}"),
    }
}

/// ★ **形状那一条不许在这里再写一遍** —— 它与本地那张 `token → HWND` 表用的
/// 必须是同一条（`bind::rbind_token_shape_ok`）。
///
/// 两处各写一遍的后果：join 在某些取值上**静默失配**，而失配与「没有令牌」
/// 在界面上同形。本条按**源文本**钉接线（行为那半由上面两条买）。
#[test]
fn the_wire_side_shape_check_is_the_same_one_the_local_table_uses() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    assert!(
        prod.len() > 20000,
        "抽出来的生产段太小，本条在空转：{}",
        prod.len()
    );
    assert!(
        prod.contains("crate::bind::rbind_token_shape_ok"),
        "`parse_frame` 没在用 `bind::rbind_token_shape_ok` —— 形状多了一份副本"
    );
    // 反向：本文件（判据）里那条形状是**手写字面量**，生产段里不许再出现第二份。
    assert!(
        !prod.contains("[0-9a-f]{32}\") "),
        "生产段里出现了第二份形状实现"
    );
    // 恒等的另一头：那个函数真的按 32 位小写十六进制判（不是恒真）。
    assert!(crate::bind::rbind_token_shape_ok(PF_TOK));
    assert!(!crate::bind::rbind_token_shape_ok(
        "0F1E2D3C4B5A69788796A5B4C3D2E1F0"
    ));
    assert!(!crate::bind::rbind_token_shape_ok("0f1e2d3c"));
}

/// 🔴 ★ 令牌的**值**不许进日志（`设计/80 §8.6 ③`）。
///
/// 消费点今天只打一句 `has_rbind_token={bool}` —— 那个布尔正是 `§8.5 ②` 要的东西，
/// 而它不泄露值。本条按源文本钉住：凡是把 `rbind_token` 交给 `tracing!` 的地方，
/// 交出去的必须是 `.is_some()`，不是那个串。
#[test]
fn the_token_value_never_reaches_a_log_macro() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let mut calls: Vec<String> = Vec::new();
    let mut cur: Option<String> = None;
    for line in prod.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        if cur.is_none() && t.contains("tracing::") {
            cur = Some(String::new());
        }
        if let Some(buf) = cur.as_mut() {
            buf.push_str(line);
            buf.push('\n');
            if t.ends_with(");") {
                calls.push(cur.take().unwrap());
            }
        }
    }
    assert!(
        calls.len() >= 30,
        "只切出 {} 处 tracing —— 切法坏了",
        calls.len()
    );
    let touching: Vec<&String> = calls.iter().filter(|c| c.contains("rbind_token")).collect();
    assert_eq!(
        touching.len(),
        2,
        "碰到 `rbind_token` 的日志调用不是 2 处（起流那句的 flag 布尔 ＋ 宣告那句的          `has_rbind_token`）—— 要么多了一处、要么其中一处搬家了：{touching:#?}"
    );
    // 判法：把**两种允许的形态**抹掉，剩下的任何 `rbind_token` 都是裸着交出去的值。
    //   · `with_rbind_token` —— 起流那句打的是**这一轮发没发那条 flag**（一个布尔）；
    //   · `rbind_token.is_some()` —— 宣告那句打的是**这条会话有没有令牌**（`§8.5 ②` 那个布尔）。
    // ⚠ 刻意不写成「必须含 `.is_some()`」：那种写法对
    //   `tracing!("… {} {}", rbind_token.is_some(), rbind_token.unwrap())` 是**瞎的**。
    for c in &touching {
        let scrubbed = c
            .replace("with_rbind_token", "•")
            .replace("rbind_token.is_some()", "•")
            .replace("has_rbind_token", "•");
        assert!(
            !scrubbed.contains("rbind_token"),
            "有日志把令牌**本身**交出去了（只许打布尔）：{c}"
        );
    }
}

/// 〔SR1a · `设计/05 §13.6 ③`〕`accounts_changed`：认得出（无载荷，多余字段忽略）；
/// 远端流收到它 ⇒ 发前端既有的 `remote-backend-ready`、带 `reason: "accounts_changed"`（恰好一处）。
#[test]
fn accounts_changed_is_recognised_and_reaches_the_frontend_as_ready() {
    assert_eq!(
        parse_frame(r#"{"kind":"accounts_changed"}"#),
        Some(InboundFrame::AccountsChanged)
    );
    assert_eq!(
        parse_frame(r#"{"kind":"accounts_changed","future":1}"#),
        Some(InboundFrame::AccountsChanged),
        "多余字段该被忽略（additive）"
    );
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let arm = guard_core::find_pinned(&prod, "Some(InboundFrame::AccountsChanged) =>")
        .expect("流循环里不是恰好一条 accounts_changed 的臂");
    // 臂体取到下一条 `Some(InboundFrame::` 臂为止（按字符切，不按字节数切 —— 中文注释会切在字中间）。
    let rest = &prod[arm + 1..];
    let body = &prod[arm..arm + 1 + rest.find("Some(InboundFrame::").unwrap_or(rest.len())];
    assert!(
        body.contains("REMOTE_BACKEND_READY") && body.contains("\"reason\": \"accounts_changed\""),
        "accounts_changed 那一臂没发 remote-backend-ready（或没带 reason）"
    );
}

/// 〔U4b · 第四波 · M1〕`session_added.container`：两个字面量认得；缺席 / 不认识的取值 ⇒ `None`（不知道 ≠ 不在）。
/// 帧串与后端 `wire_tests::session_added_container_is_additive_with_two_literals` 的精确字节逐字相同（异源：那边是后端序列化器的产物）。
#[test]
fn session_added_container_reads_two_literals_and_unknown_is_none() {
    use crate::session_facts::Container;
    let get = |line: &str| match parse_frame(line).expect("session_added 要解得出") {
        InboundFrame::SessionAdded { container, .. } => container,
        other => panic!("解出来不是 session_added：{other:?}"),
    };
    assert_eq!(
        get(r#"{"kind":"session_added","sid":"s","container":"tmux"}"#),
        Some(Container::Tmux)
    );
    assert_eq!(
        get(r#"{"kind":"session_added","sid":"s","container":"none"}"#),
        Some(Container::None)
    );
    assert_eq!(get(r#"{"kind":"session_added","sid":"s"}"#), None);
    assert_eq!(
        get(r#"{"kind":"session_added","sid":"s","container":"screen"}"#),
        None
    );
    assert_eq!(
        get(r#"{"kind":"session_added","sid":"s","container":1}"#),
        None
    );
}

/// 〔U4b · 第四波 · M1〕`sessions_replayed`（无载荷）认得。帧串 == 后端 `wire_tests::sessions_replayed_has_exactly_these_bytes`。
#[test]
fn sessions_replayed_is_known() {
    assert_eq!(
        parse_frame(r#"{"kind":"sessions_replayed"}"#),
        Some(InboundFrame::SessionsReplayed)
    );
}

/// 〔LOC1b · 第四波 4D〕`session_added.pid` 的读侧：装得进 u32 的非负整数才认，别的一律当没带（缺席 = 老后端 / 没索要）。
#[test]
fn loc1b_the_pid_on_session_added_is_read_only_when_it_is_a_real_pid() {
    let pid_of = |line: &str| match parse_frame(line) {
        Some(InboundFrame::SessionAdded { pid, .. }) => pid,
        other => panic!("解不出 session_added：{other:?}"),
    };
    assert_eq!(
        pid_of(r#"{"kind":"session_added","sid":"s","pid":4242}"#),
        Some(4242)
    );
    assert_eq!(pid_of(r#"{"kind":"session_added","sid":"s"}"#), None);
    assert_eq!(
        pid_of(r#"{"kind":"session_added","sid":"s","pid":-1}"#),
        None
    );
    assert_eq!(
        pid_of(r#"{"kind":"session_added","sid":"s","pid":"42"}"#),
        None
    );
    assert_eq!(
        pid_of(r#"{"kind":"session_added","sid":"s","pid":4294967296}"#),
        None
    );
}

/// 〔FW1 · 第四波 4D · D-d〕两个新帧认得（帧串 == 后端 `watcher_tests::the_two_session_file_frames_have_exactly_these_bytes`）；
/// `why` 认不出 / 缺字段 ⇒ 整帧跳过（不猜成哪一种）。
#[test]
fn the_session_file_frames_are_known_and_an_unknown_why_is_dropped() {
    use crate::ssh_source::FileChange;
    assert_eq!(
        parse_frame(r#"{"kind":"session_file_gone","session_id":"s","path":"/p/s.jsonl"}"#),
        Some(InboundFrame::SessionFileNotice {
            sid: "s".into(),
            path: "/p/s.jsonl".into(),
            change: FileChange::Gone
        })
    );
    for (why, want) in [
        ("truncated", FileChange::Truncated),
        ("rewritten", FileChange::Rewritten),
    ] {
        assert_eq!(
            parse_frame(&format!(
                r#"{{"kind":"session_file_reread","session_id":"s","path":"/p/s.jsonl","why":"{why}"}}"#
            )),
            Some(InboundFrame::SessionFileNotice {
                sid: "s".into(),
                path: "/p/s.jsonl".into(),
                change: want
            })
        );
    }
    for bad in [
        r#"{"kind":"session_file_reread","session_id":"s","path":"/p/s.jsonl","why":"moved"}"#,
        r#"{"kind":"session_file_reread","session_id":"s","path":"/p/s.jsonl"}"#,
        r#"{"kind":"session_file_gone","session_id":"s"}"#,
    ] {
        assert_eq!(parse_frame(bad), None, "{bad}");
    }
}
