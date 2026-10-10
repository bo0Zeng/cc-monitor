//! 换号重启：停失败一定不起 · 压缩等摘要（真记录文件、真 inotify）· 等压缩时撤单不停不起 · 回复与各码（替身记下被调了什么；tmux 一个都不起）。

use super::*;
use crate::control::launch_render::local;
use crate::control::session_batch::TmuxEntry;
use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SID: &str = "aaaaaaaa-1111-2222-3333-444444444444";

/// 一条压缩摘要（结构同那一家写的，正文是占位）。
const SUMMARY: &str = r#"{"type":"user","isCompactSummary":true,"message":{"role":"user","content":"S"},"uuid":"u-s"}"#;

fn row(name: &str, sid: &str, agent: bool) -> TmuxEntry {
    TmuxEntry {
        name: name.into(),
        terminal: format!("tmux-{name}"),
        sid: Some(sid.to_string()),
        agent,
    }
}

/// 替身：名单给定；送字 / 杀 / 起只记下来。`summarize` ⇒ 收到压缩那一句时往记录里写一条摘要（像那一家压完了）。
struct Rig {
    rows: Vec<TmuxEntry>,
    record: PathBuf,
    summarize: bool,
    kill_err: Option<&'static str>,
    ccm_exit: i32,
    /// 持着这条会话的活进程：停旧之前 · 停旧之后（替身的 pidfile）。
    live_before: Vec<u32>,
    live_after: Vec<u32>,
    calls: Mutex<Vec<String>>,
}

#[derive(Clone)]
struct Host_(Arc<Rig>);

impl Rig {
    fn new(tag: &str) -> Rig {
        let dir = std::env::temp_dir().join(format!("ccm-restart-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join(format!("{SID}.jsonl"));
        std::fs::write(
            &record,
            "{\"type\":\"user\",\"message\":{\"content\":\"x\"}}\n",
        )
        .unwrap();
        Rig {
            rows: vec![row("proj-cc", SID, true)],
            record,
            summarize: false,
            kill_err: None,
            ccm_exit: 0,
            live_before: vec![],
            live_after: vec![],
            calls: Mutex::new(vec![]),
        }
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn did(&self, what: &str) -> usize {
        self.calls().iter().filter(|c| c.starts_with(what)).count()
    }
    fn deps<T>(&self, f: impl FnOnce(&Deps<'_>) -> T) -> T {
        let caps: BTreeSet<String> = crate::ccm_launcher_with(crate::TMUX_PLATFORM)
            .into_iter()
            .map(str::to_string)
            .collect();
        let note = |s: String| self.calls.lock().unwrap().push(s);
        let list = || -> Result<Option<Vec<TmuxEntry>>, String> { Ok(Some(self.rows.clone())) };
        let record = |_: &str, _: Option<&str>| -> Result<(bool, String), String> {
            Ok((true, String::new()))
        };
        let kill = |name: &str, sid: &str| -> Result<Value, crate::stream::inbound::spec::Fail> {
            note(format!("kill {name} {sid}"));
            match self.kill_err {
                Some(c) => Err(crate::stream::inbound::spec::Fail::new(
                    c,
                    "said".to_string(),
                )),
                None => Ok(json!({})),
            }
        };
        let send_into =
            |name: &str, sid: &str, line: &str| -> Result<(), crate::stream::inbound::spec::Fail> {
                note(format!("send {name} {sid} {line}"));
                if self.summarize {
                    let mut f = std::fs::OpenOptions::new()
                        .append(true)
                        .open(&self.record)
                        .unwrap();
                    writeln!(f, "{SUMMARY}").unwrap();
                }
                Ok(())
            };
        let run_ccm = |argv: &[String]| -> Result<(i32, String, String), batch::CmdErr> {
            note(format!("ccm {}", argv.join(" ")));
            Ok((self.ccm_exit, String::new(), "boom".to_string()))
        };
        let mint =
            |_: batch::NameBase| -> Result<String, batch::CmdErr> { Ok("minted".to_string()) };
        let library = || {
            la::Library::of_product(&json!({ "meta": {"enabled": true}, "accounts": [{
                "name": "work", "configDir": "/h/.cc/work", "isDefault": false, "mode": "isolated",
                "exists": true, "authReady": true,
            }]}))
        };
        let facts = la::Facts {
            has_accounts: true,
            library: &library,
            last: &|_| None,
        };
        let writers = |_: &str| -> Vec<u32> {
            if self.did("kill") > 0 {
                self.live_after.clone()
            } else {
                self.live_before.clone()
            }
        };
        let pretrust = |dir: &str, cwd: &str| note(format!("pretrust {dir} {cwd}"));
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
            accounts: &facts,
            writers: &writers,
            pretrust: &pretrust,
        })
    }
}

/// 一次装好的假等待：「报出」那一格由替身直接答。
struct Said(bool);
impl Wait for Said {
    async fn within(self, _ms: u64) -> bool {
        self.0
    }
}

/// 两种等待在替身里的样子：压缩摘要那一种是真的（盯真记录文件），报出那一种恒「到了」。
enum Ears {
    Real(crate::observe::one_wait::Armed),
    Fake(Said),
}
impl Wait for Ears {
    async fn within(self, ms: u64) -> bool {
        match self {
            Ears::Real(a) => a.within(ms).await,
            Ears::Fake(s) => s.within(ms).await,
        }
    }
}

impl Host for Host_ {
    type Ears = Ears;
    async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Deps<'_>) -> T + Send + 'static,
    ) -> T {
        self.0.deps(f)
    }
    async fn critical<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Deps<'_>) -> T + Send + 'static,
    ) -> Option<T> {
        Some(self.0.deps(f))
    }
    fn compact_request(&self, agent: &str) -> Option<&'static str> {
        crate::agents::compact_request_of(agent)
    }
    async fn watch_compact(&self, _sid: &str) -> Result<Ears, String> {
        crate::observe::one_wait::record_line(&self.0.record, |v| {
            crate::agents::is_compact_summary(crate::agents::record_tree_kind().unwrap(), v)
        })
        .map(Ears::Real)
    }
    async fn watch_arrival(&self, _sid: &str) -> Result<Ears, String> {
        Ok(Ears::Fake(Said(true)))
    }
}

fn args(compact_first: bool, compact_ms: u64) -> Value {
    json!({
        "sid": SID, "cwd": "/p", "account": "work",
        "compact_first": compact_first, "compact_within_ms": compact_ms, "arrive_within_ms": 1000,
        "local": false, "agent": "claude", "launcher": "claude", "defaultLauncher": "claude",
        "models": { "work": "opus" },
    })
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_time()
        .build()
        .unwrap()
}

fn go(rig: Rig, a: Value) -> (Result<Value, Fault>, Arc<Rig>) {
    let rig = Arc::new(rig);
    let out = rt().block_on(run(a, None, Host_(rig.clone())));
    (out, rig)
}

/// 停失败 ⇒ 一定没起新的（ccm 零调用），码 `stop_failed`、`data` 带杀那一步的原因码。
#[test]
fn a_failed_stop_never_starts_the_new_one() {
    let mut rig = Rig::new("stopfail");
    rig.kill_err = Some("wrong_owner");
    let (out, rig) = go(rig, args(false, 0));
    let (code, _, data) = out.expect_err("停失败却回了成功");
    assert_eq!(code, "stop_failed");
    assert_eq!(data, Some(json!({ "why": "wrong_owner" })));
    assert_eq!(rig.did("kill"), 1);
    assert_eq!(rig.did("ccm"), 0, "停失败还起了新的：{:?}", rig.calls());
    assert_eq!(rig.did("pretrust"), 0, "没起却预标了：{:?}", rig.calls());
}

/// 一路走通：停那个终端、在同一个名字里用点名的号起（带那台偏好表里那个号的模型），回 `{compact, started, terminal, account}`。
#[test]
fn a_restart_stops_and_starts_under_the_same_terminal_name() {
    let (out, rig) = go(Rig::new("ok"), args(false, 0));
    let v = out.expect("应成功");
    assert_eq!(v["compact"], "skipped");
    assert_eq!(v["started"], "arrived");
    assert_eq!(v["terminal"], "proj-cc");
    assert_eq!(v["account"]["name"], "work");
    assert_eq!(v["account"]["model"], "opus");
    let calls = rig.calls();
    assert_eq!(calls[0], format!("kill proj-cc {SID}"));
    assert_eq!(
        calls[1], "pretrust /h/.cc/work /p",
        "新号起之前先把工作目录标成它信任过：{calls:?}"
    );
    assert!(
        calls[2].starts_with("ccm ") && calls[2].contains("proj-cc"),
        "{calls:?}"
    );
    assert_eq!(rig.did("send"), 0, "没要求压缩却送了那一句");
}

/// 起失败（旧的已停）⇒ `start_failed`，`data` 带终端名。
#[test]
fn a_failed_start_says_which_terminal() {
    let mut rig = Rig::new("startfail");
    rig.ccm_exit = 1;
    let (out, _) = go(rig, args(false, 0));
    let (code, _, data) = out.expect_err("起失败却回了成功");
    assert_eq!(code, "start_failed");
    let data = data.unwrap();
    assert_eq!(
        (&data["terminal"], &data["stopped"]),
        (&json!("proj-cc"), &json!(true))
    );
}

/// 号选不了 ⇒ `account_unavailable`、什么都不动；不在终端里 ⇒ `not_in_terminal`；多个在跑 ⇒ `ambiguous`（`data.names`）。
#[test]
fn nothing_moves_when_the_account_or_the_terminal_is_not_there() {
    let mut a = args(true, 1000);
    a["account"] = json!("nope");
    let (out, rig) = go(Rig::new("acct"), a);
    assert_eq!(out.unwrap_err().0, "account_unavailable");
    assert!(rig.calls().is_empty(), "{:?}", rig.calls());

    let mut rig = Rig::new("none");
    rig.rows = vec![row("proj-cc", SID, false)];
    let (out, rig) = go(rig, args(true, 1000));
    assert_eq!(out.unwrap_err().0, "not_in_terminal");
    assert!(rig.calls().is_empty());

    let mut rig = Rig::new("two");
    rig.rows = vec![row("a-cc", SID, true), row("b-cc", SID, true)];
    let (out, rig) = go(rig, args(true, 1000));
    let (code, _, data) = out.unwrap_err();
    assert_eq!(code, "ambiguous");
    assert_eq!(data, Some(json!({ "names": ["a-cc", "b-cc"] })));
    assert!(rig.calls().is_empty());
}

/// 压缩：期限内记录里长出摘要 ⇒ `done`、很快往下；不长 ⇒ 到点 `timed_out`（不早退、不拖）且照常重启。
#[test]
fn compact_waits_for_the_summary_and_goes_on_at_the_deadline() {
    let mut rig = Rig::new("compact-done");
    rig.summarize = true;
    let t0 = Instant::now();
    let (out, rig) = go(rig, args(true, 10_000));
    let took = t0.elapsed();
    let v = out.expect("应成功");
    assert_eq!(v["compact"], "done");
    assert!(
        took < Duration::from_millis(5_000),
        "摘要到了却拖到 {took:?}"
    );
    assert_eq!(rig.did("send proj-cc"), 1);
    assert!(rig.calls().iter().any(|c| c.ends_with("/compact")));

    let t0 = Instant::now();
    let (out, rig) = go(Rig::new("compact-late"), args(true, 600));
    let took = t0.elapsed();
    let v = out.expect("压缩超时也要照常重启");
    assert_eq!(v["compact"], "timed_out");
    assert!(took >= Duration::from_millis(600), "早退：{took:?}");
    assert!(took < Duration::from_millis(3_000), "拖了：{took:?}");
    assert_eq!(
        (rig.did("kill"), rig.did("ccm")),
        (1, 1),
        "{:?}",
        rig.calls()
    );
}

/// 撤单：在等压缩时撤 ⇒ 不停、不起（撤了之后摘要才来也一样）。
#[test]
fn cancelling_while_waiting_for_the_summary_stops_nothing_and_starts_nothing() {
    let rig = Arc::new(Rig::new("cancel"));
    let host = Host_(rig.clone());
    let rt = rt();
    rt.block_on(async {
        let task = tokio::spawn(run(args(true, 60_000), None, host));
        // 等到那一句送出去（此刻正在等摘要），再撤。
        for _ in 0..200 {
            if rig.did("send") == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(rig.did("send"), 1, "没走到等压缩那一步");
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    });
    // 撤了之后摘要才来：也没有谁接着做下去。
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&rig.record)
        .unwrap();
    writeln!(f, "{SUMMARY}").unwrap();
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(
        (rig.did("kill"), rig.did("ccm")),
        (0, 0),
        "{:?}",
        rig.calls()
    );
}

/// 它是可撤档：hello 的「撤不动」那张表里没有它。
#[test]
fn session_restart_is_cancellable() {
    assert!(crate::stream::inbound::command_names().contains(&"session-restart"));
    assert!(!crate::stream::inbound::uncancellable().contains(&"session-restart".to_string()));
}

/// 写这条会话的活进程不止一个（终端里那个之外另有一个）⇒ `session_already_live`（`data.pids`），不停、不起。
#[test]
fn two_live_writers_refuse_the_restart_before_anything_moves() {
    let mut rig = Rig::new("two-writers");
    rig.live_before = vec![11, 12];
    let (out, rig) = go(rig, args(true, 1000));
    let (code, _, data) = out.unwrap_err();
    assert_eq!(code, "session_already_live");
    assert_eq!(data, Some(json!({ "pids": [11, 12] })));
    assert_eq!(
        rig.did("kill") + rig.did("ccm") + rig.did("send"),
        0,
        "{:?}",
        rig.calls()
    );
}

/// 停完旧的再核：自己停掉的那个（开动前就在写的）还没退干净不算 ⇒ 照常起；
/// 停的这段时间里另有一个进程接上了这条会话 ⇒ `session_already_live`、旧的已停、新的不起。
#[test]
fn after_the_stop_only_someone_else_still_writing_holds_the_start() {
    let mut rig = Rig::new("own-writer");
    rig.live_before = vec![11];
    rig.live_after = vec![11];
    let (out, rig) = go(rig, args(false, 0));
    assert!(out.is_ok(), "自己停掉的那个把自己拦住了：{out:?}");
    assert_eq!(rig.did("ccm"), 1);

    let mut rig = Rig::new("new-writer");
    rig.live_before = vec![11];
    rig.live_after = vec![11, 13];
    let (out, rig) = go(rig, args(false, 0));
    let (code, _, data) = out.unwrap_err();
    assert_eq!(code, "session_already_live");
    assert_eq!(data, Some(json!({ "pids": [13], "stopped": true })));
    assert_eq!(rig.did("kill"), 1);
    assert_eq!(
        rig.did("ccm"),
        0,
        "另有进程在写，还起了新的：{:?}",
        rig.calls()
    );
}
