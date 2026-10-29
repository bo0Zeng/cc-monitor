//! 要你看四种的判定：顶层全签收 · 校验不通过 · 负责人的会话停了而根节点没签收 · 负责人的会话在等你；键带条目版本；计数不含「问人」那一种。

use super::*;
use crate::plan::fixture::{as_json, dump, dump_top_signed, made_with, MAIN, STRANGER, SUB};
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
    of_slice(sl, entered, &nobody_waits)
        .iter()
        .map(as_json)
        .collect()
}

/// 活会话表说谁都不在等人。
fn nobody_waits(_: &str) -> bool {
    false
}

/// 活会话表说主会话（也是子 agent 的父会话）在等人。
fn main_waits(sid: &str) -> bool {
    sid == MAIN
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
            parent_title: "甲会话".into(),
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
            parent_title: "甲会话".into(),
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
fn a_fully_signed_top_level_is_one_need_keyed_by_the_rev_it_got_there() {
    let d = dump_top_signed("/w");
    let sl = slice_with(&d, &all_working);
    assert!(top_done(&sl));
    let n = ofs(&sl, Some("r7"));
    let top: Vec<_> = n.iter().filter(|x| x["kind"] == "top").collect();
    assert_eq!(top.len(), 1);
    assert_eq!(top[0]["key"], "top:r7");
    assert_eq!(top[0]["block"], "project");
    assert_eq!(top[0]["cell"], "project");
    assert_eq!(top[0]["acked"], false);
    // 那一次的 rev 没给（还不知道）⇒ 不出这一条，不拿空键凑。
    assert!(ofs(&sl, None).iter().all(|x| x["kind"] != "top"));
}

/// 顶层还有没签收的 · 没有收的值（研究）· 顶层一个算活的都没有 ⇒ 不出 `top`；阶段叫什么不看。
#[test]
fn a_top_level_not_fully_signed_is_no_need_whatever_the_phase() {
    let mut d = dump("/w");
    d["repos"][0]["scopes"][0]["phase"] = json!("回看");
    let sl = slice_with(&d, &all_working);
    assert!(!top_done(&sl));
    assert!(ofs(&sl, Some("r7")).iter().all(|x| x["kind"] != "top"));
    let beta = made_with(&dump("/w"), &all_working).slices.remove(1);
    assert!(!top_done(&beta));
    let mut d = dump("/w");
    d["repos"][0]["top_level"] = json!(["A3", "A4"]);
    assert!(!top_done(&slice_with(&d, &all_working)), "0/0 不算全签收");
}

#[test]
fn every_failure_is_one_need_keyed_by_rule_and_scope_at_the_scopes_root_cell() {
    let mut d0 = dump("/w");
    d0["repos"][0]["check"]["failures"][0]["scope"] = json!("A2");
    let sl = slice_with(&d0, &all_working);
    let n = ofs(&sl, None);
    let red: Vec<_> = n.iter().filter(|x| x["kind"] == "red").collect();
    assert_eq!(red.len(), 1);
    assert_eq!(red[0]["key"], "red:边指着图上的节点@A2");
    assert_eq!(red[0]["block"], "A2");
    assert_eq!(red[0]["cell"], "A2");
    assert_eq!(red[0]["red"], 0, "指回 check.red 的第几条");
    // 两条：各出一条；`message` 变了键不变（认可仍作数），规则或权力范围换了键就换。
    let mut d = dump("/w");
    d["repos"][0]["check"]["failures"] = json!([
        {"rule": "悬空", "message": "A1-2 指着 A8", "scope": "A2", "fix": "x", "refs": []},
        {"rule": "无主", "message": "src/x 没人管", "scope": null, "fix": "y", "refs": []}
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
fn an_ended_owner_of_an_unsigned_scope_is_a_need_and_a_subagent_counts_by_its_parent() {
    let sl = slice_with(&dump("/w"), &all_ended);
    let n = ofs(&sl, None);
    let ended: Vec<_> = n.iter().filter(|x| x["kind"] == "ended").collect();
    // project（主会话停了、顶层没全签收）· A2（子 agent 负责，父会话也停了）。
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
fn an_ended_owner_of_a_signed_scope_or_an_unknown_owner_is_no_need() {
    // A2 的根节点签收了；顶层也全签收了（project 那一条看顶层进度）。
    let d = dump_top_signed("/w");
    let n = ofs(&slice_with(&d, &all_ended), None);
    assert!(n.iter().all(|x| x["kind"] != "ended"), "{n:?}");
    // 对不上的 id：说不清停没停 ⇒ 不出。
    let n = ofs(&slice_with(&dump("/w"), &|_: &str| Whose::Unknown), None);
    assert!(n.iter().all(|x| x["kind"] != "ended"));
}

#[test]
fn an_owner_waiting_for_you_is_an_ask_need_at_the_cell_the_block_stands_on() {
    // 在不在等人只问活会话表（脸上说在跑也一样）：主会话在等；子 agent 的父会话就是它。
    let sl = slice_with(&dump("/w"), &all_working);
    let n: Vec<Value> = of_slice(&sl, None, &main_waits)
        .iter()
        .map(as_json)
        .collect();
    let ask: Vec<_> = n.iter().filter(|x| x["kind"] == "ask").collect();
    assert_eq!(ask.len(), 2);
    let b = ask.iter().find(|x| x["block"] == "A2").unwrap();
    assert_eq!(b["cell"], "A2-1", "A2 站在 A2-1");
    assert_eq!(b["sid"], MAIN);
    assert_eq!(b["key"], format!("ask:A2@{SUB}"));
    let p = ask.iter().find(|x| x["block"] == "project").unwrap();
    assert_eq!(p["cell"], "A1-2", "project 站在 A1-2");
    // 表说谁都不在等 ⇒ 不问人（脸上说在等也一样）。
    let sl = slice_with(&dump("/w"), &crate::plan::fixture::who);
    assert!(ofs(&sl, None).iter().all(|x| x["kind"] != "ask"));
}

#[test]
fn acks_mark_needs_and_the_count_skips_acked_ones_and_asks() {
    let d = dump_top_signed("/w");
    let mut doc = made_with(&d, &crate::plan::fixture::who);
    doc.slices[0].needs = of_slice(&doc.slices[0], Some("r1"), &main_waits);
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
        children: (1..=5).map(|i| format!("A1-{i}")).collect(),
        body: body_digest(cell.body.as_deref()),
    };
    let cells = &sl.cells;
    let st = as_json(&landing(&at, &t, cells));
    assert_eq!(st["state"], "returned");
    // 送给谁照线上那一形（与在长它的 · 签它的同形）：记下的 sid 对到这一格此刻的那一位。
    assert_eq!(st["to"], as_json(&cell.owner));
    assert_eq!(st["to"]["sid"], MAIN);
    assert_eq!(st["at"], 5);
    // 这一格此刻已经没有那一位（负责人换了人）⇒ 说不出是谁。
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
    grown.children.push("A1-6".into());
    let mut more = cells.clone();
    let mut extra = cell.clone();
    extra.id = "A1-6".into();
    extra.title = Some("甲的补丁".into());
    more.push(extra);
    let st = as_json(&landing(&at, &grown, &more));
    assert_eq!(st["state"], "landed");
    assert_eq!(st["by"], "child");
    assert_eq!(st["child"], json!({"id": "A1-6", "title": "甲的补丁"}));
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

/// `project` 也是退回的对象：标题是仓库名 · 子是顶层那几格 · 负责人是 project 的负责人 · 没有签它的。
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

/// 页头「N 条不通过」与「需手动」里那几条口径一致：每条 `check.red` 恰一条 `red`（没认可时都算进 `needCount`）。
#[test]
fn every_red_check_is_exactly_one_red_need() {
    let sl = slice_with(&dump("/w"), &crate::plan::fixture::who);
    let reds = sl.check.red.len();
    let got = of_slice(&sl, None, &nobody_waits);
    assert_eq!(got.iter().filter(|n| n.kind == "red").count(), reds);
    assert!(reds > 0, "夹具里没有不通过的 —— 本条空转");
}

/// ★ 每条还要人看的落在哪一格（`open_cells`）：与 `count_slice` 同一判（没认可 · 不算问人），先后照 `needs`；
/// 落在图上的一格 ⇒ 那一格的编号与后端写好的「编号 标题」；顶块没有根格 · 红灯不在块里 ⇒ 没有格。
#[test]
fn each_need_still_open_lands_on_its_cell_labelled_by_the_backend() {
    let mut d = dump("/w");
    d["repos"][0]["check"]["failures"] = json!([
        {"rule": "悬空", "message": "x", "scope": "A2", "fix": "x", "refs": []},
        {"rule": "无主", "message": "y", "scope": null, "fix": "y", "refs": []},
        {"rule": "又一条", "message": "z", "scope": "A2", "fix": "z", "refs": []}
    ]);
    let a2 = Some(cmd_wire::needs::NeedsCell {
        id: "A2".into(),
        label: "A2 乙功能".into(),
    });
    // 停了：红灯三条（第三条认可过）· project 停了（顶块的根不是图上的格）· A2 停了。
    let mut sl = slice_with(&d, &all_ended);
    sl.needs = of_slice(&sl, None, &nobody_waits);
    for x in &mut sl.needs {
        x.acked = x.key == "red:又一条@A2";
    }
    let got = open_cells(&sl);
    assert_eq!(
        got,
        vec![a2.clone(), None, None, a2.clone()],
        "{:?}",
        sl.needs
    );
    assert_eq!(usize::try_from(count_slice(&sl)).unwrap(), got.len());
    // 在等人：问人那两条不在里头（由会话那一侧数）。
    let mut sl = slice_with(&d, &all_working);
    sl.needs = of_slice(&sl, None, &main_waits);
    assert_eq!(open_cells(&sl), vec![a2.clone(), None, a2]);
}

/// ★ 这台要人看的每一条（`sessions-needs` 计划那一截）只经 [`need_items`] 从几份成品摊开：照成品 · 片 · `open_cells` 的先后，
/// 各带工作区 · 片名 · 那一格；没有要看的片不出。
#[test]
fn need_items_spread_every_open_need_with_its_workspace_slice_and_cell() {
    let mut d = dump("/w");
    d["repos"][0]["check"]["failures"][0]["scope"] = json!("A2");
    let mut doc = made_with(&d, &all_working);
    for sl in &mut doc.slices {
        sl.needs = of_slice(sl, None, &nobody_waits);
    }
    let got: Vec<(String, String, Option<String>)> = need_items(std::slice::from_ref(&doc))
        .into_iter()
        .map(|n| (n.workspace, n.slice, n.cell.map(|c| c.label)))
        .collect();
    assert_eq!(
        got,
        vec![("/w".into(), "alpha".into(), Some("A2 乙功能".into()))],
        "beta 那一片没有要看的 ⇒ 不出"
    );
    assert!(need_items(&[]).is_empty());
}
