//! 批量停 · 问样子：每一个的结局与「单个那一条会做的事」相等（替身记下被调了什么；tmux 一个都不起）。

use super::*;
use crate::control::launch_account as la;
use serde_json::json;
use std::cell::RefCell;
use std::collections::BTreeSet;

/// 线上那一份入参 ⇒ 本体 ⇒ 线上那一份回包（解入参同 typed 分派那一步：形状不对 ⇒ `bad_args`）。
fn wired<R: serde::de::DeserializeOwned, T: serde::Serialize>(
    args: &Value,
    f: impl FnOnce(&R) -> Result<T, CmdErr>,
) -> Result<Value, CmdErr> {
    let req: R = serde_json::from_value(args.clone()).map_err(|e| bad(&e.to_string()))?;
    f(&req).map(|t| serde_json::to_value(t).unwrap())
}
fn where_(args: &Value, d: &Deps) -> Result<Value, CmdErr> {
    wired(args, |r| super::where_(r, d))
}
fn stop(args: &Value, d: &Deps) -> Result<Value, CmdErr> {
    wired(args, |r| super::stop(r, d))
}
/// 判号要的事实：这台只有一个号 `work`（能用）、没有谁的上次记录。
fn with_accounts<T>(f: impl FnOnce(&la::Facts) -> T) -> T {
    let library = || {
        la::Library::of_product(&crate::observe::accounts_query::test_support::list(vec![
            crate::observe::accounts_query::test_support::row_with("work", |a| {
                a.config_dir = Some("/h/.cc/work".into())
            }),
        ]))
    };
    f(&la::Facts {
        has_accounts: true,
        library: &library,
        last: &|_| None,
    })
}

const A: &str = "aaaaaaaa-1111-2222-3333-444444444444";
const B: &str = "bbbbbbbb-1111-2222-3333-444444444444";
const C: &str = "cccccccc-1111-2222-3333-444444444444";

fn row(name: &str, sid: Option<&str>, agent: bool) -> TmuxEntry {
    TmuxEntry {
        name: name.into(),
        terminal: format!("tmux-{name}"),
        sid: sid.map(str::to_string),
        agent,
    }
}

/// 替身那一套：名单是给定的，杀只记下来；别的几样停与问样子都用不着。
struct Rig {
    rows: Option<Vec<TmuxEntry>>,
    calls: RefCell<Vec<String>>,
    kill_err: Option<&'static str>,
    /// 列名单那一发失败的码（`None` ⇒ 照 `rows` 答）。
    list_err: Option<&'static str>,
    /// 这台的偏好。
    prefs: crate::control::machine_prefs::Prefs,
}

impl Rig {
    fn new(rows: Option<Vec<TmuxEntry>>) -> Self {
        Self {
            rows,
            calls: RefCell::new(vec![]),
            kill_err: None,
            list_err: None,
            prefs: Default::default(),
        }
    }
    fn run<T>(&self, f: impl FnOnce(&Deps) -> T) -> T {
        let caps = BTreeSet::new();
        let list = || -> Result<Option<Vec<TmuxEntry>>, CmdErr> {
            match self.list_err {
                Some(c) => Err((c, "said".to_string())),
                None => Ok(self.rows.clone()),
            }
        };
        let kill = |name: &str,
                    sid: &str|
         -> Result<crate::control::cc_bus::BusNote, crate::wire::spec::Fail> {
            self.calls.borrow_mut().push(format!("kill {name} {sid}"));
            match self.kill_err {
                Some(c) => Err(crate::wire::spec::Fail::new(c, "said".to_string())),
                None => Ok(crate::control::cc_bus::BusCleanup::default().note()),
            }
        };
        with_accounts(|accounts| {
            f(&Deps {
                list: &list,
                record: &|_, _| unreachable!("停不读记录"),
                kill: &kill,
                send_into: &|_, _, _| unreachable!("停不键入"),
                run_ccm: &|_| unreachable!("停不起"),
                mint: &|_| unreachable!("停不铸名"),
                caps: &caps,
                host: crate::control::launch_render::Host {
                    windows: false,
                    is_dir: |_| true,
                    entry: crate::platform::paths::installed_ccm_entry,
                },
                accounts,
                writers: &|_| unreachable!("停不问谁在写"),
                pretrust: &|_, _| unreachable!("停不预标"),
                open_shell: &|_, _| unreachable!("不开空 shell"),
                prefs: &self.prefs,
            })
        })
    }
}

fn outcomes(v: &Value) -> Vec<(String, String, Value)> {
    v["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["sid"].as_str().unwrap().to_string(),
                r["outcome"].as_str().unwrap().to_string(),
                r["why"].clone(),
            )
        })
        .collect()
}

#[test]
fn stop_kills_exactly_the_one_session_carrying_each_sid() {
    let rig = Rig::new(Some(vec![
        row("a-cc", Some(A), true),
        row("c-cc", Some(C), true),
        row("c-cc-2", Some(C), true),
        row("other", None, true),
    ]));
    let out = rig.run(|d| stop(&json!({ "sids": [A, B, C] }), d)).unwrap();
    assert_eq!(
        outcomes(&out),
        vec![
            (A.into(), "done".into(), Value::Null),
            (B.into(), "skipped".into(), json!("not_in_tmux")),
            (C.into(), "skipped".into(), json!("ambiguous")),
        ]
    );
    // 杀的恰好是带着 A 的那一个，按 sid 核；命中两个的那一个一下都不碰（同单个菜单「命中多个拒杀」）。
    assert_eq!(*rig.calls.borrow(), vec![format!("kill a-cc {A}")]);
    assert_eq!(out["results"][0]["session"], "a-cc");
    assert_eq!(out["results"][2]["detail"], "c-cc, c-cc-2");
}

#[test]
fn stop_says_why_per_session_and_one_failure_does_not_stop_the_rest() {
    let mut rig = Rig::new(Some(vec![
        row("a-cc", Some(A), true),
        row("b-cc", Some(B), false),
    ]));
    rig.kill_err = Some("too_many_windows");
    let out = rig.run(|d| stop(&json!({ "sids": [A, B] }), d)).unwrap();
    assert_eq!(
        outcomes(&out),
        vec![
            (A.into(), "failed".into(), json!("too_many_windows")),
            (B.into(), "failed".into(), json!("too_many_windows")),
        ]
    );
    assert_eq!(rig.calls.borrow().len(), 2);
    let none = Rig::new(None);
    let out = none.run(|d| stop(&json!({ "sids": [A] }), d)).unwrap();
    assert_eq!(
        outcomes(&out),
        vec![(A.into(), "skipped".into(), json!("no_tmux"))]
    );
}

#[test]
fn a_malformed_batch_is_refused_whole() {
    let rig = Rig::new(Some(vec![]));
    for bad in [
        json!({}),
        json!({ "sids": [] }),
        json!({ "sids": [A, A] }),
        json!({ "sids": ["a b"] }),
        json!({ "sids": (0..=MAX_BATCH).map(|i| format!("s{i}")).collect::<Vec<_>>() }),
    ] {
        let e = rig.run(|d| stop(&bad, d)).unwrap_err();
        assert_eq!(e.0, "bad_args", "{bad}");
    }
    assert!(rig.calls.borrow().is_empty());
}

#[test]
fn the_standing_of_a_sid_is_judged_once_for_menu_and_stop() {
    let rows = vec![
        row("a-cc", Some(A), true),
        row("b-cc", Some(B), false),
        row("b-cc-2", Some(B), false),
        row("c-cc", Some(C), true),
        row("c-cc-2", Some(C), true),
        row("c-idle", Some(C), false),
        // 没打上标记的、同目录的 claude：不按目录认。
        row("proj-cc", None, true),
    ];
    assert_eq!(standing(&rows, A), Standing::Running("a-cc".into()));
    assert_eq!(standing(&rows, B), Standing::Idle("b-cc".into()));
    assert_eq!(
        standing(&rows, C),
        Standing::Ambiguous(vec!["c-cc".into(), "c-cc-2".into()])
    );
    assert_eq!(
        standing(&rows, "dddddddd-1111-2222-3333-444444444444"),
        Standing::None
    );
    let rig = Rig::new(Some(rows));
    let out = rig
        .run(|d| where_(&json!({ "sids": [A, B, C] }), d))
        .unwrap();
    assert_eq!(
        out,
        json!({ "results": [
            { "sid": A, "standing": "running", "names": ["a-cc"],
              "terminals": [{ "host": "tmux", "terminal": "tmux-a-cc" }] },
            { "sid": B, "standing": "idle", "names": ["b-cc"],
              "terminals": [{ "host": "tmux", "terminal": "tmux-b-cc" }] },
            { "sid": C, "standing": "ambiguous", "names": ["c-cc", "c-cc-2"],
              "terminals": [{ "host": "tmux", "terminal": "tmux-c-cc" }, { "host": "tmux", "terminal": "tmux-c-cc-2" }] },
        ], "run": "direct" })
    );
    let none = Rig::new(None);
    assert_eq!(
        none.run(|d| where_(&json!({ "sids": [A] }), d)).unwrap()["results"][0]["standing"],
        "no_tmux"
    );
}

/// ★ `sessions-where` 顺带答这台恢复默认放在哪：偏好「恢复时进 tmux」× 有没有 tmux（同 `session-open` 恢复没说 `run` 时那一条）。
#[test]
fn where_also_says_where_a_resume_lands_by_default() {
    let run = |rows: Option<Vec<TmuxEntry>>, pref: bool| {
        let mut rig = Rig::new(rows);
        rig.prefs.resume_in_tmux = pref;
        rig.run(|d| where_(&json!({ "sids": [A] }), d)).unwrap()["run"].clone()
    };
    assert_eq!(run(Some(vec![]), true), "tmux");
    assert_eq!(run(Some(vec![]), false), "direct");
    assert_eq!(run(None, true), "direct");
}

/// 列名单那一发过了期限（总期限用完了）⇒ 两条都整条回 `child_timed_out`（同别的装了总期限的命令）；
/// 别的列不成 ⇒ 看不见（`unobservable`，不是零会话）。
#[test]
fn a_listing_that_ran_out_of_time_answers_child_timed_out_not_unobservable() {
    for (err, want) in [
        (
            crate::platform::child::TIMED_OUT,
            crate::platform::child::TIMED_OUT,
        ),
        ("no_tmux", "unobservable"),
        ("failed", "unobservable"),
    ] {
        let mut rig = Rig::new(Some(vec![row("a-cc", Some(A), true)]));
        rig.list_err = Some(err);
        let code = |r: Result<Value, CmdErr>| r.expect_err("该整条失败").0;
        assert_eq!(
            code(rig.run(|d| where_(&json!({ "sids": [A] }), d))),
            want,
            "where {err}"
        );
        assert_eq!(
            code(rig.run(|d| stop(&json!({ "sids": [A] }), d))),
            want,
            "stop {err}"
        );
    }
}
