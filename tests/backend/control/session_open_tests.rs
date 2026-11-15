//! `session-open`：默认放在哪的整张表 · 恢复 / 接回 / 新起各落在哪 · 已有进程在写就不起（直路一样）· 给了 tmux 而没有就拒 ·
//! 每个码都有后端写好的那一句（替身记下被调了什么；tmux 一个都不起）。

use crate::common::said::Said;
use crate::control::launch_account::{self as la, AccountAsk};
use crate::control::session_batch::{Deps, NameBase, TermEntry};
use crate::control::session_open::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeSet;

const A: &str = "aaaaaaaa-1111-2222-3333-444444444444";
const U: &str = "99999999-1111-2222-3333-444444444444";

fn row(name: &str, sid: Option<&str>, agent: bool) -> TermEntry {
    TermEntry {
        host: crate::wire::TerminalHost::Tmux,
        name: name.into(),
        terminal: format!("tmux-{name}"),
        sid: sid.map(str::to_string),
        agent,
    }
}

/// 替身：名单 · 记录 · 活进程是给定的；铸 · 建 · 键入 · 预标只记下来。
struct Rig {
    rows: Option<Vec<TermEntry>>,
    /// 托管那几行（名单并在 tmux 那几行后面）。
    hosted: Vec<TermEntry>,
    list_err: Option<&'static str>,
    gone: bool,
    live: Vec<u32>,
    ccm_rc: i32,
    send_err: Option<&'static str>,
    no_entry: bool,
    fork_err: bool,
    rotation_err: bool,
    prefs: crate::common::machine_prefs::Prefs,
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
    /// 这台能托管（名单那一格 `managed`）。
    hosts: bool,
    /// 这台是 Windows 那一档：能力照没有 tmux 的平台自报、本机终端是 PowerShell、目录 `C:\w` 在。
    windows: bool,
    /// 启动台账记着那条会话起在哪种宿主里（恢复照起时那一种读它）。
    started: Option<crate::wire::TerminalHost>,
    /// 起 ccm 那一趟的 stdout（托管那一形报 `ccm-session=<名>`）。
    ccm_out: String,
    ccm: RefCell<Vec<Vec<String>>>,
    sent: RefCell<Vec<(String, String)>>,
    marks: RefCell<usize>,
    shells: RefCell<Vec<(String, Option<String>)>>,
    /// 建空终端那几趟各建在哪种宿主里（与 `shells` 同序）。
    shell_hosts: RefCell<Vec<crate::wire::TerminalHost>>,
}

impl Rig {
    fn new(rows: Option<Vec<TermEntry>>) -> Self {
        Self {
            rows,
            hosted: vec![],
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
            hosts: false,
            windows: false,
            started: None,
            ccm_out: String::new(),
            ccm: RefCell::new(vec![]),
            sent: RefCell::new(vec![]),
            marks: RefCell::new(0),
            shells: RefCell::new(vec![]),
            shell_hosts: RefCell::new(vec![]),
        }
    }

    /// 线上那一份入参（`local: false`，别的格照给的）⇒ 本体。恢复那一条的记录：哪一家 claude · 目录 /p。
    fn open(&self, args: Value) -> Result<Opened, Said> {
        let fork = |_: &str, _: &str| -> Result<String, crate::common::said::Said> {
            if self.fork_err {
                Err(crate::common::said::Said::new(
                    "fork_failed",
                    "分支记录写不成".into(),
                ))
            } else {
                Ok(U.to_string())
            }
        };
        self.open_via(args, &fork)
    }

    /// 同 [`Self::open`]，叉那一手由调用方给（分叉两形的判据交真的那一手）。
    fn open_via(&self, args: Value, fork: ForkWrite) -> Result<Opened, Said> {
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
        let sid = || U.to_string();
        let write = |_: &str, _: &str, _: &str| -> Result<(), crate::common::said::Said> {
            if self.rotation_err {
                Err(crate::common::said::Said::new(
                    "no_strategy",
                    "没有这一份策略".into(),
                ))
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
        self.run(|deps| answer(req, facts, deps, fork, &pre, None))
    }

    /// 拼一份替身 `Deps` 交给 `f`。
    fn run<T>(&self, f: impl FnOnce(&Deps) -> T) -> T {
        let platform = if self.windows {
            crate::platform::tmux::TmuxPlatform::AbsentUnlessExeOnPath
        } else {
            crate::platform::tmux::TMUX_PLATFORM
        };
        let caps: BTreeSet<String> = crate::control::ccm::ccm_launcher_with(platform)
            .into_iter()
            .map(str::to_string)
            .collect();
        let list = || -> Result<crate::control::session_batch::Listed, Said> {
            match self.list_err {
                Some(c) => Err(Said::new(c, "列不出".to_string())),
                None => Ok(crate::control::session_batch::Listed::of(
                    self.rows.clone(),
                    self.hosted.clone(),
                )
                .hosting(self.hosts)),
            }
        };
        let record = |_: &str, _: Option<&str>| -> Result<(bool, String), String> {
            Ok((true, "/root".into()))
        };
        let end = |_: &str| -> Result<(), crate::common::said::Said> {
            unreachable!("session-open 不结束")
        };
        let send_into =
            |name: &str, _: &str, line: &str| -> Result<(), crate::common::said::Said> {
                self.sent.borrow_mut().push((name.into(), line.into()));
                match self.send_err {
                    Some(c) => Err(crate::wire::spec::fail(c, "门没过".to_string())),
                    None => Ok(()),
                }
            };
        let run_ccm = |argv: &[String]| -> Result<(i32, String, String), Said> {
            self.ccm.borrow_mut().push(argv.to_vec());
            Ok((self.ccm_rc, self.ccm_out.clone(), "坏了".into()))
        };
        let mint = |base: NameBase| -> Result<String, Said> {
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
        let open_shell = |name: &str,
                          cwd: Option<&str>,
                          host: crate::wire::TerminalHost|
         -> Result<bool, crate::common::said::Said> {
            self.shells
                .borrow_mut()
                .push((name.into(), cwd.map(str::to_string)));
            self.shell_hosts.borrow_mut().push(host);
            Ok(true)
        };
        let library = || {
            crate::accounts::manage::library::test_support::list(
                self.names
                    .iter()
                    .map(|n| {
                        crate::accounts::manage::library::test_support::row_with(n, |a| {
                            a.agent = Some(crate::agents::default_kind().into());
                            a.config_dir = Some(format!("/h/.cc/{n}"));
                        })
                    })
                    .collect(),
            )
        };
        let accounts = la::Facts {
            agent: crate::agents::default_kind().into(),
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
        let is_dir: fn(&str) -> bool = if self.e2e {
            |_| true
        } else if self.windows {
            |d| d == "C:\\w"
        } else {
            |d| d == "/p"
        };
        f(&Deps {
            list: &list,
            record: &record,
            end: &end,
            send_into: &send_into,
            run_ccm: &run_ccm,
            mint: &mint,
            caps: &caps,
            host: crate::control::launch_render::Host {
                windows: self.windows,
                is_dir,
                entry,
            },
            accounts: &accounts,
            writers: &writers,
            pretrust: &pretrust,
            open_shell: &open_shell,
            prefs: &self.prefs,
            started: &|sid| (sid == A).then_some(self.started).flatten(),
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
    use Launch::*;
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
                got.push((i, pref, tmux, default_run(i, pref, tmux, false)));
            }
        }
    }
    let n = |f| New { fork_tmux: f };
    // 这台不能托管（Windows · 判据没给托管的家）：同 A3-3-8 之前那一张。
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
    // 这台能托管：新起 ⇒ 托管（分叉：源会话在 tmux 里且这台有 tmux ⇒ tmux）；恢复 ⇒ 偏好开着且有 tmux 才 tmux，否则托管。直路一个都不落。
    let mut got = Vec::new();
    for i in intents {
        for pref in [false, true] {
            for tmux in [false, true] {
                got.push((i, pref, tmux, default_run(i, pref, tmux, true)));
            }
        }
    }
    let want = vec![
        (n(None), false, false, Managed),
        (n(None), false, true, Managed),
        (n(None), true, false, Managed),
        (n(None), true, true, Managed),
        (n(Some(true)), false, false, Managed),
        (n(Some(true)), false, true, Tmux),
        (n(Some(true)), true, false, Managed),
        (n(Some(true)), true, true, Tmux),
        (n(Some(false)), false, false, Managed),
        (n(Some(false)), false, true, Managed),
        (n(Some(false)), true, false, Managed),
        (n(Some(false)), true, true, Managed),
        (Resume, false, false, Managed),
        (Resume, false, true, Managed),
        (Resume, true, false, Managed),
        (Resume, true, true, Tmux),
    ];
    assert_eq!(got, want);
}

/// A3-3-8 第一条：这台能托管、不说放在哪起新会话 ⇒ 落托管：交的那一行 ccm 带 `--hosted --detach`（名字由 ccm 铸、从它的 stdout 读回），
/// 回接进去那一行 `--attach <名>`；这台不铸 tmux 名、不交 `--ccm-tmux`。
#[test]
fn where_this_machine_hosts_a_new_session_without_run_lands_in_a_managed_terminal() {
    let mut rig = Rig::new(Some(vec![]));
    rig.hosts = true;
    rig.ccm_out = "ccm-session=p-cc-2\n".into();
    let o = rig.open(new(json!({}))).unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::Started, Some("p-cc-2"))
    );
    assert!(
        o.line.as_deref().unwrap().ends_with("--attach p-cc-2"),
        "{:?}",
        o.line
    );
    let argv = rig.ccm.borrow()[0].clone();
    assert!(
        argv.iter().any(|a| a == "--hosted") && argv.iter().any(|a| a == "--detach"),
        "{argv:?}"
    );
    assert!(
        !argv.iter().any(|a| a.starts_with("--ccm-tmux")),
        "托管那一形不进 tmux：{argv:?}"
    );
    assert!(
        argv.windows(2).any(|w| w == ["--cwd", "/p"]),
        "后端替人起：工作目录随行：{argv:?}"
    );
    // 显式要托管、这台不能托管 ⇒ 拒（不悄悄改道）。
    let rig = Rig::new(Some(vec![]));
    let e = rig
        .open(
            json!({ "what": { "kind": "new", "agent": "claude", "cwd": "/p" }, "run": "managed" }),
        )
        .unwrap_err();
    assert_eq!(e.code.as_deref(), Some("place_unavailable"));
}

/// 恢复：这台能托管、偏好没开 ⇒ 托管；那一条正在一个托管终端里跑 ⇒ 接回那一行、不另起。
#[test]
fn a_resume_where_this_machine_hosts_lands_managed_or_rejoins_its_managed_terminal() {
    let mut rig = Rig::new(Some(vec![]));
    rig.hosts = true;
    rig.ccm_out = "ccm-session=p-cc\n".into();
    let o = rig.open(resume(None)).unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::Started, Some("p-cc"))
    );
    let argv = rig.ccm.borrow()[0].join(" ");
    assert!(
        argv.contains("--hosted") && argv.contains("--detach") && argv.contains(A),
        "{argv}"
    );
    let mut rig = Rig::new(Some(vec![]));
    rig.hosts = true;
    rig.hosted = vec![TermEntry {
        host: crate::wire::TerminalHost::Managed,
        name: "p-cc".into(),
        terminal: "p-cc".into(),
        sid: Some(A.into()),
        agent: true,
    }];
    let o = rig.open(resume(None)).unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::Running, Some("p-cc"))
    );
    assert!(rig.ccm.borrow().is_empty(), "在跑不另起");
}

/// ★ 直路也过「已有进程在写就拒」那道门：拒 ⇒ `already_live`，零次预标、零次起 ccm、不渲那一行。
#[test]
fn a_direct_resume_of_a_session_someone_is_writing_is_refused() {
    let mut rig = Rig::new(Some(vec![]));
    rig.live = vec![4242];
    let err = rig.open(resume(Some("direct"))).unwrap_err();
    assert_eq!(
        (err.code.as_deref(), err.raw.as_deref()),
        (Some("already_live"), Some("4242"))
    );
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
        assert_eq!(err.code.as_deref(), Some("place_unavailable"));
        assert!(rig.ccm.borrow().is_empty());
    }
    // 没说放在哪 ⇒ 这台定：没 tmux 就直路。
    let rig = Rig::new(None);
    assert_eq!(rig.open(new(json!({}))).unwrap().landed, Landed::Window);
}

/// 恢复进 tmux：没在哪个 tmux 里 ⇒ 铸名建会话（`--detach`）、回接进去那一行；空 tmux ⇒ 就地键入；在跑 ⇒ 不另起。
#[test]
fn a_resume_asked_into_tmux_lands_where_the_session_can_be() {
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

/// 恢复没说放在哪 ⇒ 照这台的偏好（三选）：托管（这台不能托管 ⇒ 直路）· tmux · 照起时那一种（台账记着 tmux 才进 tmux）。
#[test]
fn resume_without_run_follows_this_machines_preference() {
    use cmd_wire::session::ResumeWhere;
    let rig = Rig::new(Some(vec![]));
    assert_eq!(rig.open(resume(None)).unwrap().landed, Landed::Window);
    let mut rig = Rig::new(Some(vec![]));
    rig.prefs.resume_where = ResumeWhere::Tmux;
    assert_eq!(rig.open(resume(None)).unwrap().landed, Landed::Started);
    // 照起时那一种：记着 tmux ⇒ 进 tmux（`--ccm-tmux`）；记着托管 · 记不着 ⇒ 托管（`--hosted`）。
    for (started, want) in [
        (Some(crate::wire::TerminalHost::Tmux), "--ccm-tmux="),
        (Some(crate::wire::TerminalHost::Managed), "--hosted"),
        (None, "--hosted"),
    ] {
        let mut rig = Rig::new(Some(vec![]));
        rig.hosts = true;
        rig.ccm_out = "ccm-session=p-cc\n".into();
        rig.prefs.resume_where = ResumeWhere::AsStarted;
        rig.started = started;
        assert_eq!(
            rig.open(resume(None)).unwrap().landed,
            Landed::Started,
            "{started:?}"
        );
        let argv = rig.ccm.borrow()[0].join(" ");
        assert!(argv.contains(want), "{started:?} ⇒ {argv}");
    }
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

/// 接回：按会话（在跑 / 空 shell）或按会话名，回接进去那一行；不在哪种宿主里 ⇒ `not_hosted`。
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
    assert_eq!(
        o.at.map(|w| w.0),
        Some(copy_core::copy_text(
            "sessionOpen.at.tmux",
            &[("name", "idle")]
        )),
        "接上的那个终端照宿主写好：tmux 会话"
    );
    let o = rig
        .open(json!({ "what": { "kind": "look", "session": "x" } }))
        .unwrap();
    assert_eq!(o.landed, Landed::Running);
    let err = rig
        .open(json!({ "what": { "kind": "look", "sid": U } }))
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("not_hosted"));
    assert!(rig.ccm.borrow().is_empty() && rig.sent.borrow().is_empty());
}

/// 测试进程那台缺省 tmux server（`host::tmux::On::default()` 连的那一台：测试档的子进程闸把裸名 `tmux` 落到
/// `<临时目录>/ccm-unit-tmux-<pid>`，碰不到用户的 server）。生产那几个入口（`host::Housing::here` · 接回的名单）只认缺省
/// socket ⇒ 交叉判据在这一台上摆场面：判据自己那几发照 `-S` 点名那个 socket（`$TMUX_TMPDIR/tmux-<uid>/default`）；
/// 只在 [`alone`] 起的子进程里起（同一进程里别的判据读的缺省名单是空的）；掉了就 `kill-server`。
#[cfg(target_os = "linux")]
struct DefaultTmux {
    sock: std::path::PathBuf,
}

#[cfg(target_os = "linux")]
impl DefaultTmux {
    fn up() -> DefaultTmux {
        use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};
        assert!(
            std::env::var_os(ALONE).is_some(),
            "缺省 socket 上的 server 只在子进程里起"
        );
        let uid = std::fs::metadata("/proc/self")
            .expect("读本进程的属主")
            .uid();
        let dir = std::env::temp_dir()
            .join(format!("ccm-unit-tmux-{}", std::process::id()))
            .join(format!("tmux-{uid}"));
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .expect("建缺省 socket 目录");
        let t = DefaultTmux {
            sock: dir.join("default"),
        };
        t.ok(&[
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-s",
            "keep149",
            "sleep 600",
        ]);
        t.ok(&["set-option", "-g", "default-shell", "/bin/sh"]);
        t
    }
    fn run(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .arg("-S")
            .arg(&self.sock)
            .args(args)
            .output()
            .expect("tmux 不可执行 —— 本判据要求环境有 tmux（刻意不静默跳过）")
    }
    fn ok(&self, args: &[&str]) -> String {
        let o = self.run(args);
        assert!(
            o.status.success(),
            "缺省 tmux {args:?} 没成：{}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }
    /// 起一个会话 `name`，窗格前台跑 `cmd`；等窗格进程换成 `comm`、环境读得出（打标读它的 `TMUX_PANE`）。回（窗格 id，窗格进程）。
    fn session(&self, name: &str, cmd: &str, comm: &str) -> (String, u32) {
        let out = self.ok(&[
            "new-session",
            "-d",
            "-s",
            name,
            "-P",
            "-F",
            "#{pane_id} #{pane_pid}",
            cmd,
        ]);
        let (pane, pid) = out.split_once(' ').expect("窗格 id 与 pid");
        let pid: u32 = pid.parse().expect("pane_pid");
        assert!(settled(pid, comm), "窗格进程没换成 {comm}");
        (pane.to_string(), pid)
    }
}

#[cfg(target_os = "linux")]
impl Drop for DefaultTmux {
    fn drop(&mut self) {
        // 不经 `run`：没装 tmux 时 `run` 的 expect 已经在 panic，收尾再 panic 会让整个测试二进制 abort。
        let _ = std::process::Command::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .arg("-S")
            .arg(&self.sock)
            .arg("kill-server")
            .output();
    }
}

/// 子进程那一趟的标记（[`alone`]）。
#[cfg(target_os = "linux")]
const ALONE: &str = "CCM_SAME149_ALONE";

/// 在缺省 socket 上摆场面的判据只在自己一个进程里跑：没带 [`ALONE`] ⇒ 把测试二进制自己重起、`--exact` 只跑这一条，
/// 照它的结果红绿；带着 ⇒ 这一趟就是子进程，跑 `body`（子进程的 pid 另是一个 ⇒ 闸给的缺省 socket 目录也另是一个）。
#[cfg(target_os = "linux")]
fn alone(test: &str, body: impl FnOnce()) {
    if std::env::var_os(ALONE).is_some() {
        return body();
    }
    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, m)| m);
    let full = format!("{module}::{test}");
    let out = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([full.as_str(), "--exact", "--test-threads=1"])
        .env(ALONE, "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("起子进程");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // 名字对不上时 libtest 一条不跑也退 0 ⇒ 还要看到恰好跑过这一条。
    assert!(
        out.status.success() && said.contains("1 passed"),
        "子进程里 {full} 没过：\n{said}"
    );
}

/// 等到 `pid` 的 `comm` 是 `comm`、环境读得出（exec 换身那一刻 environ 是空的；墙钟只当兜底上限）。
#[cfg(target_os = "linux")]
fn settled(pid: u32, comm: &str) -> bool {
    guard_core::wall::rounds().any(|_| {
        let c = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        let env = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        let hit = c.trim() == comm && !env.is_empty();
        if !hit {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        hit
    })
}

/// 接回（`look sid`）那一趟整条交生产：命令表里登记的 `session-open` 照它的档装好（`wire::spec::invoke`）、交请求跑
/// （分派外面那几层——期限 · 出口声明——不过），
/// 帧面宿主自己拼名单（接回那一根线）、自己认、自己回；名单与落处都出自这一次调用，判据一行组合不抄。
/// 测试档没装托管家 ⇒ 名单只有 tmux 那几行（托管那一臂在另一个只装了它那一份托管家的子进程里判：
/// [`look_attaches_a_hosted_session_through_the_command_entry`]）。
#[cfg(target_os = "linux")]
fn look_here(sid: &str) -> Result<Opened, Said> {
    assert!(
        crate::host::managed::installed().is_none(),
        "测试档装了托管家：名单里会混进托管那几行"
    );
    look_by_entry(sid)
}

/// [`look_here`] 的本体：不问这个进程装没装托管家（装了的那一趟名单里就有托管那几行）。
#[cfg(target_os = "linux")]
fn look_by_entry(sid: &str) -> Result<Opened, Said> {
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == "session-open")
        .expect("命令表里没有 `session-open`");
    let own = crate::platform::fs::Own::for_tests();
    let req = crate::wire::Request {
        id: "t".to_string(),
        cmd: spec.name.to_string(),
        args: json!({ "what": { "kind": "look", "sid": sid }, "local": false }),
        ..Default::default()
    };
    let got = match crate::wire::spec::invoke(spec.run, &crate::stream::inbound::hands(Some(&own)))
    {
        Some(crate::wire::spec::Prepared::Blocking(f)) => f(req)?,
        _ => panic!("`session-open` 不在阻塞档"),
    };
    Ok(
        serde_json::from_value(got.expect("`session-open` 回了成品"))
            .expect("回包是 Opened 那一形"),
    )
}

/// ★ 凡脸上「在终端里打开」能的，接回（`look sid`）都接得上（给了按钮、点了说不行就是说假话）。两张脸与接回都交生产那一手：
/// - 清单那一路：住处是 `host::Housing::here()` 那一份名单里挂着这个 sid 的那一行（`host::housed`）；
/// - 标签页那一路：容器是宣告那一刻打标的结局（`identity_tag::tag_seen`，打上了才有容器）；
/// - 接回从命令入口走（命令表里的 `session-open` ⇒ 帧面宿主拼的接回名单 ⇒ `session_batch::carriers`，见 [`look_here`]）。
///
/// 三处说的是同一个窗格：打上标的那个会话，两张脸都亮、住处 ＝ 打标报的容器、接回落在它那个终端上；没打上标的那一形
/// （没控制终端、环境里却带着那个窗格）标签页那张脸灰着，名单里也不挂它、接回照实回不在。
/// 场面摆在缺省 socket 那一台（测试档的沙箱，子进程里）上：哪一手换了名单的来处、打的标名单不读，这里就红。
/// 托管那一臂（`Housing::here` 读 `managed::listed`、接回读 `managed::installed`）要装进程级的托管家，这里不摆：
/// 另起一个只装它那一份托管家的子进程判（[`look_attaches_a_hosted_session_through_the_command_entry`]）。
#[cfg(target_os = "linux")]
#[test]
fn every_session_the_face_lets_attach_is_found_by_look() {
    alone(
        "every_session_the_face_lets_attach_is_found_by_look",
        faces_and_look_agree_on_the_default_socket,
    );
}

/// [`every_session_the_face_lets_attach_is_found_by_look`] 在子进程里跑的那一段。
#[cfg(target_os = "linux")]
fn faces_and_look_agree_on_the_default_socket() {
    use crate::wire::{Allow, Life, SessionFace, SessionFaceExt as _, WireLife};
    const M: &str = "bbbbbbbb-1149-4222-8333-444444444444";
    const D: &str = "dddddddd-1149-4222-8333-444444444444";
    let t = DefaultTmux::up();
    let relay = test_home::scratch("same149").join("relay");
    // 打标那一口起的是 `/bin/sh <转接>`（不是裸名 tmux，闸不改它）⇒ 转接里照字面指到同一台。
    std::fs::write(
        &relay,
        format!(
            "#!/bin/sh\nexec env -u TMUX -u TMUX_PANE tmux -S '{}' \"$@\"\n",
            t.sock.display()
        ),
    )
    .expect("写转接脚本");
    let _iso = crate::control::identity_tag::door::isolate_with(&relay);
    let (pane, main) = t.session("box149", "exec cat", "cat");
    let live = WireLife::Live { activity: None };

    // 打上标的那一个：宣告那一刻的容器（标签页）· 名单里的住处（清单）· 接回落处，三处同一个窗格。
    let tagged = crate::control::identity_tag::tag_seen(main, M).container;
    let Some(crate::wire::SessionContainer::Tmux {
        terminal: Some(handle),
    }) = tagged.clone()
    else {
        panic!("窗格前台那一个没打上标：{tagged:?}");
    };
    let session_id = t.ok(&["display-message", "-p", "-t", &pane, "#{session_id}"]);
    assert_eq!(
        handle,
        crate::host::tmux::handle_of(&session_id, Some(&pane)),
        "容器指着那个窗格"
    );
    let tab = SessionFace::of(Life::live(None, tagged.as_ref()));
    assert_eq!(tab.can.attach, Allow::Yes, "标签页那张脸");
    let housed = crate::host::Housing::here().of(M);
    assert_eq!(housed, tagged, "清单的住处与宣告时的容器不是同一个终端");
    let listed = SessionFace::listed(&live, housed.as_ref());
    assert_eq!(listed.can.attach, Allow::Yes, "清单那张脸");
    let o = match look_here(M) {
        Ok(o) => o,
        Err(e) => panic!("两张脸都说接得进，接回却回 {e:?}"),
    };
    assert_eq!(o.session.as_deref(), Some("box149"), "接回落在别的终端上");
    assert!(
        o.line
            .as_deref()
            .is_some_and(|l| l.ends_with("--attach box149")),
        "{:?}",
        o.line
    );

    // 没打上标的那一形：没控制终端、环境里带着同一个窗格 ⇒ 两张脸都灰、接回回不在。
    let mut kid = std::process::Command::new("setsid")
        .args(["sleep", "600"])
        .env_remove("TMUX")
        .env("TMUX_PANE", &pane)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("起不来 setsid sleep");
    let up = settled(kid.id(), "sleep");
    let untagged = crate::control::identity_tag::tag_seen(kid.id(), D).container;
    let _ = kid.kill();
    let _ = kid.wait();
    assert!(up, "setsid sleep 没起来");
    assert_eq!(untagged, None, "没控制终端的那一形打上了标");
    let tab = SessionFace::of(Life::live(None, untagged.as_ref()));
    assert_ne!(tab.can.attach, Allow::Yes, "标签页那张脸");
    let housed = crate::host::Housing::here().of(D);
    assert_eq!(housed, None, "名单里挂上了没打上标的那一个");
    assert_ne!(
        SessionFace::listed(&live, housed.as_ref()).can.attach,
        Allow::Yes,
        "清单那张脸"
    );
    assert_eq!(
        look_here(D).err().and_then(|e| e.code).as_deref(),
        Some("not_hosted"),
        "灰着的那一个接回却接上了"
    );
    assert_eq!(
        crate::host::Housing::here().of(M),
        tagged,
        "没打上标的那一下把窗格的标顶掉了"
    );
}

/// ★ 托管终端里挂着的那条会话，按 sid 接回经命令入口接得上：落在那个托管终端上，那一行是瘦客户端 `--attach <句柄>`。
/// 托管名单的家是进程级的一份（`host::managed::install`，装一次不换）⇒ 在自己一个子进程里跑（[`alone`]），
/// 那个进程里只装这一份沙箱托管家；名单由帧面宿主那一根接回的线自己拼（[`look_by_entry`]），一行组合不抄：
/// 那根线只认 tmux、或托管那一臂丢了 ⇒ 这里红。缺省 socket 在测试档的沙箱里（子进程另一个目录），不往上起 server。
/// 假子进程（`sh`），不起真 agent。
#[cfg(target_os = "linux")]
#[test]
fn look_attaches_a_hosted_session_through_the_command_entry() {
    alone(
        "look_attaches_a_hosted_session_through_the_command_entry",
        look_a_hosted_session_alone,
    );
}

/// [`look_attaches_a_hosted_session_through_the_command_entry`] 在子进程里跑的那一段。
#[cfg(target_os = "linux")]
fn look_a_hosted_session_alone() {
    use crate::host::managed::run::test_support::{
        hello, read_until_out, sandbox_own, spec_ready, start,
    };
    use crate::host::managed::wire::{write_frame, Frame, Want};
    const S: &str = "7c1e5a2b-1153-4222-8333-444455556666";
    const ID: &str = "t-look153";
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("运行时");
    // 等在异步那一侧的几步整条套兜底上限（`guard_core::wall::CEILING`）；同步那两下（打标 · 接回）各自的期限是托管口的读写期限与列名单的子进程期限。
    let body = async {
        let (own, _data) = sandbox_own("look153");
        crate::host::managed::install(&own);
        let (sock, task) = start(&own, spec_ready(ID)).await;
        let mut viewer = hello(&sock, Want::View).await;
        read_until_out(&mut viewer, "ready").await;
        drop(viewer);
        // 打标 · 接回都是同步等回话 ⇒ 放到阻塞线程上，`timeout` 照样轮得到；结局（连 panic）先收着，收尾之后再判。
        let at = sock.clone();
        let looked = tokio::task::spawn_blocking(move || {
            crate::host::managed::client::tag(&at, S).map(|_| look_by_entry(S))
        })
        .await;
        // 收尾：让假子进程读到一个回车退出，等托管任务跑完（先收、后判：打标 · 接回哪一下红了都不留托管进程）。
        let mut s = hello(&sock, Want::Input).await;
        let sent = write_frame(&mut s, &Frame::In(b"\r".to_vec())).await;
        let done = task.await;
        let o = match looked.expect("接回那一下没崩").expect("打上 sid") {
            Ok(o) => o,
            Err(e) => panic!("托管终端里挂着的那条会话，接回却回 {e:?}"),
        };
        assert_eq!(
            o.session,
            Some(format!("name-{ID}")),
            "落在别的终端上：{o:?}"
        );
        assert!(
            o.line
                .as_deref()
                .is_some_and(|l| l.ends_with(&format!("--attach {ID}"))),
            "{o:?}"
        );
        sent.expect("送字");
        done.expect("任务没崩").expect("跑完");
    };
    let r = rt.block_on(async { tokio::time::timeout(guard_core::wall::CEILING, body).await });
    // 过了上限时阻塞线程上那一下可能还在等：运行时不等它收完，子进程照实红、退出。
    rt.shutdown_background();
    r.expect("兜底上限之内跑完");
}

/// ★ 接回托管终端：按 sid · 按名认到托管那一行 ⇒ 那一行是瘦客户端 `-- --attach <句柄>`，`session` 报终端名；
/// 这台没 tmux 也接得上；一个都没认到、又没 tmux ⇒ 照旧 `place_unavailable`。
#[test]
fn look_attaches_a_hosted_terminal_by_handle() {
    let hosted = TermEntry {
        host: crate::wire::TerminalHost::Managed,
        name: "box-cc".into(),
        terminal: "h-1".into(),
        sid: Some(A.into()),
        agent: true,
    };
    for tmux in [Some(vec![row("x", None, true)]), None] {
        let rig = Rig {
            hosted: vec![hosted.clone()],
            ..Rig::new(tmux.clone())
        };
        let o = rig
            .open(json!({ "what": { "kind": "look", "sid": A } }))
            .unwrap();
        assert_eq!(
            (o.landed, o.session.as_deref()),
            (Landed::Running, Some("box-cc")),
            "tmux {tmux:?}"
        );
        let line = o.line.unwrap();
        assert!(line.ends_with("-- --attach h-1"), "{line}");
        let at = o.at.map(|w| w.0).unwrap_or_default();
        assert_eq!(
            at,
            copy_core::copy_text("sessionOpen.at.managed", &[("name", "box-cc")]),
            "接上的是托管终端：照宿主写，不说 tmux"
        );
        assert!(!at.contains("tmux"), "{at}");
        let o = rig
            .open(json!({ "what": { "kind": "look", "session": "box-cc" } }))
            .unwrap();
        assert!(o.line.unwrap().ends_with("-- --attach h-1"));
        assert!(rig.ccm.borrow().is_empty() && rig.sent.borrow().is_empty());
    }
    let err = Rig::new(None)
        .open(json!({ "what": { "kind": "look", "sid": A } }))
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("place_unavailable"));
}

/// ★ Windows 那一档（没有 tmux · 只能托管 · 本机 PowerShell）：新起一个托管会话 ⇒ 起好、回接进去那一行
/// `& '<入口>' -- --attach <名>`；按 sid 接回一个托管终端同样回那一行。接回不要 tmux 载体（`--attach` 先认托管名单）。
#[test]
fn on_windows_a_hosted_session_comes_back_with_its_attach_line() {
    let rig = Rig {
        hosts: true,
        windows: true,
        ccm_out: "ccm-session=w-cc\n".into(),
        ..Rig::new(None)
    };
    let o = rig
        .open(json!({
            "what": { "kind": "new", "agent": "claude", "cwd": "C:\\w" },
            "run": "managed",
            "local": true
        }))
        .unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::Started, Some("w-cc"))
    );
    let line = o.line.unwrap();
    assert!(
        line.starts_with("& ") && line.ends_with("-- --attach w-cc"),
        "{line}"
    );
    let rig = Rig {
        hosts: true,
        windows: true,
        hosted: vec![TermEntry {
            host: crate::wire::TerminalHost::Managed,
            name: "box-cc".into(),
            terminal: "h-1".into(),
            sid: Some(A.into()),
            agent: true,
        }],
        ..Rig::new(None)
    };
    let o = rig
        .open(json!({ "what": { "kind": "look", "sid": A }, "local": true }))
        .unwrap();
    assert!(o.line.unwrap().ends_with("-- --attach h-1"));
}

/// 启动命令照那一行在哪台跑认：Windows 本机托管终端填 `C:\tools\claude.cmd` ⇒ 起好、那一跳交给 ccm 的启动器就是它；
/// 同一个值交给 POSIX 那一台 ⇒ `bad_command`。
#[test]
fn a_launch_command_naming_a_windows_path_is_judged_by_where_the_line_runs() {
    let cmd = r"C:\tools\claude.cmd";
    let rig = Rig {
        hosts: true,
        windows: true,
        ccm_out: "ccm-session=w-cc\n".into(),
        ..Rig::new(None)
    };
    let o = rig
        .open(json!({
            "what": { "kind": "new", "agent": "claude", "cwd": "C:\\w", "command": cmd },
            "run": "managed",
            "local": true
        }))
        .unwrap_or_else(|e| panic!("Windows 本机的绝对路径启动命令被拒了：{e:?}"));
    assert_eq!(o.landed, Landed::Started);
    assert!(
        rig.ccm
            .borrow()
            .iter()
            .any(|argv| argv.iter().any(|a| a == cmd)),
        "交给 ccm 的那一跳没带上这个启动命令：{:?}",
        rig.ccm.borrow()
    );
    let err = Rig::new(Some(vec![]))
        .open(new(json!({ "command": cmd })))
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("bad_command"));
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
    /// 一个码那一项：先把替身摆成那一形，回要交的入参。
    type Arrange = Box<dyn Fn(&mut Rig) -> Value>;
    let cases: Vec<(&str, Arrange)> = vec![
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
            "not_hosted",
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
        assert_eq!(err.code.as_deref(), Some(*code), "{err:?}");
        assert!(
            !err.said.trim().is_empty() && !err.said.contains(code),
            "{code}：那一句 {:?}",
            err.said
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
        assert_eq!(
            rig.open(bad.clone()).unwrap_err().code.as_deref(),
            Some("bad_args"),
            "{bad}"
        );
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

/// ★ 恢复再送起的那一项带上那一句（`--` 之后）· `--model`（记录里那一个）· 权限档；
/// 越过批准的档 · 那一家不认的档 · 不是模型名的都不给（跟随），那一句照带。
#[test]
fn a_resumed_item_carries_the_line_the_model_and_the_mode_and_drops_what_it_must() {
    let rig = Rig::new(Some(vec![]));
    rig.run(|d| resume_saying(&saying(Some("opus"), Some("acceptEdits")), "claude", d))
        .unwrap();
    let argv = rig.ccm.borrow()[0].clone();
    assert!(has_pair(&argv, "--", "-接着做\n第二行"), "{argv:?}");
    assert!(has_pair(&argv, "--model", "opus"), "{argv:?}");
    assert!(
        has_pair(&argv, "--permission-mode", "acceptEdits"),
        "{argv:?}"
    );
    for (model, mode) in [
        (Some("bad model;"), Some("bypassPermissions")),
        (None, Some("dontAsk")),
    ] {
        let rig = Rig::new(Some(vec![]));
        rig.run(|d| resume_saying(&saying(model, mode), "claude", d))
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
            .code
            .as_deref(),
        Some("already_live")
    );
    assert!(live.ccm.borrow().is_empty());
    let none = Rig::new(None);
    assert_eq!(
        none.run(|d| resume_saying(&saying(None, None), "claude", d))
            .unwrap_err()
            .code
            .as_deref(),
        Some("place_unavailable")
    );
    let rig = Rig::new(Some(vec![]));
    assert_eq!(
        rig.run(|d| resume_saying(&saying(None, None), "no-such-agent", d))
            .unwrap_err()
            .code
            .as_deref(),
        Some(NO_FIRST_PROMPT)
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
    assert_eq!(err.code.as_deref(), Some("place_unavailable"));
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
        assert_eq!(
            rig.open(args.clone()).unwrap_err().code.as_deref(),
            Some(code),
            "{args}"
        );
    }
    assert_eq!(rig.shells.borrow().len(), 1);
    assert!(rig.ccm.borrow().is_empty() && rig.sent.borrow().is_empty());
}

/// ★ 开一个带名字的 shell，名单里没有 ⇒ 宿主照这台缺省（同新起：能托管 ⇒ 托管，只有 tmux ⇒ tmux）：
/// 这台只能托管、没 tmux ⇒ 照样建（建在托管里）、回接进去那一行，不回 `place_unavailable`；两种都有 ⇒ 托管；
/// 名单里托管那一行已叫这个名 ⇒ 不建、接它的句柄；两种都没有 ⇒ `place_unavailable`，什么都不建。
#[test]
fn a_named_shell_lands_where_this_machine_defaults() {
    use crate::wire::TerminalHost::{Managed, Tmux};
    let only_managed = Rig {
        hosts: true,
        ..Rig::new(None)
    };
    let o = only_managed
        .open(json!({ "what": { "kind": "shell", "session": "main", "cwd": "/p" } }))
        .unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref(), o.agent),
        (Landed::Idle, Some("main"), None)
    );
    assert!(o.line.unwrap().ends_with("--attach main"));
    assert_eq!(*only_managed.shell_hosts.borrow(), [Managed]);
    assert_eq!(
        *only_managed.shells.borrow(),
        [("main".to_string(), Some("/p".to_string()))]
    );

    let both = Rig {
        hosts: true,
        ..Rig::new(Some(vec![]))
    };
    both.open(json!({ "what": { "kind": "shell", "session": "main" } }))
        .unwrap();
    assert_eq!(*both.shell_hosts.borrow(), [Managed], "能托管 ⇒ 托管");

    let tmux_only = Rig::new(Some(vec![]));
    tmux_only
        .open(json!({ "what": { "kind": "shell", "session": "main" } }))
        .unwrap();
    assert_eq!(*tmux_only.shell_hosts.borrow(), [Tmux], "只有 tmux ⇒ tmux");

    let there = Rig {
        hosts: true,
        hosted: vec![TermEntry {
            host: Managed,
            name: "main".into(),
            terminal: "m-main".into(),
            sid: None,
            agent: false,
        }],
        ..Rig::new(None)
    };
    let o = there
        .open(json!({ "what": { "kind": "shell", "session": "main" } }))
        .unwrap();
    assert_eq!(
        (o.landed, o.session.as_deref()),
        (Landed::Idle, Some("main"))
    );
    assert!(
        o.line.unwrap().ends_with("--attach m-main"),
        "托管那一行接句柄"
    );
    assert!(there.shells.borrow().is_empty(), "已在 ⇒ 不建");

    let neither = Rig::new(None);
    let err = neither
        .open(json!({ "what": { "kind": "shell", "session": "main" } }))
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("place_unavailable"));
    assert!(neither.shells.borrow().is_empty());
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
        Err(f) => Err(format!(
            "{}: {} {}",
            cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
            f.said,
            f.raw.as_deref().unwrap_or_default()
        )),
        Ok(o) => {
            let argv = rig.ccm.borrow().first().cloned();
            let typed = rig.sent.borrow().first().map(|(_, l)| l.clone());
            Ok(match (argv, typed) {
                (Some(argv), _) => argv
                    .iter()
                    .map(|t| crate::platform::shell::dialect::posix_word(t))
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
                for hosts in [false, true] {
                    let mut rig = Rig::new(tmux.then(Vec::new));
                    rig.fork_tmux = fork_tmux;
                    rig.hosts = hosts;
                    rig.ccm_out = "ccm-session=p-cc\n".into();
                    let extra = if forked {
                        json!({ "fork": { "sid": A, "uuid": U } })
                    } else {
                        json!({})
                    };
                    let got = rig
                        .open(new(extra))
                        .unwrap_or_else(|e| panic!("没起：{e:?}"));
                    let run = crate::faces::session_open_face::facts_run(fork_tmux, tmux, hosts);
                    let landed = match run {
                        Launch::Tmux | Launch::Managed => Landed::Started,
                        Launch::Direct => Landed::Window,
                    };
                    assert_eq!(
                        got.landed, landed,
                        "tmux {tmux} · 托管 {hosts} · 分叉 {forked} · 源在 tmux {fork_tmux:?}"
                    );
                    let hosted = rig
                        .ccm
                        .borrow()
                        .first()
                        .is_some_and(|a| a.iter().any(|w| w == "--hosted"));
                    assert_eq!(hosted, run == Launch::Managed, "{run:?}");
                }
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
    let by_sid = |s: &str| {
        Some(ArriveBy::Sid {
            sid: s.parse().unwrap(),
        })
    };
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
        opened(
            Landed::Started,
            Some(("w".into(), crate::wire::TerminalHost::Managed)),
            None,
        )
        .map(|o| Opened {
            sid: None,
            cwd: Some("/w".into()),
            agent: Some(crate::agents::AgentKind::new("codex")),
            ..o
        }),
        3,
    );
    assert_eq!(
        by_cwd.arrival.map(|a| (a.by, a.since, a.session, a.host)),
        Some((
            ArriveBy::Cwd { cwd: "/w".into() },
            3,
            Some("w".into()),
            Some(crate::wire::TerminalHost::Managed)
        )),
        "等报到那一问照抄落在的终端与宿主"
    );
    let shell = with_arrival(
        opened(Landed::Window, None, Some("cd /w".into())).map(|o| Opened {
            cwd: Some("/w".into()),
            ..o
        }),
        3,
    );
    assert_eq!(shell.arrival, None, "进目录那一行没起 agent ⇒ 不等");
}

// ── 分叉两形：起会话带 `fork` 时叉那一手照源会话那一家声明的分叉面走（`Own` 那一家自己叉 · `Rewrite` 照旧写一份新记录）。

/// 一棵目录底下的全部文件（相对路径，排好序）：判「没写任何文件」「写了恰一份」用。
fn files_under(root: &std::path::Path) -> Vec<String> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
        for name in test_home::list(dir) {
            let p = dir.join(name);
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.push(p.strip_prefix(root).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// 替身家的一份源会话：`convos/proj/sess-A.ndjson`，链上两条（`U` 是第二条）。
fn own_home() -> std::path::PathBuf {
    let home = test_home::scratch("ownfork");
    let dir = home.join("convos").join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let rows = [
        json!({"node": "11111111-1111-2222-3333-444444444444", "when": "t1", "role": "human", "words": "x"}),
        json!({"node": U, "up": "11111111-1111-2222-3333-444444444444", "when": "t2", "role": "bot"}),
    ];
    let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(dir.join(format!("sess-{A}.ndjson")), body).unwrap();
    home
}

/// 只认替身家那一行的注册表（替身家不进 `REGISTRY`）。
fn own_registry() -> [crate::agents::Adapter; 1] {
    [crate::agents::fake::full_row(crate::agents::fake::home)]
}

fn tmux_new(extra: Value) -> Value {
    let mut a = new(extra);
    a["run"] = json!("tmux");
    a
}

/// ★ `Own` 那一家：叉那一下交给它（它收到的是源 sid 与那条消息 id），起的是恢复它交回的 `new-1`；家目录底下一个文件都不多。
#[test]
fn an_own_fork_resumes_the_sid_the_agent_hands_back_and_writes_nothing() {
    let home = own_home();
    let reg = own_registry();
    let nf = crate::platform::fs::NewFile::issue();
    let fork = |s: &str, u: &str| {
        crate::control::fork_write::fork_for_launch_among(&reg, &nf, &home, s, u)
    };
    let before = files_under(&home);
    crate::agents::fake::own_fork_calls();
    let rig = Rig::new(Some(vec![]));
    let o = rig
        .open_via(tmux_new(json!({ "fork": { "sid": A, "uuid": U } })), &fork)
        .unwrap_or_else(|e| panic!("没起：{e:?}"));
    assert_eq!(o.sid.as_deref(), Some("new-1"), "起的是那一家交回的那一条");
    let argv = rig.ccm.borrow().first().cloned().unwrap_or_default();
    assert!(
        argv.iter().any(|w| w == "new-1"),
        "照恢复那一路起 new-1：{argv:?}"
    );
    assert_eq!(
        crate::agents::fake::own_fork_calls(),
        vec![(A.to_string(), U.to_string())],
        "那一家收到的是源 sid 与那条消息 id"
    );
    assert_eq!(
        files_under(&home),
        before,
        "自己叉的那一家：通用层一个文件都不写"
    );
}

/// ★ `Rewrite` 那一家（Claude）：照旧读源记录、交它改写、落一份新记录，再恢复那一份。
#[test]
fn a_rewrite_fork_writes_one_new_record_and_resumes_it() {
    let home = test_home::scratch("rwfork");
    let dir = home.join("projects").join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let first = "11111111-1111-2222-3333-444444444444";
    let rows = [
        json!({"type":"user","uuid":first,"parentUuid":null,"timestamp":"t1","sessionId":A}),
        json!({"type":"assistant","uuid":U,"parentUuid":first,"timestamp":"t2","sessionId":A}),
    ];
    let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(dir.join(format!("{A}.jsonl")), body).unwrap();
    let nf = crate::platform::fs::NewFile::issue();
    let fork = |s: &str, u: &str| {
        crate::control::fork_write::fork_for_launch_among(crate::agents::REGISTRY, &nf, &home, s, u)
    };
    let before = files_under(&home);
    let rig = Rig::new(Some(vec![]));
    let o = rig
        .open_via(tmux_new(json!({ "fork": { "sid": A, "uuid": U } })), &fork)
        .unwrap_or_else(|e| panic!("没起：{e:?}"));
    let after = files_under(&home);
    let added: Vec<&String> = after.iter().filter(|f| !before.contains(f)).collect();
    assert_eq!(added.len(), 1, "恰写一份新记录：{added:?}");
    let sid = o.sid.clone().expect("回包说得出新会话");
    assert!(
        added[0].ends_with(&format!("{sid}.jsonl")),
        "新记录就是恢复的那一条：{added:?} · {sid}"
    );
    let argv = rig.ccm.borrow().first().cloned().unwrap_or_default();
    assert!(
        argv.iter().any(|w| *w == sid),
        "照恢复那一路起新记录：{argv:?}"
    );
}

/// ★ 号 · 放哪 · 铸名有一格不过 ⇒ 不叫那一家叉（某格不行都不留东西）。叫没叫在叉那一手里自己数。
#[test]
fn a_launch_that_fails_a_gate_never_asks_the_agent_to_fork() {
    let home = own_home();
    let reg = own_registry();
    let nf = crate::platform::fs::NewFile::issue();
    let asked = std::cell::Cell::new(0u32);
    let fork = |s: &str, u: &str| {
        asked.set(asked.get() + 1);
        crate::control::fork_write::fork_for_launch_among(&reg, &nf, &home, s, u)
    };
    // 铸名：铸出来的名字这台已经有了。
    let mut taken = Rig::new(Some(vec![row("t", None, false)]));
    taken.minted = Some("t".into());
    let err = taken
        .open_via(tmux_new(json!({ "fork": { "sid": A, "uuid": U } })), &fork)
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("tmux_taken"));
    // 放哪：要 tmux 而这台没有。
    let no_tmux = Rig::new(None);
    let err = no_tmux
        .open_via(tmux_new(json!({ "fork": { "sid": A, "uuid": U } })), &fork)
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("place_unavailable"));
    // 号：点名一个账号库里没有的号。
    let rig = Rig::new(Some(vec![]));
    let err = rig
        .open_via(
            {
                let mut a = tmux_new(json!({ "fork": { "sid": A, "uuid": U } }));
                a["account"] = json!({ "kind": "named", "name": "ghost" });
                a
            },
            &fork,
        )
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("account_unavailable"));
    assert_eq!(asked.get(), 0, "有一格不过就不叫那一家叉");
    // 正控：几格都过 ⇒ 叫了恰一次（这一手确实数得到）。
    Rig::new(Some(vec![]))
        .open_via(tmux_new(json!({ "fork": { "sid": A, "uuid": U } })), &fork)
        .unwrap_or_else(|e| panic!("没起：{e:?}"));
    assert_eq!(asked.get(), 1);
}

/// ★ `Own` 交回 `ForkFail` ⇒ 码 `fork_failed`，那一句是核心照那个码查文案表出的那一句。
#[test]
fn an_own_fork_failure_comes_back_as_the_sentence_for_its_code() {
    let home = own_home();
    let reg = own_registry();
    let nf = crate::platform::fs::NewFile::issue();
    let fork = |s: &str, u: &str| {
        crate::control::fork_write::fork_for_launch_among(&reg, &nf, &home, s, u)
    };
    let missing = "77777777-1111-2222-3333-444444444444";
    let rig = Rig::new(Some(vec![]));
    let err = rig
        .open_via(
            tmux_new(json!({ "fork": { "sid": A, "uuid": missing } })),
            &fork,
        )
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("fork_failed"));
    assert_eq!(
        err.said,
        copy_core::copy_text("beSessionNew.fork.noTurn", &[("msgId", missing)])
    );
    assert!(
        err.said.contains(missing),
        "那一句带着认不出的那条消息：{}",
        err.said
    );
    assert!(
        !err.said.starts_with('〔'),
        "那一句是表里的文案、不是缺键时的键名：{}",
        err.said
    );
    assert!(rig.ccm.borrow().is_empty(), "叉不成就不起");
}

/// 替身家那一行，分叉面换成就地给的这一手（判据要它交回别的东西时用；不动替身家本身）。
fn own_registry_with(
    own: fn(&crate::agents::OwnFork<'_>) -> Result<String, crate::agents::ForkFail>,
) -> [crate::agents::Adapter; 1] {
    let [mut row] = own_registry();
    let mut records = row.records.expect("替身家有记录面");
    records.branch = Some(crate::agents::BranchFace::Own(own));
    row.records = Some(records);
    [row]
}

/// ★ `Own` 交回的 sid 由用它的这一侧判（`INVARIANTS §47`）：不像会话 id ⇒ 不起，回 `fork_failed` 与「没得到新会话」那一句。
#[test]
fn an_own_fork_handing_back_a_bad_sid_starts_nothing() {
    let home = own_home();
    let reg = own_registry_with(|_| Ok("../x; rm -rf ~".to_string()));
    let nf = crate::platform::fs::NewFile::issue();
    let fork = |s: &str, u: &str| {
        crate::control::fork_write::fork_for_launch_among(&reg, &nf, &home, s, u)
    };
    let rig = Rig::new(Some(vec![]));
    let err = rig
        .open_via(tmux_new(json!({ "fork": { "sid": A, "uuid": U } })), &fork)
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("fork_failed"));
    assert_eq!(
        err.said,
        copy_core::copy_text("beSessionNew.fork.failed", &[("sid", A)])
    );
    assert!(err.said.contains(A), "那一句带着源会话：{}", err.said);
    assert!(rig.ccm.borrow().is_empty(), "交回的 sid 不合格就不起");
}

/// ★ 自己叉的那一家不写分叉记录：问它要改写那一形（`session-fork` 帧面走到这一步）⇒ 照实拒。
#[test]
fn an_own_fork_agent_is_refused_a_branch_record() {
    let reg = own_registry();
    let lines = [json!({"node": U, "when": "t", "role": "bot"})];
    let got = crate::agents::build_branch_records(
        &reg,
        crate::agents::fake::AGENT_KIND,
        &lines,
        U,
        A,
        "N",
    );
    assert!(got.is_err(), "自己叉的那一家不出分叉记录：{got:?}");
}
