//! 整台一张活会话表：现扫那一份（pidfile ＋ 判活）与几个读口的判法。

use super::*;
use crate::agents::SessionActivity as A;
use std::fs;

#[cfg(unix)]
#[test]
fn the_scan_keeps_the_pidfiles_whose_process_is_still_there_as_live() {
    let home = std::env::temp_dir().join(format!("ccm-live-table-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let sessions = home.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let me = std::process::id();
    // 记下自己的启动时刻：身份核上就是活的，不靠测试二进制的路径像不像那一家。
    let ticks = crate::platform::proc::proc_starttime(me).expect("读得到自己的启动时刻");
    fs::write(
        sessions.join(format!("{me}.json")),
        format!(r#"{{"sessionId":"live-sid","cwd":"/w","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    fs::write(
        sessions.join("4194304.json"),
        r#"{"sessionId":"dead-sid","cwd":"/w"}"#,
    )
    .unwrap();
    fs::write(
        sessions.join("notapid.json"),
        r#"{"sessionId":"ignored","cwd":"/w"}"#,
    )
    .unwrap();
    let t = LiveTable::scan(&home);
    assert_eq!(
        t.sids(),
        ["live-sid".to_string()].into_iter().collect(),
        "活着的那一个要在、死了的那一个不许在"
    );
    assert_eq!(
        t.rows().len(),
        2,
        "死了的那一行也在表里（账号归属要说它死了）"
    );
    // 没有 sessionId 的活进程不算（它说不出是哪个会话）。
    fs::write(sessions.join(format!("{me}.json")), r#"{"cwd":"/w"}"#).unwrap();
    assert!(LiveTable::scan(&home).sids().is_empty());
    let _ = fs::remove_dir_all(&home);
}

/// 起一个 argv0 是 `argv0` 的 `sleep`，在 `sessions` 里照它写一份 pidfile（`procStart` 不给 ⇒ 记它此刻真的启动时刻）。
#[cfg(target_os = "linux")]
fn kid_with_pidfile(
    sessions: &std::path::Path,
    argv0: &str,
    sid: &str,
    proc_start: Option<&str>,
) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    let kid = crate::platform::proc::test_support::spawn_settled_sleep(|c| {
        c.arg0(argv0);
    });
    let pid = kid.id();
    let start = proc_start.map_or_else(
        || {
            crate::platform::proc::proc_starttime(pid)
                .expect("读得到子进程的启动时刻")
                .to_string()
        },
        str::to_string,
    );
    fs::write(
        sessions.join(format!("{pid}.json")),
        format!(r#"{{"sessionId":"{sid}","cwd":"/w","procStart":"{start}"}}"#),
    )
    .unwrap();
    kid
}

/// 守护池停泊的备用进程（cmdline 带 `bg-spare`）是那一家的进程、不是一条会话：启动时刻核上了也不算活。
#[cfg(target_os = "linux")]
#[test]
fn a_parked_spare_is_not_a_live_session_even_when_its_start_time_checks_out() {
    let home = std::env::temp_dir().join(format!("ccm-live-spare-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let sessions = crate::agents::IntoHomes::into_homes(&home)[0]
        .pidfile_dir()
        .unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let mut kid = kid_with_pidfile(&sessions, "bg-spare", "spare-sid", None);
    let t = LiveTable::scan(&home);
    let _ = kid.kill();
    let _ = kid.wait();
    let _ = fs::remove_dir_all(&home);
    assert!(t.sids().is_empty(), "bg-spare 判成了活会话：{:?}", t.rows());
}

/// 启动时刻记法对不上（Windows 上 pidfile 记 .NET ticks、这台读的是 FILETIME，恒不等），进程像那一家、起得不比 pidfile 晚 ⇒ 活。
#[cfg(target_os = "linux")]
#[test]
fn a_session_whose_recorded_start_is_in_another_format_is_still_live() {
    let home = std::env::temp_dir().join(format!("ccm-live-fmt-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let sessions = crate::agents::IntoHomes::into_homes(&home)[0]
        .pidfile_dir()
        .unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let mut kid = kid_with_pidfile(&sessions, "claude", "fmt-sid", Some("1"));
    let t = LiveTable::scan(&home);
    let _ = kid.kill();
    let _ = kid.wait();
    let _ = fs::remove_dir_all(&home);
    assert_eq!(
        t.sids(),
        ["fmt-sid".to_string()].into_iter().collect(),
        "记法对不上就判不活：{:?}",
        t.rows()
    );
    assert!(
        t.rows().iter().all(|r| !r.start_checked),
        "启动时刻没核上，账号归属不许认它"
    );
}

fn row(pid: u32, sid: &str, activity: Option<A>, frozen: bool) -> LiveRow {
    LiveRow {
        pid,
        fact: crate::agents::ProcFact {
            sid: Some(sid.to_string()),
            activity,
            ..Default::default()
        },
        alive: true,
        start_checked: true,
        frozen,
    }
}

#[test]
fn a_pending_ask_makes_a_live_session_need_you_whatever_its_pidfile_says() {
    let t = of_rows(vec![row(1, "s", Some(A::Working), false)], &["s", "gone"]);
    assert_eq!(t.doing_one("s"), Some(Some(A::NeedsYou)));
    assert_eq!(
        t.needs(),
        vec!["s".to_string()],
        "挂着请求、已经不活的那条不算"
    );
    assert!(t.waiting("s"));
    assert_eq!(t.doing_one("gone"), None);
}

#[test]
fn a_frozen_busy_process_is_not_said_to_be_working() {
    let t = of_rows(
        vec![
            row(1, "s", Some(A::Working), true),
            row(2, "s", Some(A::Idle), false),
        ],
        &[],
    );
    assert_eq!(
        t.doing_one("s"),
        Some(Some(A::Idle)),
        "冻住的那一个不说在跑，取另一个"
    );
    assert_eq!(t.doing().get("s"), Some(&Some(A::Idle)));
}

#[test]
fn several_processes_take_the_most_urgent_state() {
    let t = of_rows(
        vec![
            row(1, "s", Some(A::Idle), false),
            row(2, "s", Some(A::NeedsYou), false),
        ],
        &[],
    );
    assert_eq!(t.doing_one("s"), Some(Some(A::NeedsYou)));
    assert_eq!(t.writers("s"), vec![1, 2]);
}

#[test]
fn since_of_is_the_latest_state_start_among_live_processes() {
    let at = |pid, since: Option<u64>, alive| LiveRow {
        alive,
        fact: crate::agents::ProcFact {
            status_since_ms: since,
            ..row(pid, "s", Some(A::Idle), false).fact
        },
        ..row(pid, "s", Some(A::Idle), false)
    };
    let t = of_rows(
        vec![
            at(1, Some(10), true),
            at(2, Some(30), true),
            at(3, Some(99), false),
            at(4, None, true),
        ],
        &[],
    );
    assert_eq!(t.since_of("s"), Some(30), "活着的里最晚那一个；死了的不算");
    assert_eq!(t.since_of("other"), None);
}

/// 说在等的一行：等的是 `wait`，从 `since` 起。
fn waits(pid: u32, wait: crate::agents::WaitOn, since: u64) -> LiveRow {
    let r = row(pid, "s", Some(A::NeedsYou), false);
    LiveRow {
        fact: crate::agents::ProcFact {
            wait: Some(wait),
            status_since_ms: Some(since),
            ..r.fact.clone()
        },
        ..r
    }
}

/// 挂着请求、pidfile 说在跑 ⇒ 合起来在等人，pidfile 那一格没有（整份由请求那一格造）；
/// pidfile 也在等 ⇒ 交那一份（那一步照记录配，请求由调用方挂上去）。
#[test]
fn a_pending_ask_over_a_busy_pidfile_leaves_the_wait_to_the_ask() {
    // 挂着请求、pidfile 说在等却比钩子事件旧（被盖过了）⇒ 那一份不交（同 [`doing_heard`] 认新旧那一判），由请求说。
    let mut stale = of_rows(vec![waits(1, crate::agents::WaitOn::Dialog, 5)], &["s"]);
    stale.heard.insert(
        "s".into(),
        Heard {
            event: crate::agents::AgentEvent::TurnEnded,
            at_ms: 10,
        },
    );
    assert_eq!(
        stale.wait_of("s"),
        None,
        "被钩子事件盖过的旧 pidfile 还在说等的是什么"
    );
    let t = of_rows(vec![row(1, "s", Some(A::Working), false)], &["s"]);
    assert_eq!(t.doing_one("s"), Some(Some(A::NeedsYou)));
    assert_eq!(t.wait_of("s"), None);
    let t = of_rows(vec![waits(1, crate::agents::WaitOn::Dialog, 5)], &["s"]);
    assert_eq!(
        t.wait_of("s").and_then(|w| w.waiting_for),
        Some(crate::agents::WaitOn::Dialog)
    );
}

/// ★ 没挂请求 ⇒ 照合起来那一态：钩子事件比 pidfile 说在等那一刻新、说一轮停了 ⇒ 不在等，pidfile 那一格不交；
/// 合起来是在等 ⇒ 说在等的那几份里等得最早的那一份。
#[test]
fn without_an_ask_what_is_awaited_follows_the_combined_state() {
    use crate::agents::WaitOn as W;
    let mut t = of_rows(vec![waits(1, W::Dialog, 30), waits(2, W::Input, 10)], &[]);
    assert_eq!(
        t.wait_of("s").and_then(|w| w.waiting_for),
        Some(W::Input),
        "该取等得最早的那一份"
    );
    t.heard.insert(
        "s".into(),
        Heard {
            event: crate::agents::AgentEvent::TurnEnded,
            at_ms: 99,
        },
    );
    assert_eq!(t.doing_one("s"), Some(Some(A::Idle)));
    assert_eq!(
        t.wait_of("s"),
        None,
        "合起来不在等，等的是什么却还照 pidfile 说"
    );
}

fn of_rows(rows: Vec<LiveRow>, asked: &[&str]) -> LiveTable {
    LiveTable {
        home: std::path::PathBuf::new(),
        rows,
        asked: asked.iter().map(|s| (*s).to_string()).collect(),
        reconnectable: Default::default(),
        heard: Default::default(),
    }
}

/// 发布了的那一份里收到的 `needs_changed` 那一声（只认 `sids` 里的，别的判据也在敲）。
fn heard_after(
    rx: &mut tokio::sync::broadcast::Receiver<Option<String>>,
    sids: &[&str],
) -> BTreeSet<String> {
    let mut got = BTreeSet::new();
    while let Ok(m) = rx.try_recv() {
        if let Some(s) = m.filter(|s| sids.contains(&s.as_str())) {
            got.insert(s);
        }
    }
    got
}

#[test]
fn a_pidfile_that_stops_waiting_rings_the_needs_bell_when_the_table_is_published() {
    let home = std::env::temp_dir().join(format!("ccm-live-bell-{}", std::process::id()));
    let mine = ["bell-waits", "bell-works", "bell-leaves"];
    let table = |rows: Vec<LiveRow>| LiveTable {
        home: home.clone(),
        ..of_rows(rows, &[])
    };
    publish(table(vec![
        row(1, "bell-waits", Some(A::NeedsYou), false),
        row(2, "bell-works", Some(A::Working), false),
        row(3, "bell-leaves", Some(A::Working), false),
    ]));
    let mut rx = crate::history::needs_changed::changes().subscribe();
    publish(table(vec![
        row(1, "bell-waits", Some(A::Working), false),
        row(2, "bell-works", Some(A::Working), false),
    ]));
    assert_eq!(
        heard_after(&mut rx, &mine),
        ["bell-leaves", "bell-waits"]
            .into_iter()
            .map(String::from)
            .collect(),
        "从等人写成在跑 ⇒ 响；离开「活」⇒ 响；一直在跑 ⇒ 不响"
    );
    publish(table(vec![
        row(1, "bell-waits", Some(A::Working), false),
        row(2, "bell-works", Some(A::NeedsYou), false),
    ]));
    assert_eq!(
        heard_after(&mut rx, &mine),
        ["bell-works".to_string()].into_iter().collect(),
        "从在跑写成等人 ⇒ 响"
    );
}

// === 事件这一档：那一家的钩子交来的统一事件压过 pidfile，pidfile 比事件新才照它 ===

fn ev(event: crate::agents::AgentEvent, at_ms: u64) -> Option<Heard> {
    Some(Heard { event, at_ms })
}

#[test]
fn with_the_pidfile_path_off_the_events_alone_say_working_and_idle() {
    use crate::agents::AgentEvent as E;
    let none: [(Option<A>, Option<u64>); 0] = [];
    assert_eq!(
        doing_heard(none, false, ev(E::PromptSubmitted, 10)),
        Some(A::Working)
    );
    assert_eq!(
        doing_heard(none, false, ev(E::TurnEnded, 20)),
        Some(A::Idle)
    );
    assert_eq!(
        doing_heard(none, true, ev(E::PromptSubmitted, 10)),
        Some(A::NeedsYou),
        "挂着的请求（等批准 · 在问）照旧最先"
    );
    assert_eq!(
        doing_heard(none, false, None),
        None,
        "没事件 · 没 pidfile ⇒ 说不清"
    );
}

#[test]
fn a_pidfile_state_counts_only_when_it_is_newer_than_the_event() {
    use crate::agents::AgentEvent as E;
    assert_eq!(
        doing_heard([(Some(A::Working), Some(5))], false, ev(E::TurnEnded, 10)),
        Some(A::Idle),
        "pidfile 说在跑却比一轮结束旧 ⇒ 闲着"
    );
    assert_eq!(
        doing_heard([(Some(A::Working), None)], false, ev(E::TurnEnded, 10)),
        Some(A::Idle),
        "pidfile 没写时刻 ⇒ 当它旧"
    );
    assert_eq!(
        doing_heard(
            [(Some(A::Idle), Some(20))],
            false,
            ev(E::PromptSubmitted, 10)
        ),
        Some(A::Idle),
        "Esc 打断不发 Stop：pidfile 更新的闲着照它"
    );
    assert_eq!(
        doing_heard(
            [(Some(A::BackgroundWork), Some(20))],
            false,
            ev(E::TurnEnded, 10)
        ),
        Some(A::BackgroundWork),
        "一轮停了、后台 shell 还在跑：事件说不出，pidfile 更新照它"
    );
    assert_eq!(
        doing_heard(
            [(Some(A::Idle), Some(5)), (Some(A::NeedsYou), Some(20))],
            false,
            ev(E::PromptSubmitted, 10)
        ),
        Some(A::NeedsYou),
        "比事件新的那几个里取最要紧的；旧的不算"
    );
    assert_eq!(
        doing_heard([(None, Some(5))], false, ev(E::PromptSubmitted, 10)),
        None,
        "说在跑、可那几个进程都冻住了（交来的是说不清）⇒ 事件也不说在跑"
    );
    assert_eq!(
        doing_heard([(None, Some(5))], false, ev(E::TurnEnded, 10)),
        Some(A::Idle),
        "冻住只管「在跑」"
    );
    assert_eq!(
        doing_heard([(Some(A::Idle), Some(5))], false, None),
        Some(A::Idle),
        "没事件（不是 ccm 起的）⇒ 照旧照 pidfile"
    );
}

#[test]
fn the_table_takes_the_heard_event_over_an_older_pidfile() {
    use crate::agents::AgentEvent as E;
    let mut t = of_rows(vec![row(1, "s", Some(A::Working), false)], &[]);
    t.heard.insert(
        "s".into(),
        Heard {
            event: E::TurnEnded,
            at_ms: 10,
        },
    );
    assert_eq!(t.doing_one("s"), Some(Some(A::Idle)));
    assert_eq!(t.doing().get("s"), Some(&Some(A::Idle)));
}

#[test]
fn hearing_an_event_records_it_and_rings_that_sessions_bell() {
    use crate::agents::AgentEvent as E;
    let mut rx = crate::history::needs_changed::changes().subscribe();
    hear("hear-rings", E::TurnEnded);
    assert_eq!(
        lock(&HEARD).0.get("hear-rings").map(|h| h.event),
        Some(E::TurnEnded)
    );
    assert_eq!(
        heard_after(&mut rx, &["hear-rings"]),
        ["hear-rings".to_string()].into_iter().collect()
    );
    hear("hear-rings", E::PromptSubmitted);
    assert_eq!(
        lock(&HEARD).0.get("hear-rings").map(|h| h.event),
        Some(E::PromptSubmitted),
        "只记最后一件"
    );
}

#[test]
fn only_the_latest_sessions_events_are_kept() {
    use crate::agents::AgentEvent as E;
    let mut book = HeardBook::default();
    for i in 0..=HEARD_KEEP {
        book.put(&format!("keep-{i}"), E::TurnEnded, i as u64);
    }
    assert_eq!(book.0.len(), HEARD_KEEP);
    assert!(!book.0.contains_key("keep-0"), "最早那条挤掉");
    assert!(book.0.contains_key(&format!("keep-{HEARD_KEEP}")));
}

// === 判活那一判（原 watcher 加会话那一道闸的判据，判法挪来这里） ===

#[test]
fn imposter_when_proc_started_after_pidfile() {
    // pidfile last written at t=1000, process started at t=2000 (> 1000+60).
    let v = add_time_verdict(
        crate::agents::default_kind(),
        false,
        Some(2000),
        Some(1000),
        None,
    );
    assert_eq!(v, AddTimeVerdict::Imposter("started-after-pidfile"));
    // Reboot case is the same shape: old mtime, post-boot start.
    let v2 = add_time_verdict(
        crate::agents::default_kind(),
        false,
        Some(1_700_000_000),
        Some(1_600_000_000),
        Some("claude"),
    );
    assert_eq!(
        v2,
        AddTimeVerdict::Imposter("started-after-pidfile"),
        "time evidence must win even with a claude-looking cmdline (a NEW claude did not write the OLD pidfile)"
    );
}

#[test]
fn alive_within_tolerance() {
    // Started slightly after mtime but inside the 60s fuzz window.
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            Some(1030),
            Some(1000),
            None
        ),
        AddTimeVerdict::Alive
    );
    // Started before mtime (the normal case: claude starts, then writes).
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            Some(900),
            Some(1000),
            None
        ),
        AddTimeVerdict::Alive
    );
}

#[test]
fn imposter_by_cmdline() {
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            None,
            None,
            Some("tmux new-session -d")
        ),
        AddTimeVerdict::Imposter("cmdline")
    );
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            Some(900),
            Some(1000),
            Some("-bash")
        ),
        AddTimeVerdict::Imposter("cmdline"),
        "time check passing must not mask a non-claude cmdline"
    );
    // 并进来的样本（原 `agents/claudecode/liveness_tests.rs`，那一族退役）：
    // 编辑器 · 登录 shell · sshd 会话 —— PID 被复用时最常见的那几种「明显不像」。
    for foreign in ["/usr/bin/vim", "bash -l", "sshd: u@pts/0"] {
        assert_eq!(
            add_time_verdict(
                crate::agents::default_kind(),
                false,
                None,
                None,
                Some(foreign)
            ),
            AddTimeVerdict::Imposter("cmdline"),
            "{foreign}"
        );
    }
}

#[test]
fn imposter_by_bg_spare_before_exact_identity() {
    // bg-spare 优先于 exact-identity——即便 procStart 自洽（recorded==current）
    // 也判 Imposter（否则守护池停泊备用进程恒绿）。
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            true,
            None,
            None,
            Some("claude bg-spare")
        ),
        AddTimeVerdict::Imposter("not-a-session"),
        "bg-spare 必须在 exact-identity Alive 之前拦下"
    );
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            None,
            None,
            Some("/usr/bin/claude bg-spare --foo")
        ),
        AddTimeVerdict::Imposter("not-a-session")
    );
    // 普通 claude 会话不受影响（procStart 自洽仍 Alive）。
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            true,
            None,
            None,
            Some("claude --resume x")
        ),
        AddTimeVerdict::Alive
    );
}

#[test]
fn claude_like_cmdlines_pass() {
    for cmd in [
        "claude --resume abc",
        "/usr/bin/node /home/u/.local/bin/claude",
        "NODE_OPTIONS=x node cli.js",
        // 只看得见解释器那一形（`liveness.rs` 头注：词表里有 `node` 就是为它）——
        // 解释器不在行首（「`node` 只认行首」的写法两族都会放过这一格）。
        "/usr/bin/node cli.js",
        "node index.js",
        "Claude", // case-insensitive
    ] {
        assert_eq!(
            add_time_verdict(
                crate::agents::default_kind(),
                false,
                Some(900),
                Some(1000),
                Some(cmd)
            ),
            AddTimeVerdict::Alive,
            "{cmd}"
        );
    }
}

#[test]
fn missing_data_degrades_to_allow() {
    assert_eq!(
        add_time_verdict(crate::agents::default_kind(), false, None, None, None),
        AddTimeVerdict::Alive
    );
    assert_eq!(
        add_time_verdict(crate::agents::default_kind(), false, Some(2000), None, None),
        AddTimeVerdict::Alive
    );
    assert_eq!(
        add_time_verdict(crate::agents::default_kind(), false, None, Some(1000), None),
        AddTimeVerdict::Alive
    );
    // Empty cmdline (kernel threads read as empty) is not evidence.
    assert_eq!(
        add_time_verdict(crate::agents::default_kind(), false, None, None, Some("")),
        AddTimeVerdict::Alive
    );
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            None,
            None,
            Some("   ")
        ),
        AddTimeVerdict::Alive
    );
}

#[test]
fn procstart_identity_match_short_circuits_all_heuristics() {
    // Recorded ticks == current ticks → author confirmed, even when the
    // heuristics would individually scream imposter (stale mtime, bad
    // cmdline): identity evidence is strictly stronger.
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            true,
            Some(9_999_999),
            Some(1000),
            Some("tmux")
        ),
        AddTimeVerdict::Alive
    );
}

#[test]
fn procstart_mismatch_falls_through_to_heuristics() {
    // Mismatch + stale time evidence → imposter (the tmux reuse case).
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            Some(2000),
            Some(1000),
            None
        ),
        AddTimeVerdict::Imposter("started-after-pidfile")
    );
    // Mismatch alone with fresh mtime and claude-like cmdline → allow
    // (defends against CC changing the procStart format: a hard reject
    // would black out every real session).
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            Some(990),
            Some(1000),
            Some("claude")
        ),
        AddTimeVerdict::Alive
    );
}

#[test]
fn tolerance_exact_boundary() {
    // start == mtime + 60 → still inside tolerance (uses >, not >=).
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            Some(1060),
            Some(1000),
            None
        ),
        AddTimeVerdict::Alive
    );
    // One second past → imposter.
    assert_eq!(
        add_time_verdict(
            crate::agents::default_kind(),
            false,
            Some(1061),
            Some(1000),
            None
        ),
        AddTimeVerdict::Imposter("started-after-pidfile")
    );
}

/// 现扫：只有活着、是写它的那一条、交互的进程算在写（死进程 · 后台任务 · 冒名都不算）。家目录放在非 ASCII 路径下
/// （仓可能住在非 ASCII 的目录里）。`ccm` resume · session-window · session-terminals 问的都是这一格（`writers`）。
#[cfg(target_os = "linux")]
#[test]
fn the_scan_counts_only_live_interactive_authors_as_writers() {
    let home = std::env::temp_dir().join(format!("ccm-live-文档-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let sessions = crate::agents::IntoHomes::into_homes(&home)[0]
        .pidfile_dir()
        .unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let me = std::process::id();
    let ticks = crate::platform::proc::proc_starttime(me).expect("读得到自己的启动时刻");
    let write =
        |pid: u32, body: String| fs::write(sessions.join(format!("{pid}.json")), body).unwrap();
    write(
        me,
        format!(r#"{{"sessionId":"live-1","procStart":"{ticks}","kind":"interactive"}}"#),
    );
    // 死进程（pid 上限之外，必不在）· 后台任务 · 冒名（procStart 对不上且 cmdline 不像那一家）。
    write(4_194_300, r#"{"sessionId":"dead-1"}"#.into());
    let mut bg = kid_with_pidfile(&sessions, "claude", "bg-1", None);
    write(
        bg.id(),
        format!(
            r#"{{"sessionId":"bg-1","kind":"bg","procStart":"{}"}}"#,
            crate::platform::proc::proc_starttime(bg.id()).unwrap()
        ),
    );
    let mut imp = kid_with_pidfile(&sessions, "sleep", "imp-1", Some("1"));
    let t = LiveTable::scan(&home);
    for k in [&mut bg, &mut imp] {
        let _ = k.kill();
        let _ = k.wait();
    }
    let _ = fs::remove_dir_all(&home);
    assert_eq!(t.writers("live-1"), vec![me]);
    assert!(t.writers("dead-1").is_empty(), "死进程");
    assert!(t.writers("bg-1").is_empty(), "后台任务不算在写");
    assert!(t.background().contains("bg-1"), "后台任务照样是活的");
    assert!(t.writers("imp-1").is_empty(), "冒名");
}
