//! `session-open`：默认放在哪的整张表 · 恢复 / 接回 / 新起各落在哪 · 已有进程在写就不起（直路一样）· 给了 tmux 而没有就拒 ·
//! 每个码都有后端写好的那一句（替身记下被调了什么；tmux 一个都不起）。

use super::*;
use crate::control::launch_account as la;
use crate::control::launch_render::local;
use serde_json::json;
use std::cell::RefCell;
use std::collections::BTreeSet;

const A: &str = "aaaaaaaa-1111-2222-3333-444444444444";
const U: &str = "99999999-1111-2222-3333-444444444444";

fn row(name: &str, sid: Option<&str>, agent: bool) -> TmuxEntry {
    TmuxEntry {
        name: name.into(),
        terminal: format!("tmux-{name}"),
        sid: sid.map(str::to_string),
        agent,
    }
}

/// 替身：名单 · 记录 · 活进程是给定的；铸 · 建 · 键入 · 预标只记下来。
struct Rig {
    rows: Option<Vec<TmuxEntry>>,
    list_err: Option<&'static str>,
    gone: bool,
    live: Vec<u32>,
    ccm_rc: i32,
    send_err: Option<&'static str>,
    no_entry: bool,
    fork_err: bool,
    rotation_err: bool,
    prefs: crate::control::machine_prefs::Prefs,
    ccm: RefCell<Vec<Vec<String>>>,
    sent: RefCell<Vec<(String, String)>>,
    marks: RefCell<usize>,
    shells: RefCell<Vec<(String, Option<String>)>>,
}

impl Rig {
    fn new(rows: Option<Vec<TmuxEntry>>) -> Self {
        Self {
            rows,
            list_err: None,
            gone: false,
            live: vec![],
            ccm_rc: 0,
            send_err: None,
            no_entry: false,
            fork_err: false,
            rotation_err: false,
            prefs: Default::default(),
            ccm: RefCell::new(vec![]),
            sent: RefCell::new(vec![]),
            marks: RefCell::new(0),
            shells: RefCell::new(vec![]),
        }
    }

    /// 线上那一份入参（`local: false`，别的格照给的）⇒ 本体。恢复那一条的记录：哪一家 claude · 目录 /p。
    fn open(&self, args: Value) -> Result<Opened, Failed> {
        let mut args = args;
        if args.get("local").is_none() {
            args["local"] = json!(false);
        }
        let req: SessionOpenReq =
            serde_json::from_value(args).map_err(|e| malformed(&e.to_string()))?;
        let facts = Facts {
            resume: (!self.gone).then(|| ResumeOf {
                agent: "claude".into(),
                cwd: "/p".into(),
                model: None,
                permission_mode: None,
            }),
            fork_tmux: None,
        };
        let fork = |_: &str, _: &str| -> Result<String, (&'static str, String)> {
            if self.fork_err {
                Err(("fork_failed", "分支记录写不成".into()))
            } else {
                Ok(U.to_string())
            }
        };
        let sid = || U.to_string();
        let write = |_: &str, _: &str, _: &str| -> Result<(), (&'static str, String)> {
            if self.rotation_err {
                Err(("no_strategy", "没有这一份策略".into()))
            } else {
                Ok(())
            }
        };
        let forget = |_: &str| {};
        let pre = PreRotation {
            sid: &sid,
            write: &write,
            forget: &forget,
        };
        self.run(|deps| answer(req, facts, deps, &fork, &pre, None))
    }

    /// 拼一份替身 `Deps` 交给 `f`。
    fn run<T>(&self, f: impl FnOnce(&Deps) -> T) -> T {
        let caps: BTreeSet<String> = crate::control::launch_render::own_caps();
        let list = || -> Result<Option<Vec<TmuxEntry>>, CmdErr> {
            match self.list_err {
                Some(c) => Err((c, "列不出".to_string())),
                None => Ok(self.rows.clone()),
            }
        };
        let record = |_: &str, _: Option<&str>| -> Result<(bool, String), String> {
            Ok((true, "/root".into()))
        };
        let kill = |_: &str,
                    _: &str|
         -> Result<crate::control::cc_bus::BusNote, crate::wire::spec::Fail> {
            unreachable!("session-open 不杀")
        };
        let send_into = |name: &str, _: &str, line: &str| -> Result<(), crate::wire::spec::Fail> {
            self.sent.borrow_mut().push((name.into(), line.into()));
            match self.send_err {
                Some(c) => Err(crate::wire::spec::Fail::new(c, "门没过".to_string())),
                None => Ok(()),
            }
        };
        let run_ccm = |argv: &[String]| -> Result<(i32, String, String), CmdErr> {
            self.ccm.borrow_mut().push(argv.to_vec());
            Ok((self.ccm_rc, String::new(), "坏了".into()))
        };
        let mint = |base: NameBase| -> Result<String, CmdErr> {
            Ok(match base {
                NameBase::Cwd(_) => "p-cc".into(),
                NameBase::ForkOf(s) => format!("{s}-fork-cc"),
            })
        };
        let writers = |_: &str| self.live.clone();
        let pretrust = |_: &str, _: &str| *self.marks.borrow_mut() += 1;
        let open_shell = |name: &str, cwd: Option<&str>| -> Result<bool, crate::wire::spec::Fail> {
            self.shells
                .borrow_mut()
                .push((name.into(), cwd.map(str::to_string)));
            Ok(true)
        };
        let library = || {
            la::Library::of_product(&crate::observe::accounts_query::test_support::list(vec![
                crate::observe::accounts_query::test_support::row_with("work", |a| {
                    a.config_dir = Some("/h/.cc/work".into())
                }),
            ]))
        };
        let accounts = la::Facts {
            has_accounts: true,
            library: &library,
            last: &|_| None,
        };
        let entry: fn() -> Option<String> = if self.no_entry {
            || None
        } else {
            || Some("/b/ccm".into())
        };
        f(&Deps {
            list: &list,
            record: &record,
            kill: &kill,
            send_into: &send_into,
            run_ccm: &run_ccm,
            mint: &mint,
            caps: &caps,
            local_facts: local::Facts {
                windows: false,
                is_dir: |d| d == "/p",
                entry,
            },
            accounts: &accounts,
            writers: &writers,
            pretrust: &pretrust,
            open_shell: &open_shell,
            prefs: &self.prefs,
        })
    }
}

fn resume(run: Option<&str>) -> Value {
    let mut v = json!({ "what": { "kind": "resume", "sid": A } });
    if let Some(r) = run {
        v["run"] = json!(r);
    }
    v
}

fn new(extra: Value) -> Value {
    let mut what = json!({ "kind": "new", "agent": "claude", "cwd": "/p" });
    for (k, v) in extra.as_object().unwrap() {
        what[k] = v.clone();
    }
    json!({ "what": what })
}

/// ★ 默认放在哪：意图 × 偏好 × 有没有 tmux 的整张表（两向：表里每一行照算 ＝ 期望，组合一个不漏）。
#[test]
fn default_run_is_the_whole_table() {
    use Intent::*;
    use Run::*;
    let intents = [
        New { fork_tmux: None },
        New {
            fork_tmux: Some(true),
        },
        New {
            fork_tmux: Some(false),
        },
        Resume,
    ];
    let mut got = Vec::new();
    for i in intents {
        for pref in [false, true] {
            for tmux in [false, true] {
                got.push((i, pref, tmux, default_run(i, pref, tmux)));
            }
        }
    }
    let n = |f| New { fork_tmux: f };
    let want = vec![
        (n(None), false, false, Direct),
        (n(None), false, true, Tmux),
        (n(None), true, false, Direct),
        (n(None), true, true, Tmux),
        (n(Some(true)), false, false, Direct),
        (n(Some(true)), false, true, Tmux),
        (n(Some(true)), true, false, Direct),
        (n(Some(true)), true, true, Tmux),
        (n(Some(false)), false, false, Direct),
        (n(Some(false)), false, true, Direct),
        (n(Some(false)), true, false, Direct),
        (n(Some(false)), true, true, Direct),
        (Resume, false, false, Direct),
        (Resume, false, true, Direct),
        (Resume, true, false, Direct),
        (Resume, true, true, Tmux),
    ];
    assert_eq!(got, want);
}

/// ★ 直路也过「已有进程在写就拒」那道门：拒 ⇒ `already_live`，零次预标、零次起 ccm、不渲那一行。
#[test]
fn a_direct_resume_of_a_session_someone_is_writing_is_refused() {
    let mut rig = Rig::new(Some(vec![]));
    rig.live = vec![4242];
    let err = rig.open(resume(Some("direct"))).unwrap_err();
    assert_eq!((err.0, err.3.as_deref()), ("already_live", Some("4242")));
    assert_eq!((*rig.marks.borrow(), rig.ccm.borrow().len()), (0, 0));
    // 正控：没人在写 ⇒ 照起（回那一行）。
    let rig = Rig::new(Some(vec![]));
    let o = rig.open(resume(Some("direct"))).unwrap();
    assert_eq!(o.landed, Landed::Window);
    assert!(o.line.unwrap().contains(A));
}

/// ★ 给了 tmux 而这台没有 ⇒ `place_unavailable`，不改直路、零次起 ccm（恢复与新起都一样）。
#[test]
fn asking_for_tmux_where_there_is_none_is_refused_not_rerouted() {
    for args in [resume(Some("tmux")), {
        let mut v = new(json!({}));
        v["run"] = json!("tmux");
        v
    }] {
        let rig = Rig::new(None);
        let err = rig.open(args).unwrap_err();
        assert_eq!(err.0, "place_unavailable");
        assert!(rig.ccm.borrow().is_empty());
    }
    // 没说放在哪 ⇒ 这台定：没 tmux 就直路。
    let rig = Rig::new(None);
    assert_eq!(rig.open(new(json!({}))).unwrap().landed, Landed::Window);
}

/// 恢复进 tmux：没在哪个 tmux 里 ⇒ 铸名建会话（`--detach`）、回接进去那一行；空 tmux ⇒ 就地键入；在跑 ⇒ 不另起。
#[test]
fn resume_in_tmux_lands_where_the_session_can_be() {
    let rig = Rig::new(Some(vec![]));
    let o = rig.open(resume(Some("tmux"))).unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::Started, Some("p-cc"))
    );
    assert!(
        o.line.as_deref().unwrap().ends_with("--attach p-cc"),
        "{:?}",
        o.line
    );
    let argv = rig.ccm.borrow()[0].join(" ");
    assert!(
        argv.contains("--ccm-tmux=p-cc") && argv.contains("--detach") && argv.contains(A),
        "{argv}"
    );

    let rig = Rig::new(Some(vec![row("old", Some(A), false)]));
    let o = rig.open(resume(Some("tmux"))).unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::InPlace, Some("old"))
    );
    assert_eq!(rig.sent.borrow()[0].0, "old");
    assert!(rig.ccm.borrow().is_empty());

    let rig = Rig::new(Some(vec![
        row("live", Some(A), true),
        row("two", Some(A), true),
    ]));
    let o = rig.open(resume(Some("tmux"))).unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::Running, Some("live"))
    );
    assert!(o.note.unwrap().contains("live"));
    assert!(rig.ccm.borrow().is_empty() && rig.sent.borrow().is_empty());
}

/// 恢复没说放在哪 ⇒ 照这台的偏好（「恢复时进 tmux」开着才进）。
#[test]
fn resume_without_run_follows_this_machines_preference() {
    let rig = Rig::new(Some(vec![]));
    assert_eq!(rig.open(resume(None)).unwrap().landed, Landed::Window);
    let mut rig = Rig::new(Some(vec![]));
    rig.prefs.resume_in_tmux = true;
    assert_eq!(rig.open(resume(None)).unwrap().landed, Landed::Started);
}

/// 本机「不用 tmux」就是直路（不先建 tmux）；本机那一行也带这台给那个号的模型偏好。
#[test]
fn a_local_direct_resume_is_direct_and_carries_the_model_preference() {
    let mut rig = Rig::new(Some(vec![]));
    rig.prefs.models.insert("work".into(), "opus".into());
    let mut args = resume(Some("direct"));
    args["local"] = json!(true);
    args["account"] = json!({ "kind": "named", "name": "work" });
    let o = rig.open(args).unwrap();
    let line = o.line.unwrap();
    assert_eq!((o.landed, o.session), (Landed::Window, None));
    assert!(
        !line.contains("--ccm-tmux") && line.contains("--model opus"),
        "{line}"
    );
    assert!(rig.ccm.borrow().is_empty());
}

/// 接回：按会话（在跑 / 空 shell）或按会话名，回接进去那一行；不在哪个 tmux 里 ⇒ `not_in_tmux`。
#[test]
fn look_attaches_without_starting_anything() {
    let rig = Rig::new(Some(vec![
        row("idle", Some(A), false),
        row("x", None, true),
    ]));
    let o = rig
        .open(json!({ "what": { "kind": "look", "sid": A } }))
        .unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref(), o.agent),
        (Landed::Idle, Some("idle"), None)
    );
    assert!(o.line.unwrap().ends_with("--attach idle"));
    let o = rig
        .open(json!({ "what": { "kind": "look", "session": "x" } }))
        .unwrap();
    assert_eq!(o.landed, Landed::Running);
    let err = rig
        .open(json!({ "what": { "kind": "look", "sid": U } }))
        .unwrap_err();
    assert_eq!(err.0, "not_in_tmux");
    assert!(rig.ccm.borrow().is_empty() && rig.sent.borrow().is_empty());
}

/// 新起：放在 tmux ⇒ 后台起好、回会话名与接进去那一行；直路 ⇒ 回起会话那一行。
#[test]
fn new_lands_started_or_window() {
    let rig = Rig::new(Some(vec![]));
    let o = rig.open(new(json!({}))).unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref(), o.sid.as_deref()),
        (Landed::Started, Some("p-cc"), Some(U))
    );
    assert_eq!(
        o.agent.map(|a| a.as_str().to_string()).as_deref(),
        Some("claude")
    );
    let mut args = new(json!({}));
    args["run"] = json!("direct");
    let o = rig.open(args).unwrap();
    assert_eq!(o.landed, Landed::Window);
    assert!(o.line.unwrap().contains(" new"));
}

/// ★ 每个码都有后端写好的那一句：码表每一项喂一次（`launch_pending` 是票那一层的，判据在 `session_new_ticket`），
/// 那一句非空、不含码本身；喂到的码 ＝ 码表（两向）。
#[test]
fn every_code_comes_with_a_sentence_written_here() {
    let tmux = |args: Value| {
        let mut a = args;
        a["run"] = json!("tmux");
        a
    };
    let cases: Vec<(&str, Box<dyn Fn(&mut Rig) -> Value>)> = vec![
        (
            "bad_args",
            Box::new(|_| json!({ "what": { "kind": "resume", "sid": "a;b" } })),
        ),
        (
            "unknown_agent",
            Box::new(|_| json!({ "what": { "kind": "new", "agent": "nope", "cwd": "/p" } })),
        ),
        (
            "bad_command",
            Box::new(|_| new(json!({ "command": "a;b" }))),
        ),
        (
            "no_dir",
            Box::new(|_| json!({ "what": { "kind": "new", "agent": "claude", "cwd": "/nope" } })),
        ),
        (
            "account_unavailable",
            Box::new(|_| {
                let mut a = resume(None);
                a["account"] = json!({ "kind": "named", "name": "ghost" });
                a
            }),
        ),
        (
            "place_unavailable",
            Box::new(|r| {
                r.rows = None;
                resume(Some("tmux"))
            }),
        ),
        (
            "bad_tmux_name",
            Box::new(move |_| tmux(new(json!({ "tmuxName": "a:b" })))),
        ),
        (
            "tmux_taken",
            Box::new(move |r| {
                r.rows = Some(vec![row("t", None, false)]);
                tmux(new(json!({ "tmuxName": "t" })))
            }),
        ),
        (
            "record_gone",
            Box::new(|r| {
                r.gone = true;
                resume(None)
            }),
        ),
        (
            "already_live",
            Box::new(|r| {
                r.live = vec![7];
                resume(Some("direct"))
            }),
        ),
        (
            "not_in_tmux",
            Box::new(|_| json!({ "what": { "kind": "look", "sid": A } })),
        ),
        (
            "wrong_owner",
            Box::new(|r| {
                r.rows = Some(vec![row("idle", Some(A), false)]);
                r.send_err = Some("wrong_owner");
                resume(Some("tmux"))
            }),
        ),
        (
            "unobservable",
            Box::new(|r| {
                r.list_err = Some("failed");
                resume(None)
            }),
        ),
        (
            "fork_failed",
            Box::new(|r| {
                r.fork_err = true;
                new(json!({ "fork": { "sid": A, "uuid": U } }))
            }),
        ),
        (
            "refused",
            Box::new(|r| {
                r.no_entry = true;
                resume(Some("direct"))
            }),
        ),
        (
            "start_failed",
            Box::new(|r| {
                r.ccm_rc = 1;
                resume(Some("tmux"))
            }),
        ),
        (
            "child_timed_out",
            Box::new(|r| {
                r.list_err = Some(crate::platform::child::TIMED_OUT);
                resume(None)
            }),
        ),
        (
            "no_strategy",
            Box::new(|r| {
                r.rotation_err = true;
                new(json!({ "rotation": { "strategy": "s" } }))
            }),
        ),
    ];
    let mut fed: BTreeSet<&str> = BTreeSet::new();
    for (code, setup) in &cases {
        let mut rig = Rig::new(Some(vec![]));
        let args = setup(&mut rig);
        let err = rig.open(args).expect_err(code);
        assert_eq!(err.0, *code, "{err:?}");
        assert!(
            !err.1.trim().is_empty() && !err.1.contains(code),
            "{code}：那一句 {:?}",
            err.1
        );
        fed.insert(code);
    }
    let want: BTreeSet<&str> = OPEN_CODES
        .iter()
        .copied()
        .filter(|c| *c != "launch_pending")
        .collect();
    assert_eq!(fed, want);
}

/// 入参形状：多送一格 ⇒ 拒；接回两格给一格。
#[test]
fn the_request_shape_is_closed() {
    let rig = Rig::new(Some(vec![]));
    for bad in [
        json!({ "what": { "kind": "resume", "sid": A, "cwd": "/p" } }),
        json!({ "what": { "kind": "look", "sid": A, "session": "x" } }),
        json!({ "what": { "kind": "look" } }),
        json!({ "what": { "kind": "look", "sid": A }, "run": "tmux" }),
        json!({ "what": { "kind": "resume", "sid": A }, "extra": 1 }),
    ] {
        assert_eq!(rig.open(bad.clone()).unwrap_err().0, "bad_args", "{bad}");
    }
}

/// 恢复再送那一项：带那一句 · 记录里最后那个模型 · 权限档。
fn saying(model: Option<&str>, mode: Option<&str>) -> Item {
    Item {
        sid: A.into(),
        cwd: "/p".into(),
        account: AccountAsk::Follow,
        fresh: false,
        fork_of: None,
        say: Some("-接着做\n第二行".into()),
        model: model.map(str::to_string),
        permission_mode: mode.map(str::to_string),
    }
}

fn has_pair(argv: &[String], a: &str, b: &str) -> bool {
    argv.windows(2).any(|w| w[0] == a && w[1] == b)
}

/// ★ 起的那一项带上那一句（`--` 之后）· `--model`（记录里那一个）· 权限档 —— 本机与远端两形都带；
/// 越过批准的档 · 那一家不认的档 · 不是模型名的都不给（跟随），那一句照带。
#[test]
fn a_resumed_item_carries_the_line_the_model_and_the_mode_and_drops_what_it_must() {
    for local in [true, false] {
        let rig = Rig::new(Some(vec![]));
        rig.run(|d| {
            relaunch_in(
                &saying(Some("opus"), Some("acceptEdits")),
                "claude",
                &Settled::Unsaid,
                "p-cc",
                local,
                d,
            )
        })
        .unwrap();
        let argv = rig.ccm.borrow()[0].clone();
        assert!(
            has_pair(&argv, "--", "-接着做\n第二行"),
            "local={local}: {argv:?}"
        );
        assert!(
            has_pair(&argv, "--model", "opus"),
            "local={local}: {argv:?}"
        );
        assert!(
            has_pair(&argv, "--permission-mode", "acceptEdits"),
            "local={local}: {argv:?}"
        );
        for (model, mode) in [
            (Some("bad model;"), Some("bypassPermissions")),
            (None, Some("auto")),
        ] {
            let rig = Rig::new(Some(vec![]));
            rig.run(|d| {
                relaunch_in(
                    &saying(model, mode),
                    "claude",
                    &Settled::Unsaid,
                    "p-cc",
                    local,
                    d,
                )
            })
            .unwrap();
            let argv = rig.ccm.borrow()[0].clone();
            assert!(
                !argv
                    .iter()
                    .any(|a| a == "--permission-mode" || a == "--model"),
                "{argv:?}"
            );
            assert!(has_pair(&argv, "--", "-接着做\n第二行"), "{argv:?}");
        }
    }
}

/// ★ 恢复再送照恢复进 tmux 那一支走：铸名起（`--detach`），那一句在行里；已有活进程在写 ⇒ 不起；没 tmux ⇒ 不起；那一家不收第一句 ⇒ 不起。
#[test]
fn resuming_to_say_goes_the_tmux_way_and_stops_where_it_would() {
    let rig = Rig::new(Some(vec![]));
    let o = rig
        .run(|d| resume_saying(&saying(None, None), "claude", d))
        .unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::Started, Some("p-cc"))
    );
    let argv = rig.ccm.borrow()[0].clone();
    assert!(
        argv.iter().any(|t| t == "--detach") && has_pair(&argv, "--", "-接着做\n第二行"),
        "{argv:?}"
    );

    let mut live = Rig::new(Some(vec![]));
    live.live = vec![42];
    assert_eq!(
        live.run(|d| resume_saying(&saying(None, None), "claude", d))
            .unwrap_err()
            .0,
        "already_live"
    );
    assert!(live.ccm.borrow().is_empty());
    let none = Rig::new(None);
    assert_eq!(
        none.run(|d| resume_saying(&saying(None, None), "claude", d))
            .unwrap_err()
            .0,
        "place_unavailable"
    );
    let rig = Rig::new(Some(vec![]));
    assert_eq!(
        rig.run(|d| resume_saying(&saying(None, None), "no-such-agent", d))
            .unwrap_err()
            .0,
        NO_FIRST_PROMPT
    );
    assert!(rig.ccm.borrow().is_empty());
}

/// ★ 开一个 shell：给了会话名 ⇒ 不在就在那个目录里建一个空的、回接进去那一行（`idle`）；已在 ⇒ 不建、目录不管、照它此刻的样子回；
/// 只给目录 ⇒ 这台 shell 里进那个目录那一行（`window`）；这台没 tmux ⇒ `place_unavailable`，什么都不建；一个 agent 都不起。
#[test]
fn shell_makes_or_joins_a_bare_session_or_enters_a_dir() {
    let rig = Rig::new(Some(vec![row("busy", Some(A), true)]));
    let o = rig
        .open(json!({ "what": { "kind": "shell", "session": "main", "cwd": "/p" } }))
        .unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref(), o.agent, o.sid.as_deref()),
        (Landed::Idle, Some("main"), None, None)
    );
    assert!(o.line.unwrap().ends_with("--attach main"));
    assert_eq!(
        *rig.shells.borrow(),
        [("main".to_string(), Some("/p".to_string()))]
    );

    let o = rig
        .open(json!({ "what": { "kind": "shell", "session": "busy", "cwd": "/p" } }))
        .unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::Running, Some("busy"))
    );
    assert_eq!(rig.shells.borrow().len(), 1, "已在的会话不该再建");

    let o = rig
        .open(json!({ "what": { "kind": "shell", "cwd": "/p" } }))
        .unwrap();
    assert_eq!((o.landed, o.session), (Landed::Window, None));
    assert_eq!(o.line.as_deref(), Some("cd '/p'"));

    let none = Rig::new(None);
    let err = none
        .open(json!({ "what": { "kind": "shell", "session": "main" } }))
        .unwrap_err();
    assert_eq!(err.0, "place_unavailable");
    assert!(none.shells.borrow().is_empty());
    // 没 tmux 也进得了目录（那一行不靠 tmux）。
    assert!(none
        .open(json!({ "what": { "kind": "shell", "cwd": "/p" } }))
        .is_ok());

    for (args, code) in [
        (json!({ "what": { "kind": "shell" } }), "bad_args"),
        (
            json!({ "what": { "kind": "shell", "cwd": "/p" }, "run": "tmux" }),
            "bad_args",
        ),
        (
            json!({ "what": { "kind": "shell", "cwd": "/nope" } }),
            "no_dir",
        ),
        (
            json!({ "what": { "kind": "shell", "cwd": "/p\nx" } }),
            "bad_args",
        ),
        (
            json!({ "what": { "kind": "shell", "session": "-x" } }),
            "bad_tmux_name",
        ),
    ] {
        assert_eq!(rig.open(args.clone()).unwrap_err().0, code, "{args}");
    }
    assert_eq!(rig.shells.borrow().len(), 1);
    assert!(rig.ccm.borrow().is_empty() && rig.sent.borrow().is_empty());
}
