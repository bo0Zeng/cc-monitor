//! 机器状态成品：每一件事实收成哪一态 · 名字旁那个词 · 问题行那一句 · 哪几颗修法（字面表，两向相等）· 复制详情。

use super::*;
use cmd_wire::machine::MachineStateKind as K;
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
            text: "拨号没成".into(),
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
            text: if b == Blocker::NoBackend {
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
                "machinePage.problem.offline",
                vec![F::Retry, F::ConnSettings],
            ),
        ),
        (
            "没带码",
            vec![down_(None)],
            true,
            down(
                None,
                "machinePage.problem.offline",
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
        assert_eq!(row(&of(facts, enabled)), want(w), "「{name}」");
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
