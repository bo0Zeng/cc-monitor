//! 批量停 / 起：每一个的结局与「单个那一条会做的事」相等（替身记下被调了什么；tmux 一个都不起）。

use super::*;
use std::cell::RefCell;

/// 判号要的事实：这台只有一个号 `work`（能用）、没有谁的上次记录。
fn with_accounts<T>(f: impl FnOnce(&la::Facts) -> T) -> T {
    let library = || {
        la::Library::of_product(&json!({ "meta": {"enabled": true}, "accounts": [{
            "name": "work", "configDir": "/h/.cc/work", "isDefault": false, "mode": "isolated",
            "exists": true, "authReady": true,
        }]}))
    };
    f(&la::Facts {
        has_accounts: true,
        library: &library,
        last: &|_| None,
    })
}

/// 一批起的入参：整批那几格 ＋ 每项 `{sid, cwd, account}`。
fn batch(mode: &str, local: bool, items: Vec<Value>) -> Value {
    json!({ "mode": mode, "local": local, "agent": "claude", "launcher": "claude", "defaultLauncher": "claude", "items": items })
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
    /// 这台按工作目录铸回的名字（替身不避让：避让是这台快照的事）。
    cwd_name: &'static str,
    /// 按分叉铸回的名字：缺 ⇒ 同这台的基名规则（`fork_tmux_base`，不避让）。
    fork_name: Option<&'static str>,
    /// 此刻持着某个 sid 的活进程（替身的 pidfile）。
    live: Vec<(&'static str, u32)>,
    /// 预标信任那几下：`pretrust <号目录> <工作目录> @<此前已交的 ccm 数>/<此前已键入的数>`。
    marks: RefCell<Vec<String>>,
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
            cwd_name: "proj-cc-2",
            fork_name: None,
            live: vec![],
            marks: RefCell::new(vec![]),
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
        let send_into = |name: &str, sid: &str, line: &str| -> Result<(), CmdErr> {
            self.launched.borrow_mut().push(
                json!({ "mode": "send-into", "name": name, "payload": line, "ccm_sid": sid }),
            );
            Ok(())
        };
        // 真 ccm：建会话撞名 ⇒ 退出码 3（响亮失败）。
        let run_ccm = |argv: &[String]| -> Result<(i32, String, String), CmdErr> {
            self.ccm.borrow_mut().push(argv.to_vec());
            Ok((
                if self.created { 0 } else { 3 },
                String::new(),
                String::new(),
            ))
        };
        let mint = |base: NameBase| -> Result<String, CmdErr> {
            Ok(match base {
                NameBase::Cwd(cwd) => {
                    self.calls.borrow_mut().push(format!("mint {cwd}"));
                    self.cwd_name.to_string()
                }
                NameBase::ForkOf(source) => {
                    self.calls.borrow_mut().push(format!("mint-fork {source}"));
                    self.fork_name.map_or_else(
                        || crate::control::ccm::plan::fork_tmux_base(source),
                        str::to_string,
                    )
                }
            })
        };
        let writers = |sid: &str| -> Vec<u32> {
            self.live
                .iter()
                .filter(|(s, _)| *s == sid)
                .map(|(_, p)| *p)
                .collect()
        };
        let pretrust = |dir: &str, cwd: &str| {
            let at = format!(
                "{}/{}",
                self.ccm.borrow().len(),
                self.launched.borrow().len()
            );
            self.marks
                .borrow_mut()
                .push(format!("pretrust {dir} {cwd} @{at}"));
        };
        with_accounts(|accounts| {
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
                    entry: crate::platform::paths::installed_ccm_entry,
                },
                accounts,
                writers: &writers,
                pretrust: &pretrust,
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

fn item(sid: &str, account: Value) -> Value {
    json!({ "sid": sid, "cwd": "/w/proj", "account": account })
}

/// 单个那一条那台后端会渲出的那一行（`launch-render-cli`，直连、不建容器）。
fn single_line(sid: &str, cwd: Option<&str>, account: Value) -> String {
    let req = json!({
        "agent": "claude",
        "action": { "kind": "resume", "sid": sid },
        "container": { "kind": "none" },
        "cwd": cwd,
        "account": account,
        "ccmSid": null,
        "model": null,
        "launcher": "claude",
        "defaultLauncher": "claude",
    });
    with_accounts(|f| super::super::launch_render::answer_cli(&req, f)).unwrap()["cmd"]
        .as_str()
        .unwrap()
        .to_string()
}

/// 单个「在 tmux 里 Resume」那一行（`launch-render-cli`，建进 tmux、打 sid 标记、带 cwd）。
fn single_tmux_line(sid: &str, name: &str, account: Value) -> String {
    let req = json!({
        "agent": "claude",
        "action": { "kind": "resume", "sid": sid },
        "container": { "kind": "tmux", "name": name, "send_into": false },
        "cwd": "/w/proj",
        "account": account,
        "ccmSid": sid,
        "model": null,
        "launcher": "claude",
        "defaultLauncher": "claude",
    });
    with_accounts(|f| super::super::launch_render::answer_cli(&req, f)).unwrap()["cmd"]
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
            { "sid": A, "standing": "running", "names": ["a-cc"],
              "terminals": [{ "host": "tmux", "terminal": "tmux-a-cc" }] },
            { "sid": B, "standing": "idle", "names": ["b-cc"],
              "terminals": [{ "host": "tmux", "terminal": "tmux-b-cc" }] },
            { "sid": C, "standing": "ambiguous", "names": ["c-cc", "c-cc-2"],
              "terminals": [{ "host": "tmux", "terminal": "tmux-c-cc" }, { "host": "tmux", "terminal": "tmux-c-cc-2" }] },
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
    let acct = json!({ "kind": "named", "name": "work" });
    let mut rig = Rig::new(Some(vec![
        row("a-cc", Some(A), true),  // 在跑 ⇒ 不另起
        row("b-cc", Some(B), false), // 空 tmux ⇒ 就地键入
    ]));
    rig.gone = vec![];
    let args = batch(
        "tmux",
        false,
        vec![
            item(A, acct.clone()),
            item(B, acct.clone()),
            item(C, acct.clone()),
        ],
    );
    let out = rig.run(|d| start(&args, d)).unwrap();
    assert_eq!(
        outcomes(&out),
        vec![
            (A.into(), "skipped".into(), json!("running")),
            (B.into(), "done".into(), Value::Null),
            (C.into(), "done".into(), Value::Null),
        ]
    );
    let remote = json!({ "kind": "named", "name": "work" });
    // 空 tmux：键进去的是单个就地 resume 那一行（直路、不带 cwd），落在挂着 B 的那个窗格。
    assert_eq!(
        *rig.launched.borrow(),
        vec![
            json!({ "mode": "send-into", "name": "b-cc", "payload": single_line(B, None, remote.clone()), "ccm_sid": B })
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

/// 已有活进程在写那条会话（不在这台 tmux 里跑着）⇒ 那一项不起、说出 pid，不挡同批别的；
/// tmux 里跑着它的那一项照旧答 `running`（界面据它接回去，不是起）。开终端那一形同样不起。
#[test]
fn a_session_some_live_process_is_writing_is_not_started_again() {
    let acct = json!({ "kind": "named", "name": "work" });
    let mut rig = Rig::new(Some(vec![
        row("a-cc", Some(A), true),  // 在跑
        row("b-cc", Some(B), false), // 空 tmux，可它在别处有活进程
    ]));
    rig.live = vec![(A, 101), (B, 202), (C, 303), (C, 304)];
    let args = batch(
        "tmux",
        false,
        vec![
            item(A, acct.clone()),
            item(B, acct.clone()),
            item(C, acct.clone()),
        ],
    );
    let out = rig.run(|d| start(&args, d)).unwrap();
    assert_eq!(
        outcomes(&out),
        vec![
            (A.into(), "skipped".into(), json!("running")),
            (B.into(), "skipped".into(), json!("session_already_live")),
            (C.into(), "skipped".into(), json!("session_already_live")),
        ]
    );
    assert_eq!(out["results"][1]["detail"], "202");
    assert_eq!(out["results"][2]["detail"], "303, 304");
    assert!(
        rig.launched.borrow().is_empty(),
        "键进了已有活进程的那条会话"
    );
    assert!(rig.ccm.borrow().is_empty(), "另起了一个进程去写同一条会话");
    // 开终端那一形（本机 / 远端）同样不渲那一行。
    for here in [true, false] {
        let args = batch("window", here, vec![item(B, acct.clone())]);
        let out = rig.run(|d| start(&args, d)).unwrap();
        assert_eq!(out["results"][0]["why"], "session_already_live", "{out}");
        assert_eq!(out["results"][0]["cmd"], Value::Null);
    }
}

#[test]
fn start_skips_what_has_no_record_and_says_a_taken_name() {
    let mut rig = Rig::new(Some(vec![]));
    rig.gone = vec![A];
    rig.created = false;
    let args = batch(
        "tmux",
        true,
        vec![
            item(A, json!({ "kind": "follow" })),
            item(B, json!({ "kind": "base" })),
        ],
    );
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
    let args = batch("window", false, vec![item(A, json!({ "kind": "base" }))]);
    let out = rig.run(|d| start(&args, d)).unwrap();
    assert_eq!(
        out["results"][0]["cmd"],
        single_line(A, Some("/w/proj"), json!({ "kind": "base" }))
    );
    assert!(rig.launched.borrow().is_empty());
    // 本机：同 `launch-local`（这台铸的名建进 tmux）；身份 token 用的就是 sid ⇒ 两次渲逐字相等。
    let args = batch("window", true, vec![item(A, json!({ "kind": "base" }))]);
    let out = rig.run(|d| start(&args, d)).unwrap();
    let single = with_accounts(|f| {
        super::super::launch_render::answer_local(
            &json!({
                "agent": "claude",
                "action": { "kind": "resume", "sid": A },
                "cwd": "/w/proj",
                "launcher": null,
                "account": { "kind": "base" },
                "tmuxName": "proj-cc-2",
                "defaultLauncher": "claude",
            }),
            f,
        )
    })
    .unwrap();
    assert_eq!(out["results"][0]["cmd"], single["cmd"]);
    assert_eq!(out["results"][0]["session"], "proj-cc-2");
}

/// 一个 tmux 会话里几个窗格各挂一个 sid：空着的那个（agent 已退）就地接回时，送字带着它的 sid
/// （`launch send-into` 按 sid 落在挂着它的那个窗格），不只按会话名送（那会落在会话当前的窗格 —— 多半是另一个正在跑的 claude）。
#[test]
fn an_idle_sid_in_one_pane_of_a_shared_session_is_typed_with_its_sid() {
    let acct = json!({ "kind": "named", "name": "work" });
    let rows = vec![row("two-cc", Some(A), true), row("two-cc", Some(B), false)];
    assert_eq!(standing(&rows, B), Standing::Idle("two-cc".into()));
    let rig = Rig::new(Some(rows));
    let args = batch("tmux", false, vec![item(B, acct)]);
    let out = rig.run(|d| start(&args, d)).unwrap();
    assert_eq!(outcomes(&out), vec![(B.into(), "done".into(), Value::Null)]);
    let launched = rig.launched.borrow();
    assert_eq!(launched.len(), 1);
    assert_eq!(launched[0]["name"], "two-cc");
    assert_eq!(launched[0]["ccm_sid"], B, "{launched:?}");
}

/// 整批一个总期限：第一个卡住吃光它 ⇒ 剩下的各自回 `child_timed_out`（不起、不等），整批按期回。
#[cfg(unix)]
#[test]
fn the_first_stuck_item_spends_the_batch_total_and_the_rest_time_out_on_their_own() {
    let dir = std::env::temp_dir().join(format!("ccm-batch-budget-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建目录");
    let rows = vec![
        row("a-cc", Some(A), true),
        row("b-cc", Some(B), true),
        row("c-cc", Some(C), true),
    ];
    let caps = caps();
    let list = || -> Result<Option<Vec<TmuxEntry>>, String> { Ok(Some(rows.clone())) };
    let record =
        |_: &str, _: Option<&str>| -> Result<(bool, String), String> { Ok((true, String::new())) };
    // 杀那一下真起一发：先留个记号、再卡住（一发自己的期限 5 s）。
    let kill = |name: &str, _: &str| -> Result<Value, CmdErr> {
        let mark = dir.join(name);
        Child::new("sh")
            .args(["-c", &format!("touch {}; sleep 30", mark.display())])
            .run(Deadline::secs(5))
            .map(|_| json!({}))
            .map_err(|e| e.into_cmd_err("kill_failed", |e| e.to_string()))
    };
    let send_into = |_: &str, _: &str, _: &str| -> Result<(), CmdErr> { Ok(()) };
    let run_ccm = |_: &[String]| -> Result<(i32, String, String), CmdErr> {
        Ok((0, String::new(), String::new()))
    };
    let mint = |_: NameBase| -> Result<String, CmdErr> { Ok(String::new()) };
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
            is_dir: |_| true,
            entry: crate::platform::paths::installed_ccm_entry,
        },
        accounts: &la::Facts {
            has_accounts: false,
            library: &la::Library::default,
            last: &|_| None,
        },
        writers: &|_| Vec::new(),
        pretrust: &|_, _| {},
    };
    let sids: Vec<String> = [A, B, C].map(str::to_string).to_vec();
    let t0 = std::time::Instant::now();
    let _total = crate::platform::child::Budget::capped(Deadline::millis(600), None);
    let v = stop(&json!({ "sids": sids }), &deps).expect("整批照回");
    let took = t0.elapsed();
    let got = outcomes(&v);
    for (sid, outcome, why) in &got {
        assert_eq!(
            (outcome.as_str(), why.as_str()),
            ("failed", Some(crate::platform::child::TIMED_OUT)),
            "{sid}：{v}"
        );
    }
    assert_eq!(got.len(), 3);
    assert!(
        took < std::time::Duration::from_millis(600 + 1_500),
        "整批没按总期限回：{took:?}"
    );
    assert!(dir.join("a-cc").exists(), "第一个该真起了");
    for later in ["b-cc", "c-cc"] {
        assert!(!dir.join(later).exists(), "总期限用完了 {later} 还起了");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// 一项选不了号 ⇒ 那一项 `skipped` 带 `account_unavailable`（`detail` 是要的那个号），不挡同批别的；
/// 起成了的那一项说出实际用的号（缺 `account` ＝ 跟随）。
#[test]
fn an_item_whose_account_cannot_be_used_is_skipped_without_blocking_the_batch() {
    let rig = Rig::new(Some(vec![]));
    let args = batch(
        "tmux",
        false,
        vec![
            item(A, json!({ "kind": "named", "name": "gone" })),
            json!({ "sid": B, "cwd": "/w/proj" }),
        ],
    );
    let out = rig.run(|d| start(&args, d)).unwrap();
    assert_eq!(
        outcomes(&out),
        vec![
            (A.into(), "skipped".into(), json!("account_unavailable")),
            (B.into(), "done".into(), Value::Null),
        ]
    );
    assert_eq!(out["results"][0]["detail"], "gone");
    assert_eq!(out["results"][0]["account"], Value::Null);
    assert_eq!(
        out["results"][1]["account"],
        json!({ "name": "work", "configDir": "/h/.cc/work", "model": null })
    );
    assert_eq!(rig.ccm.borrow().len(), 1, "选不了号的那一项不许起");
}

// ───────────────────────────── 分叉出来的那一条：必铸新终端名 ─────────────────────────────

const N: &str = "eeeeeeee-1111-2222-3333-444444444444";

fn fresh_item(sid: &str) -> Value {
    json!({ "sid": sid, "cwd": "/x/proj", "account": { "kind": "base" }, "fresh_terminal": true, "fork_of": A })
}

/// 父会话占着 `proj-cc`；替身按工作目录铸的也正是 `proj-cc`（不避让）⇒ 要铸新名的那一项起出来的终端名一定不是它。
#[test]
fn a_fresh_terminal_item_never_lands_on_the_parent_terminal_name() {
    let mut rig = Rig::new(Some(vec![row("proj-cc", Some(A), true)]));
    rig.cwd_name = "proj-cc";
    let out = rig
        .run(|d| start(&batch("tmux", false, vec![fresh_item(N)]), d))
        .unwrap();
    let r = &out["results"][0];
    assert_eq!(r["outcome"], "done", "{out}");
    assert_ne!(r["session"], "proj-cc", "新会话落进了父会话那个终端名");
    assert_eq!(r["session"], "proj-fork-cc");
    let ccm = rig.ccm.borrow();
    assert_eq!(ccm.len(), 1);
    assert!(
        ccm[0].contains(&"--ccm-tmux=proj-fork-cc".to_string()),
        "{ccm:?}"
    );
    assert!(!ccm[0].iter().any(|t| t == "--ccm-tmux=proj-cc"), "{ccm:?}");
    // 基名从源会话此刻那个终端的名字来。
    assert!(rig
        .calls
        .borrow()
        .contains(&"mint-fork proj-cc".to_string()));

    // 本机开终端那一形：名字同本机 Resume（按工作目录铸、这台避让），同样不落在父会话那个名字上。
    let mut here = Rig::new(Some(vec![row("proj-cc", Some(A), true)]));
    here.cwd_name = "proj-cc-2";
    let out = here
        .run(|d| start(&batch("window", true, vec![fresh_item(N)]), d))
        .unwrap();
    let r = &out["results"][0];
    assert_eq!(r["outcome"], "done", "{out}");
    assert_eq!(r["session"], "proj-cc-2");
    assert!(
        r["cmd"].as_str().unwrap().contains("--ccm-tmux=proj-cc-2"),
        "{out}"
    );
}

/// 这台铸回来的名字仍落在名单里某个上（快照晚了一拍）⇒ 不起、说名字被占，绝不复用。
#[test]
fn a_fresh_name_that_is_already_listed_is_refused_not_reused() {
    let mut rig = Rig::new(Some(vec![row("proj-fork-cc", Some(B), true)]));
    rig.fork_name = Some("proj-fork-cc");
    let out = rig
        .run(|d| start(&batch("tmux", false, vec![fresh_item(N)]), d))
        .unwrap();
    let r = &out["results"][0];
    assert_eq!(
        (r["outcome"].clone(), r["why"].clone(), r["session"].clone()),
        (json!("failed"), json!("name_taken"), json!("proj-fork-cc"))
    );
    assert!(rig.ccm.borrow().is_empty(), "撞名了还起了");
    // 本机开终端那一形：这台铸回的名字（快照晚了一拍）正是父会话那个 ⇒ 同样不起。
    let mut here = Rig::new(Some(vec![row("proj-cc", Some(A), true)]));
    here.cwd_name = "proj-cc";
    let out = here
        .run(|d| start(&batch("window", true, vec![fresh_item(N)]), d))
        .unwrap();
    assert_eq!(out["results"][0]["why"], "name_taken");
    assert_eq!(out["results"][0]["cmd"], Value::Null);
}

/// 要铸新名的那一项不键进已有的终端（哪怕那个终端挂着它）；不带这一格的照旧就地键入。
#[test]
fn a_fresh_terminal_item_is_never_typed_into_an_existing_terminal() {
    let rig = Rig::new(Some(vec![row("n-idle", Some(N), false)]));
    let out = rig
        .run(|d| start(&batch("tmux", false, vec![fresh_item(N)]), d))
        .unwrap();
    assert_eq!(out["results"][0]["session"], "proj-fork-cc");
    assert!(rig.launched.borrow().is_empty(), "键进了已有的终端");
    let plain = Rig::new(Some(vec![row("n-idle", Some(N), false)]));
    let out = plain
        .run(|d| {
            start(
                &batch("tmux", false, vec![item(N, json!({ "kind": "base" }))]),
                d,
            )
        })
        .unwrap();
    assert_eq!(out["results"][0]["session"], "n-idle");
}

/// 新名从源会话此刻所在终端的名字铸（自定义的名字照旧：`work-cc` ⇒ `work-fork-cc`）；源会话不在任何终端里 ⇒ 从工作目录铸。
#[test]
fn a_fresh_name_comes_from_the_source_terminal_name_else_from_the_cwd() {
    let rig = Rig::new(Some(vec![row("work-cc", Some(A), true)]));
    let out = rig
        .run(|d| start(&batch("tmux", false, vec![fresh_item(N)]), d))
        .unwrap();
    assert_eq!(out["results"][0]["session"], "work-fork-cc", "{out}");
    assert!(rig
        .calls
        .borrow()
        .contains(&"mint-fork work-cc".to_string()));
    // 源会话已退出（终端里只剩 shell 的也不算）⇒ 按工作目录。
    for rows in [vec![], vec![row("work-cc", Some(A), false)]] {
        let rig = Rig::new(Some(rows));
        let out = rig
            .run(|d| start(&batch("tmux", false, vec![fresh_item(N)]), d))
            .unwrap();
        assert_eq!(out["results"][0]["session"], "proj-fork-cc", "{out}");
        assert!(rig
            .calls
            .borrow()
            .contains(&"mint-fork /x/proj".to_string()));
    }
}

#[test]
fn fresh_terminal_and_fork_of_are_checked() {
    let rig = Rig::new(Some(vec![]));
    for bad in [
        json!({ "sid": N, "cwd": "/x", "fresh_terminal": "yes" }),
        json!({ "sid": N, "cwd": "/x", "fresh_terminal": true, "fork_of": "a b" }),
        json!({ "sid": N, "cwd": "/x", "fork_of": A }),
    ] {
        let (code, _) = rig
            .run(|d| start(&batch("tmux", false, vec![bad.clone()]), d))
            .unwrap_err();
        assert_eq!(code, "bad_args", "{bad}");
    }
}

/// 起之前预标信任：用账号库里的号起的每一种起法（tmux 新建 · tmux 空终端就地键入 · 本机开窗 · 远端开窗），
/// 都在交 ccm / 键入之前把工作目录标进那个号；没起的（在跑 · 已有活进程在写）与账号 0 一个都不标。
#[test]
fn every_start_marks_the_cwd_trusted_in_its_account_before_it_starts() {
    let acct = json!({ "kind": "named", "name": "work" });
    let mut rig = Rig::new(Some(vec![
        row("a-cc", Some(A), true),  // 在跑 ⇒ 不起
        row("b-cc", Some(B), false), // 空 tmux ⇒ 就地键入
    ]));
    rig.live = vec![("dddddddd-1111-2222-3333-444444444444", 9)];
    let d = "dddddddd-1111-2222-3333-444444444444";
    let args = batch(
        "tmux",
        false,
        vec![
            item(A, acct.clone()),
            item(B, acct.clone()),
            item(C, acct.clone()),
            item(d, acct.clone()),
            json!({ "sid": "eeeeeeee-1111-2222-3333-444444444444", "cwd": "/w/base", "account": { "kind": "base" } }),
        ],
    );
    rig.run(|deps| start(&args, deps)).unwrap();
    assert_eq!(
        *rig.marks.borrow(),
        vec![
            "pretrust /h/.cc/work /w/proj @0/0".to_string(),
            "pretrust /h/.cc/work /w/proj @0/1".to_string(),
        ],
        "B 键入之前 · C 交 ccm 之前各标一次；A 在跑 · D 有活进程 · 账号 0 不标"
    );
    assert_eq!(
        (rig.ccm.borrow().len(), rig.launched.borrow().len()),
        (2, 1)
    );

    for local in [true, false] {
        let rig = Rig::new(None);
        let args = batch("window", local, vec![item(A, acct.clone())]);
        let out = rig.run(|deps| start(&args, deps)).unwrap();
        assert_eq!(out["results"][0]["outcome"], "done");
        assert_eq!(
            *rig.marks.borrow(),
            vec!["pretrust /h/.cc/work /w/proj @0/0".to_string()],
            "开窗（local={local}）交回那一行之前标"
        );
    }
}
