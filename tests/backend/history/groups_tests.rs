//! 组（`history/groups.rs`）的判据。
//!
//! | # | 判据 |
//! |---|---|
//! | G1 | ★ 组员住会话那一台、组名住家那一台：本机 `groups-read` 把两半并成一组（名字 · 家 · 组员 · 组头计数与那一句） |
//! | G2 | 家那台这一问没答、之前答过 ⇒ 组照旧出、标「上次所知」，那台的组员画成看不见（不冒充在跑） |
//! | G3 | 家那台从没答过 ⇒ 那台标读不到、组照出（名字不知道、叫它的那一句照实说不知道），组员不漏 |
//! | G4 | 各台都答了、哪台都没有那一组 ⇒ 已解散，不出 |
//! | G5 | 问到别的台却没带期限 ⇒ `bad_args` |
//! | G6 | 这台那一半：家在这台的组 ＋ 带组或固定的会话（各带此刻活不活） |
//! | G7 | 建 · 改名 · 解散只在家那台；名字空白 · 过长拒；不是这台的组 ⇒ 拒、文件不动 |
//! | G8 | 组那一节认不出的键原样留着；读不懂的文件拒写、一个字节不动 |
//! | G9 | 组员 · 固定那一行带着复活要的几格（标题 · 目录 · 记录 · 号 · 后台 · 最后活动），照那台给的原样过 `groups-read` |
//! | G10 | 这台给的那几格照清单那一行出：改过的标题优先、记录路径 · 号 · 目录照行；清单里没有的照活会话表画样子、标题照注解，没有就是清单那一句「没名字」 |

use super::*;
use crate::dial::remote_ask::test_peers::{until, Peers};
use cmd_wire::listing::RowLife;
use cmd_wire::Machine;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn m(name: &str) -> Machine {
    Machine(name.into())
}

fn face(life: RowLife) -> cmd_wire::frame::SessionFace {
    use crate::wire::SessionFaceExt as _;
    cmd_wire::frame::SessionFace::of(crate::wire::Life::of_row(Some(life)))
}

/// 复活那几格（占位的值，照真东西的形状）。
fn facts(sid: &str) -> MemberFacts {
    MemberFacts {
        title: format!("标题-{sid}").into(),
        cwd: Some(format!("/w/{sid}")),
        record: Some(format!("/r/{sid}.jsonl")),
        account: Some("work".into()),
        bg: false,
        at: Some(42),
    }
}

fn member(sid: &str, groups: &[&str], pinned: bool, life: RowLife) -> HereMember {
    HereMember {
        sid: sid.into(),
        groups: groups.iter().map(|g| g.to_string()).collect(),
        pinned,
        face: face(life),
        facts: facts(sid),
    }
}

fn running() -> RowLife {
    RowLife::Live {
        activity: Some(crate::agents::SessionActivity::Working),
    }
}

/// A 台（本机）：一条会话在 g1，在跑。
fn here_a() -> GroupsHere {
    GroupsHere {
        groups: vec![],
        members: vec![member("s-a", &["g1"], false, running())],
    }
}

/// B 台（devbox）：家里有 g1「开发」，没有组员。
fn here_b() -> String {
    serde_json::to_string(&GroupsHere {
        groups: vec![HereGroup {
            id: "g1".into(),
            name: "开发".into(),
            made: 7,
        }],
        members: vec![],
    })
    .unwrap()
}

async fn read(peers: &Peers, kept: &Kept, now_ms: i64) -> GroupsRead {
    read_with(
        &[m(cmd_wire::LOCAL), m("devbox")],
        std::future::ready(Ok(here_a())),
        &Default::default(),
        until(9_000),
        &Peers::table(&["devbox"]),
        peers,
        kept,
        now_ms,
    )
    .await
    .expect("合得出")
}

fn machine<'a>(r: &'a GroupsRead, name: &str) -> &'a super::super::history_browse::Read {
    &r.machines
        .iter()
        .find(|x| x.origin.0 == name)
        .unwrap_or_else(|| panic!("{name} 那一格"))
        .read
}

#[tokio::test]
async fn g1_members_on_one_machine_and_the_name_at_home_merge_into_one_group() {
    let kept = Kept::default();
    let got = read(&Peers::new(&[("devbox", here_b())]), &kept, 1_000).await;
    assert_eq!(got.groups.len(), 1, "{got:?}");
    let g = &got.groups[0];
    assert_eq!(g.id, "g1");
    assert_eq!(g.name.as_deref(), Some("开发"), "名字照家那台");
    assert_eq!(g.home.as_ref().map(|h| h.0.as_str()), Some("devbox"));
    assert_eq!(g.known, GroupKnown::Now);
    assert_eq!(g.members.len(), 1);
    assert_eq!(g.members[0].origin.0, cmd_wire::LOCAL);
    assert_eq!(g.members[0].sid, "s-a");
    assert_eq!(g.members[0].face.dot, cmd_wire::frame::SessionDot::Running);
    assert_eq!(g.counts.len(), 1, "只数有的那一样");
    assert_eq!(g.counts[0].dot, cmd_wire::frame::SessionDot::Running);
    assert_eq!(g.counts[0].n, 1);
    assert_eq!(g.counts[0].word, g.members[0].face.name, "字照组员那张脸");
    assert_eq!(
        g.counts[0].text.0,
        copy_core::copy_text("beGroups.count.running", &[("n", "1")]),
        "组头那一句写好了（出口照摆）"
    );
    assert_eq!(g.title.0, "开发", "叫什么：有名字 ⇒ 名字");
    assert!(matches!(
        machine(&got, "devbox"),
        super::super::history_browse::Read::Ok { .. }
    ));
}

#[tokio::test]
async fn g2_a_home_that_answered_before_is_shown_as_last_known() {
    let kept = Kept::default();
    let _ = read(&Peers::new(&[("devbox", here_b())]), &kept, 1_000).await;
    let got = read(&Peers::new(&[]), &kept, 61_000).await;
    let g = &got.groups[0];
    assert_eq!(g.name.as_deref(), Some("开发"), "上次所知的名字");
    assert!(
        matches!(g.known, GroupKnown::Before { .. }),
        "标上次所知：{:?}",
        g.known
    );
    assert!(matches!(
        machine(&got, "devbox"),
        super::super::history_browse::Read::Stale { .. }
    ));
    assert_eq!(g.members.len(), 1, "本机那条照旧");
}

#[tokio::test]
async fn g2_members_on_a_stale_machine_are_drawn_unseen_not_running() {
    let kept = Kept::default();
    let b = serde_json::to_string(&GroupsHere {
        groups: vec![HereGroup {
            id: "g1".into(),
            name: "开发".into(),
            made: 7,
        }],
        members: vec![member("s-b", &["g1"], true, running())],
    })
    .unwrap();
    let _ = read(&Peers::new(&[("devbox", b)]), &kept, 1_000).await;
    let got = read(&Peers::new(&[]), &kept, 2_000).await;
    let g = &got.groups[0];
    let sb = g
        .members
        .iter()
        .find(|x| x.sid == "s-b")
        .expect("那台的组员不漏");
    assert_eq!(sb.face.dot, cmd_wire::frame::SessionDot::Unknown);
    assert_eq!(
        g.counts.iter().map(|c| c.n).sum::<u32>(),
        1,
        "只数本机在跑那一条"
    );
    assert!(got.pinned.iter().any(|x| x.sid == "s-b"), "固定的跟着出");
}

#[tokio::test]
async fn g3_a_home_never_heard_from_leaves_the_group_nameless_but_listed() {
    let kept = Kept::default();
    let got = read(&Peers::new(&[]), &kept, 1_000).await;
    assert!(matches!(
        machine(&got, "devbox"),
        super::super::history_browse::Read::Failed { .. }
    ));
    let g = &got.groups[0];
    assert_eq!(g.id, "g1");
    assert_eq!(g.name, None);
    assert_eq!(
        g.title.0,
        copy_core::copy_text("beGroups.name.unknown", &[]),
        "名字不知道 ⇒ 照实说不知道，不出空串"
    );
    assert_eq!(g.home, None);
    assert_eq!(g.known, GroupKnown::Unknown);
    assert_eq!(g.members[0].sid, "s-a");
}

#[tokio::test]
async fn g4_a_group_nobody_holds_while_everyone_answered_is_dissolved() {
    let kept = Kept::default();
    let empty = serde_json::to_string(&GroupsHere::default()).unwrap();
    let got = read(&Peers::new(&[("devbox", empty)]), &kept, 1_000).await;
    assert!(got.groups.is_empty(), "{got:?}");
}

#[tokio::test]
async fn g5_asking_another_machine_without_a_deadline_is_bad_args() {
    let got = read_with(
        &[m(cmd_wire::LOCAL), m("devbox")],
        std::future::ready(Ok(here_a())),
        &Default::default(),
        None,
        &Peers::table(&["devbox"]),
        &Peers::new(&[]),
        &Kept::default(),
        0,
    )
    .await;
    assert!(matches!(got, Err(GroupsReadErr::BadArgs(_))));
}

#[test]
fn g6_this_machines_half_is_its_groups_and_its_grouped_or_pinned_sessions() {
    let mut t = crate::history::history_annotations::Table::new();
    t.insert(
        "s1".into(),
        cmd_wire::listing::Entry {
            groups: vec!["g1".into()],
            ..Default::default()
        },
    );
    t.insert(
        "s2".into(),
        cmd_wire::listing::Entry {
            pinned: true,
            ..Default::default()
        },
    );
    t.insert(
        "s3".into(),
        cmd_wire::listing::Entry {
            starred: true,
            ..Default::default()
        },
    );
    let mut book = Book::new();
    book.insert(
        "g1".into(),
        GroupRec {
            name: "开发".into(),
            made: 3,
        },
    );
    let got = here_of(&t, &book, |sid| {
        let life = if sid == "s1" {
            running()
        } else {
            RowLife::Ended
        };
        (face(life), facts(sid))
    });
    assert_eq!(
        got.groups,
        vec![HereGroup {
            id: "g1".into(),
            name: "开发".into(),
            made: 3
        }]
    );
    let sids: Vec<&str> = got.members.iter().map(|x| x.sid.as_str()).collect();
    assert_eq!(sids, ["s1", "s2"], "只带有组或固定的");
    assert_eq!(got.members[0].face, face(running()), "样子是这台算的");
    assert_eq!(got.members[1].facts, facts("s2"), "复活那几格跟着走");
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-groups-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn own_at(d: &Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_data_at(d)
}

fn write(
    own: &crate::platform::fs::Own,
    op: GroupsWriteReq,
) -> Result<GroupsWritten, GroupsWriteErr> {
    write_at(own, op, 5)
}

fn book_at(d: &Path) -> Book {
    crate::history::history_annotations::load_all_at(&d.join("history-metadata.json"))
        .expect("读得动")
        .1
}

#[test]
fn g7_make_rename_dissolve_happen_at_home_and_nowhere_else() {
    let d = temp_dir("write");
    let own = own_at(&d);
    let made = match write(
        &own,
        GroupsWriteReq::Make {
            name: "  开发 ".into(),
        },
    )
    .unwrap()
    {
        GroupsWritten::Made(g) => g,
        other => panic!("{other:?}"),
    };
    assert_eq!(made.name, "开发", "名字去掉首尾空白");
    assert_eq!(made.made, 5);
    assert_eq!(book_at(&d)[&made.id].name, "开发");
    write(
        &own,
        GroupsWriteReq::Rename {
            id: made.id.clone(),
            name: "测试".into(),
        },
    )
    .unwrap();
    assert_eq!(book_at(&d)[&made.id].name, "测试");
    let before = std::fs::read(d.join("history-metadata.json")).unwrap();
    for op in [
        GroupsWriteReq::Rename {
            id: "g-elsewhere".into(),
            name: "x".into(),
        },
        GroupsWriteReq::Dissolve {
            id: "g-elsewhere".into(),
        },
        GroupsWriteReq::Make { name: "   ".into() },
        GroupsWriteReq::Make {
            name: "长".repeat(NAME_MAX + 1),
        },
    ] {
        assert!(
            matches!(write(&own, op), Err(GroupsWriteErr::BadArgs(_))),
            "不是这台的组 · 名字不对 ⇒ 拒"
        );
    }
    assert_eq!(
        std::fs::read(d.join("history-metadata.json")).unwrap(),
        before,
        "拒了就不动"
    );
    write(
        &own,
        GroupsWriteReq::Dissolve {
            id: made.id.clone(),
        },
    )
    .unwrap();
    assert!(book_at(&d).is_empty());
}

#[test]
fn g8_unknown_keys_in_the_groups_section_survive_and_an_unreadable_file_is_never_written() {
    let d = temp_dir("keys");
    let p = d.join("history-metadata.json");
    std::fs::write(
        &p,
        r#"{"version":1,"entries":{"s":{"starred":true,"customTitle":null,"updatedAt":1,"x":1}},"groups":{"g1":{"name":"甲","made":1,"color":"red"}},"top":2}"#,
    )
    .unwrap();
    let own = own_at(&d);
    write(
        &own,
        GroupsWriteReq::Rename {
            id: "g1".into(),
            name: "乙".into(),
        },
    )
    .unwrap();
    let v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(v["groups"]["g1"]["name"], "乙");
    assert_eq!(v["groups"]["g1"]["color"], "red", "组里认不出的键留着");
    assert_eq!(v["top"], 2);
    assert_eq!(v["entries"]["s"]["x"], 1, "注解原样");

    std::fs::write(&p, b"{not json").unwrap();
    let before = std::fs::read(&p).unwrap();
    assert!(write(&own, GroupsWriteReq::Make { name: "丙".into() }).is_err());
    assert_eq!(std::fs::read(&p).unwrap(), before);
}

#[tokio::test]
async fn g9_member_rows_carry_what_resurrecting_a_tab_needs() {
    let kept = Kept::default();
    let b = serde_json::to_string(&GroupsHere {
        groups: vec![HereGroup {
            id: "g1".into(),
            name: "开发".into(),
            made: 7,
        }],
        members: vec![member("s-b", &["g1"], true, RowLife::Ended)],
    })
    .unwrap();
    let got = read(&Peers::new(&[("devbox", b)]), &kept, 1_000).await;
    let g = &got.groups[0];
    let sb = g.members.iter().find(|x| x.sid == "s-b").expect("组员");
    assert_eq!(sb.facts, facts("s-b"), "照那台给的原样");
    let pin = got
        .pinned
        .iter()
        .find(|x| x.sid == "s-b")
        .expect("固定那一行");
    assert_eq!(pin.facts, facts("s-b"));
    assert_eq!(pin.origin.0, "devbox");
}

fn raw_row(sid: &str, life: Option<RowLife>) -> cmd_wire::listing::RawRow {
    let mut v = serde_json::json!({
        "agent": "claude",
        "sessionId": sid,
        "projectPath": "/w/app",
        "projectName": "app",
        "aiTitle": "修登录",
        "firstUserExcerpt": "第一句",
        "title": "修登录",
        "startedAt": 1,
        "updatedAt": 99,
        "jsonlPath": format!("/r/{sid}.jsonl"),
        "life": null,
        "messageCountApprox": 3,
        "isBg": true,
        "starred": false,
        "customTitle": null,
        "projectDir": "-w-app",
        "group": "claude:/w/app",
        "lastAccount": "work",
    });
    v["life"] = serde_json::to_value(life).unwrap();
    serde_json::from_value(v).expect("清单行的形状")
}

#[test]
fn g10_this_machines_facts_come_from_its_listing_row() {
    let mut t = crate::history::history_annotations::Table::new();
    t.insert(
        "s1".into(),
        cmd_wire::listing::Entry {
            custom_title: Some("改过的".into()),
            groups: vec!["g1".into()],
            ..Default::default()
        },
    );
    let seen = seen_from(vec![raw_row("s1", Some(running()))], &t);
    let (f, x) = seen.get("s1").expect("清单里那一条");
    assert_eq!(f.dot, cmd_wire::frame::SessionDot::Running);
    assert_eq!(x.title.0, "改过的", "改过的标题优先");
    assert_eq!(x.cwd.as_deref(), Some("/w/app"));
    assert_eq!(x.record.as_deref(), Some("/r/s1.jsonl"));
    assert_eq!(x.account.as_deref(), Some("work"));
    assert!(x.bg);
    assert_eq!(x.at, Some(99));
    let (f, x) = unlisted(t.get("s1"), running());
    assert_eq!(f.dot, cmd_wire::frame::SessionDot::Running, "照活会话表");
    assert_eq!(x.record, None);
    assert_eq!(x.title.0, "改过的", "改过的标题照注解");
    let (_, x) = unlisted(None, RowLife::Ended);
    assert_eq!(
        x.title.0,
        copy_core::copy_text("history.row.untitled", &[]),
        "没名字照清单那一句"
    );
}
