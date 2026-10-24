//! 要你看四种的判定：顶块走到看全局 · 判据红 · 接手的会话停了而那一块没做完 · 接手的会话在等你；键带条目版本；计数不含「问人」那一种。

use super::*;
use crate::plan::fixture::{as_json, dump, made_with, MAIN, STRANGER, SUB};
use crate::plan::Whose;
use serde_json::{json, Value};

/// 一个工作区要你看的数（生产里由认可记录标的时候逐片数）。
fn count(doc: &PlanRead) -> u32 {
    doc.slices.iter().map(count_slice).sum()
}

/// 夹具那份 dump 照给定的身份表加工，取那一片。
fn slice_with(doc: &Value, who: &dyn Fn(&str) -> Whose) -> PlanSlice {
    made_with(doc, who).slices.remove(0)
}

/// 一片的要你看，线上那一形（判据按键看）。
fn ofs(sl: &PlanSlice, entered: Option<&str>) -> Vec<Value> {
    of_slice(sl, entered).iter().map(as_json).collect()
}

/// 谁都活着、在跑（不问人）。
fn all_working(id: &str) -> Whose {
    use crate::plan::Live;
    let live = Some(Live {
        activity: Some(crate::agents::SessionActivity::Working),
        needs: None,
    });
    match id {
        MAIN => Whose::Session {
            sid: MAIN.into(),
            live,
        },
        SUB => Whose::Subagent {
            parent: MAIN.into(),
            live,
        },
        _ => Whose::Unknown,
    }
}

/// 主会话（也是子 agent 的父会话）已经结束：记录在、不活。
fn all_ended(id: &str) -> Whose {
    match id {
        MAIN => Whose::Session {
            sid: MAIN.into(),
            live: None,
        },
        SUB => Whose::Subagent {
            parent: MAIN.into(),
            live: None,
        },
        _ => Whose::Unknown,
    }
}

fn kinds(n: &[Value]) -> Vec<(String, String)> {
    n.iter()
        .map(|x| {
            (
                x["kind"].as_str().unwrap().to_string(),
                x["key"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn the_top_block_at_the_look_global_phase_is_one_need_keyed_by_the_rev_it_entered_at() {
    let sl = slice_with(&dump("/w"), &all_working);
    assert!(top_at_look_global(&sl));
    let n = ofs(&sl, Some("r7"));
    let top: Vec<_> = n.iter().filter(|x| x["kind"] == "top").collect();
    assert_eq!(top.len(), 1);
    assert_eq!(top[0]["key"], "top:r7");
    assert_eq!(top[0]["block"], "project");
    assert_eq!(top[0]["cell"], "project");
    assert_eq!(top[0]["acked"], false);
    // 进那一步的那次 rev 没给（还不知道）⇒ 不出这一条，不拿空键凑。
    assert!(ofs(&sl, None).iter().all(|x| x["kind"] != "top"));
}

#[test]
fn a_top_block_not_at_a_look_global_phase_is_no_need() {
    let mut d = dump("/w");
    d["slices"][0]["blocks"][0]["phase"] = json!("定架构");
    let sl = slice_with(&d, &all_working);
    assert!(!top_at_look_global(&sl));
    assert!(ofs(&sl, Some("r7")).iter().all(|x| x["kind"] != "top"));
    // 看全局那个标打在别的阶段上 ⇒ 认标不认名字。
    d["slices"][0]["phases"][0]["marks"] = json!(["看全局"]);
    assert!(top_at_look_global(&slice_with(&d, &all_working)));
}

#[test]
fn every_red_check_is_one_need_keyed_by_rule_and_block_at_the_blocks_root_cell() {
    let mut d0 = dump("/w");
    d0["slices"][0]["check"]["red"][0]["block"] = json!("A2");
    let sl = slice_with(&d0, &all_working);
    let n = ofs(&sl, None);
    let red: Vec<_> = n.iter().filter(|x| x["kind"] == "red").collect();
    assert_eq!(red.len(), 1);
    assert_eq!(red[0]["key"], "red:悬空@A2");
    assert_eq!(red[0]["block"], "A2");
    assert_eq!(red[0]["cell"], "A2");
    assert_eq!(red[0]["red"], 0, "指回 check.red 的第几条");
    // 两条红：各出一条；`what` 变了键不变（认可仍作数），规则或块换了键就换。
    let mut d = dump("/w");
    d["slices"][0]["check"]["red"] = json!([
        {"rule": "悬空", "what": "A1-2 指着 A8", "block": "A2", "fix": "x", "refs": []},
        {"rule": "无主", "what": "src/x 没人管", "block": null, "fix": "y", "refs": []}
    ]);
    let n = ofs(&slice_with(&d, &all_working), None);
    let keys: Vec<_> = kinds(&n).into_iter().filter(|(k, _)| k == "red").collect();
    assert_eq!(
        keys,
        vec![
            ("red".into(), "red:悬空@A2".into()),
            ("red".into(), "red:无主@".into())
        ]
    );
    let none = n.iter().find(|x| x["key"] == "red:无主@").unwrap();
    assert_eq!(none["block"], Value::Null);
    assert_eq!(none["cell"], Value::Null);
}

#[test]
fn an_ended_owner_of_an_unfinished_block_is_a_need_and_a_subagent_counts_by_its_parent() {
    let sl = slice_with(&dump("/w"), &all_ended);
    let n = ofs(&sl, None);
    let ended: Vec<_> = n.iter().filter(|x| x["kind"] == "ended").collect();
    // 顶块（主会话停了、整片没做完）· 块 A2（子 agent 接手，父会话也停了）。
    assert_eq!(ended.len(), 2, "{n:?}");
    let b = ended.iter().find(|x| x["block"] == "A2").unwrap();
    assert_eq!(b["key"], format!("ended:A2@{SUB}"));
    assert_eq!(b["cell"], "A2");
    assert_eq!(b["sid"], MAIN, "恢复的是父会话");
    // 父会话还活着 ⇒ 子 agent 停了不算（父会话会收它）。
    let sl = slice_with(&dump("/w"), &all_working);
    assert!(ofs(&sl, None).iter().all(|x| x["kind"] != "ended"));
}

#[test]
fn an_ended_owner_of_a_finished_block_or_an_unknown_owner_is_no_need() {
    let mut d = dump("/w");
    // 块 A2 的根格（A2）做完了；整片也做完了（顶块那一条看 `done`）。
    d["slices"][0]["cells"][7]["status"] = json!("做完了");
    d["slices"][0]["done"] = json!(true);
    let n = ofs(&slice_with(&d, &all_ended), None);
    assert!(n.iter().all(|x| x["kind"] != "ended"), "{n:?}");
    // 不做了也算收了尾。
    d["slices"][0]["cells"][7]["status"] = json!("不做了");
    let n = ofs(&slice_with(&d, &all_ended), None);
    assert!(n.iter().all(|x| x["kind"] != "ended"));
    // 对不上的 id：说不清停没停 ⇒ 不出。
    let n = ofs(&slice_with(&dump("/w"), &|_: &str| Whose::Unknown), None);
    assert!(n.iter().all(|x| x["kind"] != "ended"));
}

#[test]
fn an_owner_waiting_for_you_is_an_ask_need_at_the_cell_the_block_stands_on() {
    // 夹具的身份表：主会话活着、在等你批准；子 agent 的父会话就是它。
    let sl = slice_with(&dump("/w"), &crate::plan::fixture::who);
    let n = ofs(&sl, None);
    let ask: Vec<_> = n.iter().filter(|x| x["kind"] == "ask").collect();
    assert_eq!(ask.len(), 2);
    let b = ask.iter().find(|x| x["block"] == "A2").unwrap();
    assert_eq!(b["cell"], "A2-1", "块站在 A2-1");
    assert_eq!(b["sid"], MAIN);
    assert_eq!(b["key"], format!("ask:A2@{SUB}"));
    let p = ask.iter().find(|x| x["block"] == "project").unwrap();
    assert_eq!(p["cell"], "project", "没站位 ⇒ 块根");
    // 在跑 ⇒ 不问人。
    let sl = slice_with(&dump("/w"), &all_working);
    assert!(ofs(&sl, None).iter().all(|x| x["kind"] != "ask"));
}

#[test]
fn acks_mark_needs_and_the_count_skips_acked_ones_and_asks() {
    let mut doc = made_with(&dump("/w"), &crate::plan::fixture::who);
    doc.slices[0].needs = of_slice(&doc.slices[0], Some("r1"));
    // top · red · ask×2。
    assert_eq!(count(&doc), 2, "问人那两条由会话那一侧数");
    mark_acked(&mut doc, &|slice, key| slice == "alpha" && key == "top:r1");
    let n = &doc.slices[0].needs;
    assert!(n.iter().find(|x| x.kind == "top").unwrap().acked);
    assert!(!n.iter().find(|x| x.kind == "red").unwrap().acked);
    assert_eq!(count(&doc), 1);
    // 问人那一种不收认可：键对上了也不标。
    mark_acked(&mut doc, &|_, k| k.starts_with("ask:"));
    assert!(doc.slices[0]
        .needs
        .iter()
        .filter(|x| x.kind == "ask")
        .all(|x| !x.acked));
}

#[test]
fn a_return_lands_when_a_new_child_appears_or_the_body_changes() {
    let sl = slice_with(&dump("/w"), &all_working);
    let cell = sl.cells.iter().find(|c| c.id == "A1").unwrap();
    let t = Target::of_cell(cell);
    let at = Returned {
        at: 5,
        to: MAIN.into(),
        result: "delivered".into(),
        children: (1..=6).map(|i| format!("A1-{i}")).collect(),
        body: body_digest(cell.body.as_deref()),
    };
    let cells = &sl.cells;
    let st = as_json(&landing(&at, &t, cells));
    assert_eq!(st["state"], "returned");
    // 送给谁照线上那一形（与在长它的 · 签它的同形）：记下的 sid 对到这一格此刻的那一位。
    assert_eq!(st["to"], as_json(&cell.owner));
    assert_eq!(st["to"]["sid"], MAIN);
    assert_eq!(st["at"], 5);
    // 这一格此刻已经没有那一位（接手换了人）⇒ 说不出是谁。
    let gone = Returned {
        to: STRANGER.into(),
        ..at.clone()
    };
    assert_eq!(landing(&gone, &t, cells).to, None);
    // 送达未知：还没落地时照实说。
    let unsure = Returned {
        result: "unsure".into(),
        ..at.clone()
    };
    assert_eq!(landing(&unsure, &t, cells).state, "unsure");
    // 底下多了一格 ⇒ 落地，带那一格的标题。
    let mut grown = t.clone();
    grown.children.push("A1-7".into());
    let mut more = cells.clone();
    let mut extra = cell.clone();
    extra.id = "A1-7".into();
    extra.title = Some("甲的补丁".into());
    more.push(extra);
    let st = as_json(&landing(&at, &grown, &more));
    assert_eq!(st["state"], "landed");
    assert_eq!(st["by"], "child");
    assert_eq!(st["child"], json!({"id": "A1-7", "title": "甲的补丁"}));
    // 少了一格不算落地（被顶掉不是「照退回加了一格」）。
    let mut shrunk = t.clone();
    shrunk.children = vec!["A1-1".into()];
    assert_eq!(landing(&at, &shrunk, cells).state, "returned");
    // 正文改了 ⇒ 落地。
    let mut edited = t.clone();
    edited.body = Some("照 A2 的公约，另加一条。".into());
    let st = landing(&at, &edited, cells);
    assert_eq!(st.state, "landed");
    assert_eq!(st.by.as_deref(), Some("body"));
}

/// 顶块 `project` 也是退回的对象：标题是片名 · 子是顶层那几格 · 在长它的是顶块接手 · 没有签它的。
#[test]
fn the_top_block_is_a_target_shaped_from_the_slice() {
    let sl = slice_with(&dump("/w"), &all_working);
    let t = target(&sl, TOP_BLOCK).unwrap();
    assert_eq!(t.title.as_deref(), Some("alpha"));
    assert_eq!(t.children, sl.top);
    assert_eq!(t.owner.as_ref().map(|o| o.id.as_str()), Some(MAIN));
    assert_eq!((t.body, t.signer), (None, None));
    assert_eq!(target(&sl, "A1").unwrap().title.as_deref(), Some("甲功能"));
    assert_eq!(target(&sl, "A9"), None);
}

#[test]
fn the_line_carries_only_the_facts() {
    assert_eq!(
        line("A2-1", "长度换算实现", "  单位写错了\n改成米  "),
        "人 · A2-1 长度换算实现：单位写错了 改成米"
    );
}

/// 页头「判据 N 红」与「需手动」里判据红那几条口径一致：每条 `check.red` 恰一条 `red`（没认可时都算进 `needCount`）。
#[test]
fn every_red_check_is_exactly_one_red_need() {
    let sl = slice_with(&dump("/w"), &crate::plan::fixture::who);
    let reds = sl.check.red.len();
    let got = of_slice(&sl, None);
    assert_eq!(got.iter().filter(|n| n.kind == "red").count(), reds);
    assert!(reds > 0, "夹具里没有判据红 —— 本条空转");
}
