use super::*;
use crate::control::cc_bus_wire::BusEnvelope;
use std::path::{Path, PathBuf};

const A: &str = "0f3c2a1e-1111-2222-3333-444455556666";
const B: &str = "9a8b7c6d-aaaa-bbbb-cccc-ddddeeeeffff";

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-bus-store-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn own(d: &Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_data_at(d)
}

fn bus(d: &Path) -> PathBuf {
    d.join(DIR)
}

fn env(to: &str, title: &str, id: &str) -> BusEnvelope {
    BusEnvelope {
        id: id.to_string(),
        ts: "2026-10-27T00:00:00Z".to_string(),
        from: A.to_string(),
        to: to.to_string(),
        tier: 2,
        act: cmd_wire::bus::BusAct::Inform,
        title: title.to_string(),
        re: None,
        refs: vec![],
        text: "占位正文".to_string(),
    }
}

fn token_of(d: &Path, sid: &str, max: Option<usize>) -> String {
    peek(&bus(d), sid, max).unwrap().expect("有新的").token()
}

#[test]
fn a_delivered_letter_is_peeked_then_committed_and_unread_drops_to_zero() {
    let d = tmp("cycle");
    assert_eq!(deliver(&own(&d), &env(B, "t1", "m1"), 1_000), Ok(false));
    assert_eq!(unread(&bus(&d), B).unwrap(), 1);
    let p = peek(&bus(&d), B, None).unwrap().expect("有一封");
    assert_eq!(p.lines.len(), 1);
    let e: BusEnvelope = serde_json::from_str(&p.lines[0]).unwrap();
    assert_eq!(e.title, "t1");
    // peek 不写：再看一次还是同一段。
    assert_eq!(peek(&bus(&d), B, None).unwrap().unwrap().token(), p.token());
    assert_eq!(unread(&bus(&d), B).unwrap(), 1, "peek 不推已读位置");
    assert_eq!(commit(&own(&d), B, &p.token()), Ok(1));
    assert_eq!(unread(&bus(&d), B).unwrap(), 0);
    assert!(peek(&bus(&d), B, None).unwrap().is_none(), "读完了没有新的");
}

#[test]
fn a_duplicate_is_refused_and_not_written() {
    let d = tmp("dup");
    deliver(&own(&d), &env(B, "same", "m1"), 1_000).unwrap();
    let (c, _) = deliver(&own(&d), &env(B, "same", "m2"), 1_500).unwrap_err();
    assert_eq!(c, "duplicate");
    assert_eq!(unread(&bus(&d), B).unwrap(), 1, "收件箱只一行");
}

#[test]
fn the_same_token_commits_once_and_a_changed_inbox_is_caught() {
    let d = tmp("cas");
    deliver(&own(&d), &env(B, "t1", "m1"), 1_000).unwrap();
    let tok = token_of(&d, B, None);
    assert_eq!(commit(&own(&d), B, &tok), Ok(1));
    assert_eq!(
        commit(&own(&d), B, &tok).unwrap_err().0,
        "stale",
        "第二个读者照常处理手上那段"
    );
    // 第 to 行在两跳之间换了 ⇒ changed。
    deliver(&own(&d), &env(B, "t2", "m2"), 2_000).unwrap();
    let tok = token_of(&d, B, None);
    std::fs::write(
        bus(&d).join(format!("{B}.jsonl")),
        format!(
            "{}\n{}\n",
            serde_json::to_string(&env(B, "t1", "m1")).unwrap(),
            serde_json::to_string(&env(B, "换了", "mx")).unwrap()
        ),
    )
    .unwrap();
    assert_eq!(commit(&own(&d), B, &tok).unwrap_err().0, "changed");
    assert_eq!(unread(&bus(&d), B).unwrap(), 1, "没推");
    for bad in ["", "1.2", "a.b.c", "1.2.zz", "0.0.0000000000000000"] {
        assert_eq!(
            commit(&own(&d), B, bad).unwrap_err().0,
            "bad_token",
            "{bad:?}"
        );
    }
}

#[test]
fn max_lines_reads_a_backlog_in_slices() {
    let d = tmp("slices");
    for i in 0..5 {
        deliver(
            &own(&d),
            &env(B, &format!("t{i}"), &format!("m{i}")),
            1_000 + i,
        )
        .unwrap();
    }
    let p = peek(&bus(&d), B, Some(2)).unwrap().unwrap();
    assert_eq!((p.from, p.to, p.lines.len()), (0, 2, 2));
    commit(&own(&d), B, &p.token()).unwrap();
    let p = peek(&bus(&d), B, Some(2)).unwrap().unwrap();
    assert_eq!((p.from, p.to), (2, 4));
    assert_eq!(unread(&bus(&d), B).unwrap(), 3);
}

#[test]
fn a_truncated_inbox_is_read_from_the_top_and_the_position_heals() {
    let d = tmp("trunc");
    for i in 0..3 {
        deliver(
            &own(&d),
            &env(B, &format!("t{i}"), &format!("m{i}")),
            1_000 + i,
        )
        .unwrap();
    }
    commit(&own(&d), B, &token_of(&d, B, None)).unwrap();
    std::fs::write(
        bus(&d).join(format!("{B}.jsonl")),
        format!(
            "{}\n",
            serde_json::to_string(&env(B, "only", "my")).unwrap()
        ),
    )
    .unwrap();
    assert_eq!(unread(&bus(&d), B).unwrap(), 1, "位置比行数大 ⇒ 按 0 算");
    let p = peek(&bus(&d), B, None).unwrap().unwrap();
    assert_eq!((p.from, p.to), (3, 1));
    assert_eq!(commit(&own(&d), B, &p.token()), Ok(1));
}

#[test]
fn the_roster_lists_session_shaped_inboxes_with_their_unread() {
    let d = tmp("roster");
    deliver(&own(&d), &env(B, "t", "m1"), 1_000).unwrap();
    deliver(&own(&d), &env(A, "t", "m2"), 1_000).unwrap();
    commit(&own(&d), A, &token_of(&d, A, None)).unwrap();
    std::fs::write(bus(&d).join("--help.jsonl"), "x\n").unwrap();
    std::fs::write(bus(&d).join("legacy_name.jsonl"), "x\n").unwrap();
    let mut got = inboxes(&bus(&d)).unwrap();
    got.sort();
    let mut want = vec![(A.to_string(), 0), (B.to_string(), 1)];
    want.sort();
    assert_eq!(got, want);
    assert!(
        inboxes(&d.join("nothing-here")).unwrap().is_empty(),
        "没投过信 ⇒ 空名单"
    );
}

#[test]
fn reading_never_writes() {
    let d = tmp("ro");
    deliver(&own(&d), &env(B, "t", "m1"), 1_000).unwrap();
    // 这个目录里写得出的只有那两份（收件箱 · 状态文件）：逐份比字节与修改时刻。
    let snap = || {
        [format!("{B}.jsonl"), "state.json".to_string()]
            .iter()
            .map(|n| {
                let p = bus(&d).join(n);
                (
                    std::fs::read(&p).unwrap(),
                    std::fs::metadata(&p).unwrap().modified().unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    let before = snap();
    let _ = peek(&bus(&d), B, None).unwrap();
    let _ = unread(&bus(&d), B).unwrap();
    let _ = inboxes(&bus(&d)).unwrap();
    let _ = tail(&bus(&d), B, 10).unwrap();
    assert_eq!(snap(), before);
    assert_eq!(tail(&bus(&d), B, 10).unwrap().len(), 1);
}

/// 一档那一封：收件人那条会话的输入队（同一台）此刻的名单。
fn queue_of(home: &Path, sid: &str) -> Vec<cmd_wire::queue::QueueItem> {
    let own = crate::platform::fs::Own::issue_at(home);
    crate::history::session_queue::read(&own, sid, 2_000)
        .unwrap()
        .items
}

fn tier1(to: &str, title: &str, id: &str) -> BusEnvelope {
    BusEnvelope {
        tier: 1,
        ..env(to, title, id)
    }
}

#[test]
fn a_tier_one_letter_knocks_by_queueing_one_line_for_the_recipient() {
    let h = tmp("knock");
    let own = crate::platform::fs::Own::issue_at(&h);
    assert_eq!(deliver(&own, &tier1(B, "合一下", "m1"), 1_000), Ok(true));
    let q = queue_of(&h, B);
    assert_eq!(q.len(), 1, "收件人的输入队多一条");
    assert_eq!(q[0].end, "cc-bus", "发自 cc-bus");
    assert_eq!(
        q[0].id, "cc-bus-m1",
        "id 由那一封的 id 来（重投只入队一次）"
    );
    assert!(q[0].text.contains(A), "那一句说是谁来的信");
    assert!(q[0].text.contains("cc-bus recv"), "叫对方去收信");
    assert!(
        !q[0].text.contains("合一下") && !q[0].text.contains("占位正文"),
        "不带标题与正文：那一句是对方的输入"
    );
    // 去抖窗里第二封照收、不再敲。
    assert_eq!(deliver(&own, &tier1(B, "又一封", "m2"), 1_500), Ok(false));
    assert_eq!(queue_of(&h, B).len(), 1);
    assert_eq!(unread(&h.join(".cc-monitor").join(DIR), B).unwrap(), 2);
}

#[test]
fn a_tier_two_letter_never_touches_the_input_queue() {
    let h = tmp("noknock");
    let own = crate::platform::fs::Own::issue_at(&h);
    assert_eq!(deliver(&own, &env(B, "t", "m1"), 1_000), Ok(false));
    assert!(queue_of(&h, B).is_empty());
}

#[test]
fn a_knock_that_cannot_be_queued_still_keeps_the_letter() {
    // 只有数据目录、没有家目录 ⇒ 输入队那一处写不了：信照收，回「没敲」。
    let d = tmp("knockfail");
    assert_eq!(deliver(&own(&d), &tier1(B, "t", "m1"), 1_000), Ok(false));
    assert_eq!(unread(&bus(&d), B).unwrap(), 1);
}
