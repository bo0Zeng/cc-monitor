//! `calls!` 表里入参从后端挪来的那几条：名字对；客户端照类型写出的入参就是后端收的那一形（格名 · 改名 · 可缺的不写出），
//! 后端照同一个类型收得回来。
use crate::Call;
use serde_json::json;

fn sent<C>(req: C, name: &str, wire: serde_json::Value)
where
    C: Call + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    assert_eq!(C::NAME, name);
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(v, wire, "`{name}` 写出的入参");
    let back: C = serde_json::from_value(v).unwrap_or_else(|e| panic!("`{name}` 收不回来：{e}"));
    assert_eq!(back, req, "`{name}`");
}

#[test]
fn quota_and_rotation_requests_are_the_ones_the_backend_takes() {
    use crate::quota::QuotaReadReq;
    use crate::rotation::*;
    sent(QuotaReadReq {}, "quota-read", json!({}));
    sent(
        RotationFactsReq { agent: None },
        "rotation-facts",
        json!({}),
    );
    sent(
        RotationFactsReq {
            agent: Some(crate::AgentKind::new("codex")),
        },
        "rotation-facts",
        json!({"agent": "codex"}),
    );
    sent(RotationStrategiesReq {}, "rotation-strategies", json!({}));
    sent(
        RotationViewReq {
            target: RotTarget::Session { sid: "s1".into() },
            hours: None,
        },
        "rotation-view",
        json!({"target": {"sid": "s1"}}),
    );
    sent(
        RotationViewReq {
            target: RotTarget::Strategy {
                strategy: "a".into(),
            },
            hours: Some(6),
        },
        "rotation-view",
        json!({"target": {"strategy": "a"}, "hours": 6}),
    );
    sent(
        RotationSettingsReq {
            strategy: "a".into(),
            edit: RotEdit::Seg {
                key: "cap".into(),
                row: Some(1),
                window: "5h".into(),
                at: None,
                seg: Some(RotSeg {
                    from: "8".into(),
                    to: "20:30".into(),
                    v: 0.5,
                }),
            },
            if_rev: "r1".into(),
        },
        "rotation-settings",
        json!({"strategy": "a", "ifRev": "r1",
            "edit": {"seg": {"key": "cap", "row": 1, "window": "5h", "seg": {"from": "8", "to": "20:30", "v": 0.5}}}}),
    );
    sent(
        RotationSettingsReq {
            strategy: "a".into(),
            edit: RotEdit::Set {
                key: "k".into(),
                row: None,
                v: json!(3),
            },
            if_rev: "r".into(),
        },
        "rotation-settings",
        json!({"strategy": "a", "ifRev": "r", "edit": {"set": {"key": "k", "v": 3}}}),
    );
    sent(
        RotationSettingsReq {
            strategy: "a".into(),
            edit: RotEdit::Remove { row: 2 },
            if_rev: "r".into(),
        },
        "rotation-settings",
        json!({"strategy": "a", "ifRev": "r", "edit": {"remove": {"row": 2}}}),
    );
    // `strategy` 不可缺、可为 null（不轮换）。
    sent(
        RotationUseReq {
            sid: "s".into(),
            strategy: None,
        },
        "rotation-use",
        json!({"sid": "s", "strategy": null}),
    );
    sent(
        RotationReloadReq {
            strategy: "a".into(),
        },
        "rotation-reload",
        json!({"strategy": "a"}),
    );
    sent(
        RotationForNewReq { strategy: None },
        "rotation-for-new",
        json!({"strategy": null}),
    );
    sent(
        RotationStrategyAddReq {
            source: None,
            name: "n".into(),
        },
        "rotation-strategy-add",
        json!({"name": "n"}),
    );
    sent(
        RotationStrategyCopyReq {
            id: "a".into(),
            name: "b".into(),
        },
        "rotation-strategy-copy",
        json!({"id": "a", "name": "b"}),
    );
    sent(
        RotationStrategyRenameReq {
            id: "a".into(),
            name: "b".into(),
            if_rev: "r".into(),
        },
        "rotation-strategy-rename",
        json!({"id": "a", "name": "b", "ifRev": "r"}),
    );
    sent(
        RotationStrategyTzReq {
            id: "a".into(),
            tz: "Asia/Shanghai".into(),
            if_rev: "r".into(),
        },
        "rotation-strategy-tz",
        json!({"id": "a", "tz": "Asia/Shanghai", "ifRev": "r"}),
    );
    sent(
        RotationStrategyDeleteReq { id: "a".into() },
        "rotation-strategy-delete",
        json!({"id": "a"}),
    );
    sent(
        RotationTempReq {
            sid: "s".into(),
            op: RotTempOp::Move {
                account: "x".into(),
                to: 0,
            },
        },
        "rotation-temp",
        json!({"sid": "s", "op": {"move": {"account": "x", "to": 0}}}),
    );
    sent(
        RotationTempReq {
            sid: "s".into(),
            op: RotTempOp::Clear,
        },
        "rotation-temp",
        json!({"sid": "s", "op": "clear"}),
    );
    sent(
        RotationUseNowReq {
            sid: "s".into(),
            account: "x".into(),
        },
        "rotation-use-now",
        json!({"sid": "s", "account": "x"}),
    );
    sent(
        RotationTryReq {
            source: "src".into(),
            use_now: Some("x".into()),
            ..Default::default()
        },
        "rotation-try",
        json!({"source": "src", "temp": [], "useNow": "x"}),
    );
}

#[test]
fn terminal_search_listing_and_accounts_requests_are_the_ones_the_backend_takes() {
    use crate::accounts::AccountsListReq;
    use crate::listing::{HistoryListReq, HistorySort};
    use crate::search::{HistorySearchReq, SearchScope};
    use crate::terminal::{TerminalInputReq, TerminalPreviewReq, TerminalsListReq};
    sent(TerminalsListReq::default(), "terminals-list", json!({}));
    sent(
        TerminalsListReq {
            client: Some("mobile".into()),
        },
        "terminals-list",
        json!({"client": "mobile"}),
    );
    sent(
        TerminalPreviewReq {
            sid: Some("s".into()),
            scrollback: Some(40),
            ..Default::default()
        },
        "terminal-preview",
        json!({"sid": "s", "scrollback": 40}),
    );
    sent(
        TerminalInputReq {
            terminal: Some("t1".into()),
            text: Some("hi".into()),
            enter: Some(false),
            seen_screen: Some("f".into()),
            ..Default::default()
        },
        "terminal-input",
        json!({"terminal": "t1", "text": "hi", "enter": false, "seen_screen": "f"}),
    );
    sent(
        HistorySearchReq {
            query: "q".into(),
            include_tools: None,
            scope: Some(SearchScope::Report),
            after_ms: Some(5),
            limit: None,
            titles: Some(true),
        },
        "history-search",
        json!({"query": "q", "scope": "report", "after_ms": 5, "titles": true}),
    );
    sent(HistoryListReq::default(), "history-list", json!({}));
    sent(
        HistoryListReq {
            sid: Some("s".into()),
            sort: Some(HistorySort::Created),
            within_days: Some(7),
            ..Default::default()
        },
        "history-list",
        json!({"sid": "s", "sort": "created", "within_days": 7}),
    );
    sent(
        AccountsListReq {
            agent: crate::AgentKind::new("claude"),
        },
        "accounts-list",
        json!({"agent": "claude"}),
    );
}

/// `terminal-input` 的 `pastes` 不上线：写出去不带它，线上带了它后端拒。
#[test]
fn terminal_input_pastes_stay_off_the_wire() {
    use crate::terminal::TerminalInputReq;
    let req = TerminalInputReq {
        sid: Some("s".into()),
        pastes: vec!["/tmp/a.png".into()],
        ..Default::default()
    };
    assert_eq!(serde_json::to_value(&req).unwrap(), json!({"sid": "s"}));
    assert!(
        serde_json::from_value::<TerminalInputReq>(json!({"sid": "s", "pastes": ["x"]})).is_err()
    );
}

#[test]
fn needs_history_lines_and_session_requests_are_the_ones_the_backend_takes() {
    use crate::needs::{AnswerReq, SessionsNeedsReq};
    use crate::open::{
        AccountAsk, ForkFrom, NewOpen, OpenFactsReq, OpenWhat, RotationPick, SessionOpenReq,
    };
    use crate::record::HistoryLinesReq;
    use crate::session::{Launch, SessionCommandsReq, SessionModelReq, SessionPermissionReq};
    let sid = || "s1".parse::<crate::Sid>().unwrap();
    sent(
        AnswerReq {
            sid: "s".into(),
            ask: "a".into(),
            choice: "c".into(),
            note: None,
            picks: None,
        },
        "needs-answer",
        json!({"sid": "s", "ask": "a", "choice": "c"}),
    );
    sent(SessionsNeedsReq {}, "sessions-needs", json!({}));
    sent(
        HistoryLinesReq {
            path: "/p".into(),
            from: Some(3),
            until: None,
        },
        "history-lines",
        json!({"path": "/p", "from": 3}),
    );
    sent(
        SessionCommandsReq { sid: sid() },
        "session-commands",
        json!({"sid": "s1"}),
    );
    sent(
        SessionModelReq {
            sid: sid(),
            model: "m".into(),
            client: None,
        },
        "session-model",
        json!({"sid": "s1", "model": "m"}),
    );
    sent(
        SessionPermissionReq {
            sid: sid(),
            mode: "plan".into(),
            client: Some("mobile".into()),
        },
        "session-permission",
        json!({"sid": "s1", "mode": "plan", "client": "mobile"}),
    );
    sent(
        OpenFactsReq {
            fork_of: Some("s".into()),
            at: None,
        },
        "session-open-facts",
        json!({"forkOf": "s"}),
    );
    sent(
        SessionOpenReq {
            what: OpenWhat::Resume { sid: sid() },
            account: Some(AccountAsk::Named { name: "x".into() }),
            run: Some(Launch::Tmux),
            local: false,
            ticket: None,
            client: None,
        },
        "session-open",
        json!({"what": {"kind": "resume", "sid": "s1"}, "account": {"kind": "named", "name": "x"}, "run": "tmux", "local": false}),
    );
    sent(
        SessionOpenReq {
            what: OpenWhat::New(NewOpen {
                agent: crate::AgentKind::new("claude"),
                cwd: "~/p".into(),
                tmux_name: Some("t".into()),
                command: None,
                fork: Some(ForkFrom {
                    sid: "s".into(),
                    uuid: "u".into(),
                }),
                prompt: None,
                model: None,
                permission_mode: Some("plan".into()),
                rotation: Some(RotationPick { strategy: None }),
                alias: None,
            }),
            account: Some(AccountAsk::Follow),
            run: None,
            local: true,
            ticket: Some("k".into()),
            client: None,
        },
        "session-open",
        json!({"what": {"kind": "new", "agent": "claude", "cwd": "~/p", "tmuxName": "t",
                "fork": {"sid": "s", "uuid": "u"}, "permissionMode": "plan", "rotation": {"strategy": null}},
            "account": {"kind": "follow"}, "local": true, "ticket": "k"}),
    );
    sent(
        SessionOpenReq {
            what: OpenWhat::Look {
                sid: None,
                session: Some("t".into()),
            },
            account: None,
            run: None,
            local: false,
            ticket: None,
            client: None,
        },
        "session-open",
        json!({"what": {"kind": "look", "session": "t"}, "local": false}),
    );
    sent(
        SessionOpenReq {
            what: OpenWhat::Shell {
                session: None,
                cwd: Some("/w".into()),
            },
            account: Some(AccountAsk::Base),
            run: None,
            local: false,
            ticket: None,
            client: Some("web".into()),
        },
        "session-open",
        json!({"what": {"kind": "shell", "cwd": "/w"}, "account": {"kind": "base"}, "local": false, "client": "web"}),
    );
}

#[test]
fn machine_and_exit_policy_requests_are_the_ones_the_backend_takes() {
    use crate::exit_policy::ExitPolicySetReq;
    use crate::machine::{MachineReadReq, MachineStopReq, MachineWakeReq};
    sent(MachineReadReq {}, "machine-read", json!({}));
    sent(
        MachineWakeReq {
            machine: "m".into(),
        },
        "machine-wake",
        json!({"machine": "m"}),
    );
    sent(
        MachineStopReq {
            machine: "m".into(),
        },
        "machine-stop",
        json!({"machine": "m"}),
    );
    sent(
        ExitPolicySetReq { kill_on_exit: true },
        "exit-policy-set",
        json!({"killOnExit": true}),
    );
}

#[test]
fn catalog_requests_are_the_ones_the_backend_takes() {
    use crate::assets::{AssetsCatalogMergeReq, AssetsCatalogReq, CatalogIn};
    sent(AssetsCatalogReq {}, "assets-catalog", json!({}));
    sent(
        AssetsCatalogMergeReq {
            catalog: CatalogIn { machines: vec![] },
        },
        "assets-catalog-merge",
        json!({"catalog": {"machines": []}}),
    );
    // 并进来的那一份是另一台的整份应答：只读 `machines`，别的键照收。
    let theirs: AssetsCatalogMergeReq =
        serde_json::from_value(json!({"catalog": {"self": "x", "machines": [], "rows": []}}))
            .unwrap();
    assert!(theirs.catalog.machines.is_empty());
}

/// `machine-stop` 的回包照两端同一个类型解；`machine-read` 的回包是各台成品 ＋ 首屏那一句。
#[test]
fn machine_stop_and_read_replies_are_the_shared_types() {
    let stopped = json!({"stopped": "graceful", "pid": 7});
    assert_eq!(
        round_trip("machine-stop", stopped.clone()),
        Some(Ok(stopped))
    );
    assert!(matches!(
        round_trip("machine-read", json!({"machines": {}})),
        Some(Err(_))
    ));
}

/// 照类型解再写回、读成一份（比 JSON 值，不比键序）。
fn round_trip(cmd: &str, data: serde_json::Value) -> Option<Result<serde_json::Value, String>> {
    super::typed_text(cmd, data).map(|r| r.map(|t| serde_json::from_str(&t).expect("JSON")))
}

/// 表里每一条都解得动（名字没写错），表外的不碰。
#[test]
fn every_listed_command_has_a_type_and_others_have_none() {
    for cmd in super::COMMANDS {
        assert!(round_trip(cmd, serde_json::Value::Null).is_some(), "{cmd}");
    }
    assert!(round_trip("history-facts", json!({})).is_none());
}

/// `needs-answer` 的回包按两端同一个类型（`needs::Answered`）解：闭集一个词，别的字解不开。
#[test]
fn needs_answer_reply_is_the_shared_type() {
    let ok = json!({"result": "answered"});
    assert_eq!(round_trip("needs-answer", ok.clone()), Some(Ok(ok)));
    assert!(matches!(
        round_trip("needs-answer", json!({"result": "maybe"})),
        Some(Err(_))
    ));
}

/// `quota-read` 线上金样（后端真出口写的那几份）每一份都解得开、按类型重写回来一个字节不差（展平的几格 · 可缺的几格都对得上）。
#[test]
fn every_quota_read_golden_is_decoded_byte_for_byte() {
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/__fixtures__/quota-read.golden.json"
    ))
    .unwrap();
    let cases = g["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let reply = c["reply"].clone();
        assert_eq!(
            round_trip("quota-read", reply.clone()),
            Some(Ok(reply)),
            "{}",
            c["name"]
        );
    }
}

/// 缺一格 ⇒ 解不开；多出来的格不拦（回包不 deny_unknown_fields：新版多一格不算对不上，形状指纹另管）。
#[test]
fn a_missing_cell_is_refused_and_the_rewrite_is_the_same_bytes() {
    let tail = json!({"total": 3, "tail_from": 1, "split_at": 10, "end": 30, "how": "resume", "from": 2,
        "rows": [{"end": 30, "hash": 7}], "next": 30, "eof": true,
        "held": {"path": "/p", "next": 3, "next_byte": 30, "mark": "m"}});
    assert_eq!(round_trip("history-tail", tail.clone()), Some(Ok(tail)));
    let short = json!({"total": 3, "tail_from": 1, "split_at": 10, "end": 30});
    assert!(matches!(round_trip("history-tail", short), Some(Err(_))));
    let refused = json!({"result": "refused", "why": "not_yours", "said": "x"});
    assert_eq!(
        round_trip("terminal-input", refused.clone()),
        Some(Ok(refused))
    );
    assert!(matches!(
        round_trip("terminal-input", json!({"result": "maybe"})),
        Some(Err(_))
    ));
}

/// 记录那一格按 `reads` 那一形解：解不开完整记录的那一条成认不出那一条，整份照样解得开。
#[test]
fn a_record_the_client_cannot_read_becomes_the_unread_one() {
    let lines = json!({"from": 0, "next": 1, "eof": true, "end": 9,
        "lines": [{"session_id": "s", "path": "/p", "seq": 0, "end": 9, "cwd": null,
            "record": {"agent": "claude", "id": "r1", "t": "future-kind"}}]});
    let back = round_trip("history-lines", lines).unwrap().unwrap();
    assert_eq!(back["lines"][0]["record"]["t"], "unread");
}
