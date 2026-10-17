//! `session-open`：默认放在哪的整张表 · 恢复 / 接回 / 新起各落在哪 · 已有进程在写就不起（直路一样）· 给了 tmux 而没有就拒 ·
//! 每个码都有后端写好的那一句（替身记下被调了什么；tmux 一个都不起）。

use super::*;
use crate::control::launch_account as la;
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
    /// 恢复那一条记录里的目录与模型。
    cwd: String,
    model: Option<String>,
    /// 账号库里的号（名字）；铸名替身回的名字（缺 ⇒ 按基名那一形）。
    names: Vec<String>,
    minted: Option<String>,
    /// 给 e2e 出那一行：哪个目录都在、入口是这台装好的那一份。
    e2e: bool,
    /// 分叉的源会话此刻在不在 tmux 里（宿主读的那一格）。
    fork_tmux: Option<bool>,
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
            cwd: "/p".into(),
            model: None,
            names: vec!["work".into()],
            minted: None,
            e2e: false,
            fork_tmux: None,
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
                cwd: self.cwd.clone(),
                model: self.model.clone(),
                permission_mode: None,
            }),
            fork_tmux: self.fork_tmux,
            since: 0,
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
            if let Some(n) = &self.minted {
                return Ok(n.clone());
            }
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
            la::Library::of_product(&crate::observe::accounts_query::test_support::list(
                self.names
                    .iter()
                    .map(|n| {
                        crate::observe::accounts_query::test_support::row_with(n, |a| {
                            a.config_dir = Some(format!("/h/.cc/{n}"))
                        })
                    })
                    .collect(),
            ))
        };
        let accounts = la::Facts {
            has_accounts: true,
            library: &library,
            last: &|_| None,
        };
        let entry: fn() -> Option<String> = if self.no_entry {
            || None
        } else if self.e2e {
            crate::platform::paths::installed_ccm_entry
        } else {
            || Some("/b/ccm".into())
        };
        let is_dir: fn(&str) -> bool = if self.e2e { |_| true } else { |d| d == "/p" };
        f(&Deps {
            list: &list,
            record: &record,
            kill: &kill,
            send_into: &send_into,
            run_ccm: &run_ccm,
            mint: &mint,
            caps: &caps,
            host: crate::control::launch_render::Host {
                windows: false,
                is_dir,
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

/// e2e 要的那一份意图（`CCM_E2E_OPEN`，只给 `tests/e2e/open-line-emit.sh` 用）。
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct E2eOpen {
    /// `resume-tmux` · `into-existing` · `direct` · `attach` · `new-tmux`。
    shape: String,
    #[serde(default)]
    sid: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    /// tmux 会话名（新建的 · 键进去的 · 接进去的）。
    #[serde(default)]
    name: Option<String>,
    launcher: String,
    /// 线上 `account` 那一形；缺 ＝ 跟随。
    #[serde(default)]
    account: Option<Value>,
    #[serde(default)]
    model: Option<String>,
    /// `local` · `remote`。
    here: String,
}

/// **e2e 取「app 起会话时真正跑的那一行」的数据出口**：意图照 `session-open` 的入参走一遍本体（替身只记不做），
/// 吐它交出去的那一行 —— 后台起的那几形是交给 ccm 的 argv（逐个引号化连起来）、键进空 tmux 的是键入的那一行、
/// 开窗与接回的是回包里的 `line`。`OPEN_LINE<<<…>>>` 或 `OPEN_LINE_ERR<<<…>>>`。
#[test]
#[ignore]
fn emit_open_line_for_e2e() {
    let raw = std::env::var("CCM_E2E_OPEN")
        .expect("缺 CCM_E2E_OPEN —— 本出口只给 tests/e2e/open-line-emit.sh 用");
    let e: E2eOpen =
        serde_json::from_str(&raw).unwrap_or_else(|err| panic!("意图解析不了（{err}）：{raw}"));
    let named = e
        .account
        .as_ref()
        .and_then(|a| a["name"].as_str())
        .map(str::to_string);
    let name = e.name.clone().unwrap_or_default();
    let sid = e.sid.clone().unwrap_or_default();
    let mut rig = Rig::new(Some(vec![]));
    rig.e2e = true;
    rig.names = named.into_iter().collect();
    rig.cwd = e.cwd.clone().unwrap_or_default();
    rig.model = e.model.clone();
    rig.minted = e.name.clone();
    rig.prefs
        .launchers
        .insert("claude".into(), e.launcher.clone());
    let (what, run) = match e.shape.as_str() {
        "resume-tmux" => (json!({ "kind": "resume", "sid": sid }), Some("tmux")),
        "direct" => (json!({ "kind": "resume", "sid": sid }), Some("direct")),
        "into-existing" => {
            rig.rows = Some(vec![row(&name, Some(&sid), false)]);
            (json!({ "kind": "resume", "sid": sid }), Some("tmux"))
        }
        "attach" => {
            rig.rows = Some(vec![row(&name, None, false)]);
            (json!({ "kind": "look", "session": name }), None)
        }
        "new-tmux" => {
            let mut w =
                json!({ "kind": "new", "agent": "claude", "cwd": rig.cwd, "tmuxName": name });
            if let Some(m) = &e.model {
                w["model"] = json!(m);
            }
            (w, Some("tmux"))
        }
        other => panic!("不认识的 shape `{other}`"),
    };
    let mut req = json!({ "what": what, "local": e.here == "local" });
    if let Some(r) = run {
        req["run"] = json!(r);
    }
    if let Some(a) = &e.account {
        req["account"] = a.clone();
    }
    let said = match rig.open(req) {
        Err((code, said, _, raw)) => Err(format!("{code}: {said} {}", raw.unwrap_or_default())),
        Ok(o) => {
            let argv = rig.ccm.borrow().first().cloned();
            let typed = rig.sent.borrow().first().map(|(_, l)| l.clone());
            Ok(match (argv, typed) {
                (Some(argv), _) => argv
                    .iter()
                    .map(|t| crate::control::launch_render::ccm_invocation::argv(t))
                    .collect::<Vec<_>>()
                    .join(" "),
                (None, Some(line)) => line,
                (None, None) => o.line.unwrap_or_default(),
            })
        }
    };
    match said {
        Ok(line) => {
            assert!(
                !line.contains('\n'),
                "那一行带换行，标记行会被截断：{line:?}"
            );
            println!("OPEN_LINE<<<{line}>>>");
        }
        Err(e) => println!("OPEN_LINE_ERR<<<{}>>>", e.replace('\n', " ")),
    }
}

/// ★ 每一种起法起之前都把工作目录标成要用的那个号信任过、恰好一次：恢复进新建的 tmux · 键进空 tmux · 直路 · 新起。
#[test]
fn every_start_marks_the_cwd_trusted_in_its_account_once() {
    for (rows, mut req) in [
        (Some(vec![]), resume(Some("tmux"))),
        (
            Some(vec![row("idle", Some(A), false)]),
            resume(Some("tmux")),
        ),
        (Some(vec![]), resume(Some("direct"))),
        (Some(vec![]), new(json!({}))),
    ] {
        req["account"] = json!({ "kind": "named", "name": "work" });
        let rig = Rig::new(rows);
        rig.open(req.clone())
            .unwrap_or_else(|e| panic!("{req} 没起：{e:?}"));
        assert_eq!(*rig.marks.borrow(), 1, "{req}");
    }
}

/// 一个 tmux 会话里两个窗格：别的会话在跑的那个不碰，空着、挂着这一条的那个就地键入（按挂着它的那个窗格认，不按会话名）。
#[test]
fn an_idle_pane_in_a_shared_session_is_typed_into_in_place() {
    let other = "bbbbbbbb-1111-2222-3333-444444444444";
    let rig = Rig::new(Some(vec![
        row("two-cc", Some(other), true),
        row("two-cc", Some(A), false),
    ]));
    let got = rig.open(resume(Some("tmux"))).unwrap();
    assert_eq!(
        (got.landed, got.session.as_deref()),
        (Landed::InPlace, Some("two-cc"))
    );
    assert_eq!(rig.sent.borrow().len(), 1);
    assert!(rig.ccm.borrow().is_empty(), "不该另建一个会话");
}

/// ★ **F1** 新建框默认「运行于」只有一处：`session-open-facts` 的 `run` == `session-open {what: new}` 不给 `run` 时真落的那一形。
/// 全表两向：这台有没有 tmux × 是不是分叉 × 源会话在不在 tmux（说不出也算一格）。
#[test]
fn the_facts_run_is_where_a_new_session_without_run_lands() {
    for tmux in [true, false] {
        for fork_tmux in [None, Some(true), Some(false)] {
            for forked in [false, true] {
                if !forked && fork_tmux.is_some() {
                    continue;
                }
                let mut rig = Rig::new(tmux.then(Vec::new));
                rig.fork_tmux = fork_tmux;
                let extra = if forked {
                    json!({ "fork": { "sid": A, "uuid": U } })
                } else {
                    json!({})
                };
                let got = rig
                    .open(new(extra))
                    .unwrap_or_else(|e| panic!("没起：{e:?}"));
                let landed = match crate::faces::session_open_face::facts_run(fork_tmux, tmux) {
                    Run::Tmux => Landed::Started,
                    Run::Direct => Landed::Window,
                };
                assert_eq!(
                    got.landed, landed,
                    "tmux {tmux} · 分叉 {forked} · 源在 tmux {fork_tmux:?}"
                );
            }
        }
    }
}

/// ★ **P1** 新会话的权限档：请求给了照给的；没给 ⇒ 这台偏好里那一家的那一档；偏好里那一档那一家已不认 ⇒ 不带。
/// 恢复不读偏好（正控：设了偏好也不带，照记录里最后那一档）。
#[test]
fn a_new_session_takes_this_machines_permission_mode_unless_asked_and_resume_never_does() {
    let flag_of = |argv: &[String]| -> Option<String> {
        argv.windows(2)
            .find(|w| w[0] == "--permission-mode")
            .map(|w| w[1].clone())
    };
    for (pref, asked, resume, want) in [
        (None, None, false, None),
        (Some("plan"), None, false, Some("plan")),
        (
            Some("plan"),
            Some("acceptEdits"),
            false,
            Some("acceptEdits"),
        ),
        (None, Some("acceptEdits"), false, Some("acceptEdits")),
        (Some("yolo"), None, false, None),
        (Some("plan"), None, true, None),
    ] {
        let mut rig = Rig::new(Some(vec![]));
        if let Some(m) = pref {
            rig.prefs.permission_modes.insert("claude".into(), m.into());
        }
        let mut req = if resume {
            resume_req_tmux()
        } else {
            new(asked.map_or(json!({}), |m| json!({ "permissionMode": m })))
        };
        req["run"] = json!("tmux");
        rig.open(req.clone())
            .unwrap_or_else(|e| panic!("{req} 没起：{e:?}"));
        let argv = rig.ccm.borrow()[0].clone();
        assert_eq!(
            flag_of(&argv).as_deref(),
            want,
            "偏好 {pref:?} · 请求 {asked:?} · 恢复 {resume}：{argv:?}"
        );
    }
}

fn resume_req_tmux() -> Value {
    resume(Some("tmux"))
}

/// ★ 起了一家 ⇒ 回包带等它报到那一问（有 sid 按 sid，没有按目录；抓屏按落在的那个 tmux 会话）；在跑 · 接回 · 进目录 ⇒ 不等。
#[test]
fn what_was_started_comes_back_with_its_arrival_and_nothing_else_does() {
    let by_sid = |s: &str| Some(ArriveBy::Sid { sid: s.into() });
    let rig = Rig::new(Some(vec![]));
    let o = rig.open(resume(Some("tmux"))).unwrap();
    let a = o.arrival.expect("恢复进 tmux ⇒ 等它报到");
    assert_eq!(
        (Some(a.by), a.since, a.session.as_deref()),
        (by_sid(A), 0, Some("p-cc"))
    );
    let o = rig.open(resume(Some("direct"))).unwrap();
    let a = o.arrival.expect("直路恢复：开窗那一行跑起来才起，照样等");
    assert_eq!((Some(a.by), a.session), (by_sid(A), None));
    let o = rig.open(new(json!({}))).unwrap();
    assert_eq!(
        o.arrival.map(|a| a.by),
        by_sid(U),
        "先定好 sid 的新起按 sid"
    );

    let rig = Rig::new(Some(vec![
        row("live", Some(A), true),
        row("idle", None, false),
    ]));
    assert_eq!(
        rig.open(resume(Some("tmux"))).unwrap().arrival,
        None,
        "在跑 ⇒ 不等"
    );
    let looked = rig
        .open(json!({ "what": { "kind": "look", "session": "idle" } }))
        .unwrap();
    assert_eq!(looked.arrival, None, "接回 ⇒ 不等");

    let by_cwd = with_arrival(
        Opened {
            sid: None,
            cwd: Some("/w".into()),
            agent: Some(crate::agents::AgentKind::new("codex")),
            ..opened(Landed::Started, Some("w".into()), None)
        },
        3,
    );
    assert_eq!(
        by_cwd.arrival.map(|a| (a.by, a.since)),
        Some((ArriveBy::Cwd { cwd: "/w".into() }, 3))
    );
    let shell = with_arrival(
        Opened {
            cwd: Some("/w".into()),
            ..opened(Landed::Window, None, Some("cd /w".into()))
        },
        3,
    );
    assert_eq!(shell.arrival, None, "进目录那一行没起 agent ⇒ 不等");
}
