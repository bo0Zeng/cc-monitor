//! 认可 · 退回记录那份后端自有文件：写完读回 · 认可只留此刻还在的键 · 读不懂不覆盖 · 照它给成品标认可与退回的状态。

use super::*;
use crate::plan::fixture::{dump, dump_top_signed, scratch, who, MAIN};

fn file(tag: &str) -> PathBuf {
    scratch(tag).join(".cc-monitor").join("plan-review.json")
}

/// 写句柄：家目录 ＝ 那份文件上两级（`<家>/.cc-monitor/plan-review.json`）。
fn own_of(f: &Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_at(f.parent().and_then(Path::parent).expect("两级"))
}

fn rec(at: u64) -> Returned {
    Returned {
        at,
        to: MAIN.into(),
        result: "delivered".into(),
        children: (1..=6).map(|i| format!("A1-{i}")).collect(),
        body: "x".into(),
    }
}

#[test]
fn an_ack_is_written_and_read_back_and_only_live_keys_stay() {
    let f = file("review-ack");
    let live = vec!["red:悬空@B".to_string(), "top:r1".to_string()];
    ack_at(&own_of(&f), "/w", "alpha", "top:r1", &live).unwrap();
    // 先前认可过的一个键此刻已经不在（版本换了）⇒ 下一次写时清掉。
    let r = current(Some(&f));
    assert!(r.acked("/w", "alpha", "top:r1"));
    assert!(!r.acked("/w", "alpha", "red:悬空@B"));
    assert!(!r.acked("/w", "beta", "top:r1"), "分片记");
    assert!(!r.acked("/v", "alpha", "top:r1"), "分工作区记");
    ack_at(
        &own_of(&f),
        "/w",
        "alpha",
        "red:悬空@B",
        &["red:悬空@B".to_string()],
    )
    .unwrap();
    let r = current(Some(&f));
    assert!(r.acked("/w", "alpha", "red:悬空@B"));
    assert!(!r.acked("/w", "alpha", "top:r1"), "不在此刻的键里 ⇒ 清掉");
    unack_at(&own_of(&f), "/w", "alpha", "red:悬空@B").unwrap();
    assert!(!current(Some(&f)).acked("/w", "alpha", "red:悬空@B"));
}

#[test]
fn the_top_ack_remembers_the_rev_it_was_given_for() {
    let f = file("review-prior");
    ack_at(
        &own_of(&f),
        "/w",
        "alpha",
        "top:r9",
        &["top:r9".to_string()],
    )
    .unwrap();
    let r = current(Some(&f));
    assert_eq!(r.prior("/w", "alpha").as_deref(), Some("r9"));
    assert_eq!(r.prior("/w", "beta"), None);
}

#[test]
fn a_later_return_replaces_the_earlier_one_on_that_cell() {
    let f = file("review-return");
    returned_at(&own_of(&f), "/w", "alpha", "A1", rec(1)).unwrap();
    returned_at(&own_of(&f), "/w", "alpha", "A1", rec(2)).unwrap();
    let r = current(Some(&f));
    assert_eq!(r.returned("/w", "alpha", "A1").map(|x| x.at), Some(2));
    assert_eq!(r.returned("/w", "alpha", "A2"), None);
}

#[test]
fn an_unreadable_file_is_never_overwritten() {
    let f = file("review-bad");
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, "{not json").unwrap();
    let e = ack_at(
        &own_of(&f),
        "/w",
        "alpha",
        "top:r1",
        &["top:r1".to_string()],
    )
    .unwrap_err();
    assert_eq!(e.code, "review_unreadable");
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "{not json");
    // 读的那一侧照什么都没记算。
    assert!(!current(Some(&f)).acked("/w", "alpha", "top:r1"));
}

#[test]
fn annotate_marks_acks_counts_and_return_states_on_the_product() {
    let b = crate::plan::book::Book::default();
    let raw = dump_top_signed("/w").to_string().into_bytes();
    let mut doc = b
        .ingest(
            crate::plan::dump::Ran::Dump {
                doc: dump_top_signed("/w"),
                raw,
            },
            Path::new("/w"),
            &who,
            1,
        )
        .unwrap();
    let rev = doc.rev.clone();
    let f = file("review-annotate");
    let keys = crate::plan::needs::keys_of(&doc.slices[0]);
    ack_at(&own_of(&f), "/w", "alpha", &format!("top:{rev}"), &keys).unwrap();
    let mut r = rec(3);
    r.body = crate::plan::needs::body_digest(doc.slices[0].cells[0].body.as_deref());
    returned_at(&own_of(&f), "/w", "alpha", "A1", r).unwrap();
    current(Some(&f)).annotate(&mut doc);
    let sl = &doc.slices[0];
    assert!(sl.needs.iter().find(|n| n.kind == "top").unwrap().acked);
    // top 已认可 ⇒ 剩不通过那一条（问人那两条不算）。
    assert_eq!(sl.need_count, 1);
    // 整个工作区：alpha 那一条（beta 没有要看的）。
    assert_eq!(doc.need_count, 1);
    let back = sl.cells[0].returned.as_ref().unwrap();
    assert_eq!(back.state, "returned");
    assert_eq!(back.to.as_ref().and_then(|t| t.sid.as_deref()), Some(MAIN));
    assert_eq!(sl.cells[1].returned, None);
}

/// 退回过 project ⇒ 那一片顶上带 `returned`：顶层多出退回之后新建的一格 ⇒ 已落地（带那一格的标题）；没退回过 ⇒ `null`。
#[test]
fn a_return_of_the_top_block_lands_when_a_new_top_cell_appears() {
    let b = crate::plan::book::Book::default();
    let take = |b: &crate::plan::book::Book| {
        b.ingest(
            crate::plan::dump::Ran::Dump {
                doc: dump("/w"),
                raw: dump("/w").to_string().into_bytes(),
            },
            Path::new("/w"),
            &who,
            1,
        )
        .unwrap()
    };
    let mut doc = take(&b);
    let f = file("review-project");
    current(Some(&f)).annotate(&mut doc);
    assert_eq!(doc.slices[0].returned, None);
    let mut r = rec(5);
    r.children = vec!["A1".into()];
    returned_at(&own_of(&f), "/w", "alpha", "project", r).unwrap();
    let mut doc = take(&b);
    current(Some(&f)).annotate(&mut doc);
    let got = crate::plan::fixture::as_json(&doc.slices[0].returned);
    assert_eq!(got["state"], "landed");
    assert_eq!(got["by"], "child");
    assert_eq!(got["child"]["id"], "A2");
    assert_eq!(got["child"]["title"], "乙功能");
    assert_eq!(got["to"]["sid"], MAIN);
    let mut r = rec(6);
    r.children = ["A1", "A2", "A3", "A4", "A5", "A6", "A7"]
        .map(String::from)
        .to_vec();
    // project 没有正文：记下的摘要是空正文那一份（送的那一刻也是这么记的）。
    r.body = crate::plan::needs::body_digest(None);
    returned_at(&own_of(&f), "/w", "alpha", "project", r).unwrap();
    let mut doc = take(&b);
    current(Some(&f)).annotate(&mut doc);
    assert_eq!(doc.slices[0].returned.as_ref().unwrap().state, "returned");
}

#[test]
fn the_file_is_written_with_its_version_and_read_back() {
    let f = file("review-ver");
    ack_at(&own_of(&f), "/w", "alpha", "top:r1", &["top:r1".to_string()]).unwrap();
    let back: serde_json::Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
    assert_eq!(back["version"], serde_json::json!(1), "写回没带版本");
    assert!(current(Some(&f)).acked("/w", "alpha", "top:r1"), "带版本的那一份读不回");
}
