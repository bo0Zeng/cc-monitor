//! 「会话 / tmux 账本（会话表 · tmux 快照 · 容器判定 · 可重连判定）进那台后端；monitor 只收成品帧」· 可恢复性由容器定。
//!
//! 一张真值表钉 `observe::session_ledger` 的全部裁决：摘除按 cause ＋ 摘除之后才起的快照 · 只收割可重连的（活的只认 pidfile）·
//! 按会话句柄认会话（改名不算关）· 一个会话里几个窗格各挂各的 · 不可观测不收割 · 每一份快照推出（没报过的）可重连 · 清单压到快照之后。
use super::*;
use crate::stream::wire::{Frame, RemovalCause, SessionFate};

/// 账本的一格输入：一帧（要发出去的）或一份 tmux 观测（观测不再是帧）。
enum Ev {
    F(Frame),
    T(String, Option<&'static str>),
    Probe,
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
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        activity_text: crate::stream::wire::activity_cells(None).0,
        activity_tone: crate::stream::wire::activity_cells(None).1,
        waiting_for: None,
        container: None,
        pid: 0,
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

/// `tmux ls -F` 的一行（8 列：…⇥活动窗格的 `@ccm_sid`⇥`#{session_id}`⇥各窗格的 `@ccm_sid`，空格分隔）。
fn row(id: &str, name: &str, sids: &[&str]) -> String {
    let lead = sids.first().copied().unwrap_or("");
    format!("{name}\t/w\tzsh\t0\t1\t{lead}\t{id}\t{} \n", sids.join(" "))
}

/// 一份可观测的快照：`(句柄, 会话名, 各窗格挂着的 sid)`。
fn tmux(rows: &[(&str, &str, &[&str])]) -> Ev {
    if rows.is_empty() {
        return Ev::T(String::new(), Some("zero_sessions"));
    }
    Ev::T(rows.iter().map(|(i, n, s)| row(i, n, s)).collect(), None)
}

fn unobservable() -> Ev {
    Ev::T(String::new(), Some("unobservable"))
}

/// 喂一串帧，收每一帧的「发不发」与补发的成品（按 `(sid, 成品)` 记；清单记成 `("*", None)`）。
/// `Ev::Probe` = 一份观测从这一刻起了（watcher 每起一次探测都告诉账本）。
fn run(frames: Vec<Ev>) -> (Vec<bool>, Vec<(String, Option<SessionFate>)>) {
    let mut l = SessionLedger::new();
    let mut passed = Vec::new();
    let mut out = Vec::new();
    for f in frames {
        let (pass, extra) = match f {
            Ev::F(f) => l.on_frame(&f),
            Ev::T(raw, o) => (true, l.on_tmux(&raw, o)),
            Ev::Probe => {
                l.observing();
                (true, Vec::new())
            }
        };
        passed.push(pass);
        for e in extra {
            match e {
                Frame::SessionState { sid, state, .. } => out.push((sid, Some(state))),
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

/// 一份「起了、回来了」的观测（探测在摘除之后起）。
fn fresh(rows: &[(&str, &str, &[&str])]) -> [Ev; 2] {
    [Ev::Probe, tmux(rows)]
}

#[test]
fn removal_is_judged_by_cause_then_by_a_snapshot_taken_after_it() {
    // superseded 恒已结束（快照里那一格已改挂新 sid）· gone ⇒ 等一份摘除之后才起的快照：挂着 ⇒ 可重连 · 不挂 ⇒ 已结束。
    let mut evs = vec![added("a"), added("b"), added("c")];
    evs.extend(fresh(&[("$1", "ta", &["a"]), ("$2", "tb", &["b"])]));
    evs.push(removed("a", RemovalCause::Superseded));
    evs.push(removed("b", RemovalCause::Gone));
    evs.push(removed("c", RemovalCause::Gone));
    evs.extend(fresh(&[("$1", "ta", &[]), ("$2", "tb", &["b"])]));
    let (_, out) = run(evs);
    assert_eq!(out, fates(&[("a", E), ("b", R), ("c", E)]));
}

/// claude 退出与快照的先后不再让去向翻一次：摘除之前起的那份（它回来时还挂着 / 还没打上标）不裁，等下一份。
#[test]
fn a_snapshot_started_before_the_removal_does_not_judge_it() {
    // ① 快照起于摘除之前、回来时 tmux 会话还在；摘除之后那一份里会话已随 claude 关了 ⇒ 只一格「已结束」。
    let mut evs = vec![added("a"), Ev::Probe, removed("a", RemovalCause::Gone)];
    evs.push(tmux(&[("$1", "t", &["a"])]));
    evs.extend(fresh(&[]));
    let (_, out) = run(evs);
    assert_eq!(out, fates(&[("a", E)]), "先报可重连、再翻已结束");

    // ② 快照起于打标之前（claude 刚起就退）：那份里没挂它；摘除之后那一份挂着 ⇒ 只一格「可重连」。
    let mut evs = vec![added("b"), Ev::Probe, removed("b", RemovalCause::Gone)];
    evs.push(tmux(&[("$1", "t", &[])]));
    evs.extend(fresh(&[("$1", "t", &["b"])]));
    let (_, out) = run(evs);
    assert_eq!(out, fates(&[("b", R)]), "先报已结束、再翻可重连");
}

#[test]
fn a_removal_waits_for_an_observation_and_asks_for_one() {
    let mut l = SessionLedger::new();
    l.on_frame(&Frame::SessionAdded {
        sid: "a".into(),
        agent_kind: None,
        liveness_confidence: None,
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        activity_text: crate::stream::wire::activity_cells(None).0,
        activity_tone: crate::stream::wire::activity_cells(None).1,
        waiting_for: None,
        container: None,
        pid: 0,
    });
    assert!(!l.awaits_observation());
    let (pass, extra) = l.on_frame(&Frame::SessionRemoved {
        sid: "a".into(),
        cause: RemovalCause::Gone,
    });
    assert!(pass && extra.is_empty(), "摘除本身照发，去向等观测");
    assert!(l.awaits_observation(), "要一份摘除之后才起的观测");
    l.observing();
    assert!(!l.awaits_observation());
    let (_, out) = run(vec![
        added("z"),
        removed("z", RemovalCause::Gone),
        removed("zz", RemovalCause::Gone),
        Ev::Probe,
        unobservable(),
    ]);
    assert_eq!(
        out,
        fates(&[("z", E)]),
        "看不见 tmux ⇒ 照它裁；不认识的 sid 不说话"
    );
}

/// 活不活只认 pidfile / pidfd：tmux 那边怎么变（改名 · 标签不见 · 会话关了）都不把活会话裁成已结束。
#[test]
fn tmux_never_ends_a_live_session() {
    // 标签不见了（几份都不见）。
    let (_, out) = run(vec![
        added("live"),
        tmux(&[("$1", "t", &["live"])]),
        tmux(&[("$1", "t", &[])]),
        tmux(&[("$1", "t", &[])]),
        tmux(&[("$1", "t", &[])]),
    ]);
    assert_eq!(out, Vec::new(), "标签不见了");
    // 它所在的 tmux 会话关了（claude 若真随之退出，会有它自己的摘除）。
    let (_, out) = run(vec![
        added("live"),
        tmux(&[("$1", "t", &["live"])]),
        tmux(&[]),
    ]);
    assert_eq!(out, Vec::new(), "会话关了");
}

/// 活会话 / 可重连会话所在的 tmux 会话改名：按句柄认，一格都不报。
#[test]
fn renaming_a_tmux_session_changes_nothing() {
    let (_, out) = run(vec![
        added("live"),
        tmux(&[("$1", "work", &["live"]), ("$2", "idle", &["old"])]),
        tmux(&[("$1", "work2", &["live"]), ("$2", "idle2", &["old"])]),
        tmux(&[("$1", "work2", &["live"]), ("$2", "idle2", &["old"])]),
    ]);
    assert_eq!(out, fates(&[("old", R)]), "只该有起初那一格可重连");
}

/// 一个 tmux 会话里两个窗格各跑一个 claude：都挂着，谁也不被裁；一个退了 ⇒ 它可重连，另一个照旧活。
#[test]
fn two_claudes_in_one_tmux_session_are_both_bound() {
    let mut evs = vec![
        added("a"),
        added("b"),
        tmux(&[("$1", "t", &["a", "b"])]),
        tmux(&[("$1", "t", &["b", "a"])]),
        removed("a", RemovalCause::Gone),
    ];
    evs.extend(fresh(&[("$1", "t", &["a", "b"])]));
    evs.push(tmux(&[("$1", "t", &["a", "b"])]));
    let (_, out) = run(evs);
    assert_eq!(out, fates(&[("a", R)]));
}

#[test]
fn a_closed_tmux_session_ends_its_reconnectable_at_once() {
    // 句柄消失 = 确证关了 ⇒ 不等去抖；同一个 sid 还挂在别的会话里就不算。
    let mut evs = vec![added("idle"), added("twice")];
    evs.extend(fresh(&[
        ("$1", "t1", &["idle"]),
        ("$2", "t2", &["twice"]),
        ("$3", "t3", &["twice"]),
    ]));
    evs.push(removed("idle", RemovalCause::Gone));
    evs.push(removed("twice", RemovalCause::Gone));
    evs.extend(fresh(&[
        ("$1", "t1", &["idle"]),
        ("$2", "t2", &["twice"]),
        ("$3", "t3", &["twice"]),
    ]));
    evs.push(tmux(&[("$3", "t3", &["twice"])]));
    let (_, out) = run(evs);
    assert_eq!(out, fates(&[("idle", R), ("twice", R), ("idle", E)]));
}

/// 按句柄认会话：一个会话关了、同名的新会话同一刻就起来了 ⇒ 照样是「关了」。
#[test]
fn a_closed_session_whose_name_is_reused_at_once_still_ends_its_reconnectable() {
    let (_, out) = run(vec![
        tmux(&[("$1", "t", &["old"])]),
        tmux(&[("$2", "t", &[])]),
    ]);
    assert_eq!(out, fates(&[("old", R), ("old", E)]));
}

#[test]
fn a_reconnectable_missing_from_two_snapshots_is_reaped() {
    // tmux 会话还在、哪个窗格都不再挂它（去抖两份）⇒ 已结束。
    let (_, out) = run(vec![
        tmux(&[("$1", "t", &["gone"])]),
        tmux(&[("$1", "t", &[])]),
        tmux(&[("$1", "t", &[])]),
        tmux(&[("$1", "t", &[])]),
    ]);
    assert_eq!(out, fates(&[("gone", R), ("gone", E)]));
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
    // 新连接从 tmux 自己推出可重连（挂着 `@ccm_sid`、却不在活会话里的），**先于**清单。
    // 之后的快照照样推：后来才出现的 `later` 也报一次；已报过的 `old` 不重报。
    let (passed, out) = run(vec![
        added("live"),
        listed(),
        tmux(&[
            ("$1", "t1", &["live"]),
            ("$2", "t2", &["old"]),
            ("$3", "t3", &[]),
        ]),
        tmux(&[
            ("$1", "t1", &["live"]),
            ("$2", "t2", &["old"]),
            ("$4", "t4", &["later"]),
        ]),
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
    // 后端先起：第一份快照是「零会话」（可观测）；tmux server 后起、上面挂着 `@ccm_sid` 的会话 ⇒ 可重连（只报一次；它关了 ⇒ 已结束）。
    let (_, out) = run(vec![
        listed(),
        tmux(&[]),
        tmux(&[("$0", "late", &["sid-late"])]),
        tmux(&[("$0", "late", &["sid-late"])]),
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
    let mut evs = vec![tmux(&[("$1", "t", &["a"])]), added("a"), listed()];
    evs.push(removed("a", RemovalCause::Gone));
    evs.extend(fresh(&[("$1", "t", &["a"])]));
    evs.push(added("a"));
    evs.push(removed("a", RemovalCause::Gone));
    evs.extend(fresh(&[]));
    let (passed, out) = run(evs);
    assert_eq!(passed[2], true, "快照已经到过 ⇒ 清单不压");
    // 起初 `a` 不活 ⇒ 推导为可重连；又活了 ⇒ 摘掉；离开 ⇒ 可重连；又活了；离开时 tmux 已关 ⇒ 已结束。
    assert_eq!(out, fates(&[("a", R), ("a", R), ("a", E)]));
}
