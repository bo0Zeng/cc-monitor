//! agent 自问（`history/session_self.rs`）的判据。
//!
//! | # | 判据 |
//! |---|---|
//! | S1 | ★ 进程链 [ccm ← bash ← claude ← tmux]，claude 的 pid 在活会话表里 ⇒ 认出那条会话 |
//! | S2 | 链上没有表里的进程 ⇒ 不在会话里 |
//! | S3 | pid 对上、却已不是写那份 pidfile 的那一个（pid 复用）⇒ 不认 |
//! | S4 | 往上找：到顶 · 绕回 · 超过上限都停 |
//! | S5 | 组只留有「这台这一条」的那几组（别的台同名的会话不算） |
//! | S6 | 常驻答了 ⇒ 看了各台；常驻不在 ⇒ 只问本机、照实标「只看了本机」 |

use super::*;
use crate::history::groups::{GroupKnown, GroupMember, GroupRead, GroupsRead, MemberFacts};
use crate::history::live_table::LiveRow;
use cmd_wire::Machine;

fn row(pid: u32, sid: &str, alive: bool) -> LiveRow {
    LiveRow {
        pid,
        fact: crate::agents::ProcFact {
            sid: Some(sid.to_string()),
            ..Default::default()
        },
        alive,
        start_checked: alive,
        frozen: false,
    }
}

#[test]
fn s1_the_first_ancestor_in_the_live_table_is_this_session() {
    let chain = [10, 11, 12, 13];
    let rows = [row(99, "别的", true), row(12, "s1", true)];
    assert_eq!(first_session(&chain, &rows).as_deref(), Some("s1"));
}

#[test]
fn s2_no_ancestor_in_the_table_means_not_in_a_session() {
    assert_eq!(first_session(&[10, 11, 1], &[row(99, "别的", true)]), None);
}

#[test]
fn s3_a_reused_pid_is_not_taken_for_the_session() {
    assert_eq!(first_session(&[10, 12], &[row(12, "s1", false)]), None);
}

#[test]
fn s4_walking_up_stops_at_the_top_on_a_loop_and_at_the_cap() {
    let up = |p: u32| match p {
        10 => Some(11),
        11 => Some(12),
        12 => Some(1),
        1 => Some(0),
        _ => None,
    };
    assert_eq!(chain_from(10, up), vec![10, 11, 12, 1]);
    assert_eq!(
        chain_from(5, |p| Some(if p == 5 { 6 } else { 5 })),
        vec![5, 6],
        "绕回就停"
    );
    assert_eq!(chain_from(1_000, |p| Some(p + 1)).len(), MAX_HOPS);
}

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

fn group(id: &str, members: Vec<GroupMember>) -> GroupRead {
    GroupRead {
        id: id.into(),
        name: Some(id.into()),
        title: id.into(),
        home: None,
        made: None,
        known: GroupKnown::Now,
        members,
        counts: vec![],
    }
}

fn read() -> GroupsRead {
    GroupsRead {
        groups: vec![
            group(
                "g1",
                vec![member(cmd_wire::LOCAL, "s1"), member("devbox", "s9")],
            ),
            group("g2", vec![member("devbox", "s1")]),
            group("g3", vec![member(cmd_wire::LOCAL, "s2")]),
        ],
        pinned: vec![],
        machines: vec![],
    }
}

#[test]
fn s5_only_groups_holding_this_machines_session_are_kept() {
    let got = mine(read(), "s1");
    assert_eq!(
        got.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
        ["g1"]
    );
    assert_eq!(got[0].members.len(), 2, "组员整组带上（别的台的也在）");
}

#[tokio::test]
async fn s6_resident_answered_means_every_machine_else_this_machine_only() {
    let all = answer("s1".into(), async { Ok(read()) }, async {
        panic!("常驻答了就不该再问本机")
    })
    .await;
    assert_eq!(all.looked, Looked::Every);
    assert_eq!(all.sid, "s1");
    assert_eq!(all.groups.len(), 1);
    let here = answer(
        "s1".into(),
        async { Err("没有常驻在听".to_string()) },
        async { Ok(read()) },
    )
    .await;
    assert_eq!(here.looked, Looked::ThisMachine);
    assert_eq!(here.groups.len(), 1);
    let none = answer("s1".into(), async { Err("x".to_string()) }, async {
        Err("y".to_string())
    })
    .await;
    assert_eq!(
        none.looked,
        Looked::Nothing,
        "两边都没答 ⇒ 照实说没看成，组一个不编"
    );
    assert!(none.groups.is_empty());
}

/// ★ 真走一遍：这台的 pidfile 目录里有一份写着本进程的会话 ⇒ 从本进程往上（真读父进程号）认出它；没有 ⇒ 不在会话里。
#[cfg(unix)]
#[test]
fn s7_the_real_chain_from_this_process_finds_the_session_whose_pidfile_names_an_ancestor() {
    let home = std::env::temp_dir().join(format!("ccm-session-self-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let sessions = crate::agents::pidfile_dir(&home);
    std::fs::create_dir_all(&sessions).unwrap();
    let parent = crate::platform::proc::parent_of(std::process::id()).expect("读得出父进程号");
    assert_eq!(
        who_at(&home, std::process::id()),
        None,
        "没有 pidfile ⇒ 不在会话里"
    );
    std::fs::write(
        sessions.join(format!("{parent}.json")),
        r#"{"sessionId":"s-live","cwd":"/w"}"#,
    )
    .unwrap();
    assert_eq!(
        who_at(&home, std::process::id()).as_deref(),
        Some("s-live"),
        "父进程那一份就是这条会话"
    );
    let _ = std::fs::remove_dir_all(&home);
}
