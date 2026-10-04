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
            unavailable: vec![],
            uncancellable: vec![],
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
        version_warning(EXPECTED_PROTO_V, EXPECTED_BACKEND_BUILD_ID, "pi", false),
        None
    );
    let stale = version_warning(EXPECTED_PROTO_V, "p1a-history", "pi", true).expect("stale warns");
    assert!(stale.contains("pi") && stale.contains("p1a-history"));
    let incompat =
        version_warning(2, EXPECTED_BACKEND_BUILD_ID, "wsl", false).expect("incompat warns");
    assert!(incompat.contains("wsl") && incompat.contains("不兼容"));
}

/// B4：版本不同那句按新旧分两句（期望手写）—— 那台旧 ⇒「下次连上时自动换成这一版」；
/// 那台不比这一版旧 ⇒「不会把它换回去」、叫人升级**这个** monitor。两句互不相同，都不再说过期的「后续将支持自动部署」。
/// 要求：「部署只在「我的比盘上的新」时才换（BUILD_ID 可比序）」。
/// 新旧不在 monitor 里比：接上那一刻本机常驻后端答的 `older` 交进来，这里只钉「按答挑哪一句」
/// （「解不出序 ⇒ 不比这一版旧」那一格随判定进了后端：`deploy_plan_tests.rs::the_verdict_replaces_only_upward_and_only_once`）。
#[test]
fn hx2_the_version_warning_says_which_side_is_older() {
    let older = version_warning(EXPECTED_PROTO_V, "p1a-history", "pi", true).expect("旧的该提示");
    assert!(
        older.contains("旧版 p1a-history") && older.contains("下次连上时自动换成这一版"),
        "{older}"
    );
    let newer = version_warning(EXPECTED_PROTO_V, "p99a-future", "pi", false).expect("新的该提示");
    assert!(
        newer.contains("p99a-future") && newer.contains("不会把它换回去") && newer.contains("升级"),
        "{newer}"
    );
    let odd =
        version_warning(EXPECTED_PROTO_V, "hand-built", "pi", false).expect("解不出序的也该提示");
    assert!(
        odd.contains("不会把它换回去"),
        "后端答「不旧」⇒ 按「不比这一版旧」说：{odd}"
    );
    for m in [&older, &newer, &odd] {
        assert!(!m.contains("后续将支持"), "过期的那句回来了：{m}");
    }
    assert_ne!(older, newer);
}

/// 两条 line 帧：逐字段断言 session_id / path / seq / message / cwd 都原样取出。
#[test]
fn parses_two_line_frames_with_all_fields() {
    // 帧上带的是成品（`message` 原样收下、monitor 不读它）与这条记录自己的 `cwd`，不再是原文 `raw`。
    let l0 = r#"{"kind":"line","session_id":"s-1","path":"/home/pi/.claude/projects/p/s-1.jsonl","seq":0,"message":{"type":"user"},"cwd":"/w"}"#;
    let l1 = r#"{"kind":"line","session_id":"s-1","path":"/home/pi/.claude/projects/p/s-1.jsonl","seq":1}"#;

    let f0 = parse_frame(l0).expect("line 0 must parse");
    assert_eq!(
        f0,
        InboundFrame::Line {
            session_id: "s-1".to_string(),
            path: "/home/pi/.claude/projects/p/s-1.jsonl".to_string(),
            seq: 0,
            message: crate::ui_contract::RecordBody::from_json(r#"{"type":"user"}"#.to_string()),
            cwd: Some("/w".to_string()),
            end: None, // 这条金样没带 `byte_offset`
            rid: None,
        }
    );

    let f1 = parse_frame(l1).expect("line 1 must parse");
    match f1 {
        InboundFrame::Line { seq, message, .. } => {
            assert_eq!(seq, 1);
            assert!(message.is_none(), "没带成品 ⇒ 不进界面（照占号）");
        }
        other => panic!("expected Line, got {other:?}"),
    }
}

// tmux 观测两帧（`tmux_session_closed` · `tmux_sessions`）的解析判据随帧删了：老后端发来 ⇒ 落下面那条「未知 kind」。
#[test]
fn a_retired_tmux_frame_from_an_old_backend_is_an_unknown_kind() {
    assert_eq!(
        parse_frame(r#"{"kind":"tmux_session_closed","name":"cc-abc123"}"#),
        None
    );
    assert_eq!(
        parse_frame(r#"{"kind":"tmux_sessions","raw":"NO_TMUX"}"#),
        None
    );
}

/// 测试连接的进度帧：票 ＋ 那一格原样（对象的 JSON 文本，monitor 不解释）；那一格不是对象 / 缺票 ⇒ 坏帧。
#[test]
fn a_probe_frame_carries_its_ticket_and_the_cell_verbatim() {
    assert_eq!(
        parse_frame(r#"{"kind":"probe","ticket":"t-1","cell":{"reached":"ssh"}}"#),
        Some(InboundFrame::Probe {
            ticket: "t-1".into(),
            cell: r#"{"reached":"ssh"}"#.into(),
        })
    );
    assert_eq!(
        parse_frame(r#"{"kind":"probe","ticket":"t-1","cell":"ssh"}"#),
        None
    );
    assert_eq!(
        parse_frame(r#"{"kind":"probe","cell":{"reached":"ssh"}}"#),
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
            project_dir: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
            container: None,
            pid: None,
        }
    );
}

/// Batch7-F24：p1e backend 的 session_added 附加元信息正确解析；
/// 旧后端缺字段 → None（上一测试已覆盖）。
#[test]
fn session_added_metadata_parses() {
    let line = r#"{"kind":"session_added","sid":"s-bg","session_kind":"bg","cwd":"/proj/x","project_dir":"/proj","name":"评估任务","path":"/home/u/.claude/projects/p/s-bg.jsonl","lines":42}"#;
    let frame = parse_frame(line).expect("must parse");
    assert_eq!(
        frame,
        InboundFrame::SessionAdded {
            sid: "s-bg".to_string(),
            session_kind: Some("bg".to_string()),
            attachable: None,
            cwd: Some("/proj/x".to_string()),
            project_dir: Some("/proj".to_string()),
            name: Some("评估任务".to_string()),
            path: Some("/home/u/.claude/projects/p/s-bg.jsonl".to_string()),
            lines: Some(42),
            status: None,
            waiting_for: None,
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

/// session_removed 映射到对应 variant。monitor 不再读 `cause`（去向由后端裁成 `session_state`）：带不带都解成同一形。
#[test]
fn parses_session_removed() {
    for line in [
        r#"{"kind":"session_removed","sid":"s-dead"}"#,
        r#"{"kind":"session_removed","sid":"s-dead","cause":"superseded"}"#,
    ] {
        assert_eq!(
            parse_frame(line),
            Some(InboundFrame::SessionRemoved {
                sid: "s-dead".to_string(),
            })
        );
    }
}

/// `session_state`：两个字面量认得（帧串 == 后端 `wire_tests::mig1_session_state_has_exactly_these_bytes`，异源）；
/// 不认识的取值 / 缺格 ⇒ 整帧坏帧（`None`，不猜成哪一种）。
#[test]
fn session_state_reads_two_literals_and_anything_else_is_a_bad_frame() {
    use crate::session_book::Fate;
    assert_eq!(
        parse_frame(r#"{"kind":"session_state","sid":"abc","state":"reconnectable"}"#),
        Some(InboundFrame::SessionState {
            sid: "abc".into(),
            state: Fate::Reconnectable
        })
    );
    assert_eq!(
        parse_frame(r#"{"kind":"session_state","sid":"abc","state":"ended"}"#),
        Some(InboundFrame::SessionState {
            sid: "abc".into(),
            state: Fate::Ended
        })
    );
    assert_eq!(
        parse_frame(r#"{"kind":"session_state","sid":"abc","state":"idle"}"#),
        None
    );
    assert_eq!(parse_frame(r#"{"kind":"session_state","sid":"abc"}"#), None);
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

/// `accounts_changed`：认得出（无载荷，多余字段忽略）；
/// 远端流收到它 ⇒ 交给前端（恰好一处）。
/// 〔「前端只有两个动作」〕交法从裸 Tauri 事件（`remote-backend-ready`）换成通道订阅：
/// 那一臂调 `replay.accounts_changed`（订了这台 `accounts-changed` 的订阅收一格 `Frame`），且**整份** `stream_source/`
/// 生产段里那个裸事件的常量名零处（零命中带正控：同一个找法认得出这一臂真在调的那个名字）。
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
    let prod = crate::guard_support::stream_source_production();
    let arm = guard_core::find_pinned(&prod, "Some(InboundFrame::AccountsChanged) =>")
        .expect("流循环里不是恰好一条 accounts_changed 的臂");
    // 臂体取到下一条 `Some(InboundFrame::` 臂为止（按字符切，不按字节数切 —— 中文注释会切在字中间）。
    let rest = &prod[arm + 1..];
    let body = &prod[arm..arm + 1 + rest.find("Some(InboundFrame::").unwrap_or(rest.len())];
    let code = guard_core::strip_comment_lines(body);
    assert_eq!(
        code.matches("replay.accounts_changed(").count(),
        1,
        "accounts_changed 那一臂没经通道交给前端（`replay.accounts_changed` 不是恰好一处）"
    );
    let dead = ["REMOTE", "BACKEND", "READY"].join("_");
    let whole = guard_core::strip_comment_lines(&prod);
    assert!(
        !guard_core::contains_word(&whole, &dead),
        "`{dead}` 又出现在 stream_source 生产段里 —— 那个裸事件回来了"
    );
    assert!(
        guard_core::contains_word(&whole, "accounts_changed"),
        "正控失败：同一个找法认不出这一臂真在调的名字 —— 上面的零命中不可信"
    );
}

/// `session_added.container`：两个字面量认得；缺席 / 不认识的取值 ⇒ `None`（不知道 ≠ 不在）。
/// 帧串与后端 `wire_tests::session_added_container_is_additive_with_two_literals` 的精确字节逐字相同（异源：那边是后端序列化器的产物）。
#[test]
fn session_added_container_reads_two_literals_and_unknown_is_none() {
    use crate::session_book::Container;
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

/// `sessions_replayed`（无载荷）认得。帧串 == 后端 `wire_tests::sessions_replayed_has_exactly_these_bytes`。
#[test]
fn sessions_replayed_is_known() {
    assert_eq!(
        parse_frame(r#"{"kind":"sessions_replayed"}"#),
        Some(InboundFrame::SessionsReplayed)
    );
}

/// `tap` 的几形（字面量与后端 `wire_tests::tap_frames_have_exactly_these_bytes` 同一串 —— 异源 = 各自对手写字面量）；
/// `ev` 与 `end` 都缺 · `end` 不认识 · `ev` 不是对象 · 缺 `n` · `run` 不是串 ⇒ 整帧 `None`（坏帧，不猜）。
#[test]
fn tap_frames_parse_into_their_shapes_and_bad_ones_are_none() {
    use crate::session_tap::{Tap, TapBody, TapEnd};
    let body =
        |s: &str| TapBody::Ev(crate::ui_contract::RecordBody::from_json(s.to_string()).unwrap());
    let start = "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"resp\":12,\"n\":0,\"ev\":{\"t\":\"start\",\"rid\":\"r-1\"}}";
    assert_eq!(
        parse_frame(start),
        Some(InboundFrame::Tap(Tap {
            stream: "0b6c1f7e-sid".into(),
            run: None,
            resp: 12,
            n: 0,
            body: body(r#"{"t":"start","rid":"r-1"}"#),
        }))
    );
    let block = "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"run\":\"a1\",\"resp\":12,\"n\":1,\"ev\":{\"t\":\"block\",\"i\":0,\"kind\":\"tool\",\"tool\":\"Bash\"}}";
    match parse_frame(block) {
        Some(InboundFrame::Tap(t)) => {
            assert_eq!(t.run.as_deref(), Some("a1"));
            assert_eq!(t.n, 1);
            assert!(matches!(t.body, TapBody::Ev(_)));
        }
        other => panic!("带运行的那一形没解出来：{other:?}"),
    }
    for (word, want) in [("done", TapEnd::Done), ("broken", TapEnd::Broken)] {
        let line = format!(
            "{{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"resp\":12,\"n\":9,\"end\":\"{word}\"}}"
        );
        assert_eq!(
            parse_frame(&line),
            Some(InboundFrame::Tap(Tap {
                stream: "0b6c1f7e-sid".into(),
                run: None,
                resp: 12,
                n: 9,
                body: TapBody::End(want),
            }))
        );
    }
    for bad in [
        r#"{"kind":"tap","stream":"s","resp":1,"n":0}"#,
        r#"{"kind":"tap","stream":"s","resp":1,"n":0,"end":"maybe"}"#,
        r#"{"kind":"tap","stream":"s","resp":1,"n":0,"ev":"{}"}"#,
        r#"{"kind":"tap","stream":"s","resp":1,"ev":{"t":"stop","ok":true}}"#,
        r#"{"kind":"tap","stream":"s","run":7,"resp":1,"n":0,"ev":{"t":"stop","ok":true}}"#,
    ] {
        assert_eq!(parse_frame(bad), None, "坏的 tap 帧被当成好帧解了：{bad}");
    }
}

/// 运行表那一帧（字面量与后端 `wire_tests::session_runs_frames_have_exactly_these_bytes` 同形）：`runs` · `ended` 原样收下（不解释）；
/// 不是数组 / 缺了 ⇒ 坏帧。
#[test]
fn session_runs_frames_carry_the_runs_verbatim() {
    let line = r#"{"kind":"session_runs","sid":"s1","runs":[{"run":"a2","state":"done","last":{"t":"say"}}],"ended":[{"run":"a0","tool":"t0","state":"failed"}]}"#;
    match parse_frame(line) {
        Some(InboundFrame::SessionRuns { sid, runs, ended }) => {
            assert_eq!(sid, "s1");
            assert_eq!(
                runs.0.get(),
                r#"[{"run":"a2","state":"done","last":{"t":"say"}}]"#
            );
            assert_eq!(
                ended.0.get(),
                r#"[{"run":"a0","tool":"t0","state":"failed"}]"#
            );
        }
        other => panic!("运行表没解出来：{other:?}"),
    }
    for bad in [
        r#"{"kind":"session_runs","sid":"s1","runs":{},"ended":[]}"#,
        r#"{"kind":"session_runs","sid":"s1","runs":[],"ended":{}}"#,
        r#"{"kind":"session_runs","sid":"s1","runs":[]}"#,
    ] {
        assert_eq!(parse_frame(bad), None, "坏的运行表帧被收下了：{bad}");
    }
}

/// 转交是纯照搬：帧 → `session-tap` 的 payload（origin 由调用方给；`end` 用线上那个字）。
#[test]
fn a_tap_frame_becomes_the_session_tap_payload_field_for_field() {
    use crate::session_tap::{to_payload, Tap, TapBody, TapEnd};
    let p = to_payload(
        "<local>",
        Tap {
            stream: "sid".into(),
            run: None,
            resp: 4,
            n: 2,
            body: TapBody::End(TapEnd::Broken),
        },
    );
    assert_eq!(
        serde_json::to_string(&p).unwrap(),
        r#"{"origin":"<local>","stream":"sid","resp":4,"n":2,"end":"broken"}"#
    );
    let p = to_payload(
        "<local>",
        Tap {
            stream: "sid".into(),
            run: Some("a1".into()),
            resp: 4,
            n: 1,
            body: TapBody::Ev(
                crate::ui_contract::RecordBody::from_json(r#"{"t":"stop","ok":true}"#.into())
                    .unwrap(),
            ),
        },
    );
    assert_eq!(
        serde_json::to_string(&p).unwrap(),
        r#"{"origin":"<local>","stream":"sid","run":"a1","resp":4,"n":1,"ev":{"t":"stop","ok":true}}"#
    );
}

/// `session_added.pid` 的读侧：装得进 u32 的非负整数才认，别的一律当没带（缺席 = 老后端 / 没索要）。
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

/// 两个新帧认得（帧串 == 后端 `watcher_tests::the_two_session_file_frames_have_exactly_these_bytes`）；
/// `why` 认不出 / 缺字段 ⇒ 整帧跳过（不猜成哪一种）。
#[test]
fn the_session_file_frames_are_known_and_an_unknown_why_is_dropped() {
    use crate::stream_source::FileChange;
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

/// `tasks_changed`：认得出（带 sid；缺 sid ⇒ 坏帧），流循环那一臂交 `replay.tasks_changed`（恰好一处）。
#[test]
fn tasks_changed_is_recognised_and_reaches_the_frontend_stream() {
    assert_eq!(
        parse_frame(r#"{"kind":"tasks_changed","sid":"s1"}"#),
        Some(InboundFrame::TasksChanged { sid: "s1".into() })
    );
    assert_eq!(parse_frame(r#"{"kind":"tasks_changed"}"#), None);
    let prod = crate::guard_support::stream_source_production();
    assert_eq!(
        prod.matches("replay.tasks_changed(").count(),
        2,
        "远端流循环那一臂 ＋ 本机消费者那一臂，各一处交 `replay.tasks_changed`"
    );
}
