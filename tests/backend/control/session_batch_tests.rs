//! 批量停 · 问样子：每一个的结局与「单个那一条会做的事」相等（替身记下被调了什么；tmux 一个都不起）。

use crate::common::said::Said;
use crate::control::launch_account as la;
use crate::control::session_batch::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeSet;

/// 线上那一份入参 ⇒ 本体 ⇒ 线上那一份回包（解入参同 typed 分派那一步：形状不对 ⇒ `bad_args`）。
fn wired<R: serde::de::DeserializeOwned, T: serde::Serialize>(
    args: &Value,
    f: impl FnOnce(&R) -> Result<T, Said>,
) -> Result<Value, Said> {
    let req: R = serde_json::from_value(args.clone()).map_err(|e| bad(&e.to_string()))?;
    f(&req).map(|t| serde_json::to_value(t).unwrap())
}
fn where_(args: &Value, d: &Deps) -> Result<Value, Said> {
    wired(args, |r| crate::control::session_batch::where_(r, d))
}
/// 停（会话脸都说结束得了，称呼是 sid 的头八位）。
fn stop(args: &Value, d: &Deps) -> Result<Value, Said> {
    let title = |sid: &str| sid.chars().take(8).collect::<String>();
    stop_with(args, d, &|_, _| crate::wire::Allow::Yes, &title)
}
fn stop_with(
    args: &Value,
    d: &Deps,
    can_end: &dyn Fn(&str, &crate::wire::SessionContainer) -> crate::wire::Allow,
    title: &dyn Fn(&str) -> String,
) -> Result<Value, Said> {
    wired(args, |r| {
        crate::control::session_batch::stop(r, d, &StopFaces { can_end, title })
    })
}
/// 判号要的事实：这台只有一个号 `work`（能用）、没有谁的上次记录。
fn with_accounts<T>(f: impl FnOnce(&la::Facts) -> T) -> T {
    let library = || {
        crate::accounts::manage::library::test_support::list(vec![
            crate::accounts::manage::library::test_support::row_with("work", |a| {
                a.config_dir = Some("/h/.cc/work".into())
            }),
        ])
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

fn row(name: &str, sid: Option<&str>, agent: bool) -> TermEntry {
    TermEntry {
        host: crate::wire::TerminalHost::Tmux,
        name: name.into(),
        terminal: format!("tmux-{name}"),
        sid: sid.map(str::to_string),
        agent,
    }
}

/// 替身那一套：名单是给定的，结束只记下来；别的几样停与问样子都用不着。
struct Rig {
    /// tmux 那一臂（`None` ＝ 这台没装 tmux）。
    rows: Option<Vec<TermEntry>>,
    /// 托管那几行（并在 tmux 那几行后面）。
    hosted: Vec<TermEntry>,
    calls: RefCell<Vec<String>>,
    end_err: Option<&'static str>,
    /// 列名单那一发失败的码（`None` ⇒ 照 `rows` 答）。
    list_err: Option<&'static str>,
    /// 这台的偏好。
    prefs: crate::common::machine_prefs::Prefs,
    /// 启动台账记着的 `sid → 起在哪种宿主里`。
    started: Vec<(String, crate::wire::TerminalHost)>,
}

impl Rig {
    fn new(rows: Option<Vec<TermEntry>>) -> Self {
        Self {
            rows,
            hosted: vec![],
            calls: RefCell::new(vec![]),
            end_err: None,
            list_err: None,
            prefs: Default::default(),
            started: vec![],
        }
    }
    fn run<T>(&self, f: impl FnOnce(&Deps) -> T) -> T {
        let list = || -> Result<Listed, Said> {
            match self.list_err {
                Some(c) => Err(Said::new(c, "said".to_string())),
                None => Ok(Listed::of(self.rows.clone(), self.hosted.clone())),
            }
        };
        let end = |terminal: &str| -> Result<(), crate::common::said::Said> {
            self.calls.borrow_mut().push(format!("end {terminal}"));
            match self.end_err {
                Some(c) => Err(crate::wire::spec::fail(c, "said".to_string())),
                None => Ok(()),
            }
        };
        let started = |sid: &str| self.started.iter().find(|(s, _)| s == sid).map(|(_, h)| *h);
        deps_with(&list, &end, &self.prefs, &started, f)
    }
}

/// 停与问样子要的那一套：名单与结束由调用方给，别的几样都用不着。
fn deps_with<T>(
    list: &dyn Fn() -> Result<Listed, Said>,
    end: &dyn Fn(&str) -> Result<(), crate::common::said::Said>,
    prefs: &crate::common::machine_prefs::Prefs,
    started: &dyn Fn(&str) -> Option<crate::wire::TerminalHost>,
    f: impl FnOnce(&Deps) -> T,
) -> T {
    let caps = BTreeSet::new();
    with_accounts(|accounts| {
        f(&Deps {
            list,
            record: &|_, _| unreachable!("停不读记录"),
            end,
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
            open_shell: &|_, _, _| unreachable!("不开空 shell"),
            prefs,
            started,
        })
    })
}

/// ★ 一条在托管终端里的会话，标签页菜单［结束］（`sessions-stop`）⇒ 那个托管终端结束、名单少一行，不回「找不到」；
/// 请求方前端对不上 ⇒ 那一项 `wrong_owner`（界面「对齐后重试」那一支），托管终端还在。
/// 生产那一份名单（`listed_on`：tmux 指向不存在的私有 socket ＋ 沙箱里的托管家）与 `terminal-end` 那一条；假子进程（`sh`），不起真 claude。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_ends_a_session_living_in_a_hosted_terminal() {
    use crate::host::managed::run::test_support::{
        hello, read_until_out, sandbox_own, spec, start,
    };
    use crate::host::managed::wire::{read_frame, Ctl, Frame, Want};
    let (own, data) = sandbox_own("batch-stop");
    let script = r#"trap 'exit 7' HUP; printf 'ready\r\n'; while :; do read x; done"#;
    // 桌面端起的那一条（托管那一行的 `client` 照起它的前端）。
    let mut s = spec("t-batchstop", script);
    s.client = "desktop".into();
    let (sock, task) = start(&own, s).await;
    let mut viewer = hello(&sock, Want::View).await;
    read_until_out(&mut viewer, "ready").await;
    crate::host::managed::client::tag(&sock, A).expect("打上 sid");
    let bogus = data.join("no-tmux-here").to_string_lossy().into_owned();
    let stop_as = |client: Option<&'static str>| {
        let (o, b) = (own.clone(), bogus.clone());
        tokio::task::spawn_blocking(move || {
            let on = || crate::host::tmux::On { socket: Some(&b) };
            let list = || listed_on(on(), Some(&o));
            let end = |terminal: &str| {
                crate::control::kill::end_terminal(
                    on(),
                    Some(&o),
                    &crate::control::kill::TerminalEndReq {
                        terminal: Some(terminal.into()),
                        sid: None,
                        client: client.map(str::to_string),
                    },
                )
                .map(|_| ())
            };
            deps_with(&list, &end, &Default::default(), &|_| None, |d| {
                stop(&json!({ "sids": [A] }), d)
            })
        })
    };
    // 前端对不上（手机来结束桌面端起的那一条）⇒ 过不了门，那一句照单条结束那张表。
    let refused = stop_as(Some("mobile")).await.expect("线没崩").unwrap();
    assert_eq!(
        outcomes(&refused),
        vec![(A.into(), "failed".into(), json!("wrong_owner"))]
    );
    assert_eq!(
        refused["results"][0]["said"],
        json!(crate::control::kill::said(
            "name-t-batchstop",
            "wrong_owner"
        ))
    );
    assert_eq!(
        crate::host::managed::rows_at(&own).len(),
        1,
        "过不了门 ⇒ 还在"
    );
    let out = stop_as(Some("desktop")).await.expect("线没崩").unwrap();
    assert_eq!(outcomes(&out), vec![(A.into(), "done".into(), Value::Null)]);
    assert_eq!(out["results"][0]["session"], "name-t-batchstop");
    loop {
        match read_frame(&mut viewer).await.expect("读") {
            Some(Frame::Ctl(Ctl::Ended { .. })) | None => break,
            Some(_) => {}
        }
    }
    task.await.expect("任务没崩").expect("跑完");
    assert!(
        crate::host::managed::rows_at(&own).is_empty(),
        "名单里那一行没了"
    );
}

/// ★ 一条在托管终端里的会话 ⇒ `sessions-where` 回它 `terminals: [{host: managed, …}]`（标签页菜单照它出［结束］）。
/// 生产那一份名单（`listed_on`：tmux 指向不存在的私有 socket ＋ 沙箱里的托管家）；假子进程（`sh`），不起真 claude。
/// （这台装没装 tmux 随机器，另一个 sid 与 `run` 在下一条里用替身钉。）
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn where_names_the_hosted_terminal_a_session_lives_in() {
    use crate::host::managed::run::test_support::{
        hello, read_until_out, sandbox_own, spec, start,
    };
    use crate::host::managed::wire::Want;
    let (own, data) = sandbox_own("batch-where");
    let script = r#"printf 'ready\r\n'; while :; do read x; done"#;
    let (sock, task) = start(&own, spec("t-batchwhere", script)).await;
    let mut viewer = hello(&sock, Want::View).await;
    read_until_out(&mut viewer, "ready").await;
    crate::host::managed::client::tag(&sock, A).expect("打上 sid");
    let handle = crate::host::managed::rows_at(&own)[0].handle();
    let bogus = data.join("no-tmux-here").to_string_lossy().into_owned();
    let o = own.clone();
    let out = tokio::task::spawn_blocking(move || {
        let list = || {
            listed_on(
                crate::host::tmux::On {
                    socket: Some(&bogus),
                },
                Some(&o),
            )
        };
        deps_with(
            &list,
            &|_| unreachable!("问样子不结束"),
            &Default::default(),
            &|_| None,
            |d| where_(&json!({ "sids": [A] }), d),
        )
    })
    .await
    .expect("线没崩")
    .unwrap();
    assert_eq!(
        out["results"],
        json!([
            { "sid": A, "standing": "idle", "names": ["name-t-batchwhere"],
              "terminals": [{ "host": "managed", "terminal": handle }], "run": "managed" },
        ])
    );
    let s = sock.clone();
    tokio::task::spawn_blocking(move || crate::host::managed::client::end(&s))
        .await
        .expect("线没崩")
        .expect("结束那个托管终端");
    drop(viewer);
    task.await.expect("任务没崩").expect("跑完");
}

/// ★ 这台没装 tmux、只有托管终端：带着它的那一行照答（宿主 `managed`）；哪个终端都不带着的照旧 `no_tmux`（问样子 · 停同一判法）；
/// 恢复默认放在哪只看 tmux（偏好 tmux 也是直路）。
#[test]
fn without_tmux_a_hosted_terminal_still_answers_and_the_rest_say_no_tmux() {
    let hosted = TermEntry {
        host: crate::wire::TerminalHost::Managed,
        terminal: "h-1".into(),
        ..row("box-cc", Some(A), true)
    };
    let mut rig = Rig::new(None);
    rig.hosted = vec![hosted];
    rig.prefs.resume_where = cmd_wire::session::ResumeWhere::Tmux;
    assert_eq!(
        rig.run(|d| where_(&json!({ "sids": [A, B] }), d)).unwrap(),
        json!({ "results": [
            { "sid": A, "standing": "running", "names": ["box-cc"],
              "terminals": [{ "host": "managed", "terminal": "h-1" }], "run": "direct" },
            { "sid": B, "standing": "no_tmux", "names": [], "terminals": [], "run": "direct" },
        ] })
    );
    let out = rig.run(|d| stop(&json!({ "sids": [A, B] }), d)).unwrap();
    assert_eq!(
        outcomes(&out),
        vec![
            (A.into(), "done".into(), Value::Null),
            (B.into(), "skipped".into(), json!("no_tmux")),
        ]
    );
    assert_eq!(*rig.calls.borrow(), vec!["end h-1".to_string()]);
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
fn stop_ends_exactly_the_one_terminal_carrying_each_sid() {
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
            (B.into(), "skipped".into(), json!("not_hosted")),
            (C.into(), "skipped".into(), json!("ambiguous")),
        ]
    );
    // 结束的恰好是带着 A 的那一行（按名单句柄交 terminal-end）；命中两个的那一个一下都不碰（同单个菜单「命中多个不结束」）。
    assert_eq!(*rig.calls.borrow(), vec!["end tmux-a-cc".to_string()]);
    assert_eq!(out["results"][0]["session"], "a-cc");
}

#[test]
fn stop_says_why_per_session_and_one_failure_does_not_stop_the_rest() {
    let mut rig = Rig::new(Some(vec![
        row("a-cc", Some(A), true),
        row("b-cc", Some(B), false),
    ]));
    rig.end_err = Some("too_many_windows");
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

/// 停的每一项没做成 ⇒ 那一句由这台写好（`said`）：名单里的三种（没 tmux · 没找到 · 命中多个）与结束不成的每个码；
/// 做成的那一项没有那一句。句子里不出现码。
#[test]
fn every_stop_item_not_done_carries_its_sentence() {
    let said_of = |rig: Rig, sid: &str| -> (String, Value) {
        let out = rig.run(|d| stop(&json!({ "sids": [sid] }), d)).unwrap();
        let r = &out["results"][0];
        (
            r["why"].as_str().unwrap_or_default().to_string(),
            r["said"].clone(),
        )
    };
    let mut cases = vec![
        said_of(Rig::new(None), A),
        said_of(Rig::new(Some(vec![])), A),
        said_of(
            Rig::new(Some(vec![
                row("c-cc", Some(C), true),
                row("c-cc-2", Some(C), true),
            ])),
            C,
        ),
    ];
    for code in [
        "too_many_windows",
        "wrong_owner",
        "child_timed_out",
        "kill_failed",
        "no_such_session",
        "odd_new_code",
    ] {
        let mut rig = Rig::new(Some(vec![row("a-cc", Some(A), true)]));
        rig.end_err = Some(code);
        cases.push(said_of(rig, A));
    }
    let codes: Vec<&str> = cases.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(
        codes,
        [
            "no_tmux",
            "not_hosted",
            "ambiguous",
            "too_many_windows",
            "wrong_owner",
            "child_timed_out",
            "kill_failed",
            "no_such_session",
            "odd_new_code"
        ]
    );
    for (code, said) in &cases {
        let said = said
            .as_str()
            .unwrap_or_else(|| panic!("{code}：没有那一句"));
        assert!(
            !said.is_empty() && !said.contains(code.as_str()),
            "{code}：{said}"
        );
    }
    assert!(cases[2].1.as_str().unwrap().contains("c-cc, c-cc-2"));
    let done = Rig::new(Some(vec![row("a-cc", Some(A), true)]));
    let out = done.run(|d| stop(&json!({ "sids": [A] }), d)).unwrap();
    assert_eq!(out["results"][0]["said"], Value::Null);
}

/// ★ 跳不跳过由这台照会话脸的 `can.end` 判（界面不先筛）：结束不了的那一项 `skipped`、那一句就是脸上那一句、不碰名单句柄；
/// 每一项带这台写的称呼；容器那一格照这台名单（带着它的那一行 ⇒ 那种宿主；哪行都不带着 ⇒ 不在任何宿主里）。
#[test]
fn stop_skips_what_the_session_face_says_cannot_end_and_titles_every_item() {
    use crate::wire::{Allow, SessionContainer};
    let rig = Rig::new(Some(vec![
        row("a-cc", Some(A), true),
        row("b-cc", Some(B), true),
        row("c-cc", Some(C), true),
    ]));
    let asked = RefCell::new(Vec::<(String, SessionContainer)>::new());
    let can_end = |sid: &str, c: &SessionContainer| {
        asked.borrow_mut().push((sid.to_string(), c.clone()));
        if sid == B {
            Allow::No {
                why: crate::common::cells::Words("脸上那一句".into()),
            }
        } else {
            Allow::Yes
        }
    };
    let title = |sid: &str| format!("proj 标题-{}", &sid[..1]);
    let out = rig
        .run(|d| stop_with(&json!({ "sids": [A, B, C] }), d, &can_end, &title))
        .unwrap();
    assert_eq!(
        outcomes(&out),
        vec![
            (A.into(), "done".into(), Value::Null),
            (B.into(), "skipped".into(), json!("not_endable")),
            (C.into(), "done".into(), Value::Null),
        ]
    );
    assert_eq!(out["results"][1]["said"], "脸上那一句");
    assert_eq!(out["results"][1]["copyDetail"], "");
    assert_eq!(
        *rig.calls.borrow(),
        vec!["end tmux-a-cc".to_string(), "end tmux-c-cc".to_string()]
    );
    let titles: Vec<&str> = out["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["proj 标题-a", "proj 标题-b", "proj 标题-c"]);
    for r in out["results"].as_array().unwrap() {
        assert_eq!(r["machine"], "<local>", "每台那一跳答的是它自己");
    }
    assert_eq!(
        asked.borrow()[0].1,
        SessionContainer::Tmux {
            terminal: Some("tmux-a-cc".into())
        }
    );
    // 哪行都不带着 ⇒ 交给脸的容器是「不在任何宿主里」。
    let bare = Rig::new(Some(vec![]));
    asked.borrow_mut().clear();
    bare.run(|d| stop_with(&json!({ "sids": [A] }), d, &can_end, &title))
        .unwrap();
    assert_eq!(asked.borrow()[0].1, SessionContainer::None);
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
        assert_eq!(e.code.as_deref(), Some("bad_args"), "{bad}");
    }
    assert!(rig.calls.borrow().is_empty());
}

/// 名单 ⇒ 这个 sid 的样子，带着的那几行换成 tmux 会话名（生产那条是 [`carriers`]）。
fn standing(rows: &[TermEntry], sid: &str) -> Standing {
    match carriers(rows, sid) {
        Standing::Running(e) => Standing::Running(e.name.clone()),
        Standing::Ambiguous(es) => Standing::Ambiguous(es.iter().map(|e| e.name.clone()).collect()),
        Standing::Idle(e) => Standing::Idle(e.name.clone()),
        Standing::None => Standing::None,
    }
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
              "terminals": [{ "host": "tmux", "terminal": "tmux-a-cc" }], "run": "direct" },
            { "sid": B, "standing": "idle", "names": ["b-cc"],
              "terminals": [{ "host": "tmux", "terminal": "tmux-b-cc" }], "run": "direct" },
            { "sid": C, "standing": "ambiguous", "names": ["c-cc", "c-cc-2"],
              "terminals": [{ "host": "tmux", "terminal": "tmux-c-cc" }, { "host": "tmux", "terminal": "tmux-c-cc-2" }], "run": "direct" },
        ] })
    );
    let none = Rig::new(None);
    assert_eq!(
        none.run(|d| where_(&json!({ "sids": [A] }), d)).unwrap()["results"][0]["standing"],
        "no_tmux"
    );
}

/// ★ `sessions-where` 每一项顺带答那条恢复默认放在哪：偏好（三选）× 那条起在哪 × 有没有 tmux（同 `session-open` 恢复没说 `run` 时那一条）。
#[test]
fn where_also_says_where_each_resume_lands_by_default() {
    use cmd_wire::session::ResumeWhere;
    let run = |rows: Option<Vec<TermEntry>>, pref: ResumeWhere| {
        let mut rig = Rig::new(rows);
        rig.prefs.resume_where = pref;
        rig.run(|d| where_(&json!({ "sids": [A] }), d)).unwrap()["results"][0]["run"].clone()
    };
    assert_eq!(run(Some(vec![]), ResumeWhere::Tmux), "tmux");
    assert_eq!(run(Some(vec![]), ResumeWhere::Managed), "direct");
    assert_eq!(run(None, ResumeWhere::Tmux), "direct");
    // 照起时那一种：两条会话，A 记着起在 tmux 里、B 没记 ⇒ 各自不同。
    let mut rig = Rig::new(Some(vec![]));
    rig.prefs.resume_where = ResumeWhere::AsStarted;
    rig.started = vec![(A.to_string(), crate::wire::TerminalHost::Tmux)];
    let out = rig.run(|d| where_(&json!({ "sids": [A, B] }), d)).unwrap();
    assert_eq!(
        (
            out["results"][0]["run"].clone(),
            out["results"][1]["run"].clone()
        ),
        (json!("tmux"), json!("direct"))
    );
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
        let code =
            |r: Result<Value, Said>| r.expect_err("该整条失败").code_or_fallback().to_string();
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

/// 给了托管的家 ⇒ 这台能托管（每个平台都是，Windows 也是）；没给 ⇒ 不能。
#[test]
fn a_given_hosting_home_hosts_on_every_platform() {
    let (own, _data) = crate::host::managed::run::test_support::sandbox_own("batch-hosts");
    assert!(hosts_with(Some(&own)));
    assert!(!hosts_with(None));
}
