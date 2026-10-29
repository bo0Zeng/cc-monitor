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
    sent(RotationFactsReq { agent: None }, "rotation-facts", json!({}));
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
