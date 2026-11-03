//! 组的待补（`history/groups_owed.rs`）的判据：没送到的那几台记一条、送到的消掉、照那台那一片判补不补。
//!
//! | # | 判据 |
//! |---|---|
//! | O1 | 改名没送到 E ⇒ E 那一组记一条（带名字与那一刻）；再改名又没送到 ⇒ 盖掉先前那条；送到了 ⇒ 消掉 |
//! | O2 | 解散 ⇒ 那一组欠各台的改名都消掉，没送到的那几台记一条解散 |
//! | O3 | 改名：那台那一片有那一组、名字比欠的那一刻旧 ⇒ 补；名字不旧 · 没有那一组 ⇒ 丢 |
//! | O4 | 解散：那台那一片还有那一组（记录或组员指着它）⇒ 补；都没有 ⇒ 丢 |
//! | O5 | 欠的那台已不在机器表 ⇒ 丢；落盘读回来是同一份 |

use super::*;
use cmd_wire::groups::{GroupsHere, HereGroup, HereMember, MemberFacts};
use cmd_wire::Machine;

fn m(name: &str) -> Machine {
    Machine::named(name)
}

fn rename(name: &str, named: i64) -> Owe {
    Owe::Rename {
        name: name.into(),
        named,
    }
}

fn slice(groups: &[(&str, i64)], member_in: &[&str]) -> GroupsHere {
    GroupsHere {
        groups: groups
            .iter()
            .map(|(id, named)| HereGroup {
                id: id.to_string(),
                name: "旧".into(),
                made: 1,
                named: *named,
            })
            .collect(),
        members: vec![HereMember {
            sid: "s".into(),
            groups: member_in.iter().map(|g| g.to_string()).collect(),
            pinned: false,
            face: face_core::book::unseen(cmd_wire::cells::Words("f".into())),
            facts: MemberFacts {
                title: cmd_wire::cells::Words("t".into()),
                cwd: None,
                record: None,
                account: None,
                bg: false,
                at: None,
            },
        }],
    }
}

#[test]
fn o1_a_missed_rename_is_noted_overwritten_and_cleared_once_sent() {
    let mut o = Owed::default();
    note(&mut o, "g1", &[], &[m("E")], rename("乙", 10));
    assert_eq!(
        o.owed,
        vec![Debt {
            id: "g1".into(),
            machine: m("E"),
            what: rename("乙", 10),
        }]
    );
    note(&mut o, "g1", &[], &[m("E")], rename("丙", 20));
    assert_eq!(o.owed.len(), 1, "同一台同一组只留最新那条：{o:?}");
    assert_eq!(o.owed[0].what, rename("丙", 20));
    note(&mut o, "g1", &[m("E")], &[], rename("丁", 30));
    assert!(o.owed.is_empty(), "送到了 ⇒ 先前欠的消掉：{o:?}");
}

#[test]
fn o2_dissolving_clears_every_rename_owed_for_that_group() {
    let mut o = Owed::default();
    note(&mut o, "g1", &[], &[m("E"), m("F")], rename("乙", 10));
    note(&mut o, "g2", &[], &[m("E")], rename("x", 10));
    note(&mut o, "g1", &[m("E")], &[m("G")], Owe::Dissolve);
    let left: Vec<(&str, String, &Owe)> = o
        .owed
        .iter()
        .map(|d| (d.id.as_str(), d.machine.to_string(), &d.what))
        .collect();
    assert_eq!(
        left,
        [
            ("g2", "E".to_string(), &rename("x", 10)),
            ("g1", "G".to_string(), &Owe::Dissolve),
        ]
    );
}

#[test]
fn o3_a_rename_is_due_only_where_that_slice_still_holds_an_older_name() {
    let d = |named| Debt {
        id: "g1".into(),
        machine: m("E"),
        what: rename("乙", named),
    };
    assert_eq!(due(&d(10), &slice(&[("g1", 5)], &["g1"])), Due::Send);
    assert_eq!(due(&d(10), &slice(&[("g1", 10)], &["g1"])), Due::Drop);
    assert_eq!(due(&d(10), &slice(&[("g2", 1)], &["g2"])), Due::Drop);
}

#[test]
fn o4_a_dissolve_is_due_while_the_group_is_still_there() {
    let d = Debt {
        id: "g1".into(),
        machine: m("E"),
        what: Owe::Dissolve,
    };
    assert_eq!(due(&d, &slice(&[("g1", 1)], &[])), Due::Send);
    assert_eq!(due(&d, &slice(&[], &["g1"])), Due::Send, "只有组员指着它");
    assert_eq!(due(&d, &slice(&[], &["g2"])), Due::Drop);
}

#[test]
fn o5_debts_for_machines_off_the_table_expire_and_the_file_round_trips() {
    let mut o = Owed::default();
    note(&mut o, "g1", &[], &[m("E"), m("gone")], rename("乙", 10));
    let off = gone(&o, |name| name == "E");
    assert_eq!(off.len(), 1);
    assert_eq!(off[0].machine, m("gone"));
    let d = std::env::temp_dir().join(format!("ccm-owed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let own = crate::platform::fs::Own::issue_at(&d);
    edit_at(&own, |x| *x = o.clone()).expect("写得下");
    assert_eq!(load_at(&own), Some(o));
    let _ = std::fs::remove_dir_all(&d);
}
