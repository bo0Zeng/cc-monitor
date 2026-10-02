//! 批量停 / 起：每一个的结局与「单个那一条会做的事」相等（替身记下被调了什么；tmux 一个都不起）。

use super::*;
use std::cell::RefCell;

const A: &str = "aaaaaaaa-1111-2222-3333-444444444444";
const B: &str = "bbbbbbbb-1111-2222-3333-444444444444";
const C: &str = "cccccccc-1111-2222-3333-444444444444";

fn row(name: &str, sid: Option<&str>, agent: bool) -> TmuxEntry {
    TmuxEntry {
        name: name.into(),
        sid: sid.map(str::to_string),
        agent,
    }
}

fn caps() -> BTreeSet<String> {
    crate::ccm_launcher_with(crate::TMUX_PLATFORM)
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// 替身那一套：名单与记录是给定的，杀 / 建 / 铸只记下来。
struct Rig {
    rows: Option<Vec<TmuxEntry>>,
    gone: Vec<&'static str>,
    calls: RefCell<Vec<String>>,
    launched: RefCell<Vec<Value>>,
    ccm: RefCell<Vec<Vec<String>>>,
    created: bool,
    kill_err: Option<&'static str>,
}

impl Rig {
    fn new(rows: Option<Vec<TmuxEntry>>) -> Self {
        Self {
            rows,
            gone: vec![],
            calls: RefCell::new(vec![]),
            launched: RefCell::new(vec![]),
            ccm: RefCell::new(vec![]),
            created: true,
            kill_err: None,
        }
    }
    fn run<T>(&self, f: impl FnOnce(&Deps) -> T) -> T {
        let caps = caps();
        let list = || -> Result<Option<Vec<TmuxEntry>>, String> { Ok(self.rows.clone()) };
        let record = |sid: &str, dir: Option<&str>| -> Result<(bool, String), String> {
            self.calls
                .borrow_mut()
                .push(format!("record {sid} {dir:?}"));
            Ok((!self.gone.contains(&sid), "/root/x".to_string()))
        };
        let kill = |name: &str, sid: &str| -> Result<Value, CmdErr> {
            self.calls.borrow_mut().push(format!("kill {name} {sid}"));
            match self.kill_err {
                Some(c) => Err((c, "said".to_string())),
                None => Ok(json!({ "removed": [], "failed": [], "unread": null })),
            }
        };
        let send_into = |name: &str, line: &str| -> Result<(), CmdErr> {
            self.launched
                .borrow_mut()
                .push(json!({ "mode": "send-into", "name": name, "payload": line }));
            Ok(())
        };
        // 真 ccm：建会话撞名 ⇒ 退出码 3（响亮失败）。
        let run_ccm = |argv: &[String]| -> Result<(i32, String, String), String> {
            self.ccm.borrow_mut().push(argv.to_vec());
            Ok((
                if self.created { 0 } else { 3 },
                String::new(),
                String::new(),
            ))
        };
        let mint = |cwd: &str| -> Result<String, CmdErr> {
            self.calls.borrow_mut().push(format!("mint {cwd}"));
            Ok("proj-cc-2".to_string())
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
                is_dir: |_| true,
            },
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
        assert_eq!(e.0, "invalid_args", "{bad}");
    }
    assert!(rig.calls.borrow().is_empty());
}

fn item(sid: &str, account: Value) -> Value {
    json!({ "sid": sid, "cwd": "/w/proj", "account": account, "model": null, "launcher": "claude", "defaultLauncher": "claude" })
}

/// 单个那一条那台后端会渲出的那一行（`launch-render-cli`，直连、不建容器）。
fn single_line(sid: &str, cwd: Option<&str>, account: Value) -> String {
    let req = json!({
        "action": { "kind": "resume", "sid": sid },
        "container": { "kind": "none" },
        "cwd": cwd,
        "account": account,
        "ccmSid": null,
        "model": null,
        "launcher": "claude",
        "defaultLauncher": "claude",
        "rbindToken": null,
    });
    super::super::launch_render::answer_cli(&req).unwrap()["cmd"]
        .as_str()
        .unwrap()
        .to_string()
}

/// 单个「在 tmux 里 Resume」那一行（`launch-render-cli`，建进 tmux、打 sid 标记、带 cwd）。
fn single_tmux_line(sid: &str, name: &str, account: Value) -> String {
    let req = json!({
        "action": { "kind": "resume", "sid": sid },
        "container": { "kind": "tmux", "name": name, "send_into": false },
        "cwd": "/w/proj",
        "account": account,
        "ccmSid": sid,
        "model": null,
        "launcher": "claude",
        "defaultLauncher": "claude",
        "rbindToken": null,
    });
    super::super::launch_render::answer_cli(&req).unwrap()["cmd"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn the_standing_of_a_sid_is_judged_once_for_menu_stop_and_start() {
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
            { "sid": A, "standing": "running", "names": ["a-cc"] },
            { "sid": B, "standing": "idle", "names": ["b-cc"] },
            { "sid": C, "standing": "ambiguous", "names": ["c-cc", "c-cc-2"] },
        ] })
    );
    let none = Rig::new(None);
    assert_eq!(
        none.run(|d| where_(&json!({ "sids": [A] }), d)).unwrap()["results"][0]["standing"],
        "no_tmux"
    );
}

#[test]
fn start_in_tmux_is_the_single_tmux_item_without_attaching() {
    let acct = json!({ "kind": "named", "name": "work", "configDir": "/h/.cc/work" });
    let mut rig = Rig::new(Some(vec![
        row("a-cc", Some(A), true),  // 在跑 ⇒ 不另起
        row("b-cc", Some(B), false), // 空 tmux ⇒ 就地键入
    ]));
    rig.gone = vec![];
    let args = json!({ "mode": "tmux", "local": false, "items": [item(A, acct.clone()), item(B, acct.clone()), item(C, acct.clone())] });
    let out = rig.run(|d| start(&args, d)).unwrap();
    assert_eq!(
        outcomes(&out),
        vec![
            (A.into(), "skipped".into(), json!("running")),
            (B.into(), "done".into(), Value::Null),
            (C.into(), "done".into(), Value::Null),
        ]
    );
    let remote = json!({ "kind": "account", "name": "work", "configDir": "/h/.cc/work" });
    // 空 tmux：键进去的是单个就地 resume 那一行（直路、不带 cwd）。
    assert_eq!(
        *rig.launched.borrow(),
        vec![
            json!({ "mode": "send-into", "name": "b-cc", "payload": single_line(B, None, remote.clone()) })
        ]
    );
    // 都不是：交的是单个「在 tmux 里 Resume」那一行（界面经 `launch-render-cli` 拿到的同一行），只多一个 `--detach`。
    let single = single_tmux_line(C, "proj-cc-2", remote);
    let mut want: Vec<String> = single.split(' ').map(str::to_string).collect();
    let at = want
        .iter()
        .position(|t| t == "--ccm-tmux=proj-cc-2")
        .unwrap()
        + 1;
    want.insert(at, "--detach".to_string());
    assert_eq!(*rig.ccm.borrow(), vec![want]);
    // 每一个起之前都按这次要用的账号根问过记录。
    let calls = rig.calls.borrow();
    for sid in [A, B, C] {
        assert!(
            calls.contains(&format!("record {sid} Some(\"/h/.cc/work\")")),
            "{calls:?}"
        );
    }
    assert_eq!(out["results"][2]["session"], "proj-cc-2");
}

#[test]
fn start_skips_what_has_no_record_and_says_a_taken_name() {
    let mut rig = Rig::new(Some(vec![]));
    rig.gone = vec![A];
    rig.created = false;
    let args = json!({ "mode": "tmux", "local": true, "items": [item(A, json!({ "kind": "inherit" })), item(B, json!({ "kind": "base" }))] });
    let out = rig.run(|d| start(&args, d)).unwrap();
    assert_eq!(
        outcomes(&out),
        vec![
            (A.into(), "skipped".into(), json!("record_gone")),
            (B.into(), "failed".into(), json!("name_taken")),
        ]
    );
    assert_eq!(out["results"][0]["detail"], "/root/x");
    assert_eq!(rig.ccm.borrow().len(), 1, "记录没了的那一个不许起");
    // 本机：交的是 `launch-local` 那一行（同一份映射），只多 `--detach`。
    assert!(rig.ccm.borrow()[0].contains(&"--detach".to_string()));
    let none = Rig::new(None);
    let out = none.run(|d| start(&args, d)).unwrap();
    assert_eq!(out["results"][1]["why"], "no_tmux");
}

#[test]
fn a_window_start_renders_what_the_single_item_renders() {
    // 远端：同 `launch-render-cli` 直连那一行。
    let rig = Rig::new(None);
    let args =
        json!({ "mode": "window", "local": false, "items": [item(A, json!({ "kind": "base" }))] });
    let out = rig.run(|d| start(&args, d)).unwrap();
    assert_eq!(
        out["results"][0]["cmd"],
        single_line(A, Some("/w/proj"), json!({ "kind": "base" }))
    );
    assert!(rig.launched.borrow().is_empty());
    // 本机：同 `launch-local`（这台铸的名建进 tmux）；身份 token 用的就是 sid ⇒ 两次渲逐字相等。
    let args = json!({ "mode": "window", "local": true, "items": [item(A, json!({ "kind": "inherit" }))] });
    let out = rig.run(|d| start(&args, d)).unwrap();
    let single = super::super::launch_render::answer_local(&json!({
        "action": { "kind": "resume", "sid": A },
        "cwd": "/w/proj",
        "launcher": null,
        "tmuxName": "proj-cc-2",
        "defaultLauncher": "claude",
    }))
    .unwrap();
    assert_eq!(out["results"][0]["cmd"], single["cmd"]);
    assert_eq!(out["results"][0]["session"], "proj-cc-2");
}
