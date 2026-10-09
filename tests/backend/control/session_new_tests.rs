//! 起新会话一个请求：每一格不行时说是哪一格、什么都不起不写；都过了才写分支记录、才起（替身记下被调了什么；tmux 一个都不起）。

use super::*;
use crate::control::session_batch::TmuxEntry;
use std::cell::RefCell;
use std::collections::BTreeSet;

const SRC: &str = "aaaaaaaa-1111-2222-3333-444444444444";
const MSG: &str = "bbbbbbbb-1111-2222-3333-444444444444";
const NEW: &str = "cccccccc-1111-2222-3333-444444444444";
const ENTRY: &str = "/h/.cc-monitor/bin/ccm";
/// 选了规则起时这台先定好的 sid（替身铸的）。
const PRESET: &str = "dddddddd-1111-4222-8333-444444444444";

fn row(name: &str, sid: Option<&str>, agent: bool) -> TmuxEntry {
    TmuxEntry {
        name: name.into(),
        terminal: format!("tmux-{name}"),
        sid: sid.map(str::to_string),
        agent,
    }
}

struct Rig {
    rows: Option<Vec<TmuxEntry>>,
    ccm_rc: i32,
    ccm: RefCell<Vec<Vec<String>>>,
    mints: RefCell<Vec<String>>,
    forks: RefCell<Vec<(String, String)>>,
    /// 号 `work` 能用；`gone` 选不了（替代 ＝ `work`，这台的默认号）。
    last: Option<&'static str>,
    /// 预标信任那几下：`<号目录> <工作目录> @<此前已交的 ccm 数>`。
    marks: RefCell<Vec<String>>,
    /// 起之前写的轮换来源：`<sid> <哪一家> <规则> @<此前已交的 ccm 数>`；撤掉的：`forget <sid>`。
    rot: RefCell<Vec<String>>,
}

impl Rig {
    fn new(rows: Option<Vec<TmuxEntry>>) -> Self {
        Self {
            rows,
            ccm_rc: 0,
            ccm: RefCell::new(vec![]),
            mints: RefCell::new(vec![]),
            forks: RefCell::new(vec![]),
            last: None,
            marks: RefCell::new(vec![]),
            rot: RefCell::new(vec![]),
        }
    }

    fn call(&self, args: Value) -> Result<Value, Failed> {
        let caps: BTreeSet<String> = crate::ccm_launcher_with(crate::TMUX_PLATFORM)
            .into_iter()
            .map(str::to_string)
            .collect();
        let list = || -> Result<Option<Vec<TmuxEntry>>, String> { Ok(self.rows.clone()) };
        let record = |_: &str, _: Option<&str>| -> Result<(bool, String), String> {
            Ok((true, String::new()))
        };
        let kill = |_: &str, _: &str| -> Result<Value, crate::control::session_batch::CmdErr> {
            unreachable!("起新会话不杀")
        };
        let send_into =
            |_: &str, _: &str, _: &str| -> Result<(), crate::control::session_batch::CmdErr> {
                unreachable!("起新会话不往已有的终端里键入")
            };
        let run_ccm = |argv: &[String]| -> Result<(i32, String, String), crate::control::session_batch::CmdErr> {
            self.ccm.borrow_mut().push(argv.to_vec());
            Ok((self.ccm_rc, String::new(), "boom".into()))
        };
        let mint = |base: NameBase| -> Result<String, crate::control::session_batch::CmdErr> {
            Ok(match base {
                NameBase::Cwd(cwd) => {
                    self.mints.borrow_mut().push(format!("cwd {cwd}"));
                    "proj-cc-2".into()
                }
                NameBase::ForkOf(src) => {
                    self.mints.borrow_mut().push(format!("fork {src}"));
                    crate::control::ccm::plan::fork_tmux_base(src)
                }
            })
        };
        let writers = |_: &str| -> Vec<u32> { vec![] };
        let fork = |sid: &str, uuid: &str| -> Result<String, (&'static str, String)> {
            self.forks.borrow_mut().push((sid.into(), uuid.into()));
            Ok(NEW.into())
        };
        let library = || {
            la::Library::of_product(&json!({ "meta": {"enabled": true}, "accounts": [
                { "name": "work", "configDir": "/h/.cc/work", "isDefault": true, "mode": "isolated", "exists": true, "authReady": true },
                { "name": "gone", "configDir": "/h/.cc/gone", "isDefault": false, "mode": "isolated", "exists": false, "authReady": false },
            ]}))
        };
        let last_of = |_: &str| self.last.map(str::to_string);
        let accounts = la::Facts {
            has_accounts: true,
            library: &library,
            last: &last_of,
        };
        let pretrust = |dir: &str, cwd: &str| {
            let at = self.ccm.borrow().len();
            self.marks.borrow_mut().push(format!("{dir} {cwd} @{at}"));
        };
        let deps = Deps {
            list: &list,
            record: &record,
            kill: &kill,
            send_into: &send_into,
            run_ccm: &run_ccm,
            mint: &mint,
            caps: &caps,
            local_facts: local::Facts {
                windows: false,
                is_dir: |p| p != "/gone" && p != "/h/gone",
                entry: || Some(ENTRY.to_string()),
            },
            accounts: &accounts,
            writers: &writers,
            pretrust: &pretrust,
        };
        let sid = || PRESET.to_string();
        let write = |sid: &str, kind: &str, rule: &str| -> Result<(), (&'static str, String)> {
            if rule == "r_gone" {
                return Err(("no_such_rule", "规则不在".into()));
            }
            let at = self.ccm.borrow().len();
            self.rot
                .borrow_mut()
                .push(format!("{sid} {kind} {rule} @{at}"));
            Ok(())
        };
        let forget = |sid: &str| self.rot.borrow_mut().push(format!("forget {sid}"));
        let pre = PreRotation {
            sid: &sid,
            write: &write,
            forget: &forget,
        };
        answer(&args, &deps, &fork, &pre, Some(std::path::Path::new("/h")))
    }
}

fn req(extra: Value) -> Value {
    let mut v = json!({ "agent": "claude", "cwd": "/srv/proj", "place": "tmux", "local": false });
    for (k, x) in extra.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}

fn field_of(e: &Failed) -> Value {
    e.2.as_ref().map_or(Value::Null, |d| d["field"].clone())
}

#[test]
fn a_new_session_in_tmux_is_minted_from_the_directory_and_started_detached() {
    let rig = Rig::new(Some(vec![row("proj-cc", None, true)]));
    let out = rig.call(req(json!({}))).unwrap();
    assert_eq!(out["outcome"], "started");
    assert_eq!(out["session"], "proj-cc-2");
    assert_eq!(out["sid"], Value::Null);
    assert_eq!(out["agent"], "claude");
    assert_eq!(out["cwd"], "/srv/proj");
    assert_eq!(
        out["account"]["name"], "work",
        "跟随、没有上次的号 ⇒ 这台的默认号"
    );
    assert_eq!(*rig.mints.borrow(), vec!["cwd /srv/proj".to_string()]);
    let argv = rig.ccm.borrow()[0].clone();
    assert!(
        argv.contains(&"--ccm-tmux=proj-cc-2".to_string()),
        "{argv:?}"
    );
    assert!(argv.contains(&"--detach".to_string()), "{argv:?}");
    assert!(
        argv.windows(2).any(|w| w == ["--cwd", "/srv/proj"]),
        "{argv:?}"
    );
    assert!(
        !argv.iter().any(|a| a == "--resume"),
        "新起的不 resume：{argv:?}"
    );
    assert!(rig.forks.borrow().is_empty());
}

#[test]
fn every_slot_that_fails_names_itself_and_nothing_is_started_or_written() {
    let cases: Vec<(Value, Option<Vec<TmuxEntry>>, &str, Value)> = vec![
        (
            json!({ "agent": "" }),
            Some(vec![]),
            "unknown_agent",
            json!("agent"),
        ),
        (
            json!({ "agent": "nope" }),
            Some(vec![]),
            "unknown_agent",
            json!("agent"),
        ),
        (
            json!({ "command": "claude; rm -rf /" }),
            Some(vec![]),
            "bad_command",
            json!("command"),
        ),
        (
            json!({ "cwd": "/gone" }),
            Some(vec![]),
            "no_dir",
            json!("cwd"),
        ),
        (
            json!({ "cwd": "~/gone" }),
            Some(vec![]),
            "no_dir",
            json!("cwd"),
        ),
        (
            json!({ "account": {"kind": "named", "name": "gone"} }),
            Some(vec![]),
            "account_unavailable",
            json!("account"),
        ),
        (json!({}), None, "place_unavailable", json!("place")),
        (
            json!({ "tmuxName": "a:b" }),
            Some(vec![]),
            "bad_tmux_name",
            json!("tmuxName"),
        ),
        (
            json!({ "tmuxName": "mine" }),
            Some(vec![row("mine", None, false)]),
            "tmux_taken",
            json!("tmuxName"),
        ),
    ];
    for (extra, rows, code, field) in cases {
        let mut extra = extra;
        extra["forkFrom"] = if extra.get("tmuxName").is_some() {
            Value::Null
        } else {
            json!({ "sid": SRC, "uuid": MSG })
        };
        if extra["forkFrom"].is_null() {
            extra.as_object_mut().unwrap().remove("forkFrom");
        }
        let rig = Rig::new(rows);
        let e = rig.call(req(extra.clone())).unwrap_err();
        assert_eq!(e.0, code, "{extra}");
        assert_eq!(field_of(&e), field, "{extra}");
        assert!(rig.ccm.borrow().is_empty(), "{extra}：不许起");
        assert!(
            rig.forks.borrow().is_empty(),
            "{extra}：某一格不行就不写分支记录"
        );
    }
}

#[test]
fn an_unavailable_account_carries_the_alternative_and_is_never_swapped_silently() {
    let mut rig = Rig::new(Some(vec![]));
    rig.last = Some("gone");
    let e = rig
        .call(req(json!({ "forkFrom": { "sid": SRC, "uuid": MSG } })))
        .unwrap_err();
    assert_eq!(e.0, "account_unavailable");
    let d = e.2.unwrap();
    assert_eq!(d["field"], "account");
    assert_eq!(d["unavailable"]["requested"], "gone");
    assert_eq!(d["unavailable"]["pinned"], true, "跟随的是源会话上次那个号");
    assert_eq!(d["unavailable"]["alternative"], "work");
    assert!(rig.ccm.borrow().is_empty());
}

#[test]
fn a_fork_writes_its_record_only_after_every_slot_passed_and_resumes_the_new_session() {
    let rig = Rig::new(Some(vec![row("orders-cc", Some(SRC), true)]));
    let out = rig
        .call(req(json!({ "forkFrom": { "sid": SRC, "uuid": MSG } })))
        .unwrap();
    assert_eq!(
        *rig.forks.borrow(),
        vec![(SRC.to_string(), MSG.to_string())]
    );
    assert_eq!(out["sid"], NEW);
    assert_eq!(out["outcome"], "started");
    assert_eq!(
        out["session"], "orders-fork-cc",
        "新名从源会话此刻的终端名铸，绝不复用它"
    );
    let argv = rig.ccm.borrow()[0].clone();
    assert!(argv.windows(2).any(|w| w == ["--resume", NEW]), "{argv:?}");
    assert!(
        argv.iter()
            .any(|a| a.contains(NEW) && a.starts_with("--ccm-sid"))
            || argv.windows(2).any(|w| w[0] == "--ccm-sid" && w[1] == NEW),
        "身份标记打上新 sid：{argv:?}"
    );
}

#[test]
fn a_fork_does_not_take_a_tmux_name() {
    let rig = Rig::new(Some(vec![]));
    let e = rig
        .call(req(
            json!({ "tmuxName": "x", "forkFrom": { "sid": SRC, "uuid": MSG } }),
        ))
        .unwrap_err();
    assert_eq!(e.0, "bad_args");
    assert!(rig.forks.borrow().is_empty());
}

#[test]
fn a_taken_name_at_start_time_is_reported_on_the_name_slot() {
    let mut rig = Rig::new(Some(vec![]));
    rig.ccm_rc = 3;
    let e = rig.call(req(json!({}))).unwrap_err();
    assert_eq!((e.0, field_of(&e)), ("tmux_taken", json!("tmuxName")));
    rig.ccm_rc = 1;
    let e = rig.call(req(json!({}))).unwrap_err();
    assert_eq!(
        (e.0, field_of(&e), e.1.as_str()),
        ("start_failed", Value::Null, "boom")
    );
}

#[test]
fn a_window_launch_hands_back_the_line_and_starts_nothing() {
    let rig = Rig::new(None);
    let out = rig.call(req(json!({ "place": "window" }))).unwrap();
    assert_eq!(out["outcome"], "open");
    assert_eq!(out["session"], Value::Null);
    let cmd = out["cmd"].as_str().unwrap();
    assert!(cmd.contains(ENTRY) && cmd.contains("--cwd"), "{cmd}");
    assert!(!cmd.contains("--ccm-tmux"), "开窗那一形不进 tmux：{cmd}");
    assert!(rig.ccm.borrow().is_empty() && rig.mints.borrow().is_empty());
    let out = rig
        .call(req(
            json!({ "place": "window", "local": true, "command": "claude --verbose", "cwd": "~/srv" }),
        ))
        .unwrap();
    assert_eq!(
        out["cwd"], "/h/srv",
        "报回来的是展开过的目录（界面按它认报到的会话）"
    );
    let cmd = out["cmd"].as_str().unwrap();
    assert!(
        cmd.contains("claude --verbose") && !cmd.contains("--ccm-tmux"),
        "{cmd}"
    );
}

#[test]
fn unknown_fields_are_refused() {
    let rig = Rig::new(Some(vec![]));
    assert_eq!(
        rig.call(req(json!({ "machine": "devbox" }))).unwrap_err().0,
        "bad_args"
    );
}

#[test]
fn a_tilde_directory_is_read_under_this_machines_home() {
    let h = Some(std::path::Path::new("/h"));
    assert_eq!(expand_home("~/srv/x", h), "/h/srv/x");
    assert_eq!(expand_home("~", h), "/h");
    assert_eq!(expand_home("~bob/x", h), "~bob/x");
    assert_eq!(expand_home("/srv", h), "/srv");
    assert_eq!(expand_home("~/x", None), "~/x");
}

#[test]
fn the_directory_question_answers_presence_and_the_name_this_machine_would_mint() {
    let caps = BTreeSet::new();
    let none = || -> Result<Option<Vec<TmuxEntry>>, String> { Ok(None) };
    let some = || -> Result<Option<Vec<TmuxEntry>>, String> {
        Ok(Some(vec![row("orders-cc", Some(SRC), true)]))
    };
    let mint = |b: NameBase| -> Result<String, crate::control::session_batch::CmdErr> {
        Ok(match b {
            NameBase::Cwd(c) => format!("cwd:{c}"),
            NameBase::ForkOf(s) => format!("fork:{s}"),
        })
    };
    let no = |_: &str, _: Option<&str>| -> Result<(bool, String), String> { unreachable!() };
    let library = || la::Library::of_product(&json!({}));
    let accounts = la::Facts {
        has_accounts: false,
        library: &library,
        last: &|_| None,
    };
    let run = |list: &dyn Fn() -> Result<Option<Vec<TmuxEntry>>, String>, args: Value| {
        let deps = Deps {
            list,
            record: &no,
            kill: &|_, _| unreachable!(),
            send_into: &|_, _, _| unreachable!(),
            run_ccm: &|_| unreachable!(),
            mint: &mint,
            caps: &caps,
            local_facts: local::Facts {
                windows: false,
                is_dir: |p| p == "/h/srv",
                entry: || None,
            },
            accounts: &accounts,
            writers: &|_| vec![],
            pretrust: &|_, _| unreachable!("核目录不起会话"),
        };
        dir_answer(&args, &deps, Some(std::path::Path::new("/h")))
    };
    assert_eq!(
        run(&some, json!({ "cwd": "~/srv" })).unwrap(),
        json!({ "exists": true, "tmuxName": "cwd:/h/srv" })
    );
    assert_eq!(
        run(&none, json!({ "cwd": "/x" })).unwrap(),
        json!({ "exists": false, "tmuxName": null })
    );
    assert_eq!(
        run(&some, json!({ "cwd": "/h/srv", "forkOf": SRC })).unwrap()["tmuxName"],
        "fork:orders-cc"
    );
    assert_eq!(
        run(&some, json!({ "cwd": "/x", "extra": 1 }))
            .unwrap_err()
            .0,
        "bad_args"
    );
}

/// 起之前预标信任：三种起法（tmux · 本机开窗 · 远端开窗）都在起 / 交回那一行之前把工作目录标进要用的那个号；
/// 某一格不行（什么都不起）⇒ 不标；账号 0 ⇒ 不标（那一份是用户主配置）。
#[test]
fn a_new_session_marks_its_cwd_trusted_in_its_account_before_it_starts() {
    let named = json!({ "account": { "kind": "named", "name": "work" } });
    let rig = Rig::new(Some(vec![]));
    rig.call(req(named.clone())).unwrap();
    assert_eq!(
        *rig.marks.borrow(),
        vec!["/h/.cc/work /srv/proj @0".to_string()]
    );
    assert_eq!(rig.ccm.borrow().len(), 1);

    for local in [true, false] {
        let rig = Rig::new(Some(vec![]));
        let mut a = req(named.clone());
        a["place"] = json!("window");
        a["local"] = json!(local);
        rig.call(a).unwrap();
        assert_eq!(
            *rig.marks.borrow(),
            vec!["/h/.cc/work /srv/proj @0".to_string()],
            "开窗 local={local}"
        );
    }

    let rig = Rig::new(Some(vec![]));
    rig.call(req(json!({ "account": { "kind": "base" } })))
        .unwrap();
    assert!(rig.marks.borrow().is_empty(), "账号 0 不标");

    let rig = Rig::new(Some(vec![]));
    assert!(rig
        .call(req(
            json!({ "account": { "kind": "named", "name": "work" }, "cwd": "/gone" })
        ))
        .is_err());
    assert!(rig.marks.borrow().is_empty(), "目录不在 ⇒ 什么都不起、不标");
}

/// ★ 选了规则起（`rotation: {rule}`）：起之前这台先定好 sid（那一家起新会话时认的 `--session-id`）、把那个会话的来源写成那条规则，再起；
/// 回包带这个 sid（报到按 sid 认），tmux 那一形身份标记也是它。界面不再补写。
#[test]
fn a_rule_at_launch_is_written_under_a_preset_sid_before_starting() {
    let rig = Rig::new(Some(vec![]));
    let got = rig
        .call(req(json!({"rotation": {"rule": "r_night"}})))
        .expect("ok");
    assert_eq!(got["sid"], PRESET, "{got}");
    assert_eq!(
        *rig.rot.borrow(),
        vec![format!("{PRESET} claude r_night @0")],
        "起之前写好"
    );
    let argv = rig.ccm.borrow()[0].clone();
    let sep = argv.iter().position(|a| a == "--").expect("--");
    assert!(
        argv[..sep]
            .windows(2)
            .any(|w| w[0] == "--session-id" && w[1] == PRESET),
        "交给那一家的那一串里带 --session-id：{argv:?}"
    );
    assert!(
        argv[sep..].iter().any(|a| a.contains(PRESET)),
        "身份标记：{argv:?}"
    );
}

/// 跟随默认（或不给）⇒ 不定 sid、不写（新会话本来就跟随默认）；开窗那一形同样带 --session-id。
#[test]
fn follow_at_launch_writes_nothing_and_a_window_launch_carries_the_sid_too() {
    let rig = Rig::new(Some(vec![]));
    let got = rig.call(req(json!({"rotation": "follow"}))).expect("ok");
    assert_eq!(got["sid"], Value::Null);
    assert!(rig.rot.borrow().is_empty());
    assert!(!rig.ccm.borrow()[0].iter().any(|a| a == "--session-id"));
    let rig = Rig::new(Some(vec![]));
    let got = rig
        .call(req(
            json!({"place": "window", "rotation": {"rule": "r_night"}}),
        ))
        .expect("ok");
    assert_eq!(got["sid"], PRESET);
    assert!(
        got["cmd"]
            .as_str()
            .expect("cmd")
            .contains(&format!("--session-id {PRESET}")),
        "{got}"
    );
}

/// 规则不在 ⇒ `no_such_rule`，什么都不起；起不成（ccm 非 0）⇒ 撤掉起之前写的那一条；形状不对 ⇒ `bad_args`。
#[test]
fn a_rule_at_launch_that_fails_leaves_nothing_behind() {
    let rig = Rig::new(Some(vec![]));
    let e = rig
        .call(req(json!({"rotation": {"rule": "r_gone"}})))
        .expect_err("不在");
    assert_eq!(e.0, "no_such_rule");
    assert!(rig.ccm.borrow().is_empty(), "规则不在就不起");
    let mut rig = Rig::new(Some(vec![]));
    rig.ccm_rc = 1;
    let e = rig
        .call(req(json!({"rotation": {"rule": "r_night"}})))
        .expect_err("起不成");
    assert_eq!(e.0, "start_failed");
    assert_eq!(
        rig.rot.borrow().last().map(String::as_str),
        Some(format!("forget {PRESET}").as_str())
    );
    let rig = Rig::new(Some(vec![]));
    assert_eq!(
        rig.call(req(json!({"rotation": 3}))).expect_err("形状").0,
        "bad_args"
    );
}
