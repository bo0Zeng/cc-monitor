//! 组（`history/groups.rs`）的判据。组跟着组员走：每台只存自己有组员的那一片，名字带「几时起的」抄进每一片。
//!
//! | # | 判据 |
//! |---|---|
//! | G1 | ★ B 建 g1 收 C 上的 c 与 E 上的 e；只连 C · D 的 A 读 ⇒ 有 g1、名字是 B 起的、组员只有 c（D 读不到，不冒充看全） |
//! | G2 | 几片的名字不同 ⇒ 取 `named` 最新的那份；建组时刻取最早的；回包里没有哪一台是「家」 |
//! | G3 | 某台这一问没答、之前答过 ⇒ 那一片照旧用、标「上次所知」，那台的组员画成看不见（不冒充在跑），固定的跟着出 |
//! | G4 | 哪一片都没有那一组的记录、只有组员指着它 ⇒ 组照出、名字不知道（叫它的那一句照实说）；谁都没有的组 ⇒ 不出 |
//! | G5 | 问到别的台却没带期限 ⇒ `bad_args`（读 · 发给各片都是） |
//! | G6 | 这台那一片：这一片的组（带 `named`）＋ 带组或固定的会话（各带此刻活不活） |
//! | G7 | `make` 只起 id、不写盘；`move` 进组把记录抄进这一片、这一片已有 ⇒ 名字照 `named` 新的；最后一位出组 ⇒ 这一片的记录删；名字 · id 不对 ⇒ 拒、文件不动 |
//! | G8 | 组那一节认不出的键原样留着；读不懂的文件拒写、一个字节不动 |
//! | G9 | 组员 · 固定那一行带着复活要的几格，照那台给的原样过 `groups-read` |
//! | G10 | 这台给的那几格照清单那一行出：改过的标题优先、记录路径 · 号 · 目录照行；清单里没有的照活会话表画样子、标题照注解 |
//! | G11 | 改名 · 解散发给每一片：本机那一片当场写，转给别的台的那一问只写那台自己；没答的那台照实列进 `missed`；没有那一组的片不动 |
//! | G12 | 解散在那一片摘掉每条会话身上的那一组、记录删掉，会话别的格不动 |

use super::*;
use crate::dial::remote_ask::test_peers::{until, Peers};
use crate::wire::WireLife;
use cmd_wire::Machine;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn m(name: &str) -> Machine {
    Machine::named(name)
}

fn face(life: WireLife) -> cmd_wire::frame::SessionFace {
    use crate::wire::SessionFaceExt as _;
    cmd_wire::frame::SessionFace::of(crate::wire::Life::of_wire(&life))
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

fn member(sid: &str, groups: &[&str], pinned: bool, life: WireLife) -> HereMember {
    HereMember {
        sid: sid.into(),
        groups: groups.iter().map(|g| g.to_string()).collect(),
        pinned,
        face: face(life),
        facts: facts(sid),
    }
}

fn running() -> WireLife {
    WireLife::Live {
        activity: Some(crate::agents::SessionActivity::Working),
    }
}

fn rec(id: &str, name: &str, made: i64, named: i64) -> HereGroup {
    HereGroup {
        id: id.into(),
        name: name.into(),
        made,
        named,
    }
}

/// 一片：这几组 ＋ 这几条组员。
fn slice(groups: Vec<HereGroup>, members: Vec<HereMember>) -> GroupsHere {
    GroupsHere { groups, members }
}

fn wire(h: &GroupsHere) -> String {
    serde_json::to_string(h).unwrap()
}

/// 本机那一片是 `local`，别的台照 `peers` 答；问 `machines`（机器表里有 `table` 那几台）。
async fn read_at(
    machines: &[&str],
    local: GroupsHere,
    table: &[&str],
    peers: &Peers,
    kept: &Kept,
    now_ms: i64,
) -> GroupsRead {
    let ms: Vec<Machine> = machines.iter().map(|x| m(x)).collect();
    read_with(
        &ms,
        std::future::ready(Ok(local)),
        &Default::default(),
        until(9_000),
        &Peers::table(table),
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
        .find(|x| x.origin.to_string() == name)
        .unwrap_or_else(|| panic!("{name} 那一格"))
        .read
}

/// B 建的 g1「甲」：C 那一片收着 c，E 那一片收着 e（B 自己没有组员 ⇒ 不存）。
fn on_c() -> GroupsHere {
    slice(
        vec![rec("g1", "甲", 5, 5)],
        vec![member("c", &["g1"], false, running())],
    )
}

fn on_e() -> GroupsHere {
    slice(
        vec![rec("g1", "甲", 5, 5)],
        vec![member("e", &["g1"], false, WireLife::Ended { by: None })],
    )
}

#[tokio::test]
async fn g1_a_sees_the_group_through_the_one_slice_it_can_reach() {
    let peers = Peers::new(&[("C", wire(&on_c())), ("E", wire(&on_e()))]);
    let got = read_at(
        &[cmd_wire::LOCAL, "C", "D"],
        GroupsHere::default(),
        &["C", "D"],
        &peers,
        &Kept::default(),
        1_000,
    )
    .await;
    assert_eq!(got.groups.len(), 1, "{got:?}");
    let g = &got.groups[0];
    assert_eq!(g.id, "g1");
    assert_eq!(
        g.name.as_deref(),
        Some("甲"),
        "名字是 B 起的那个（C 那一片抄着）"
    );
    assert_eq!(g.known, GroupKnown::Now);
    let sids: Vec<&str> = g.members.iter().map(|x| x.sid.as_str()).collect();
    assert_eq!(sids, ["c"], "E 不在 A 的表里 ⇒ e 不出，也不经别台转问");
    assert_eq!(g.members[0].origin.to_string(), "C");
    assert_eq!(g.counts.len(), 1, "只数有的那一样");
    assert_eq!(g.counts[0].dot, cmd_wire::frame::SessionDot::Running);
    assert_eq!(
        g.counts[0].text.0,
        copy_core::copy_text("beGroups.count.running", &[("n", "1")]),
        "组头那一句写好了（出口照摆）"
    );
    assert!(matches!(
        machine(&got, "D"),
        super::super::history_browse::Read::Failed { .. }
    ));
    let asked: Vec<String> = peers.asked().into_iter().map(|(c, _)| c).collect();
    assert_eq!(asked.len(), 2, "只问 C · D 各一问：{asked:?}");
}

#[tokio::test]
async fn g2_the_newest_name_across_slices_wins_and_nobody_is_home() {
    let c = slice(
        vec![rec("g1", "旧名", 3, 10)],
        vec![member("c", &["g1"], false, WireLife::Ended { by: None })],
    );
    let local = slice(
        vec![rec("g1", "新名", 5, 20)],
        vec![member("a", &["g1"], false, WireLife::Ended { by: None })],
    );
    let got = read_at(
        &["C", cmd_wire::LOCAL],
        local,
        &["C"],
        &Peers::new(&[("C", wire(&c))]),
        &Kept::default(),
        1_000,
    )
    .await;
    let g = &got.groups[0];
    assert_eq!(
        g.name.as_deref(),
        Some("新名"),
        "named 新的那份，不照问的先后"
    );
    assert_eq!(g.named, Some(20));
    assert_eq!(g.made, Some(3), "建组时刻取最早的");
    assert_eq!(g.members.len(), 2);
    let v = serde_json::to_value(g).unwrap();
    assert!(v.get("home").is_none(), "没有哪一台是家：{v}");
}

#[tokio::test]
async fn g3_a_slice_that_answered_before_is_shown_as_last_known() {
    let kept = Kept::default();
    let b = slice(
        vec![rec("g1", "开发", 7, 7)],
        vec![member("s-b", &["g1"], true, running())],
    );
    let _ = read_at(
        &[cmd_wire::LOCAL, "devbox"],
        GroupsHere::default(),
        &["devbox"],
        &Peers::new(&[("devbox", wire(&b))]),
        &kept,
        1_000,
    )
    .await;
    let got = read_at(
        &[cmd_wire::LOCAL, "devbox"],
        GroupsHere::default(),
        &["devbox"],
        &Peers::new(&[]),
        &kept,
        61_000,
    )
    .await;
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
    let sb = g
        .members
        .iter()
        .find(|x| x.sid == "s-b")
        .expect("那台的组员不漏");
    assert_eq!(sb.face.dot, cmd_wire::frame::SessionDot::Unknown);
    assert!(g.counts.is_empty(), "看不见的不数");
    assert!(got.pinned.iter().any(|x| x.sid == "s-b"), "固定的跟着出");
}

#[tokio::test]
async fn g4_a_group_only_members_point_at_is_listed_nameless_and_a_group_nobody_holds_is_gone() {
    let local = slice(vec![], vec![member("s-a", &["g1"], false, running())]);
    let empty = wire(&GroupsHere::default());
    let got = read_at(
        &[cmd_wire::LOCAL, "devbox"],
        local,
        &["devbox"],
        &Peers::new(&[("devbox", empty.clone())]),
        &Kept::default(),
        1_000,
    )
    .await;
    let g = &got.groups[0];
    assert_eq!(g.id, "g1");
    assert_eq!(g.name, None);
    assert_eq!(g.named, None);
    assert_eq!(
        g.title.0,
        copy_core::copy_text("beGroups.name.unknown", &[]),
        "名字不知道 ⇒ 照实说不知道，不出空串"
    );
    assert_eq!(g.known, GroupKnown::Unknown);
    assert_eq!(g.members[0].sid, "s-a");
    let none = read_at(
        &[cmd_wire::LOCAL, "devbox"],
        GroupsHere::default(),
        &["devbox"],
        &Peers::new(&[("devbox", empty)]),
        &Kept::default(),
        1_000,
    )
    .await;
    assert!(none.groups.is_empty(), "{none:?}");
}

#[tokio::test]
async fn g5_asking_another_machine_without_a_deadline_is_bad_args() {
    let got = read_with(
        &[m(cmd_wire::LOCAL), m("devbox")],
        std::future::ready(Ok(GroupsHere::default())),
        &Default::default(),
        None,
        &Peers::table(&["devbox"]),
        &Peers::new(&[]),
        &Kept::default(),
        0,
    )
    .await;
    assert!(matches!(got, Err(GroupsReadErr::BadArgs(_))));
    let sent = send_with(
        GroupsWriteReq::Dissolve {
            id: "g1".into(),
            machines: vec![m("devbox")],
        },
        |_| panic!("没带期限就不该写"),
        &Default::default(),
        None,
        &Peers::table(&["devbox"]),
        &Peers::new(&[]),
    )
    .await;
    assert!(matches!(sent, Err(GroupsWriteErr::BadArgs(_))), "{sent:?}");
}

#[test]
fn g6_this_machines_slice_is_its_groups_and_its_grouped_or_pinned_sessions() {
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
            name: crate::history::history_annotations::Named {
                name: "开发".into(),
                named: 4,
            },
            made: 3,
        },
    );
    let got = here_of(&t, &book, |sid| {
        let life = if sid == "s1" {
            running()
        } else {
            WireLife::Ended { by: None }
        };
        (face(life), facts(sid))
    });
    assert_eq!(got.groups, vec![rec("g1", "开发", 3, 4)]);
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
    now: i64,
) -> Result<GroupsWritten, GroupsWriteErr> {
    write_at(own, op, now)
}

fn load(d: &Path) -> (crate::history::history_annotations::Table, Book) {
    crate::history::history_annotations::load_all_at(&d.join("history-metadata.json"))
        .expect("读得动")
}

fn mv(sid: &str, join: Option<HereGroup>, leave: Option<&str>) -> GroupsWriteReq {
    GroupsWriteReq::Move {
        sid: sid.into(),
        join,
        leave: leave.map(str::to_string),
    }
}

#[test]
fn g7_make_only_mints_and_move_keeps_the_slice_to_the_groups_with_members_here() {
    let d = temp_dir("move");
    let own = own_at(&d);
    let made = match write(
        &own,
        GroupsWriteReq::Make {
            name: "  开发 ".into(),
        },
        5,
    )
    .unwrap()
    {
        GroupsWritten::Made(g) => g,
        other => panic!("{other:?}"),
    };
    assert_eq!(made.name, "开发", "名字去掉首尾空白");
    assert_eq!((made.made, made.named), (5, 5));
    assert!(
        !d.join("history-metadata.json").exists(),
        "一个组员都没有的组不住任何一台"
    );
    write(&own, mv("s1", Some(made.clone()), None), 6).unwrap();
    write(&own, mv("s2", Some(made.clone()), None), 6).unwrap();
    let (t, book) = load(&d);
    assert_eq!(book[&made.id].name.name, "开发", "进组把记录抄进这一片");
    assert_eq!(t["s1"].groups, vec![made.id.clone()]);
    assert_eq!(t["s1"].updated_at, 6);
    // 别处改过名的那份更新 ⇒ 这一片照它；更旧的不盖。
    let newer = HereGroup {
        name: "改过".into(),
        named: 9,
        ..made.clone()
    };
    write(&own, mv("s3", Some(newer), None), 7).unwrap();
    assert_eq!(load(&d).1[&made.id].name.name, "改过");
    write(&own, mv("s4", Some(made.clone()), None), 8).unwrap();
    assert_eq!(load(&d).1[&made.id].name.name, "改过", "更旧的名字不盖新的");
    for sid in ["s1", "s2", "s3"] {
        write(&own, mv(sid, None, Some(&made.id)), 9).unwrap();
        assert!(load(&d).1.contains_key(&made.id), "还有人在 ⇒ 记录留着");
    }
    match write(&own, mv("s4", None, Some(&made.id)), 10).unwrap() {
        GroupsWritten::Moved { sid, groups } => {
            assert_eq!(sid, "s4");
            assert!(groups.is_empty());
        }
        other => panic!("{other:?}"),
    }
    let (t, book) = load(&d);
    assert!(book.is_empty(), "最后一位出组 ⇒ 这一片删掉：{book:?}");
    assert!(t["s4"].groups.is_empty());
    let before = std::fs::read(d.join("history-metadata.json")).unwrap();
    for op in [
        GroupsWriteReq::Make { name: "   ".into() },
        GroupsWriteReq::Make {
            name: "长".repeat(NAME_MAX + 1),
        },
        mv("s1", Some(rec("g 1", "x", 1, 1)), None),
        mv("s1", Some(rec("g1", " ", 1, 1)), None),
        mv("s1", None, Some("g/1")),
    ] {
        assert!(
            matches!(write(&own, op, 11), Err(GroupsWriteErr::BadArgs(_))),
            "名字 · id 不对 ⇒ 拒"
        );
    }
    assert_eq!(
        std::fs::read(d.join("history-metadata.json")).unwrap(),
        before,
        "拒了就不动"
    );
}

#[test]
fn g8_unknown_keys_in_the_groups_section_survive_and_an_unreadable_file_is_never_written() {
    let d = temp_dir("keys");
    let p = d.join("history-metadata.json");
    std::fs::write(
        &p,
        r#"{"version":1,"entries":{"s":{"starred":true,"customTitle":null,"groups":["g1"],"updatedAt":1,"x":1}},"groups":{"g1":{"name":"甲","made":1,"named":1,"color":"red"}},"top":2}"#,
    )
    .unwrap();
    let own = own_at(&d);
    write(
        &own,
        GroupsWriteReq::Rename {
            id: "g1".into(),
            name: "乙".into(),
            machines: vec![m(cmd_wire::LOCAL)],
        },
        30,
    )
    .unwrap();
    let v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(v["groups"]["g1"]["name"], "乙");
    assert_eq!(v["groups"]["g1"]["named"], 30, "名字记下几时起的");
    assert_eq!(v["groups"]["g1"]["color"], "red", "组里认不出的键留着");
    assert_eq!(v["top"], 2);
    assert_eq!(v["entries"]["s"]["x"], 1, "注解原样");
    assert_eq!(v["entries"]["s"]["updatedAt"], 1, "没变的那条不碰");

    std::fs::write(&p, b"{not json").unwrap();
    let before = std::fs::read(&p).unwrap();
    assert!(write(&own, mv("s", Some(rec("g2", "丙", 1, 1)), None), 31).is_err());
    assert_eq!(std::fs::read(&p).unwrap(), before);
}

#[tokio::test]
async fn g9_member_rows_carry_what_resurrecting_a_tab_needs() {
    let b = slice(
        vec![rec("g1", "开发", 7, 7)],
        vec![member("s-b", &["g1"], true, WireLife::Ended { by: None })],
    );
    let got = read_at(
        &[cmd_wire::LOCAL, "devbox"],
        GroupsHere::default(),
        &["devbox"],
        &Peers::new(&[("devbox", wire(&b))]),
        &Kept::default(),
        1_000,
    )
    .await;
    let g = &got.groups[0];
    let sb = g.members.iter().find(|x| x.sid == "s-b").expect("组员");
    assert_eq!(sb.facts, facts("s-b"), "照那台给的原样");
    let pin = got
        .pinned
        .iter()
        .find(|x| x.sid == "s-b")
        .expect("固定那一行");
    assert_eq!(pin.facts, facts("s-b"));
    assert_eq!(pin.origin.to_string(), "devbox");
}

fn raw_row(sid: &str, life: Option<WireLife>) -> cmd_wire::listing::RawRow {
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
    let (_, x) = unlisted(None, WireLife::Ended { by: None });
    assert_eq!(
        x.title.0,
        copy_core::copy_text("history.row.untitled", &[]),
        "没名字照清单那一句"
    );
}

#[tokio::test]
async fn g11_rename_goes_to_every_slice_and_says_which_were_missed() {
    let d = temp_dir("send");
    let own = own_at(&d);
    write(&own, mv("s1", Some(rec("g1", "甲", 1, 1)), None), 2).unwrap();
    let sent_ok = serde_json::to_string(&GroupsWritten::Sent {
        id: "g1".into(),
        missed: vec![],
    })
    .unwrap();
    let peers = Peers::new(&[("devbox", sent_ok)]);
    let got = send_with(
        GroupsWriteReq::Rename {
            id: "g1".into(),
            name: " 乙 ".into(),
            machines: vec![m(cmd_wire::LOCAL), m("devbox"), m("laptop")],
        },
        |r| write_at(&own, r, 50),
        &Default::default(),
        until(9_000),
        &Peers::table(&["devbox", "laptop"]),
        &peers,
    )
    .await
    .unwrap();
    match got {
        GroupsWritten::Sent { id, missed } => {
            assert_eq!(id, "g1");
            let who: Vec<String> = missed.iter().map(|x| x.origin.to_string()).collect();
            assert_eq!(who, ["laptop"], "没答的那台照实列出：{missed:?}");
            assert!(!missed[0].why.is_empty(), "带为什么");
        }
        other => panic!("{other:?}"),
    }
    let g1 = &load(&d).1["g1"];
    assert_eq!(
        (g1.name.name.as_str(), g1.name.named),
        ("乙", 50),
        "本机那一片当场写"
    );
    let fwd: Vec<Value> = peers.asked().into_iter().map(|(_, line)| line).collect();
    assert_eq!(fwd.len(), 2, "devbox · laptop 各一问");
    for line in &fwd {
        let s = line.to_string();
        assert!(s.contains("\"rename\"") && s.contains("乙"), "{s}");
        assert!(
            s.contains(&format!("\"machines\":[\"{}\"]", cmd_wire::LOCAL)),
            "转过去的那一问只写那台自己：{s}"
        );
    }
    // 没有那一组的片：不动。
    write(
        &own,
        GroupsWriteReq::Rename {
            id: "g-other".into(),
            name: "丙".into(),
            machines: vec![m(cmd_wire::LOCAL)],
        },
        51,
    )
    .unwrap();
    assert_eq!(
        load(&d).1.keys().collect::<Vec<_>>(),
        ["g1"],
        "没有那一组 ⇒ 不凭空建"
    );
}

#[tokio::test]
async fn g12_dissolve_takes_the_group_off_every_session_in_the_slice() {
    let d = temp_dir("dissolve");
    let own = own_at(&d);
    let g = rec("g1", "甲", 1, 1);
    write(&own, mv("s1", Some(g.clone()), None), 2).unwrap();
    write(&own, mv("s2", Some(g.clone()), None), 2).unwrap();
    write(&own, mv("s2", Some(rec("g2", "乙", 1, 1)), None), 2).unwrap();
    let got = send_with(
        GroupsWriteReq::Dissolve {
            id: "g1".into(),
            machines: vec![m(cmd_wire::LOCAL)],
        },
        |r| write_at(&own, r, 9),
        &Default::default(),
        None,
        &Peers::table(&[]),
        &Peers::new(&[]),
    )
    .await
    .unwrap();
    assert!(
        matches!(&got, GroupsWritten::Sent { missed, .. } if missed.is_empty()),
        "{got:?}"
    );
    let (t, book) = load(&d);
    assert_eq!(book.keys().collect::<Vec<_>>(), ["g2"]);
    assert!(t["s1"].groups.is_empty());
    assert_eq!(t["s2"].groups, vec!["g2".to_string()], "别的组不动");
}
