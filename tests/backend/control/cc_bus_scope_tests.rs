//! cc-bus 组范围（`control/cc_bus_scope.rs`）的判据。
//!
//! | # | 判据 |
//! |---|---|
//! | G1 | ★ a、b 同组，c 不在 ⇒ a→c `not_same_group`、a→b 过 |
//! | G2 | a 在两组 ⇒ 两组的组员都过（几组取并集） |
//! | G3 | a 不在任何组 ⇒ 发给谁都过（照今天） |
//! | G4 | ★ 名单没读成 ⇒ `no_roster`，不退回成全放行 |
//! | G5 | 这台那一片没读成（别的台读到了也不算）⇒ 名单算没读成 |

use super::*;
use crate::history::groups::{
    GroupKnown, GroupMember, GroupRead, GroupsMachine, GroupsRead, MemberFacts,
};
use crate::history::history_browse::Read;
use cmd_wire::Machine;

fn member(origin: &str, sid: &str) -> GroupMember {
    GroupMember {
        origin: Machine(origin.into()),
        sid: sid.into(),
        pinned: false,
        face: crate::wire::unseen_face(),
        facts: MemberFacts {
            title: "t".into(),
            cwd: None,
            record: None,
            account: None,
            bg: false,
            at: None,
        },
    }
}

fn group(id: &str, name: &str, sids: &[&str]) -> GroupRead {
    GroupRead {
        id: id.into(),
        name: Some(name.into()),
        title: name.into(),
        made: None,
        named: None,
        known: GroupKnown::Now,
        members: sids.iter().map(|s| member(cmd_wire::LOCAL, s)).collect(),
        counts: vec![],
    }
}

fn read(groups: Vec<GroupRead>, local: Read) -> GroupsRead {
    GroupsRead {
        groups,
        pinned: vec![],
        machines: vec![GroupsMachine {
            origin: Machine(cmd_wire::LOCAL.into()),
            read: local,
        }],
    }
}

fn ok() -> Read {
    Read::Ok { notice: None }
}

#[test]
fn g1_same_group_passes_and_outside_the_group_is_refused() {
    let r = read(vec![group("g1", "开发", &["a", "b"])], ok());
    assert_eq!(scope("a", "b", Ok(&r)), Ok(()));
    let (code, raw) = scope("a", "c", Ok(&r)).unwrap_err();
    assert_eq!(code, "not_same_group");
    assert!(raw.contains("开发"), "原话说发件人在哪几组：{raw}");
}

#[test]
fn g2_several_groups_are_a_union() {
    let r = read(
        vec![
            group("g1", "甲", &["a", "b"]),
            group("g2", "乙", &["a", "d"]),
            group("g3", "丙", &["c"]),
        ],
        ok(),
    );
    assert_eq!(scope("a", "b", Ok(&r)), Ok(()));
    assert_eq!(scope("a", "d", Ok(&r)), Ok(()));
    assert_eq!(scope("a", "c", Ok(&r)).unwrap_err().0, "not_same_group");
}

#[test]
fn g3_a_sender_in_no_group_reaches_anyone() {
    let r = read(vec![group("g1", "开发", &["b", "c"])], ok());
    assert_eq!(scope("a", "b", Ok(&r)), Ok(()));
    assert_eq!(scope("a", "z", Ok(&r)), Ok(()));
}

#[test]
fn g4_no_roster_is_refused_not_waved_through() {
    let (code, raw) = scope("a", "b", Err("常驻没答 · 这台也没读成")).unwrap_err();
    assert_eq!(code, "no_roster");
    assert!(raw.contains("这台也没读成"));
}

#[test]
fn g5_this_machines_slice_unread_means_no_roster() {
    let r = read(
        vec![],
        Read::Failed {
            why: "注解文件读不懂".into(),
        },
    );
    assert_eq!(scope("a", "b", Ok(&r)).unwrap_err().0, "no_roster");
    let r = read(vec![], Read::Unread);
    assert_eq!(scope("a", "b", Ok(&r)).unwrap_err().0, "no_roster");
    let r = read(
        vec![],
        Read::Stale {
            why: "x".into(),
            at: "t".into(),
        },
    );
    assert_eq!(scope("a", "b", Ok(&r)), Ok(()), "上次所知也是一份名单");
}
