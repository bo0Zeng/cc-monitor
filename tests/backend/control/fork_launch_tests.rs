//! 分叉之后起的推断：逐格「知道 ＋ 来源 / 不知道 ＋ 原因」，推不出来的时候绝不猜。
//! 例子与界面原先那两组（推断 · 从两份名单推源会话事实）同一组。

use super::*;

fn live(account: Option<AccountFact>, terminal: Option<&str>) -> Source {
    Source {
        live: true,
        cwd: Some("/home/u/p".into()),
        account,
        terminal: terminal.map(str::to_string),
    }
}

fn known<T>(value: T, from: &'static str) -> Slot<T> {
    Slot::Known { value, from }
}

fn unknown<T>(why: &'static str) -> Slot<T> {
    Slot::Unknown { why }
}

#[test]
fn an_exited_source_knows_no_account_even_when_one_is_handed_in() {
    // 调用方顺手塞进来的号（已退出时不可信）一概不看。
    let f = infer(&Source {
        live: false,
        cwd: Some("/home/u/p".into()),
        account: Some(AccountFact::Named("z".into())),
        terminal: Some("tmux-p".into()),
    });
    assert_eq!(f.account, unknown(WHY_EXITED));
    assert_eq!(f.terminal, unknown(WHY_EXITED));
    // 工作目录来自记录本身，已退出的会话照样答得出。
    assert_eq!(f.cwd, known("/home/u/p".to_string(), FROM_RECORD));
}

#[test]
fn a_live_source_knows_account_and_terminal_and_says_where_from() {
    let f = infer(&live(Some(AccountFact::Named("z".into())), Some("tmux-p")));
    assert_eq!(
        f.account,
        known(AccountFact::Named("z".into()), FROM_PROCESS)
    );
    assert_eq!(
        f.terminal,
        known(
            TerminalFact::Hosted {
                terminal: "tmux-p".into()
            },
            FROM_TERMINAL_LIST
        )
    );
}

#[test]
fn account_zero_is_a_known_answer_not_an_unknown_one() {
    let f = infer(&live(Some(AccountFact::Base), None));
    assert_eq!(f.account, known(AccountFact::Base, FROM_PROCESS));
}

#[test]
fn live_but_account_unreadable_is_unknown_and_distinct_from_account_zero() {
    let f = infer(&live(None, None));
    assert_eq!(f.account, unknown(WHY_LIVE_NO_ACCOUNT));
}

#[test]
fn live_and_in_no_terminal_is_known_none_not_unknown() {
    for t in [None, Some("  ")] {
        let f = infer(&live(Some(AccountFact::Base), t));
        assert_eq!(
            f.terminal,
            known(TerminalFact::None, FROM_TERMINAL_LIST),
            "{t:?}"
        );
    }
}

#[test]
fn no_cwd_or_a_blank_one_is_unknown() {
    for cwd in [None, Some("   ".to_string())] {
        let f = infer(&Source {
            live: true,
            cwd,
            ..Source::default()
        });
        assert_eq!(f.cwd, unknown(WHY_NO_CWD));
    }
}

/// 要问哪几格（界面照这一形问）：已退出 ⇒ 号与终端；活着且齐全 ⇒ 一格都不问。
#[test]
fn an_exited_source_leaves_account_and_terminal_to_ask_and_a_full_live_one_nothing() {
    let unknowns = |l: &Launch| -> Vec<&str> {
        let mut v = Vec::new();
        if matches!(l.account, Slot::Unknown { .. }) {
            v.push("account");
        }
        if matches!(l.terminal, Slot::Unknown { .. }) {
            v.push("terminal");
        }
        if matches!(l.cwd, Slot::Unknown { .. }) {
            v.push("cwd");
        }
        v
    };
    let dead = Source {
        live: false,
        cwd: Some("/p".into()),
        ..Source::default()
    };
    assert_eq!(unknowns(&infer(&dead)), vec!["account", "terminal"]);
    assert!(unknowns(&infer(&live(Some(AccountFact::Base), Some("tmux-p")))).is_empty());
}

#[test]
fn the_wire_shape_is_codes_and_facts_only() {
    let v = infer(&live(
        Some(AccountFact::Named("z".into())),
        Some("tmux-3-7"),
    ))
    .to_json();
    assert_eq!(
        v,
        json!({
            "cwd": {"kind": "known", "value": "/home/u/p", "from": "record"},
            "account": {"kind": "known", "value": "z", "from": "process"},
            "terminal": {"kind": "known", "value": {"host": "tmux", "terminal": "tmux-3-7"}, "from": "terminal_list"},
        })
    );
    let v = infer(&live(Some(AccountFact::Base), None)).to_json();
    assert_eq!(v["account"]["value"], Value::Null);
    assert_eq!(v["terminal"]["value"], json!({"host": "none"}));
    let v = infer(&Source::default()).to_json();
    assert_eq!(
        v,
        json!({
            "cwd": {"kind": "unknown", "why": "no_cwd"},
            "account": {"kind": "unknown", "why": "exited"},
            "terminal": {"kind": "unknown", "why": "exited"},
        })
    );
}

// ───────────────────────────── 从两份名单推源会话事实 ─────────────────────────────

const SID: &str = "s1";

fn proc(sid: &str, config_dir: Option<&str>, account: Option<&str>, alive: bool) -> ProcessRow {
    ProcessRow {
        sid: sid.into(),
        alive,
        config_dir: config_dir.map(str::to_string),
        account: account.map(str::to_string),
        bare: alive && config_dir.is_none(),
    }
}

fn term(name: &str, sid: &str, agent: bool) -> TmuxEntry {
    TmuxEntry {
        name: name.into(),
        terminal: format!("tmux-{name}"),
        sid: Some(sid.into()),
        agent,
    }
}

fn src(p: &[ProcessRow], t: Option<&[TmuxEntry]>) -> Source {
    source_of(p, t, SID, Some("/p".into()))
}

#[test]
fn both_signals_present_means_live_with_account_and_terminal() {
    let s = src(
        &[proc(SID, Some("/acct/z"), Some("z"), true)],
        Some(&[term("p-cc", SID, true)]),
    );
    assert!(s.live);
    assert_eq!(s.account, Some(AccountFact::Named("z".into())));
    assert_eq!(s.terminal.as_deref(), Some("tmux-p-cc"));
}

/// 进程名单里没有它、终端名单证明它活着 ⇒ 号是「说不出」，**不是**账号 0（那会静默起在账号 0 上）。
#[test]
fn found_in_a_terminal_but_not_in_the_process_list_means_account_unknown_not_zero() {
    let s = src(&[], Some(&[term("p-cc", SID, true)]));
    assert!(s.live, "终端名单命中即证明它活着");
    assert_eq!(s.account, None);
    assert_eq!(infer(&s).account, unknown(WHY_LIVE_NO_ACCOUNT));
}

#[test]
fn a_bare_process_is_account_zero_for_real() {
    let s = src(
        &[proc(SID, None, None, true)],
        Some(&[term("p-cc", SID, true)]),
    );
    assert_eq!(s.account, Some(AccountFact::Base));
}

/// 配置目录设了、账号库不认得 ⇒ 说不出名字 ⇒ 不知道（不按目录交）。环境这一刻读不出 ⇒ 也不知道（不是账号 0）。
#[test]
fn an_unnamed_dir_or_an_unreadable_environment_is_account_unknown() {
    let s = src(&[proc(SID, Some("/acct/x"), None, true)], None);
    assert!(s.live);
    assert_eq!(s.account, None);
    let unreadable = ProcessRow {
        bare: false,
        ..proc(SID, None, None, true)
    };
    assert_eq!(src(&[unreadable], None).account, None);
}

#[test]
fn a_dead_process_row_does_not_count() {
    let s = src(&[proc(SID, Some("/acct/z"), Some("z"), false)], Some(&[]));
    assert!(!s.live);
    assert_eq!(s.account, None);
}

/// 终端里只剩 shell（前台不是 agent）⇒ 不算活着。
#[test]
fn an_idle_terminal_does_not_count_as_live() {
    let s = src(&[], Some(&[term("p-cc", SID, false)]));
    assert!(!s.live);
    assert_eq!(s.terminal, None);
}

#[test]
fn another_session_in_the_same_dir_is_not_taken_for_this_one() {
    let s = src(
        &[proc("other", Some("/acct/z"), Some("z"), true)],
        Some(&[term("p-cc", "other", true)]),
    );
    assert!(!s.live);
    assert_eq!(s.terminal, None);
}

#[test]
fn neither_list_readable_means_unknown_everywhere_but_cwd() {
    let s = src(&[], None);
    assert!(!s.live);
    assert_eq!(s.account, None);
    assert_eq!(s.terminal, None);
    assert_eq!(
        s.cwd.as_deref(),
        Some("/p"),
        "工作目录来自记录，与名单读不读得出无关"
    );
}

/// 只有进程那一半（没有终端名单）：号知道，终端那一格不凭空多出一个。
#[test]
fn process_list_only_knows_the_account_and_no_terminal() {
    let s = src(&[proc(SID, Some("/acct/z"), Some("z"), true)], None);
    assert!(s.live);
    assert_eq!(s.account, Some(AccountFact::Named("z".into())));
    assert_eq!(s.terminal, None);
}

#[test]
fn several_running_carriers_take_the_first() {
    let s = src(
        &[],
        Some(&[term("a-cc", SID, true), term("b-cc", SID, true)]),
    );
    assert!(s.live);
    assert_eq!(s.terminal.as_deref(), Some("tmux-a-cc"));
}

#[test]
fn a_process_list_line_reads_into_a_row() {
    let r = process_row_of(&json!({
        "pid": 1, "sessionId": "s1", "cwd": "/p", "configDir": "/acct/z", "account": "z",
        "bare": false, "alive": true, "viaRelay": null,
    }))
    .unwrap();
    assert_eq!(r, proc("s1", Some("/acct/z"), Some("z"), true));
    let bare = process_row_of(&json!({"sessionId": "s1", "configDir": null, "account": "base", "bare": true, "alive": true})).unwrap();
    assert_eq!(account_of(&bare), Some(AccountFact::Base));
    assert!(process_row_of(&json!({"sessionId": null, "alive": true})).is_none());
}

/// 跨语言金样（界面那一侧读同一份解码）：`launch` 那一格 == 推断对同一组事实的真产出。
#[test]
fn the_golden_launch_is_what_the_inference_emits() {
    let golden: Value =
        serde_json::from_str(include_str!("../../__fixtures__/session-fork.golden.json")).unwrap();
    let live = Source {
        live: true,
        cwd: Some("/home/u/proj".into()),
        account: Some(AccountFact::Named("work".into())),
        terminal: Some("tmux-3-7".into()),
    };
    assert_eq!(golden["product"]["launch"], infer(&live).to_json());
    let exited = Source {
        live: false,
        cwd: Some("/home/u/proj".into()),
        ..Source::default()
    };
    assert_eq!(golden["launchExited"], infer(&exited).to_json());
}
