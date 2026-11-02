//! 会话消息里提到的格：认片 · 一批只问一次 pb · 码位拆回各段换 UTF-16 · 标签照 pb 的标题接 · 缓存。
//! pb 一律是替身（回写好的那一份、数自己被问了几次）；成品用合成 dump 加工，不碰真的计划仓。

use super::*;
use crate::plan::fixture;
use serde_json::{json, Value};
use std::cell::RefCell;

const WS: &str = "/synthetic/ws";

fn at() -> At {
    At {
        workspace: WS.into(),
        slice: "alpha".into(),
        rev: "r0".into(),
    }
}

fn doc() -> PlanRead {
    let mut d = fixture::dump(WS);
    d["repos"][0]["path"] = json!("repos/alpha");
    fixture::made_with(&d, &fixture::who)
}

/// 替身：记下每次交来的那一段，回 `reply(那一段)`。
struct Fake<F: Fn(&str) -> Value> {
    asked: RefCell<Vec<String>>,
    reply: F,
}

impl<F: Fn(&str) -> Value> Fake<F> {
    fn new(reply: F) -> Self {
        Fake {
            asked: RefCell::new(Vec::new()),
            reply,
        }
    }
    fn ask(&self, one: &str) -> Option<Value> {
        self.asked.borrow_mut().push(one.to_string());
        Some((self.reply)(one))
    }
}

/// pb 照实回的那一形：`refs` 按码位、`nodes` 只带提到的那几个。
fn pb_reply(refs: Value) -> Value {
    json!({
        "pb": "0.5.4", "shape": 6, "workspace": WS, "repo": "alpha",
        "refs": refs,
        "nodes": {"A1": {"title": "甲功能"}, "A1-1": {"title": "甲的读入"}, "A1-2": {"title": "甲的写出"}}
    })
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn two_refs_each_get_their_title_with_utf16_offsets() {
    // 「😀 」在前：码位 2 个、UTF-16 3 个 ⇒ 下标要换。
    let text = "😀 看 A1-1，再看 A1-2/1";
    let a = text.chars().position(|c| c == 'A').unwrap() as u64;
    let b = text
        .char_indices()
        .filter(|(_, c)| *c == 'A')
        .nth(1)
        .map(|(i, _)| text[..i].chars().count())
        .unwrap() as u64;
    let fake = Fake::new(|_| {
        pb_reply(json!([
            {"start": a, "end": a + 4, "ids": ["A1-1"]},
            {"start": b, "end": b + 6, "ids": ["A1-2", "A1-1"]}
        ]))
    });
    let got = refs_in(&Memo::new(16), &at(), &strs(&[text]), &|o| fake.ask(o));
    assert_eq!(got.len(), 1);
    let r = &got[0];
    assert_eq!(r.len(), 2, "两处各一项：{r:?}");
    let u16s: Vec<u16> = text.encode_utf16().collect();
    let cut = |s: u32, e: u32| String::from_utf16(&u16s[s as usize..e as usize]).unwrap();
    assert_eq!(
        cut(r[0].start, r[0].end),
        "A1-1",
        "UTF-16 下标切出来是编号本身"
    );
    assert_eq!(r[0].label, "甲的读入");
    assert_eq!(cut(r[1].start, r[1].end), "A1-2/1");
    assert_eq!(r[1].ids, strs(&["A1-2", "A1-1"]));
    assert_eq!(
        r[1].label,
        format!(
            "甲的写出{}甲的读入",
            copy_core::copy_text("plan.page.listSep", &[])
        ),
        "简写一处两个编号 ⇒ 两个标题照先后接"
    );
}

#[test]
fn empty_reply_leaves_every_text_without_refs() {
    let fake = Fake::new(|_| pb_reply(json!([])));
    let got = refs_in(
        &Memo::new(16),
        &at(),
        &strs(&["没有编号", "也没有"]),
        &|o| fake.ask(o),
    );
    assert_eq!(got, vec![Vec::new(), Vec::new()]);
}

#[test]
fn a_batch_asks_pb_once_and_offsets_land_in_their_own_text() {
    let texts = strs(&["第一段 A1", "", "第三段说 A1-2 与 A1-1"]);
    let fake = Fake::new(|one: &str| {
        // 替身自己在接好的那一段里找编号（只为造一份合乎 pb 形状的回答，判据不判它）。
        let cps: Vec<char> = one.chars().collect();
        let mut refs = Vec::new();
        for id in ["A1-2", "A1-1"] {
            let pat: Vec<char> = id.chars().collect();
            for s in 0..cps.len().saturating_sub(pat.len() - 1) {
                if cps[s..s + pat.len()] == pat[..] {
                    refs.push(json!({"start": s, "end": s + pat.len(), "ids": [id]}));
                }
            }
        }
        let s = one.chars().position(|c| c == 'A').unwrap();
        refs.push(json!({"start": s, "end": s + 2, "ids": ["A1"]}));
        pb_reply(Value::Array(refs))
    });
    let memo = Memo::new(16);
    let got = refs_in(&memo, &at(), &texts, &|o| fake.ask(o));
    assert_eq!(fake.asked.borrow().len(), 1, "一批只问一次 pb");
    assert_eq!(
        fake.asked.borrow()[0],
        format!("第一段 A1{SEAM}第三段说 A1-2 与 A1-1"),
        "空段不交，余下的用接缝接好"
    );
    assert_eq!(got.len(), 3);
    assert_eq!(
        got[0].iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
        ["甲功能"]
    );
    assert_eq!(got[0][0].start, 4);
    assert!(got[1].is_empty());
    let mut third: Vec<(u32, &str)> = got[2].iter().map(|r| (r.start, r.label.as_str())).collect();
    third.sort();
    assert_eq!(
        third,
        [(5, "甲的写出"), (12, "甲的读入")],
        "第三段的下标从它自己的头算"
    );

    // 同一批再问 ⇒ 全中，不起 pb。
    let again = refs_in(&memo, &at(), &texts, &|o| fake.ask(o));
    assert_eq!(again, got);
    assert_eq!(fake.asked.borrow().len(), 1, "缓存全中 ⇒ 不再问 pb");

    // 图变了（成品摘要换了）⇒ 旧的不作数。
    let moved = At {
        rev: "r1".into(),
        ..at()
    };
    refs_in(&memo, &moved, &texts, &|o| fake.ask(o));
    assert_eq!(fake.asked.borrow().len(), 2);
}

#[test]
fn pb_failure_or_error_gives_empty_and_is_not_remembered() {
    let memo = Memo::new(16);
    let texts = strs(&["A1"]);
    let got = refs_in(&memo, &at(), &texts, &|_| None);
    assert_eq!(got, vec![Vec::new()]);
    let err = refs_in(&memo, &at(), &texts, &|_| {
        Some(json!({"repo": "alpha", "error": "alpha 读不成"}))
    });
    assert_eq!(err, vec![Vec::new()]);
    let fake = Fake::new(|_| pb_reply(json!([{"start": 0, "end": 2, "ids": ["A1"]}])));
    let ok = refs_in(&memo, &at(), &texts, &|o| fake.ask(o));
    assert_eq!(fake.asked.borrow().len(), 1, "上两回没记 ⇒ 这一回照问");
    assert_eq!(ok[0][0].label, "甲功能");
}

#[test]
fn refs_without_any_title_or_out_of_range_are_dropped() {
    let fake = Fake::new(|_| {
        json!({"refs": [
            {"start": 0, "end": 2, "ids": ["Z9"]},
            {"start": 0, "end": 99, "ids": ["A1"]}
        ], "nodes": {"Z9": {"title": ""}}})
    });
    let got = refs_in(&Memo::new(16), &at(), &strs(&["Z9 话"]), &|o| fake.ask(o));
    assert_eq!(got, vec![Vec::new()]);
}

#[test]
fn slice_is_the_claimed_blocks_then_the_cwds_then_none() {
    let mut d = doc();
    // 接手的会话：照它那一块的片（造成另一片，免得与当前片撞）。
    let sid = fixture::MAIN;
    d.by_session
        .get_mut(sid)
        .expect("夹具里 MAIN 接着一块")
        .slice = "beta".into();
    let docs = vec![d];
    assert_eq!(
        at_for(&docs, sid, Some("/elsewhere")).map(|a| a.slice),
        Some("beta".into()),
        "接手的会话 ⇒ 那块的片（不看工作目录）"
    );
    let a = at_for(
        &docs,
        "no-such-session",
        Some(&format!("{WS}/repos/alpha/src")),
    )
    .unwrap();
    assert_eq!(
        (a.workspace.as_str(), a.slice.as_str(), a.rev.as_str()),
        (WS, "alpha", "r0")
    );
    let cur = at_for(&docs, "no-such-session", Some(&format!("{WS}/notes"))).unwrap();
    assert_eq!(cur.slice, "alpha", "工作区里、不在哪片的仓库里 ⇒ 当前片");
    assert_eq!(
        at_for(&docs, "no-such-session", Some("/synthetic/wsx")),
        None,
        "按路径段认，不按字头"
    );
    assert_eq!(at_for(&docs, "no-such-session", None), None);
}

/// 回复正文是树：投成一段字跟人说的话同一批交 pb（代码里的编号换成占位字，pb 认不到）；认出的那几处插回树上成引用节点，
/// 标题照 pb 的写；一处都没有的那棵 ⇒ `null`。
#[test]
fn reply_trees_get_ref_nodes_where_pb_found_numbers() {
    let tree = md_core::parse("😀 先看 **A1-1**，`A1` 在代码里");
    let plain = md_core::parse("没有编号");
    let fake = Fake::new(|one: &str| {
        let cps: Vec<char> = one.chars().collect();
        let pat: Vec<char> = "A1-1".chars().collect();
        let s = (0..cps.len())
            .find(|&s| cps[s..].starts_with(&pat))
            .unwrap();
        pb_reply(json!([{"start": s, "end": s + 4, "ids": ["A1-1"]}]))
    });
    let out = answer(
        &[doc()],
        "no-such-session",
        Some(&format!("{WS}/repos/alpha/src")),
        &strs(&["人说的"]),
        &[tree.clone(), plain],
        &Memo::new(16),
        &|_, one| fake.ask(one),
    );
    assert_eq!(
        fake.asked.borrow().as_slice(),
        [format!(
            "人说的{SEAM}😀 先看 A1-1，\u{FFFC} 在代码里{SEAM}没有编号"
        )],
        "一批只问一次；树投成字，代码换成占位"
    );
    assert_eq!(out.refs, vec![Vec::new()]);
    let start = "😀 先看 ".encode_utf16().count();
    let want = md_core::with_refs(
        &tree,
        &[md_core::RefAt {
            start,
            end: start + 4,
            ids: vec!["A1-1".into()],
            label: "甲的读入".into(),
        }],
    );
    assert_ne!(want, tree, "夹具：那一处真插得进去");
    assert_eq!(out.docs, vec![Some(want), None]);
}

#[test]
fn answer_without_a_slice_never_asks_pb() {
    let asked = RefCell::new(0);
    let out = answer(
        &[doc()],
        "no-such-session",
        Some("/elsewhere"),
        &strs(&["A1", "A1-1"]),
        &[md_core::parse("A1")],
        &Memo::new(4),
        &|_, _| {
            *asked.borrow_mut() += 1;
            None
        },
    );
    assert_eq!(*asked.borrow(), 0);
    assert_eq!((out.workspace, out.slice), (None, None));
    assert_eq!(out.refs, vec![Vec::new(), Vec::new()], "一段对一段");
    assert_eq!(out.docs, vec![None], "一棵对一棵");
}

/// 真起一次（假 pb 插件）：`dump --refs <片> <工作区>`、正文从标准输入进去，回的那一份原样交回。
#[test]
fn run_refs_feeds_the_text_on_stdin_and_hands_back_pbs_answer() {
    let d = fixture::scratch("refs-run");
    let entry = fixture::fake_pb(&d.join("pb"), crate::plan::locate::PLUGIN_NAME, None);
    let ws = fixture::workspace(&d.join("ws"), &fixture::dump("/synthetic"));
    let reply = pb_reply(json!([{"start": 2, "end": 4, "ids": ["A1"]}]));
    std::fs::write(ws.join("refs.json"), reply.to_string()).unwrap();
    let text = format!("看 A1{SEAM}😀 第二段");
    let got = crate::plan::dump::run_refs(&entry, &ws, "alpha", &text);
    assert_eq!(got, Some(reply));
    let ws_s = ws.to_string_lossy().to_string();
    assert_eq!(
        fixture::runs(&d.join("pb")),
        vec![vec![
            "dump".to_string(),
            "--refs".into(),
            "alpha".into(),
            ws_s
        ]]
    );
    assert_eq!(
        std::fs::read_to_string(d.join("pb/stdin.txt")).unwrap(),
        text,
        "正文从标准输入进去（UTF-8 原样）"
    );
    // pb 起不来 · 退出码不是 0 ⇒ 没有答案（那一批回空）。
    let bad = fixture::fake_pb(&d.join("pb-bad"), crate::plan::locate::PLUGIN_NAME, Some(1));
    assert_eq!(crate::plan::dump::run_refs(&bad, &ws, "alpha", &text), None);
    let _ = std::fs::remove_dir_all(&d);
}
