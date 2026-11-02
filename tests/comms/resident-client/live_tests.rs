//! 活卡板的折法（桌面与手机都只调这一份）：几种 `op` 照折；`n` 接不上 ⇒ 那张先不画、等下一张整卡；子运行的卡归它那一处；撤 ⇒ 交空的；
//! 一段字只换一张卡的尾巴 ⇒ 交 `tail`（前几块不变），别的交那一处整份。
use super::*;

/// 手写的一帧（期望只从这里来）。
fn f(v: serde_json::Value) -> LiveCard {
    serde_json::from_value(v).expect("夹具是一帧 live_card")
}

/// 一段字的块。
fn p(s: &str) -> MdBlock {
    MdBlock::P {
        k: vec![Inl::S(s.to_string())],
    }
}

fn pj(s: &str) -> serde_json::Value {
    serde_json::to_value(p(s)).unwrap()
}

/// 一张卡从开到说完：整卡（思考块 ＋ 空正文块）→ 几段字（都接在同一段里：每帧 `keep` 0、尾巴是到这里为止的那一段）→ 工具块 → 说完了。
/// `n` 从 0 连着编。
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
            "blocks": [{ "kind": "thinking" }, { "kind": "text", "md": [] }] })
        .as_object()
        .unwrap()
        .clone(),
    );
    out.push(f(open));
    let mut acc = String::new();
    for s in text {
        acc.push_str(s);
        let mut v = base(out.len() as u64);
        v["op"] = "text".into();
        v["i"] = 1.into();
        v["keep"] = 0.into();
        v["tail"] = serde_json::json!([pj(&acc)]);
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

/// 每张卡正文里各段的字。
fn texts(b: &Board, sid: &str, run: Option<&str>) -> Vec<Vec<String>> {
    b.cards_of(sid, run)
        .cards
        .iter()
        .map(|c| {
            c.body
                .iter()
                .map(|b| match b {
                    MdBlock::P { k } => k
                        .iter()
                        .map(|i| match i {
                            Inl::S(s) => s.clone(),
                            Inl::N(_) => String::new(),
                        })
                        .collect(),
                    other => format!("{other:?}"),
                })
                .collect()
        })
        .collect()
}

fn at(c: Option<LiveChange>) -> Option<Where> {
    c.map(|c| c.at())
}

#[test]
fn one_card_grows_block_by_block_and_ends_awaiting_the_record() {
    let mut b = Board::default();
    let fs = card("s1", 7, "m1", &["你", "好"], None);
    for x in &fs[..3] {
        assert_eq!(at(b.apply(x)), Some(("s1".into(), None)));
    }
    let mid = b.cards_of("s1", None);
    assert_eq!(mid.cards.len(), 1);
    assert_eq!(mid.cards[0].phase, LivePhase::Streaming);
    let think = copy_text("liveCard.block.thinking", &[]);
    assert_eq!(texts(&b, "s1", None), [[think.clone(), "你好".to_string()]]);
    for x in &fs[3..] {
        b.apply(x);
    }
    let end = &b.cards_of("s1", None).cards[0];
    assert_eq!(end.phase, LivePhase::Awaiting);
    assert_eq!((end.rid.as_str(), end.card), ("m1", 7));
    assert_eq!(
        texts(&b, "s1", None),
        [[
            think,
            "你好".to_string(),
            copy_text("liveCard.block.toolUse", &[("tool", "Read")])
        ]]
    );
}

/// 一段字 ⇒ 只交那一张卡的尾巴：`keep` 数到整张正文里（截头那一句 · 思考那一句也算），尾巴之后的块（工具那一句）跟着交；
/// 别的帧交整份。
#[test]
fn a_text_frame_hands_over_only_that_cards_tail_and_everything_else_hands_over_the_whole_place() {
    let mut b = Board::default();
    let open = f(
        serde_json::json!({"sid":"s1","card":7,"n":0,"op":"open","rid":"m","phase":"streaming","clipped":true,
        "blocks":[{"kind":"thinking"},{"kind":"text","md":[pj("一"), pj("二")]},{"kind":"tool","tool":"Read"}]}),
    );
    assert!(matches!(b.apply(&open), Some(LiveChange::Whole { .. })));
    let before = b.cards_of("s1", None).cards[0].body.clone();
    let grow = f(
        serde_json::json!({"sid":"s1","card":7,"n":1,"op":"text","i":1,"keep":1,"tail":[pj("二三"), pj("四")]}),
    );
    let Some(LiveChange::Tail { tail }) = b.apply(&grow) else {
        panic!("一段字没交尾巴");
    };
    let read = copy_text("liveCard.block.toolUse", &[("tool", "Read")]);
    assert_eq!(tail.card, 7);
    assert_eq!(tail.keep, 3, "截头那一句 · 思考那一句 · 「一」不变");
    assert_eq!(tail.blocks, [p("二三"), p("四"), p(&read)]);
    assert_eq!(tail.last, Some(read.clone()));
    let now = &b.cards_of("s1", None).cards[0].body;
    assert_eq!(now[..3], before[..3], "前 keep 块没动");
    assert_eq!(now[3..], tail.blocks[..], "板上那一张就是前几块 ＋ 尾巴");
    let wait = f(serde_json::json!({"sid":"s1","card":7,"n":2,"op":"awaiting"}));
    assert!(matches!(b.apply(&wait), Some(LiveChange::Whole { .. })));
}

#[test]
fn an_open_replaces_the_whole_card_and_a_close_takes_it_away() {
    let mut b = Board::default();
    for x in card("s1", 7, "m1", &["a"], None) {
        b.apply(&x);
    }
    b.apply(&f(serde_json::json!({"sid":"s1","card":7,"n":0,"op":"open","rid":"m1","phase":"streaming","clipped":true,
        "blocks":[{"kind":"text","md":[pj("尾巴")]}]})));
    let now = b.cards_of("s1", None);
    assert!(now.cards[0].clipped);
    assert_eq!(
        texts(&b, "s1", None),
        [[copy_text("liveCard.body.clipped", &[]), "尾巴".to_string()]]
    );
    let closed = b.apply(&f(
        serde_json::json!({"sid":"s1","card":7,"n":1,"op":"close","why":"record"}),
    ));
    assert_eq!(
        closed,
        Some(LiveChange::Whole {
            whole: LiveCards {
                sid: "s1".into(),
                run: None,
                cards: Vec::new()
            }
        }),
        "撤 ⇒ 那一处交空的"
    );
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
        "blocks":[{"kind":"text","md":[pj("abc")]}]})));
    assert_eq!(texts(&b, "s1", None), [["abc".to_string()]]);
}

#[test]
fn a_sub_run_card_lives_under_its_run_and_cards_keep_their_arrival_order() {
    let mut b = Board::default();
    for x in &card("s1", 3, "r", &["子"], Some("a1"))[..2] {
        assert_eq!(at(b.apply(x)), Some(("s1".into(), Some("a1".into()))));
    }
    assert!(
        b.cards_of("s1", None).cards.is_empty(),
        "子运行的卡不上主运行"
    );
    let sub = b.cards_of("s1", Some("a1"));
    assert_eq!(sub.run.as_deref(), Some("a1"));
    assert_eq!(
        texts(&b, "s1", Some("a1")),
        [[copy_text("liveCard.block.thinking", &[]), "子".to_string()]]
    );
    // 后开的卡排在后面；先开的那张再来一张整卡也不挪位。
    b.apply(&card("s1", 1, "m2", &[], None)[0]);
    b.apply(&card("s1", 0, "m3", &[], None)[0]);
    b.apply(&card("s1", 1, "m2", &[], None)[0]);
    let order: Vec<String> = b
        .cards_of("s1", None)
        .cards
        .into_iter()
        .map(|c| c.rid)
        .collect();
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
    assert_eq!(
        gone,
        [
            ("s1".to_string(), None),
            ("s1".to_string(), Some("a1".to_string()))
        ]
    );
    assert_eq!(b.len(), 1);
    assert_eq!(b.clear(), [("s2".to_string(), None)]);
    assert!(b.clear().is_empty(), "空板再清 ⇒ 没有哪处变了");
}

/// 板交出来的每张卡带写好的头一行 · 正文 · 「最近：…」那一截（两端照摆、不按块的种类取字）。
#[test]
fn each_card_comes_with_its_head_body_and_last_line_written() {
    let mut b = Board::default();
    let fs = card("s1", 3, "m1", &["你好"], None);
    for x in &fs[..2] {
        b.apply(x);
    }
    let c = b.cards_of("s1", None).cards.remove(0);
    assert_eq!(c.head, copy_text("liveCard.state.streaming", &[]));
    assert_eq!(
        c.body,
        [p(&copy_text("liveCard.block.thinking", &[])), p("你好")],
        "思考块一句、文字照树"
    );
    assert_eq!(c.last, Some(copy_text("runs.last.say", &[])));
    b.apply(&fs[2]);
    let c = b.cards_of("s1", None).cards.remove(0);
    let read = copy_text("liveCard.block.toolUse", &[("tool", "Read")]);
    assert_eq!(
        c.last,
        Some(read.clone()),
        "最后一块是工具 ⇒ 准备调用那一句"
    );
    assert_eq!(c.body.last(), Some(&p(&read)));
    b.apply(&fs[3]);
    let c = b.cards_of("s1", None).cards.remove(0);
    assert_eq!(c.head, copy_text("liveCard.state.awaitingRecord", &[]));
    assert_eq!(c.last, None, "说完了 ⇒ 不再是「最近：…」那一截");

    let mut b = Board::default();
    b.apply(&f(serde_json::json!({ "sid": "s2", "card": 1, "n": 0, "op": "open", "rid": "m",
        "phase": "streaming", "clipped": true, "blocks": [{ "kind": "tool" }, { "kind": "text" }, { "kind": "other" }] })));
    let c = b.cards_of("s2", None).cards.remove(0);
    assert_eq!(
        c.body,
        [
            p(&copy_text("liveCard.body.clipped", &[])),
            p(&copy_text("liveCard.block.toolUse", &[("tool", "?")]))
        ],
        "截过头先说一句；工具没名字 ⇒ 「?」；还没字的文字块不出"
    );
    assert_eq!(c.last, None, "最后一块认不出 ⇒ 没有那一截");
}
