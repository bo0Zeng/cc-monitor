use super::*;
use crate::wire::SessionFaceExt as _;
use cmd_wire::frame::SessionFace;

const DEVBOX: &str = "devbox";

fn row(sid: &str, order: u8, live: bool, running: u32) -> MonitorHereRow {
    MonitorHereRow {
        sid: sid.into(),
        order,
        live,
        running,
    }
}

fn asked(v: &[(&Machine, &str)]) -> Vec<(Machine, String)> {
    v.iter()
        .map(|(m, s)| ((*m).clone(), s.to_string()))
        .collect()
}

fn called() -> BTreeMap<String, Called> {
    let mut c = BTreeMap::new();
    c.insert(
        cmd_wire::machine::LOCAL.to_string(),
        Called {
            name: copy_core::local_machine(),
            offline: "本机离线".into(),
        },
    );
    c.insert(
        DEVBOX.to_string(),
        Called {
            name: "devbox 那台".into(),
            offline: "devbox 离线那一句".into(),
        },
    );
    c
}

fn title(name: &str, n: usize) -> Words {
    Words(copy_text(
        "monitorBoard.group.title",
        &[("machine", name), ("n", &n.to_string())],
    ))
}

fn summary(machines: usize, live: usize, running: usize) -> Words {
    Words(copy_text(
        "monitorBoard.summary",
        &[
            ("machines", &machines.to_string()),
            ("liveSessions", &live.to_string()),
            ("runningAgents", &running.to_string()),
        ],
    ))
}

/// ★ 两台三会话：组序本机在前 · 组头是那台的名字 ＋ 几条 · 组内按序（同序照请求先后）· 汇总那一句。
#[test]
fn two_machines_three_sessions_merge_into_ordered_groups_and_one_summary() {
    let local = Machine::local();
    let dev = Machine::named(DEVBOX);
    // 请求先列 devbox：组序不照请求，照「本机在前、其余照机器表」。
    let ask = asked(&[(&dev, "d1"), (&local, "l3"), (&local, "l1")]);
    let got = vec![
        (
            dev.clone(),
            Some(MonitorHere {
                rows: vec![row("d1", 1, true, 2)],
            }),
        ),
        (
            local.clone(),
            Some(MonitorHere {
                rows: vec![row("l3", 3, true, 0), row("l1", 1, true, 1)],
            }),
        ),
    ];
    let b = merge(&ask, got, &[DEVBOX.to_string()], &called());
    let machines: Vec<&Machine> = b.groups.iter().map(|g| &g.machine).collect();
    assert_eq!(machines, [&local, &dev], "本机在前，其余照机器表");
    assert_eq!(b.groups[0].title, title(&copy_core::local_machine(), 2));
    assert_eq!(b.groups[1].title, title("devbox 那台", 1));
    assert_eq!(b.groups[0].sids, ["l1", "l3"], "组内 order 小的在前");
    assert_eq!(b.groups[1].sids, ["d1"]);
    assert!(b.groups.iter().all(|g| g.down.is_none()));
    assert_eq!(b.summary, summary(2, 3, 3));
}

/// ★ 一台没答：那一组照请求先后列、带那台的离线那一句；活会话与在跑不数它。
#[test]
fn a_machine_that_did_not_answer_keeps_its_group_with_its_offline_line_and_is_not_counted() {
    let local = Machine::local();
    let dev = Machine::named(DEVBOX);
    let ask = asked(&[(&local, "l1"), (&dev, "d2"), (&dev, "d1")]);
    let got = vec![
        (
            local.clone(),
            Some(MonitorHere {
                rows: vec![row("l1", 1, true, 4)],
            }),
        ),
        (dev.clone(), None),
    ];
    let b = merge(&ask, got, &[DEVBOX.to_string()], &called());
    let g = &b.groups[1];
    assert_eq!(g.machine, dev);
    assert_eq!(g.sids, ["d2", "d1"], "没答 ⇒ 照请求先后");
    assert_eq!(g.down, Some(Words("devbox 离线那一句".into())));
    assert_eq!(g.title, title("devbox 那台", 2));
    assert_eq!(b.summary, summary(2, 1, 4), "没答的那台不数活会话与在跑");
}

/// 同序照请求先后（稳定）· 不活的不算活会话 · 这台不认得的排在组尾、不丢。
#[test]
fn ties_keep_request_order_dead_ones_are_not_live_and_unknown_ones_trail() {
    let local = Machine::local();
    let ask = asked(&[
        (&local, "x"),
        (&local, "gone"),
        (&local, "a"),
        (&local, "b"),
    ]);
    let got = vec![(
        local.clone(),
        Some(MonitorHere {
            rows: vec![
                row("b", 1, true, 0),
                row("a", 1, true, 0),
                row("gone", 5, false, 0),
            ],
        }),
    )];
    let b = merge(&ask, got, &[], &called());
    assert_eq!(b.groups[0].sids, ["a", "b", "gone", "x"]);
    assert_eq!(b.summary, summary(1, 2, 0));
}

/// 机器表里已没有的那几台排最后、按键序。
#[test]
fn machines_gone_from_the_table_come_last_in_key_order() {
    let local = Machine::local();
    let z = Machine::named("zeta");
    let a = Machine::named("alpha");
    let dev = Machine::named(DEVBOX);
    let ask = asked(&[(&z, "1"), (&a, "2"), (&dev, "3"), (&local, "4")]);
    let got = [&z, &a, &dev, &local]
        .into_iter()
        .map(|m| (m.clone(), Some(MonitorHere { rows: vec![] })))
        .collect();
    let b = merge(&ask, got, &[DEVBOX.to_string()], &called());
    let order: Vec<&Machine> = b.groups.iter().map(|g| &g.machine).collect();
    assert_eq!(order, [&local, &dev, &a, &z]);
    assert_eq!(
        b.groups[2].title,
        title("alpha", 1),
        "不在成品里 ⇒ 名字照键"
    );
}

/// 一台都没有 ⇒ 汇总空、没有组。
#[test]
fn nothing_asked_means_no_groups_and_an_empty_summary() {
    let b = merge(&[], vec![], &[DEVBOX.to_string()], &called());
    assert!(b.groups.is_empty());
    assert_eq!(b.summary, Words::default());
}

fn added(sid: &str, face: &SessionFace) -> String {
    serde_json::json!({
        "kind": "session_added", "sid": sid, "agent_kind": "claude",
        "face": face, "label": sid, "pid": 1,
    })
    .to_string()
}

/// ★ 这台那几行：order 与 live 照这台的会话簿（脸上那一格），在跑照给的数；不认得的不进。
#[test]
fn rows_here_read_order_and_life_from_the_book_and_skip_unknown_sids() {
    let mut book = SessionBook::default();
    let needs = SessionFace::of(crate::wire::Life::Live {
        activity: Some(crate::agents::SessionActivity::NeedsYou),
        host: None,
    });
    let ended = SessionFace::of(crate::wire::Life::Ended);
    for line in [added("a", &needs), added("b", &ended)] {
        book.fold(&crate::wire::decode::<serde_json::Value>(&line).expect("一帧"));
    }
    let running = |sid: &str| if sid == "a" { 2 } else { 0 };
    let here = rows_here(
        &["b".to_string(), "nope".to_string(), "a".to_string()],
        &book,
        &running,
    );
    assert_eq!(
        here.rows,
        [
            row("b", ended.order, false, 0),
            row("a", needs.order, true, 2),
        ]
    );
}
