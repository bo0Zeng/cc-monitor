//! 活卡板的折法（桌面与手机都只调这一份）：几种 `op` 照折；`n` 接不上 ⇒ 那张先不画、等下一张整卡；子运行的卡归它那一处；撤 ⇒ 交空的。
use super::*;

/// 手写的一帧（期望只从这里来）。
fn f(v: serde_json::Value) -> LiveCard {
    serde_json::from_value(v).expect("夹具是一帧 live_card")
}

/// 一张卡从开到说完：整卡（思考块 ＋ 空正文块）→ 几段字 → 工具块 → 说完了。`n` 从 0 连着编。
fn card(sid: &str, no: u64, rid: &str, text: &[&str], run: Option<&str>) -> Vec<LiveCard> {
    let base = |n: u64| {
        let mut v = serde_json::json!({ "sid": sid, "card": no, "n": n });
        if let Some(r) = run {
            v["run"] = r.into();
        }
        v
    };
    let mut out = Vec::new();
    let mut open = base(0);
    open.as_object_mut().unwrap().extend(
        serde_json::json!({ "op": "open", "rid": rid, "phase": "streaming", "clipped": false,
            "blocks": [{ "kind": "thinking" }, { "kind": "text", "text": "" }] })
        .as_object()
        .unwrap()
        .clone(),
    );
    out.push(f(open));
    for s in text {
        let mut v = base(out.len() as u64);
        v["op"] = "text".into();
        v["i"] = 1.into();
        v["s"] = (*s).into();
        out.push(f(v));
    }
    let mut v = base(out.len() as u64);
    v["op"] = "block".into();
    v["i"] = 2.into();
    v["block"] = "tool".into();
    v["tool"] = "Read".into();
    out.push(f(v));
    let mut v = base(out.len() as u64);
    v["op"] = "awaiting".into();
    out.push(f(v));
    out
}

fn texts(b: &Board, sid: &str, run: Option<&str>) -> Vec<Vec<Option<String>>> {
    b.cards_of(sid, run)
        .cards
        .iter()
        .map(|c| c.blocks.iter().map(|b| b.text.clone()).collect())
        .collect()
}

#[test]
fn one_card_grows_block_by_block_and_ends_awaiting_the_record() {
    let mut b = Board::default();
    let fs = card("s1", 7, "m1", &["你", "好"], None);
    for x in &fs[..3] {
        assert_eq!(b.apply(x), Some(("s1".into(), None)));
    }
    let mid = b.cards_of("s1", None);
    assert_eq!(mid.cards.len(), 1);
    assert_eq!(mid.cards[0].phase, LivePhase::Streaming);
    assert_eq!(texts(&b, "s1", None), [[None, Some("你好".to_string())]]);
    for x in &fs[3..] {
        b.apply(x);
    }
    let end = &b.cards_of("s1", None).cards[0];
    assert_eq!(end.phase, LivePhase::Awaiting);
    assert_eq!((end.rid.as_str(), end.card), ("m1", 7));
    assert_eq!(end.blocks[2].kind, BlockKind::Tool);
    assert_eq!(end.blocks[2].tool.as_deref(), Some("Read"));
}

#[test]
fn an_open_replaces_the_whole_card_and_a_close_takes_it_away() {
    let mut b = Board::default();
    for x in card("s1", 7, "m1", &["a"], None) {
        b.apply(&x);
    }
    b.apply(&f(serde_json::json!({"sid":"s1","card":7,"n":0,"op":"open","rid":"m1","phase":"streaming","clipped":true,
        "blocks":[{"kind":"text","text":"尾巴"}]})));
    let now = b.cards_of("s1", None);
    assert!(now.cards[0].clipped);
    assert_eq!(texts(&b, "s1", None), [[Some("尾巴".to_string())]]);
    let closed = b.apply(&f(serde_json::json!({"sid":"s1","card":7,"n":1,"op":"close","why":"record"})));
    assert_eq!(closed, Some(("s1".into(), None)), "撤 ⇒ 那一处变了（交空的）");
    assert!(b.cards_of("s1", None).cards.is_empty());
    assert!(b.is_empty());
}

#[test]
fn a_frame_that_does_not_follow_hides_the_card_until_the_next_whole_one() {
    let mut b = Board::default();
    let fs = card("s1", 7, "m1", &["a", "b", "c"], None);
    b.apply(&fs[0]);
    assert!(b.apply(&fs[2]).is_some(), "跳过了 n = 1 ⇒ 那张撤下");
    assert!(b.cards_of("s1", None).cards.is_empty());
    assert_eq!(b.apply(&fs[3]), None, "之后的增量不收");
    b.apply(&f(serde_json::json!({"sid":"s1","card":7,"n":0,"op":"open","rid":"m1","phase":"streaming","clipped":false,
        "blocks":[{"kind":"text","text":"abc"}]})));
    assert_eq!(texts(&b, "s1", None), [[Some("abc".to_string())]]);
}

#[test]
fn a_sub_run_card_lives_under_its_run_and_cards_keep_their_arrival_order() {
    let mut b = Board::default();
    for x in &card("s1", 3, "r", &["子"], Some("a1"))[..2] {
        assert_eq!(b.apply(x), Some(("s1".into(), Some("a1".into()))));
    }
    assert!(b.cards_of("s1", None).cards.is_empty(), "子运行的卡不上主运行");
    let sub = b.cards_of("s1", Some("a1"));
    assert_eq!(sub.run.as_deref(), Some("a1"));
    assert_eq!(texts(&b, "s1", Some("a1")), [[None, Some("子".to_string())]]);
    // 后开的卡排在后面；先开的那张再来一张整卡也不挪位。
    b.apply(&card("s1", 1, "m2", &[], None)[0]);
    b.apply(&card("s1", 0, "m3", &[], None)[0]);
    b.apply(&card("s1", 1, "m2", &[], None)[0]);
    let order: Vec<String> = b.cards_of("s1", None).cards.into_iter().map(|c| c.rid).collect();
    assert_eq!(order, ["m2", "m3"]);
}

#[test]
fn dropping_a_session_or_clearing_says_which_places_changed() {
    let mut b = Board::default();
    b.apply(&card("s1", 1, "m", &[], None)[0]);
    b.apply(&card("s1", 2, "m", &[], Some("a1"))[0]);
    b.apply(&card("s2", 3, "m", &[], None)[0]);
    let mut gone = b.drop_session("s1");
    gone.sort();
    assert_eq!(gone, [("s1".to_string(), None), ("s1".to_string(), Some("a1".to_string()))]);
    assert_eq!(b.len(), 1);
    assert_eq!(b.clear(), [("s2".to_string(), None)]);
    assert!(b.clear().is_empty(), "空板再清 ⇒ 没有哪处变了");
}
