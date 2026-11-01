//! 会话输入队：序号由主人发、不看发件端的钟 · 同 id 只入队一次 · 同一端缺号先收着、补上或超时再排 · 重启序号接着数 ·
//! 撤 · 会话结束标未送达 · 读不出来不覆盖。
//!
//! ⚠ 家目录一律是临时目录（`Own::issue_at`），真 `~/.cc-monitor/` 一个字节都不碰。

use super::*;
use cmd_wire::queue::{QueuePutOutcome as O, QueueState as S};
use cmd_wire::{Machine, Sid};

fn home(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-squeue-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn own(h: &std::path::Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_at(h)
}

const SID: &str = "s1";

fn sid() -> Sid {
    SID.parse().unwrap()
}

fn req(id: &str, end: &str, n: Option<u64>, at_ms: i64) -> SessionQueuePutReq {
    SessionQueuePutReq {
        machine: Machine::local(),
        sid: sid(),
        id: id.to_string(),
        end: end.to_string(),
        n,
        at_ms,
        text: format!("text of {id}"),
        attachments: Vec::new(),
        rewind: None,
    }
}

fn ids(q: &QueueList) -> Vec<(&str, Option<u64>, S)> {
    q.items
        .iter()
        .map(|i| (i.id.as_str(), i.seq, i.state))
        .collect()
}

/// 排在最前、还没送的那一条（[`due`] 的那一格）。
fn next_due(
    o: &Own,
    sid: &str,
    now_ms: i64,
) -> Result<Option<QueueItem>, crate::common::said::Said> {
    due(o, sid, now_ms).map(|d| d.item)
}

const T0: i64 = 1_800_000_000_000;
const TEN_MIN: i64 = 10 * 60 * 1000;

#[test]
fn skewed_clocks_do_not_matter_owner_numbers_in_arrival_order() {
    let o = own(&home("skew"));
    // A 台的钟快 10 分钟、先到；B 台的钟准、后到。
    let a = put(&o, &req("a", "deskA", None, T0 + TEN_MIN), T0).unwrap();
    let b = put(&o, &req("b", "deskB", None, T0), T0 + 1).unwrap();
    assert_eq!((a.outcome, a.seq, a.place), (O::Queued, Some(1), Some(1)));
    assert_eq!((b.outcome, b.seq, b.place), (O::Queued, Some(2), Some(2)));
    let due = next_due(&o, &sid(), T0 + 2).unwrap().expect("有一条可送");
    assert_eq!(due.id, "a", "先送先到的那一条，不看它自报的几点");
}

#[test]
fn same_id_twice_enters_once() {
    let o = own(&home("dup"));
    put(&o, &req("a", "phone", None, T0), T0).unwrap();
    let again = put(&o, &req("a", "phone", None, T0), T0 + 5).unwrap();
    assert_eq!(
        (again.outcome, again.seq, again.place),
        (O::Seen, Some(1), Some(1))
    );
    let q = read(&o, &sid(), T0 + 6).unwrap();
    assert_eq!(ids(&q), vec![("a", Some(1), S::Queued)]);
    // 送掉之后再重发：仍认得，不复活。
    assert!(delivered(&o, &sid(), "a").unwrap());
    let late = put(&o, &req("a", "phone", None, T0), T0 + 7).unwrap();
    assert_eq!((late.outcome, late.seq, late.place), (O::Seen, None, None));
    assert!(read(&o, &sid(), T0 + 8).unwrap().items.is_empty());
}

#[test]
fn a_gap_from_one_end_is_held_until_filled() {
    let o = own(&home("gap"));
    put(&o, &req("p1", "phone", Some(1), T0), T0).unwrap();
    // 第 3 号先到、第 2 号在路上：先收着，不发序号。
    let p3 = put(&o, &req("p3", "phone", Some(3), T0), T0 + 10).unwrap();
    assert_eq!((p3.outcome, p3.seq, p3.place), (O::Held, None, None));
    // 别的端照常排。
    put(&o, &req("d1", "desk", Some(1), T0), T0 + 20).unwrap();
    let p2 = put(&o, &req("p2", "phone", Some(2), T0), T0 + 30).unwrap();
    assert_eq!((p2.outcome, p2.seq), (O::Queued, Some(3)));
    let q = read(&o, &sid(), T0 + 40).unwrap();
    assert_eq!(
        ids(&q),
        vec![
            ("p1", Some(1), S::Queued),
            ("d1", Some(2), S::Queued),
            ("p2", Some(3), S::Queued),
            ("p3", Some(4), S::Queued),
        ],
        "补上之后两条按端序号排"
    );
}

#[test]
fn a_gap_that_never_fills_times_out_into_arrival_order() {
    let o = own(&home("timeout"));
    put(&o, &req("p1", "phone", Some(1), T0), T0).unwrap();
    put(&o, &req("p4", "phone", Some(4), T0), T0 + 1).unwrap();
    put(&o, &req("p3", "phone", Some(3), T0), T0 + 2).unwrap();
    let before = read(&o, &sid(), T0 + HOLD_MS).unwrap();
    assert_eq!(
        ids(&before),
        vec![
            ("p1", Some(1), S::Queued),
            ("p4", None, S::Held),
            ("p3", None, S::Held)
        ],
        "收着的照到达先后排在后面"
    );
    let after = read(&o, &sid(), T0 + 2 + HOLD_MS).unwrap();
    assert_eq!(
        ids(&after),
        vec![
            ("p1", Some(1), S::Queued),
            ("p4", Some(2), S::Queued),
            ("p3", Some(3), S::Queued)
        ],
        "等不到缺的那一号 ⇒ 照到达先后排"
    );
    // 缺的那一号晚到了：照到达先后排在最后。
    let p2 = put(&o, &req("p2", "phone", Some(2), T0), T0 + 3 * HOLD_MS).unwrap();
    assert_eq!((p2.outcome, p2.seq), (O::Queued, Some(4)));
}

#[test]
fn sequence_survives_a_restart() {
    let h = home("restart");
    put(&own(&h), &req("a", "desk", None, T0), T0).unwrap();
    put(&own(&h), &req("b", "desk", None, T0), T0).unwrap();
    assert!(delivered(&own(&h), &sid(), "a").unwrap());
    assert!(delivered(&own(&h), &sid(), "b").unwrap());
    // 换一个进程（重新从盘上读）：接着数，不从 1 重来。
    let c = put(&own(&h), &req("c", "desk", None, T0), T0).unwrap();
    assert_eq!(c.seq, Some(3));
}

#[test]
fn cancel_only_takes_what_is_not_delivered() {
    let o = own(&home("cancel"));
    put(&o, &req("a", "desk", None, T0), T0).unwrap();
    put(&o, &req("b", "desk", None, T0), T0).unwrap();
    put(&o, &req("h", "phone", Some(2), T0), T0).unwrap();
    let c = cancel(&o, &sid(), "a", T0 + 1).unwrap();
    assert!(c.cancelled);
    assert_eq!(
        ids(&c.queue),
        vec![("b", Some(2), S::Queued), ("h", None, S::Held)]
    );
    let held = cancel(&o, &sid(), "h", T0 + 2).unwrap();
    assert!(held.cancelled, "收着的也撤得了");
    assert!(delivered(&o, &sid(), "b").unwrap());
    let gone = cancel(&o, &sid(), "b", T0 + 3).unwrap();
    assert!(!gone.cancelled, "送过的撤不了");
    assert!(!cancel(&o, &sid(), "nope", T0 + 3).unwrap().cancelled);
    // 撤掉的 id 重发不复活。
    let again = put(&o, &req("a", "desk", None, T0), T0 + 4).unwrap();
    assert_eq!(again.outcome, O::Seen);
    assert!(read(&o, &sid(), T0 + 5).unwrap().items.is_empty());
}

#[test]
fn session_end_marks_whatever_is_left_undelivered() {
    let o = own(&home("end"));
    put(&o, &req("a", "desk", None, T0), T0).unwrap();
    put(&o, &req("h", "phone", Some(5), T0), T0 + 1).unwrap();
    let q = end(&o, &sid(), T0 + 2).unwrap();
    assert_eq!(
        ids(&q),
        vec![
            ("a", Some(1), S::Undelivered),
            ("h", Some(2), S::Undelivered)
        ],
        "没送的都标未送达、留给发件人看"
    );
    assert!(
        next_due(&o, &sid(), T0 + 3).unwrap().is_none(),
        "未送达的不再投递"
    );
    assert!(
        cancel(&o, &sid(), "a", T0 + 4).unwrap().cancelled,
        "撤掉才消失"
    );
}

#[test]
fn next_due_is_the_lowest_sequence_still_queued() {
    let o = own(&home("due"));
    assert!(next_due(&o, &sid(), T0).unwrap().is_none());
    put(&o, &req("a", "desk", None, T0), T0).unwrap();
    put(&o, &req("b", "desk", None, T0), T0).unwrap();
    assert_eq!(next_due(&o, &sid(), T0).unwrap().unwrap().id, "a");
    assert!(delivered(&o, &sid(), "a").unwrap());
    assert_eq!(next_due(&o, &sid(), T0).unwrap().unwrap().id, "b");
    assert!(!delivered(&o, &sid(), "a").unwrap(), "送过的再报一次不算");
}

#[test]
fn queues_are_per_session_and_land_in_the_declared_place() {
    let h = home("place");
    let o = own(&h);
    put(&o, &req("a", "desk", None, T0), T0).unwrap();
    let mut other = req("x", "desk", None, T0);
    other.sid = "s2".parse().unwrap();
    assert_eq!(
        put(&o, &other, T0).unwrap().seq,
        Some(1),
        "另一条会话自己数"
    );
    let file = h.join(DIR_REL).join(format!("{SID}.json"));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(
        v["version"],
        serde_json::json!(DOC.version()),
        "顶层整数版本"
    );
    assert_eq!(v["nextSeq"], serde_json::json!(2));
}

#[test]
fn an_unreadable_queue_is_reported_and_left_untouched() {
    let h = home("bad");
    let dir = h.join(DIR_REL);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{SID}.json"));
    std::fs::write(&file, b"{not json").unwrap();
    let o = own(&h);
    let e = put(&o, &req("a", "desk", None, T0), T0).unwrap_err();
    assert_eq!(e.code.as_deref(), Some("queue_unreadable"));
    assert_eq!(std::fs::read(&file).unwrap(), b"{not json", "不覆盖");
    // 更新的版本也不覆盖。
    std::fs::write(&file, format!("{{\"version\":{}}}", DOC.version() + 1)).unwrap();
    assert_eq!(
        read(&o, &sid(), T0).unwrap_err().code.as_deref(),
        Some("queue_unreadable")
    );
}

#[test]
fn a_full_queue_refuses_the_next_one() {
    let o = own(&home("full"));
    for i in 0..ITEMS_MAX {
        put(&o, &req(&format!("m{i}"), "desk", None, T0), T0).unwrap();
    }
    let e = put(&o, &req("over", "desk", None, T0), T0).unwrap_err();
    assert_eq!(e.code.as_deref(), Some("queue_full"));
}

#[test]
fn the_delivery_side_lists_queues_and_names_touched_ones() {
    let h = home("list");
    let o = own(&h);
    assert!(sids(&o).is_empty(), "队目录还没有 ⇒ 空");
    put(&o, &req("a", "desk", None, T0), T0).unwrap();
    let dir = h.join(DIR_REL);
    std::fs::write(dir.join("not a sid.json"), b"{}").unwrap();
    assert_eq!(sids(&o), vec![SID.to_string()], "只认 `<sid>.json`");
    let paths = [
        dir.join(format!("{SID}.json")),
        dir.join(format!("{SID}.json")),
        dir.join(format!(".{SID}.json.tmp")),
        dir.join("not a sid.json"),
        h.join("s2.json"),
    ];
    assert_eq!(
        sids_touched(paths.iter().map(|p| p.as_path()), &dir),
        vec![SID.to_string()],
        "批内去重；旁名 · 别的目录不算"
    );
}

#[test]
fn a_refused_line_is_kept_as_undelivered_and_a_held_one_says_when_it_is_due() {
    let o = own(&home("giveup"));
    put(&o, &req("a", "desk", None, T0), T0).unwrap();
    assert!(give_up(&o, SID, "a", None).unwrap());
    assert!(!give_up(&o, SID, "a", None).unwrap(), "不在排着 ⇒ 不动");
    assert_eq!(
        ids(&read(&o, &sid(), T0).unwrap()),
        vec![("a", Some(1), S::Undelivered)]
    );
    put(&o, &req("h", "deskB", Some(2), T0), T0 + 5).unwrap();
    let d = due(&o, SID, T0 + 6).unwrap();
    assert_eq!((d.item, d.held_until), (None, Some(T0 + 5 + HOLD_MS)));
}

/// 附件编号随那一条落盘、名单与投递口原样交出；送掉的 id 进名单的 `sent`（发件端据它标「已送进」），撤掉的不进；
/// `sent` 只留最近 [`SENT_KEEP`] 个。
#[test]
fn attachments_ride_with_the_line_and_sent_ids_are_listed() {
    let o = own(&home("sent"));
    let mut a = req("a", "desk", None, T0);
    a.attachments = vec!["1027-1200-a3f2".into()];
    put(&o, &a, T0).unwrap();
    put(&o, &req("b", "desk", None, T0), T0).unwrap();
    let l = read(&o, SID, T0).unwrap();
    assert_eq!(l.items[0].attachments, ["1027-1200-a3f2"]);
    assert!(l.sent.is_empty());
    assert_eq!(
        next_due(&o, SID, T0).unwrap().map(|i| i.attachments),
        Some(vec!["1027-1200-a3f2".to_string()])
    );
    assert!(delivered(&o, SID, "a").unwrap());
    cancel(&o, SID, "b", T0).unwrap();
    let l = read(&o, SID, T0).unwrap();
    assert_eq!((l.items.len(), l.sent.clone()), (0, vec!["a".to_string()]));
    // 重投收过的编号：回执那一句照那条此刻的样子写（送进去了 · 撤了）。
    let again = |id: &str| put(&o, &req(id, "desk", None, T0), T0).unwrap();
    assert_eq!(
        (again("a").outcome, again("a").said),
        (O::Seen, copy_text("beQueue.said.sent", &[]))
    );
    assert_eq!(again("b").said, copy_text("beQueue.said.cancelled", &[]));
    for i in 0..SENT_KEEP + 3 {
        let id = format!("m{i}");
        put(&o, &req(&id, "desk", None, T0), T0).unwrap();
        assert!(delivered(&o, SID, &id).unwrap());
    }
    let sent = read(&o, SID, T0).unwrap().sent;
    assert_eq!(sent.len(), SENT_KEEP);
    assert_eq!(
        sent.last().map(String::as_str),
        Some(format!("m{}", SENT_KEEP + 2).as_str())
    );
}

/// ★ 名单里每一条带会话那台写好的字：那一条此刻的样子（同回执那一句：排队 · 第 N · 排队（收着）· 失败 · 未送进会话）
/// 与发自哪一端（认得的两端写名字，别的照原样）；撤掉第 1 条 ⇒ 原第 2 条那一句跟着变「第 1」。各端照摆，不按 `state` · `end` 自己挑字。
#[test]
fn each_item_carries_the_owner_written_words() {
    let o = own(&home("words"));
    put(&o, &req("a", "mobile", None, T0), T0).unwrap();
    put(&o, &req("b", "monitor", None, T0), T0 + 1).unwrap();
    put(&o, &req("c", "cc-bus", Some(2), T0), T0 + 2).unwrap();
    let words = |q: &QueueList| -> Vec<(String, String)> {
        q.items
            .iter()
            .map(|i| (i.said.clone(), i.from.clone()))
            .collect()
    };
    let place = |n: u32| copy_text("beQueue.said.place", &[("place", &n.to_string())]);
    assert_eq!(
        words(&read(&o, &sid(), T0 + 3).unwrap()),
        vec![
            (place(1), copy_text("beQueue.from.phone", &[])),
            (place(2), copy_text("beQueue.from.desktop", &[])),
            (copy_text("beQueue.said.held", &[]), "cc-bus".to_string()),
        ]
    );
    let after = cancel(&o, &sid(), "a", T0 + 4).unwrap().queue;
    assert_eq!(
        words(&after)[0].0,
        place(1),
        "撤掉第 1 条 ⇒ 原第 2 条写第 1"
    );
}

/// 瘦客户端底行那一句（会话那台写）：排着 · 收着合计 N 条；未送达的不算；一条都没在等 ⇒ 没有。
#[test]
fn the_waiting_line_counts_queued_and_held_but_not_undelivered() {
    let o = own(&home("waiting"));
    let waiting = |n: usize| Some(copy_text("beQueue.said.waiting", &[("n", &n.to_string())]));
    assert_eq!(waiting_said(&o, SID, T0), None, "空队 ⇒ 没有");
    put(&o, &req("a", "mobile", None, T0), T0).unwrap();
    put(&o, &req("h", "deskA", Some(2), T0), T0 + 1).unwrap();
    assert_eq!(
        waiting_said(&o, SID, T0 + 2),
        waiting(2),
        "排着 1 ＋ 收着 1"
    );
    end(&o, SID, T0 + 3).unwrap();
    assert_eq!(
        waiting_said(&o, SID, T0 + 4),
        None,
        "会话结束标未送达 ⇒ 不算在等"
    );
}

/// 托管终端的沙箱：数据目录（`hosts/`）与家目录（队）分开，都在判据的沙箱里。
fn host_own(tag: &str) -> Own {
    let root = test_home::sandbox_root().join(format!("ql-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).expect("建沙箱数据目录");
    std::fs::create_dir_all(root.join("home")).expect("建沙箱家目录");
    let d = root.join("data").to_string_lossy().into_owned();
    let h = root.join("home").to_string_lossy().into_owned();
    Own::issue_with(&move |k| match k {
        "CCM_DATA_DIR" => Some(d.clone()),
        "HOME" => Some(h.clone()),
        _ => None,
    })
}

/// ★ 瘦客户端底行那一句跟着会话那台的名单走：打标前投的两条，打标那一下现读 ⇒「排队 2 条」；撤一条 ⇒「排队 1 条」；撤完 ⇒ 没有。
#[test]
fn the_host_line_follows_the_owners_list_from_tag_to_empty() {
    use crate::host::managed::home::{Facts, Home};
    let o = host_own("follow");
    let id = "ql-follow";
    let home = Home::claim(&o, id).expect("占家");
    let mut facts = Facts {
        id: id.into(),
        name: id.into(),
        ..Facts::default()
    };
    home.write_facts(&facts).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let _ears = host_line_ears(
        &o,
        id,
        Box::new(move |l| {
            let _ = tx.lock().unwrap().send(l);
        }),
    )
    .expect("挂上");
    let next = || {
        rx.recv_timeout(guard_core::wall::CEILING)
            .expect("等到下一句")
    };
    let waiting = |n: usize| Some(copy_text("beQueue.said.waiting", &[("n", &n.to_string())]));
    let now = || crate::common::time::now_ms() as i64;
    assert_eq!(next(), None, "还没打标 ⇒ 没有");
    put(&o, &req("a", "mobile", None, T0), now()).unwrap();
    put(&o, &req("b", "mobile", None, T0), now()).unwrap();
    facts.sid = SID.into();
    home.write_facts(&facts).unwrap();
    assert_eq!(next(), waiting(2), "打标那一下现读");
    cancel(&o, SID, "a", now()).unwrap();
    assert_eq!(next(), waiting(1));
    cancel(&o, SID, "b", now()).unwrap();
    assert_eq!(next(), None, "撤完 ⇒ 没有");
    home.clear();
}
