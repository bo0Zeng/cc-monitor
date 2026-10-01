//! 「会话 / tmux 账本（会话表 · tmux 快照 · 容器判定 · 可重连判定）进那台后端；monitor 只收成品帧」· 可恢复性由容器定。
//!
//! 一张真值表钉 `observe::session_ledger` 的全部裁决（从 monitor 搬来的那几条性质，本机远端同一份）：
//! 摘除按 cause ＋ 快照 · 两种收割 · 没进过 tmux 的不收割 · 不可观测不收割 · 每一份快照推出（没报过的）可重连 · 清单压到快照之后。
use super::*;
use crate::stream::wire::{Frame, RemovalCause, SessionFate};

/// 账本的一格输入：一帧（要发出去的）或一份 tmux 观测（观测不再是帧）。
enum Ev {
    F(Frame),
    T(String, Option<&'static str>),
}

impl From<Frame> for Ev {
    fn from(f: Frame) -> Self {
        Ev::F(f)
    }
}

fn listed() -> Ev {
    Frame::SessionsReplayed.into()
}

fn added(sid: &str) -> Ev {
    Frame::SessionAdded {
        sid: sid.into(),
        agent_kind: None,
        liveness_confidence: None,
        session_kind: None,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
        container: None,
        pid: None,
    }
    .into()
}

fn removed(sid: &str, cause: RemovalCause) -> Ev {
    Frame::SessionRemoved {
        sid: sid.into(),
        cause,
    }
    .into()
}

/// `tmux ls -F` 的一行（6 列，末列 `@ccm_sid`）。
fn row(name: &str, sid: &str) -> String {
    format!("{name}\t/w\tzsh\t0\t1\t{sid}\n")
}

fn tmux(rows: &[(&str, &str)]) -> Ev {
    if rows.is_empty() {
        return Ev::T(String::new(), Some("zero_sessions"));
    }
    Ev::T(rows.iter().map(|(n, s)| row(n, s)).collect(), None)
}

fn unobservable() -> Ev {
    Ev::T(String::new(), Some("unobservable"))
}

/// 喂一串帧，收每一帧的「发不发」与补发的成品（按 `(sid, 成品)` 记；清单记成 `("*", None)`）。
fn run(frames: Vec<Ev>) -> (Vec<bool>, Vec<(String, Option<SessionFate>)>) {
    let mut l = SessionLedger::new();
    let mut passed = Vec::new();
    let mut out = Vec::new();
    for f in frames {
        let (pass, extra) = match f {
            Ev::F(f) => l.on_frame(&f),
            Ev::T(raw, o) => (true, l.on_tmux(&raw, o)),
        };
        passed.push(pass);
        for e in extra {
            match e {
                Frame::SessionState { sid, state } => out.push((sid, Some(state))),
                Frame::SessionsReplayed => out.push(("*".into(), None)),
                other => panic!("账本只该补发成品与清单，却补了 {other:?}"),
            }
        }
    }
    (passed, out)
}

use SessionFate::{Ended as E, Reconnectable as R};

fn fates(v: &[(&str, SessionFate)]) -> Vec<(String, Option<SessionFate>)> {
    v.iter().map(|(s, f)| (s.to_string(), Some(*f))).collect()
}

#[test]
fn removal_is_judged_by_cause_then_by_the_latest_snapshot() {
    // 原 `classify_removed` 的三格：superseded 恒已结束（快照里那一格已改挂新 sid）· gone ＋ 挂着 ⇒ 可重连 · gone ＋ 不挂 ⇒ 已结束。
    let (_, out) = run(vec![
        added("a"),
        added("b"),
        added("c"),
        tmux(&[("ta", "a"), ("tb", "b")]),
        removed("a", RemovalCause::Superseded),
        removed("b", RemovalCause::Gone),
        removed("c", RemovalCause::Gone),
    ]);
    assert_eq!(out, fates(&[("a", E), ("b", R), ("c", E)]));
}

#[test]
fn a_removal_before_any_snapshot_is_ended_and_an_unknown_sid_says_nothing() {
    let (_, out) = run(vec![
        added("a"),
        removed("a", RemovalCause::Gone),
        removed("zz", RemovalCause::Gone),
    ]);
    assert_eq!(out, fates(&[("a", E)]));
}

#[test]
fn a_closed_tmux_session_ends_its_reconnectable_and_its_live_session_at_once() {
    // 名字消失 = 确证关了 ⇒ 不等去抖（原 `tmux_session_closed` 那一臂）；活的也落（带外杀掉 tmux，#60-A）。
    let (_, out) = run(vec![
        added("live"),
        added("idle"),
        tmux(&[("t1", "live"), ("t2", "idle")]),
        removed("idle", RemovalCause::Gone),
        tmux(&[]),
    ]);
    assert_eq!(out, fates(&[("idle", R), ("live", E), ("idle", E)]));
}

#[test]
fn a_bound_session_missing_from_two_snapshots_is_reaped_but_never_a_never_bound_one() {
    // tmux 会话还在、`@ccm_sid` 却不再是它（去抖两份）⇒ 已结束；从没挂进过 tmux 的（bg / 直起）永不收割。
    let (_, out) = run(vec![
        added("bound"),
        added("free"),
        tmux(&[("t", "bound")]),
        tmux(&[("t", "")]),
        tmux(&[("t", "")]),
        tmux(&[("t", "")]),
    ]);
    assert_eq!(out, fates(&[("bound", E)]));
}

#[test]
fn an_unobservable_snapshot_reaps_nothing_and_releases_the_held_list() {
    let (passed, out) = run(vec![
        added("a"),
        listed(),
        unobservable(),
        unobservable(),
        unobservable(),
    ]);
    assert_eq!(passed[1], false, "清单在第一份快照之前压住");
    assert_eq!(
        out,
        vec![("*".to_string(), None)],
        "不可观测：不收割、不推导，只放清单"
    );
}

#[test]
fn the_first_observable_snapshot_announces_reconnectables_before_the_list() {
    // 新连接从 tmux 自己推出可重连（挂着 `@ccm_sid`、却不在活会话里的），**先于**清单（替掉 monitor「重连后重新裁」那一套）。
    // 〔MIG-1 续四〕之后的快照照样推：后来才出现的 `later` 也报一次；已报过的 `old` 不重报。
    let (passed, out) = run(vec![
        added("live"),
        listed(),
        tmux(&[("t1", "live"), ("t2", "old"), ("t3", "")]),
        tmux(&[("t1", "live"), ("t2", "old"), ("t4", "later")]),
    ]);
    assert_eq!(passed, vec![true, false, true, true]);
    assert_eq!(
        out,
        vec![
            ("old".to_string(), Some(R)),
            ("*".to_string(), None),
            ("later".to_string(), Some(R))
        ],
    );
}

#[test]
fn a_tagged_session_on_a_server_that_started_after_the_backend_is_announced() {
    // 〔MIG-1 续四 · `tests/e2e/backend-tmux-late-server.sh` 那一格〕后端先起：第一份快照是「零会话」（可观测）；
    //   tmux server 后起、上面挂着 `@ccm_sid` 的会话 ⇒ 可重连（只报一次；它关了 ⇒ 已结束）。
    let (_, out) = run(vec![
        listed(),
        tmux(&[]),
        tmux(&[("late", "sid-late")]),
        tmux(&[("late", "sid-late")]),
        tmux(&[]),
    ]);
    assert_eq!(
        out,
        vec![
            ("*".to_string(), None),
            ("sid-late".to_string(), Some(R)),
            ("sid-late".to_string(), Some(E))
        ]
    );
}

#[test]
fn a_session_that_comes_back_leaves_reconnectable_and_the_list_passes_once_tmux_was_seen() {
    let (passed, out) = run(vec![
        tmux(&[("t", "a")]),
        added("a"),
        listed(),
        removed("a", RemovalCause::Gone),
        added("a"),
        tmux(&[]),
    ]);
    assert_eq!(passed[2], true, "快照已经到过 ⇒ 清单不压");
    // 起初 `a` 不活 ⇒ 推导为可重连；又活了 ⇒ 摘掉；离开 ⇒ 可重连；又活了；tmux 关了 ⇒ 已结束（只一次）。
    assert_eq!(out, fates(&[("a", R), ("a", R), ("a", E)]));
}
