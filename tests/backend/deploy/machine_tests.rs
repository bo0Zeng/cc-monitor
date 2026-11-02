//! 机器状态成品：每一件事实收成哪一态 · 名字旁那个词 · 问题行那一句 · 哪几颗修法（字面表，两向相等）· 复制详情。

use super::*;
use cmd_wire::machine::{MachineFix, MachineStateKind as K};
use copy_core::copy_text;

const M: &str = "gpu-01";
const VER: &str = "9.9.9";

fn round(r: Round) -> Fact {
    Fact::Round {
        round: r,
        version: Some(VER.to_string()),
    }
}

fn up(relation: Relation, differs: &[&str]) -> Fact {
    round(Round::Up {
        relation,
        differs: differs.iter().map(|s| s.to_string()).collect(),
    })
}

fn down_(why: Option<Why>) -> Fact {
    round(Round::Down {
        problem: RoundProblem {
            said: "拨号没成".into(),
            detail: "命令：dial\n原话：raw words".into(),
            why,
            seen_key: Some("SHA256:new".into()),
            expected_key: Some("SHA256:old".into()),
        },
        blocker: None,
    })
}

/// 断在那台本身的一轮：那一句是通信层写的那一句（[`NO_BACKEND`] 那种照原样上问题行）。
fn blocked(b: Blocker) -> Fact {
    round(Round::Down {
        problem: RoundProblem {
            said: if b == Blocker::NoBackend {
                NO_BACKEND.into()
            } else {
                "起不了".into()
            },
            detail: String::new(),
            why: None,
            seen_key: None,
            expected_key: None,
        },
        blocker: Some(b),
    })
}

const NO_BACKEND: &str = "远端回的不是我们的后端";

fn unsupported() -> Fact {
    blocked(Blocker::NotUnix)
}

fn folded(facts: Vec<Fact>) -> Entry {
    let mut e = Entry::default();
    for f in facts {
        fold(&mut e, f);
    }
    e
}

/// 桌面那一端（自己会装后端）。
fn of(facts: Vec<Fact>, enabled: bool) -> MachineState {
    product(&folded(facts), M, enabled, true)
}

/// 手机那一端（不自己装：没有后端那一轮给［安装］）。
fn phone(facts: Vec<Fact>) -> MachineState {
    product(&folded(facts), M, true, false)
}

/// 问题行那一句：没有 · 文案表那一条（`{machine}` 那一格填那台的名字）· 部署那一步的那一句 · 那一轮通信层写的那一句原样。
enum S {
    No,
    Key(&'static str),
    Step(Step),
    Said(&'static str),
}

/// 一行：那一态 · 原因码 · 名字旁那个词的键（空 ＝ 空串）· 问题行那一句 · 修法（依次）。
type Row = (K, Option<Why>, &'static str, S, Vec<MachineFix>);

fn row(p: &MachineState) -> (K, Option<Why>, String, String, Vec<(MachineFix, String)>) {
    (
        p.state,
        p.reason,
        p.word.clone(),
        p.said.clone(),
        p.fixes.iter().map(|f| (f.fix, f.label.clone())).collect(),
    )
}

fn text(key: &str) -> String {
    if key.is_empty() {
        String::new()
    } else {
        copy_text(key, &[("machine", M)])
    }
}

fn want(r: Row) -> (K, Option<Why>, String, String, Vec<(MachineFix, String)>) {
    let said = match r.3 {
        S::No => String::new(),
        S::Key(k) => text(k),
        S::Step(s) => s.said(M),
        S::Said(t) => t.to_string(),
    };
    (
        r.0,
        r.1,
        text(r.2),
        said,
        r.4.into_iter().map(|f| (f, label(f))).collect(),
    )
}

#[test]
fn every_fact_lands_on_its_state_word_sentence_and_fixes() {
    use MachineFix as F;
    use Why as W;
    let down = |why: Option<W>, said: &'static str, fixes: Vec<F>| -> Row {
        (
            K::Down,
            Some(why.unwrap_or(W::Other)),
            "machinePage.state.down",
            S::Key(said),
            fixes,
        )
    };
    let cases: Vec<(&str, Vec<Fact>, bool, Row)> = vec![
        (
            "没轮过",
            vec![],
            true,
            (K::Unknown, None, "", S::No, vec![]),
        ),
        (
            "接常驻",
            vec![round(Round::Connecting)],
            true,
            (
                K::Connecting,
                None,
                "machineState.word.connecting",
                S::Step(Step::Restart),
                vec![],
            ),
        ),
        (
            "预检",
            vec![round(Round::Connecting), Fact::Step(Step::Plan)],
            true,
            (
                K::Connecting,
                None,
                "machineState.word.connecting",
                S::Step(Step::Plan),
                vec![],
            ),
        ),
        (
            "装",
            vec![Fact::Step(Step::Place { updating: false })],
            true,
            (
                K::Installing,
                None,
                "machineState.word.installing",
                S::Step(Step::Place { updating: false }),
                vec![],
            ),
        ),
        (
            "换",
            vec![Fact::Step(Step::Place { updating: true })],
            true,
            (
                K::Updating,
                None,
                "machineState.word.updating",
                S::Step(Step::Place { updating: true }),
                vec![],
            ),
        ),
        (
            "装完接常驻",
            vec![
                Fact::Step(Step::Place { updating: false }),
                Fact::Deployed {
                    os: Some("Linux".into()),
                },
            ],
            true,
            (
                K::Connecting,
                None,
                "machineState.word.connecting",
                S::Step(Step::Restart),
                vec![],
            ),
        ),
        (
            "同一版",
            vec![up(Relation::Same, &[])],
            true,
            (K::Up, None, "", S::No, vec![]),
        ),
        (
            "不可比",
            vec![up(Relation::Incomparable, &[])],
            true,
            (K::Up, None, "machineState.word.incomparable", S::No, vec![]),
        ),
        (
            "那台旧",
            vec![up(Relation::Older, &[])],
            true,
            (
                K::NeedsUpdate,
                None,
                "machineState.word.needsUpdate",
                S::Key("machineState.problem.needsUpdate"),
                vec![F::Update],
            ),
        ),
        (
            "同一版而帧形状对不上",
            vec![up(Relation::Same, &["session_runs"])],
            true,
            (
                K::NeedsUpdate,
                None,
                "machineState.word.needsUpdate",
                S::Key("machineState.problem.needsUpdate"),
                vec![F::Update],
            ),
        ),
        (
            "那台新",
            vec![up(Relation::Newer, &[])],
            true,
            (
                K::Newer,
                None,
                "machineState.word.newer",
                S::Key("machineState.problem.newer"),
                vec![],
            ),
        ),
        (
            "resolve",
            vec![down_(Some(W::Resolve))],
            true,
            down(
                Some(W::Resolve),
                "machineState.down.resolve",
                vec![F::ConnSettings],
            ),
        ),
        (
            "unreachable",
            vec![down_(Some(W::Unreachable))],
            true,
            down(
                Some(W::Unreachable),
                "machineState.down.unreachable",
                vec![F::Retry, F::ConnSettings],
            ),
        ),
        (
            "timeout",
            vec![down_(Some(W::Timeout))],
            true,
            down(
                Some(W::Timeout),
                "machineState.down.timeout",
                vec![F::Retry, F::ConnSettings],
            ),
        ),
        (
            "auth",
            vec![down_(Some(W::Auth))],
            true,
            down(
                Some(W::Auth),
                "machineState.down.auth",
                vec![F::PushKey, F::ConnSettings],
            ),
        ),
        (
            "password",
            vec![down_(Some(W::Password))],
            true,
            down(
                Some(W::Password),
                "machineState.down.password",
                vec![F::PushKey],
            ),
        ),
        (
            "key_unreadable",
            vec![down_(Some(W::KeyUnreadable))],
            true,
            down(
                Some(W::KeyUnreadable),
                "machineState.down.keyUnreadable",
                vec![F::PushKey, F::ConnSettings],
            ),
        ),
        (
            "jump",
            vec![down_(Some(W::Jump))],
            true,
            down(
                Some(W::Jump),
                "machineState.down.jump",
                vec![F::ConnSettings],
            ),
        ),
        (
            "other",
            vec![down_(Some(W::Other))],
            true,
            down(
                Some(W::Other),
                "link.machine.offline",
                vec![F::Retry, F::ConnSettings],
            ),
        ),
        (
            "没带码",
            vec![down_(None)],
            true,
            down(
                None,
                "link.machine.offline",
                vec![F::Retry, F::ConnSettings],
            ),
        ),
        (
            "host_key",
            vec![down_(Some(W::HostKey))],
            true,
            (
                K::HostKeyChanged,
                Some(W::HostKey),
                "machineState.word.hostKey",
                S::Key("machineState.problem.hostKey"),
                vec![F::CompareFingerprint],
            ),
        ),
        (
            "不是 Unix",
            vec![unsupported()],
            true,
            (
                K::Unsupported,
                None,
                "machineState.word.unsupported",
                S::Key("machineState.problem.notUnix"),
                vec![],
            ),
        ),
        (
            "没有后端（自己会装 ⇒ 不给［安装］）",
            vec![blocked(Blocker::NoBackend)],
            true,
            (
                K::Down,
                Some(W::Other),
                "machinePage.state.down",
                S::Said(NO_BACKEND),
                vec![F::Retry, F::ConnSettings],
            ),
        ),
        (
            "后端旧了（接不上）",
            vec![blocked(Blocker::OldBackend)],
            true,
            (
                K::NeedsUpdate,
                None,
                "machineState.word.needsUpdate",
                S::Key("machineState.problem.needsUpdate"),
                vec![F::Update],
            ),
        ),
        (
            "关了连接这台",
            vec![down_(Some(W::Auth))],
            false,
            (
                K::Disabled,
                None,
                "machinePage.state.disabled",
                S::Key("machinePage.problem.disabled"),
                vec![F::Connect],
            ),
        ),
    ];
    for (name, facts, enabled, w) in cases {
        let p = of(facts, enabled);
        assert_eq!(row(&p), want(w), "「{name}」");
        // 离线那一句哪一态都带着，是连接层写的那一句（出口照摆）。
        assert_eq!(
            p.offline_said,
            comms_inward::chan::said::offline(M),
            "「{name}」"
        );
    }
}

/// 不自己装后端的那一端（手机）：没有后端那一轮只给［安装］；别的与桌面同一份（`installs` 只动这一格）。
#[test]
fn a_host_that_does_not_install_is_offered_install_only_when_the_backend_is_missing() {
    use MachineFix as F;
    let cases: Vec<(&str, Vec<Fact>, Row)> = vec![
        (
            "没有后端",
            vec![blocked(Blocker::NoBackend)],
            (
                K::Down,
                Some(Why::Other),
                "machinePage.state.down",
                S::Said(NO_BACKEND),
                vec![F::Install],
            ),
        ),
        (
            "后端旧了（接不上）",
            vec![blocked(Blocker::OldBackend)],
            (
                K::NeedsUpdate,
                None,
                "machineState.word.needsUpdate",
                S::Key("machineState.problem.needsUpdate"),
                vec![F::Update],
            ),
        ),
        (
            "那台旧（接上了）",
            vec![up(Relation::Older, &[])],
            (
                K::NeedsUpdate,
                None,
                "machineState.word.needsUpdate",
                S::Key("machineState.problem.needsUpdate"),
                vec![F::Update],
            ),
        ),
        (
            "密钥被拒",
            vec![down_(Some(Why::Auth))],
            (
                K::Down,
                Some(Why::Auth),
                "machinePage.state.down",
                S::Key("machineState.down.auth"),
                vec![F::PushKey, F::ConnSettings],
            ),
        ),
        (
            "不是 Unix",
            vec![unsupported()],
            (
                K::Unsupported,
                None,
                "machineState.word.unsupported",
                S::Key("machineState.problem.notUnix"),
                vec![],
            ),
        ),
    ];
    for (name, facts, w) in cases {
        assert_eq!(row(&phone(facts)), want(w), "「{name}」");
    }
}

#[test]
fn the_version_is_the_product_number_only_for_the_same_bytes() {
    let p = of(vec![up(Relation::Same, &[])], true);
    assert_eq!(
        (p.version.as_deref(), p.version_relation),
        (Some(VER), Some(Relation::Same))
    );
    let p = of(vec![up(Relation::Older, &[])], true);
    assert_eq!(
        (p.version, p.version_relation),
        (None, Some(Relation::Older))
    );
}

#[test]
fn the_os_from_the_deploy_stays_across_rounds() {
    let p = of(
        vec![
            Fact::Step(Step::Plan),
            Fact::Deployed {
                os: Some("Linux".into()),
            },
            down_(Some(Why::Timeout)),
            round(Round::Connecting),
            Fact::Deployed { os: None },
        ],
        true,
    );
    assert_eq!(p.os.as_deref(), Some("Linux"));
}

/// 上一趟更新没换上（部署收了、带着那一句）⇒ 那台照旧「要更新」，问题行写「上次没更新成」＋ 那一句、修法照旧［更新］；
/// 再起一趟部署 · 换上了 ⇒ 那一句不再挂着（A2-17-14-2：不弹红 toast，写在机器条那一行）。
#[test]
fn a_failed_update_stays_on_the_machines_line_until_the_next_attempt() {
    let failed = || Fact::UpdateFailed {
        said: "推不上去-甲".into(),
    };
    let p = of(
        vec![
            up(Relation::Older, &[]),
            Fact::Step(Step::Plan),
            failed(),
            up(Relation::Older, &[]),
        ],
        true,
    );
    assert_eq!(p.state, K::NeedsUpdate);
    assert_eq!(
        p.said,
        copy_text(
            "machineState.problem.updateFailed",
            &[("machine", M), ("why", "推不上去-甲")]
        )
    );
    assert_eq!(
        p.fixes.iter().map(|f| f.fix).collect::<Vec<_>>(),
        vec![MachineFix::Update]
    );
    // 再起一趟：那一句摘掉（正在更新那一态照常说自己那一句）。
    let p = of(
        vec![
            up(Relation::Older, &[]),
            failed(),
            up(Relation::Older, &[]),
            Fact::Step(Step::Plan),
            up(Relation::Older, &[]),
        ],
        true,
    );
    assert_eq!(
        p.said,
        copy_text("machineState.problem.needsUpdate", &[("machine", M)])
    );
    // 换上了：同一版连上 ⇒ 没有问题行。
    let p = of(
        vec![failed(), up(Relation::Same, &[]), up(Relation::Older, &[])],
        true,
    );
    assert_eq!(
        p.said,
        copy_text("machineState.problem.needsUpdate", &[("machine", M)])
    );
}

/// 两枚指纹（那台出示的 · 库里记下的）只在「指纹变了」那一态带；下一轮换成别的原因就不挂着。
#[test]
fn the_two_keys_ride_only_the_host_key_state() {
    let p = of(vec![down_(Some(Why::HostKey))], true);
    assert_eq!(p.seen_host_key.as_deref(), Some("SHA256:new"));
    assert_eq!(
        p.expected_host_key.as_deref(),
        Some("SHA256:old"),
        "库里记下的那枚照拨号核心说的带进状态"
    );
    let p = of(
        vec![down_(Some(Why::HostKey)), down_(Some(Why::Timeout))],
        true,
    );
    assert_eq!((p.seen_host_key, p.expected_host_key), (None, None));
}

/// 没连上那一轮带复制详情：那一句进「断在」、连的是哪台进「对象」、码是这一轮的原因码，原话照那一句带的那份；
/// 新一轮起步 · 连上 · 停用 ⇒ 不带。
#[test]
fn a_failed_round_carries_the_copy_detail() {
    let d = of(vec![down_(Some(Why::Auth))], true)
        .detail
        .expect("没连上那一轮没有详情");
    assert_eq!(
        d,
        "命令：dial\n对象：gpu-01\n断在：拨号没成\n码：auth\n原话：raw words"
    );
    let d = of(vec![unsupported()], true)
        .detail
        .expect("做不了那一轮没有详情");
    assert_eq!(d, "对象：gpu-01\n断在：起不了\n码：not_unix");
    let d = of(vec![blocked(Blocker::OldBackend)], true)
        .detail
        .expect("后端旧了那一轮没有详情");
    assert_eq!(d, "对象：gpu-01\n断在：起不了\n码：old_backend");
    let d = phone(vec![blocked(Blocker::NoBackend)])
        .detail
        .expect("没有后端那一轮没有详情");
    assert_eq!(
        d,
        format!("对象：gpu-01\n断在：{NO_BACKEND}\n码：no_backend")
    );
    for (name, facts, enabled) in [
        (
            "新一轮",
            vec![down_(Some(Why::Auth)), round(Round::Connecting)],
            true,
        ),
        (
            "连上",
            vec![down_(Some(Why::Auth)), up(Relation::Same, &[])],
            true,
        ),
        ("停用", vec![down_(Some(Why::Auth))], false),
    ] {
        assert_eq!(of(facts, enabled).detail, None, "{name}");
    }
}

/// ★ **登录过了、开会话被拒**（共用登录机进程数到上限那一形）那一轮：`reason = session_denied`、问题行说「{机器} 登录通过、
/// 但拒绝开会话」、修法只有［重试］（钥匙没问题，不给［推送公钥］· 连接设置）、**不算离线**；桌面与手机同一份。
#[test]
fn a_session_refused_after_login_is_its_own_state_not_offline() {
    for p in [
        of(vec![down_(Some(Why::SessionDenied))], true),
        phone(vec![down_(Some(Why::SessionDenied))]),
    ] {
        assert_eq!(p.state, K::Down);
        assert_eq!(p.reason, Some(Why::SessionDenied));
        assert_eq!(
            p.said,
            copy_text("machineState.down.sessionDenied", &[("machine", M)])
        );
        assert!(p.said.contains(M), "那一句要说是哪台：{}", p.said);
        assert_ne!(p.said, comms_inward::chan::said::offline(M));
        assert_eq!(
            p.fixes.iter().map(|f| f.fix).collect::<Vec<_>>(),
            vec![MachineFix::Retry]
        );
        assert!(!p.offline, "登录过了的那台不是离线");
        assert!(
            p.detail
                .as_deref()
                .is_some_and(|d| d.contains("码：session_denied")),
            "{:?}",
            p.detail
        );
    }
}

/// 成品自带名字与「算不算离线」：出口只照画，不再自己按那一态算（页头「N 台离线」数的就是这一格）。
#[test]
fn the_product_carries_its_name_and_whether_it_is_offline() {
    let cases: Vec<(&str, Vec<Fact>, bool, bool)> = vec![
        ("连着", vec![up(Relation::Same, &[])], true, false),
        ("正在连", vec![round(Round::Connecting)], true, false),
        ("密钥被拒", vec![down_(Some(Why::Auth))], true, true),
        ("拨号超时", vec![down_(Some(Why::Timeout))], true, true),
        ("没带码", vec![down_(None)], true, true),
        ("指纹变了", vec![down_(Some(Why::HostKey))], true, true),
        ("不是 Unix", vec![unsupported()], true, true),
        ("没有后端", vec![blocked(Blocker::NoBackend)], true, true),
        ("后端旧了", vec![blocked(Blocker::OldBackend)], true, false),
        ("那台旧", vec![up(Relation::Older, &[])], true, false),
        (
            "拒绝开会话",
            vec![down_(Some(Why::SessionDenied))],
            true,
            false,
        ),
        ("停用", vec![down_(Some(Why::Auth))], false, false),
        ("没轮过", vec![], true, false),
    ];
    for (name, facts, enabled, offline) in cases {
        let p = of(facts, enabled);
        assert_eq!(p.name, M, "{name}");
        assert_eq!(p.offline, offline, "{name}");
    }
}

/// 一台的成品与它报完了没有（首屏那一句的一行）。
fn seat(name: &str, facts: Vec<Fact>, enabled: bool) -> (MachineState, bool) {
    let e = folded(facts);
    (product(&e, name, enabled, true), e.listed())
}

fn face_of(rows: &[(MachineState, bool)]) -> cmd_wire::machine::FleetLines {
    fleet(&rows.iter().map(|(m, l)| (m, *l)).collect::<Vec<_>>())
}

fn listed_up() -> Vec<Fact> {
    vec![up(Relation::Same, &[]), Fact::Listed]
}

/// ★ A2-17-6-2 先红的那一条：本机报完了、为空，gpu-01 还在连 ⇒ 首屏不说「无会话」，说本机无会话 · gpu-01 还在连。
#[test]
fn the_empty_line_says_who_is_still_connecting_instead_of_no_sessions() {
    let f = face_of(&[
        seat("here", listed_up(), true),
        seat("gpu-01", vec![Fact::Step(crate::Step::Plan)], true),
    ]);
    assert_eq!(f.line, copy_text("beEmpty.line.some", &[("names", "here")]));
    assert_eq!(
        f.hint,
        copy_text("beEmpty.hint.pending", &[("names", "gpu-01")])
    );
    assert_ne!(f.line, copy_text("beEmpty.line.none", &[]));
}

/// 各台都报完了 ⇒ 「无会话」＋ 怎么让它有；停用的那台不算；连上了还没报完 ⇒ 仍在连；一台都没报完 ⇒ 不说谁无会话。
#[test]
fn the_empty_line_says_no_sessions_only_once_every_machine_has_reported() {
    let all = face_of(&[
        seat("here", listed_up(), true),
        seat("nas", listed_up(), true),
        seat("off", vec![], false),
    ]);
    assert_eq!(
        (all.line, all.hint),
        (
            copy_text("beEmpty.line.none", &[]),
            copy_text("beEmpty.hint.none", &[])
        )
    );
    let unreported = face_of(&[
        seat("here", listed_up(), true),
        seat("nas", vec![up(Relation::Same, &[])], true),
    ]);
    assert_eq!(
        unreported.hint,
        copy_text("beEmpty.hint.pending", &[("names", "nas")])
    );
    let none_yet = face_of(&[seat("nas", vec![], true)]);
    assert_eq!(none_yet.line, copy_text("beEmpty.line.waiting", &[]));
}

/// 没连上 · 要更新的那几台 ⇒ 名字 ＋ 那台的词、会话未计入；与还在连的那几台各一段。
#[test]
fn the_empty_line_says_which_machines_are_not_counted() {
    let f = face_of(&[
        seat("here", listed_up(), true),
        seat("devbox", vec![down_(Some(Why::Timeout))], true),
        seat("nas", vec![up(Relation::Older, &[])], true),
        seat("gpu-01", vec![round(Round::Connecting)], true),
    ]);
    let one = |name: &str, word: &str| {
        copy_text(
            "beEmpty.hint.outOne",
            &[("name", name), ("word", &copy_text(word, &[]))],
        )
    };
    let out = [
        one("devbox", "machinePage.state.down"),
        one("nas", "machineState.word.needsUpdate"),
    ]
    .join(&copy_text("beEmpty.list.sep", &[]));
    assert_eq!(
        f.hint,
        [
            copy_text("beEmpty.hint.pending", &[("names", "gpu-01")]),
            copy_text("beEmpty.hint.out", &[("names", &out)]),
        ]
        .join(&copy_text("kit.text.sep", &[]))
    );
}

/// 机器列表页头那一句与首屏那一句同一处合：全部数每台（停用的也数）；离线照成品的 `offline`、要更新照那一态，停用的都不数；
/// 没有离线 · 要更新的那一段就不出。
#[test]
fn the_count_line_counts_every_machine_and_skips_disabled_ones_for_offline_and_update() {
    let n = |n: usize| copy_text("machineList.count.all", &[("n", &n.to_string())]);
    let sep = copy_text("kit.text.sep", &[]);
    let f = face_of(&[
        seat("here", listed_up(), true),
        seat("devbox", vec![down_(Some(Why::Timeout))], true),
        seat("nas", vec![up(Relation::Older, &[])], true),
        seat("off", vec![down_(Some(Why::Auth))], false),
        seat("old-off", vec![up(Relation::Older, &[])], false),
    ]);
    assert_eq!(
        f.count,
        [
            n(5),
            copy_text("machineList.count.off", &[("a", "1")]),
            copy_text("machineList.count.update", &[("b", "1")]),
        ]
        .join(&sep)
    );
    let calm = face_of(&[
        seat("here", listed_up(), true),
        seat("off", vec![down_(Some(Why::Auth))], false),
    ]);
    assert_eq!(calm.count, n(2));
}

/// 报完了是这一条连接上的事实：又在连 · 断了 ⇒ 作废；连上那一轮不清（那台随后才报完）。
#[test]
fn listed_is_dropped_when_the_machine_reconnects_or_goes_down() {
    assert!(folded(listed_up()).listed());
    let mut again = listed_up();
    again.push(round(Round::Connecting));
    assert!(!folded(again).listed());
    let mut lost = listed_up();
    lost.push(down_(None));
    assert!(!folded(lost).listed());
    let mut kept = listed_up();
    kept.push(up(Relation::Same, &[]));
    assert!(folded(kept).listed());
}

/// 接上过的那台又在连 ⇒ 「正在重新连接 X」、不带修法（两端机器条照摆）；头一回连照旧是连接那一句；部署那一步照部署那一句。
#[test]
fn a_machine_that_was_up_and_is_connecting_again_says_reconnecting() {
    let first = of(vec![round(Round::Connecting)], true);
    assert_eq!(
        first.said,
        said(K::Connecting, Some(Stage::Attach), None, M),
        "头一回连就说了重连"
    );
    let again = of(
        vec![up(Relation::Same, &[]), round(Round::Connecting)],
        true,
    );
    assert_eq!(
        (again.state, again.said.as_str(), again.fixes.len()),
        (
            K::Connecting,
            text("machineState.busy.reconnecting").as_str(),
            0
        )
    );
    let after_drop = of(
        vec![
            up(Relation::Same, &[]),
            down_(Some(Why::Timeout)),
            round(Round::Connecting),
        ],
        true,
    );
    assert_eq!(
        after_drop.said,
        text("machineState.busy.reconnecting"),
        "断过一轮再连也是重连"
    );
    let deploying = of(vec![up(Relation::Same, &[]), Fact::Step(Step::Plan)], true);
    assert_ne!(
        deploying.said,
        text("machineState.busy.reconnecting"),
        "部署那一步说成了重连"
    );
}

/// 机器条那一行（两端的条都只照它画）：每一态进不进条 · 语气 · 带不带进度，字面表两向相等。
#[test]
fn every_state_lands_on_its_strip_line_or_stays_off_the_strip() {
    use cmd_wire::cells::Tone as T;
    use cmd_wire::machine::MachineStripLine as L;
    let line = |tone, progress| Some(L { tone, progress });
    let cases: Vec<(&str, MachineState, Option<L>)> = vec![
        ("连着", of(vec![up(Relation::Same, &[])], true), None),
        ("比这一版新", of(vec![up(Relation::Newer, &[])], true), None),
        ("停用", of(vec![up(Relation::Same, &[])], false), None),
        ("还不知道", of(vec![], true), None),
        (
            "正在连",
            of(vec![round(Round::Connecting)], true),
            line(T::Busy, false),
        ),
        (
            "正在装",
            of(vec![Fact::Step(Step::Place { updating: false })], true),
            line(T::Busy, true),
        ),
        (
            "正在更新",
            of(vec![Fact::Step(Step::Place { updating: true })], true),
            line(T::Busy, true),
        ),
        (
            "那台旧",
            of(vec![up(Relation::Older, &[])], true),
            line(T::Warn, false),
        ),
        (
            "没连上",
            of(vec![down_(Some(Why::Timeout))], true),
            line(T::Offline, false),
        ),
        (
            "指纹不对",
            of(vec![down_(Some(Why::HostKey))], true),
            line(T::Fail, false),
        ),
        (
            "做不了",
            of(vec![unsupported()], true),
            line(T::Fail, false),
        ),
        (
            "缺后端、这一端自己装",
            of(vec![blocked(Blocker::NoBackend)], true),
            line(T::Offline, false),
        ),
        (
            "缺后端、这一端不装",
            phone(vec![blocked(Blocker::NoBackend)]),
            line(T::Fail, false),
        ),
    ];
    for (name, got, want) in cases {
        assert_eq!(got.strip, want, "{name}（{:?}）", got.state);
    }
}

/// 机器条的语气就是共享的那一格语气：缺后端那一态是 `Tone::Fail`，没连上是 `Tone::Offline`，上线照 `Tone` 的小写词。
#[test]
fn the_strip_tone_is_the_shared_cell_tone() {
    let fault = phone(vec![blocked(Blocker::NoBackend)])
        .strip
        .expect("进条");
    let tone: cmd_wire::cells::Tone = fault.tone;
    assert_eq!(tone, cmd_wire::cells::Tone::Fail);
    assert_eq!(serde_json::to_value(fault).unwrap()["tone"], "fail");
    let down = of(vec![down_(Some(Why::Timeout))], true)
        .strip
        .expect("进条");
    assert_eq!(down.tone, cmd_wire::cells::Tone::Offline);
    assert_eq!(serde_json::to_value(down).unwrap()["tone"], "offline");
}
