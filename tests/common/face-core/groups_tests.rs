//! 几台的组合成一份（`face_core::groups::merge`）的判据：本机后端 `groups-read` 与手机核心调的是这一份。
//!
//! 1. 同一组在两片里 ⇒ 并成一组：名字取 `named` 新的那片（不照问的先后），`made` 取最早，组员照问的台序接。
//! 2. 同一刻起的名字 ⇒ 这一问答了的那片优先于上次所知那片。
//! 3. 上次所知那片的组员画成看不见那一张（那一句是调用方交的离线那一句），不冒充在跑；固定的另出一列。
//! 4. 哪一片都没有记录、只有组员指着的组 ⇒ 照出、名字缺；组照建的先后排、没记录的排最后。

use super::*;
use cmd_wire::cells::Words;
use cmd_wire::groups::{GroupKnown, GroupsHere, HereGroup, HereMember, MemberFacts};
use cmd_wire::Machine;

fn rec(id: &str, name: &str, made: i64, named: i64) -> HereGroup {
    HereGroup {
        id: id.into(),
        name: name.into(),
        made,
        named,
    }
}

fn member(sid: &str, groups: &[&str], pinned: bool) -> HereMember {
    HereMember {
        sid: sid.into(),
        groups: groups.iter().map(|g| g.to_string()).collect(),
        pinned,
        face: crate::book::unseen(Words(format!("face-{sid}"))),
        facts: MemberFacts {
            title: Words(format!("标题-{sid}")),
            cwd: None,
            record: None,
            account: None,
            bg: false,
            at: None,
        },
    }
}

fn slice(groups: Vec<HereGroup>, members: Vec<HereMember>) -> GroupsHere {
    GroupsHere { groups, members }
}

fn m(name: &str) -> Machine {
    Machine::named(name)
}

#[test]
fn one_group_across_two_slices_is_one_group_named_by_the_newest_slice() {
    let c = slice(
        vec![rec("g1", "甲", 5, 5)],
        vec![member("c", &["g1"], false)],
    );
    let e = slice(
        vec![rec("g1", "乙", 7, 9)],
        vec![member("e", &["g1"], false)],
    );
    let got = merge(
        &[(m("C"), Half::Now(c)), (m("E"), Half::Now(e))],
        Vec::new(),
    );
    assert_eq!(got.groups.len(), 1, "同一个组 id 并成一组：{got:?}");
    let g = &got.groups[0];
    assert_eq!(g.id, "g1");
    assert_eq!(g.name.as_deref(), Some("乙"), "名字取 named 新的那片");
    assert_eq!(g.title.0, "乙");
    assert_eq!(g.named, Some(9));
    assert_eq!(g.made, Some(5), "建组时刻取最早的");
    assert_eq!(g.known, GroupKnown::Now);
    let who: Vec<(String, &str)> = g
        .members
        .iter()
        .map(|x| (x.origin.to_string(), x.sid.as_str()))
        .collect();
    assert_eq!(
        who,
        [("C".to_string(), "c"), ("E".to_string(), "e")],
        "组员照问的台序"
    );
}

#[test]
fn at_the_same_instant_the_slice_that_answered_now_wins() {
    let before = slice(vec![rec("g1", "旧片", 1, 4)], vec![]);
    let now = slice(vec![rec("g1", "这一问", 1, 4)], vec![]);
    let got = merge(
        &[
            (
                m("B"),
                Half::Before {
                    here: before,
                    at: Words("刚才".into()),
                    offline: Words("B 离线".into()),
                },
            ),
            (m("C"), Half::Now(now)),
        ],
        Vec::new(),
    );
    assert_eq!(got.groups[0].name.as_deref(), Some("这一问"));
    assert_eq!(got.groups[0].known, GroupKnown::Now);
}

#[test]
fn last_known_members_wear_the_unseen_face_with_the_given_line_and_pins_list_apart() {
    let before = slice(
        vec![rec("g1", "甲", 1, 1)],
        vec![member("b", &["g1"], true)],
    );
    let got = merge(
        &[(
            m("B"),
            Half::Before {
                here: before,
                at: Words("刚才".into()),
                offline: Words("B 离线".into()),
            },
        )],
        Vec::new(),
    );
    let g = &got.groups[0];
    assert_eq!(
        g.known,
        GroupKnown::Before {
            at: Words("刚才".into())
        }
    );
    assert_eq!(
        g.members[0].face,
        crate::book::unseen(Words("B 离线".into()))
    );
    assert_eq!(got.pinned.len(), 1, "固定的另出一列");
    assert_eq!(got.pinned[0].sid, "b");
}

#[test]
fn a_group_only_members_point_at_is_listed_nameless_and_last() {
    let c = slice(
        vec![rec("g2", "乙", 3, 3)],
        vec![member("c", &["g9"], false), member("d", &["g2"], false)],
    );
    let got = merge(&[(m("C"), Half::Now(c)), (m("D"), Half::None)], Vec::new());
    let ids: Vec<&str> = got.groups.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(ids, ["g2", "g9"]);
    let g9 = &got.groups[1];
    assert_eq!(g9.name, None);
    assert_eq!(g9.known, GroupKnown::Unknown);
    assert_eq!(
        g9.title.0,
        copy_core::copy_text("beGroups.name.unknown", &[])
    );
    assert_eq!(g9.members.len(), 1, "组员不漏");
}
