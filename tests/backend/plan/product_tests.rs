//! dump（形状 6，照 pb 0.5 的字段表写的合成那一份）⇒ 成品：倒过来的边与文件 · id 对到会话 · 收的值从领域表来 ·
//! 顶层进度与行尾短词照它数 · 待定 · 权力范围带交接说明 · 校验四样 · 「编号 标题」后端拼 · refs 的下标换成 UTF-16。

use super::*;
use crate::plan::fixture::{as_json, dump, dump_broken, who, MAIN, STRANGER, SUB};
use serde_json::json;

/// 成品的线上那一份（判据按键看）。
struct Made {
    doc: Value,
}

fn make(doc: &Value, who: WhoPort) -> Made {
    Made {
        doc: as_json(&super::make(doc, who, "r0".into(), 0)),
    }
}

fn made() -> Made {
    make(&dump("/w"), &who)
}

fn cell<'a>(doc: &'a Value, id: &str) -> &'a Value {
    cell_in(doc, 0, id)
}

fn cell_in<'a>(doc: &'a Value, slice: usize, id: &str) -> &'a Value {
    doc["slices"][slice]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("没有 {id}"))
}

fn node_mut<'a>(d: &'a mut Value, id: &str) -> &'a mut Value {
    d["repos"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == id)
        .unwrap()
}

/// 整份 dump 里没有 agent_view（点到一格再 `--node` 取）：成品也不带。
#[test]
fn the_product_carries_no_agent_view() {
    assert!(!made().doc.to_string().contains("agent_view"));
}

/// 顶上照 0.5 的键：`current_repo` ⇒ `repo` · `auto_continue` ⇒ `auto`；片从 `repos[]` 来。
#[test]
fn the_top_keys_come_from_shape_six() {
    let m = made();
    assert_eq!(m.doc["repo"], "alpha");
    assert_eq!(m.doc["auto"], false);
    let mut d = dump("/w");
    d["auto_continue"] = json!(true);
    d["current_repo"] = json!("beta");
    let m2 = make(&d, &who);
    assert_eq!(m2.doc["auto"], true);
    assert_eq!(m2.doc["slices"][0]["current"], false);
    assert_eq!(m2.doc["slices"][1]["current"], true);
    let names: Vec<&str> = m.doc["slices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["alpha", "beta"]);
}

/// 只收 permanent 那一种图：排期的节点不进格、不进倒排、不进 coveredBy，「编号 标题」照样进 `labels`。
#[test]
fn schedule_nodes_are_not_cells_but_keep_their_labels() {
    let m = made();
    let ids: Vec<&str> = m.doc["slices"][0]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(!ids.iter().any(|i| i.starts_with('P')), "{ids:?}");
    assert_eq!(ids.len(), 15);
    assert_eq!(cell(&m.doc, "A1")["pointedBy"]["to"], json!([]));
    assert_eq!(cell(&m.doc, "A2")["coveredBy"], json!(["A4"]));
    assert_eq!(m.doc["slices"][0]["labels"]["P3"], "P3 划出");
}

#[test]
fn edges_pointing_at_a_cell_are_indexed_backwards() {
    let m = made();
    assert_eq!(cell(&m.doc, "A1-1")["pointedBy"]["after"], json!(["A1-2"]));
    assert_eq!(
        cell(&m.doc, "A1-1")["pointedBy"]["with"],
        json!(["A1-3", "A1-4"])
    );
    assert_eq!(cell(&m.doc, "A1-2")["edges"]["after"], json!(["A1-1"]));
}

/// 文件照 `file_check` 给码（present · missing · empty · invalid ⇒ ok · missing · empty · broken）；同一份文件两格声明 ⇒ 各自写出另一格。
#[test]
fn files_carry_their_check_code_and_who_else_declares_them() {
    let m = made();
    assert_eq!(
        cell(&m.doc, "A1-1")["files"][0],
        json!({"path": "src/read.txt", "stateCode": "ok", "checkText": copy_core::copy_text("plan.fileState.ok", &[]), "note": "3 字", "alsoBy": []})
    );
    assert_eq!(cell(&m.doc, "A1-2")["files"][0]["stateCode"], "empty");
    assert_eq!(cell(&m.doc, "A1-3")["files"][0]["stateCode"], "broken");
    // 那一格写的字由后端照码写好（界面照抄，不认码）。
    assert_eq!(
        cell(&m.doc, "A1-3")["files"][0]["checkText"],
        copy_core::copy_text("plan.fileState.broken", &[])
    );
    assert_eq!(cell(&m.doc, "A1-3")["files"][0]["note"], "是个目录");
    assert_eq!(cell(&m.doc, "A7")["files"][0]["stateCode"], "missing");
    assert_eq!(
        cell(&m.doc, "A7")["files"][0]["note"],
        Value::Null,
        "missing 的空串不当一句话"
    );
    assert_eq!(cell(&m.doc, "A2")["files"][0]["alsoBy"], json!(["A2-1"]));
    for (w, c) in [
        ("present", "ok"),
        ("missing", "missing"),
        ("empty", "empty"),
        ("invalid", "broken"),
    ] {
        assert_eq!(as_json(&file_code(Some(w))), json!(c));
    }
    assert_eq!(file_code(Some("在")), None, "0.3.8 的中文原话不再认");
}

/// 归谁：`assignee` ⇒ owner（要改它说给的那一位）· `signer` 照旧 · `children_in` ⇒ block；都对到会话。
#[test]
fn assignee_and_signer_are_resolved_to_sessions() {
    let m = made();
    let owner = &cell(&m.doc, "A2-1")["owner"];
    assert_eq!(owner["kind"], "subagent");
    assert_eq!(owner["sid"], MAIN);
    assert_eq!(owner["id"], SUB);
    assert_eq!(owner["face"]["life"]["activity"], "needs_you");
    assert_eq!(owner["needs"], "approve");
    // 负责人是某个会话底下的 agent ⇒ 后端把「{父会话} 的 agent」写好交出去（界面照画，不按码取字）；会话本身 ⇒ 不写（界面写标签页的名字）。
    assert_eq!(
        owner["label"],
        copy_core::copy_text("plan.who.sub", &[("session", "甲会话")])
    );
    assert_eq!(cell(&m.doc, "A1-2")["owner"]["label"], Value::Null);
    assert_eq!(cell(&m.doc, "A2-1")["signer"]["id"], SUB);
    // 划出的根节点：底下归它自己那个权力范围，元数据写在上一级 ⇒ signer 是上一级的负责人。
    assert_eq!(cell(&m.doc, "A2")["block"], "A2");
    assert_eq!(cell(&m.doc, "A2")["owner"]["id"], SUB);
    assert_eq!(cell(&m.doc, "A2")["signer"]["id"], MAIN);
    assert_eq!(cell(&m.doc, "A1-2")["block"], "project");
    assert_eq!(cell_in(&m.doc, 1, "B1")["owner"], Value::Null);
    let mut d = dump("/w");
    node_mut(&mut d, "A1-2")["signer"] = json!(STRANGER);
    assert_eq!(
        make(&d, &who).doc["slices"][0]["cells"][2]["signer"]["kind"],
        "unknown"
    );
}

/// 收的值从领域表来（软件：update 规则加 · 去的「已签收」）；类许记它才算活：测试 · 验收记录 · 规矩 · 原话 · 待定 都是 `null`。
#[test]
fn the_done_value_comes_from_the_table_and_only_its_kinds_count() {
    let m = made();
    let sl = &m.doc["slices"][0];
    assert_eq!(sl["doneValue"], "已签收");
    assert_eq!(cell(&m.doc, "A1-1")["statusCode"], "done");
    assert_eq!(cell(&m.doc, "A1-2")["statusCode"], "open");
    assert_eq!(cell(&m.doc, "A1")["statusCode"], "open");
    for id in ["A1-3", "A1-4", "A1-5", "A3", "A4", "A5", "A6", "A6-1"] {
        assert_eq!(cell(&m.doc, id)["statusCode"], Value::Null, "{id}");
    }
    // 没签收的：A1 · A1-2 · A7（A2 等收下也算没签收）。
    assert_eq!(sl["openCells"], 4);
    // 顶层算活的：A1 · A2 · A7。
    assert_eq!(sl["progress"], json!({"done": 0, "of": 3}));
    // 表里换一个值来加 · 去 ⇒ 照那个值数（本族不写死「已签收」）。
    let mut d = dump("/w");
    for r in d["repos"][0]["rules"].as_array_mut().unwrap() {
        if r["arg"] == "已签收" {
            r["arg"] = json!("decided");
        }
    }
    let m2 = make(&d, &who);
    assert_eq!(m2.doc["slices"][0]["doneValue"], "decided");
    assert_eq!(cell(&m2.doc, "A6")["statusCode"], "done");
    assert_eq!(cell(&m2.doc, "A5")["statusCode"], "open");
    // 研究：表里没有加 · 去的值 ⇒ 没有进度，谁都不算活。
    let beta = &m.doc["slices"][1];
    assert_eq!(beta["doneValue"], Value::Null);
    assert_eq!(beta["progress"], Value::Null);
    assert_eq!(beta["openCells"], 0);
    assert_eq!(cell_in(&m.doc, 1, "B1-1")["statusCode"], Value::Null);
}

/// 状态原样一值一项带笔数，后端另写一行字（同 `pb render`）；依据它的照给。
#[test]
fn state_values_come_with_counts_a_text_line_and_evidence_links() {
    let m = made();
    assert_eq!(
        cell(&m.doc, "A1-1")["state"],
        json!([{"value": "已签收", "count": 2}])
    );
    assert_eq!(cell(&m.doc, "A1-1")["stateLine"], "已签收 2");
    assert_eq!(cell(&m.doc, "A1-1")["evidencedBy"], json!(["A1-4"]));
    assert_eq!(cell(&m.doc, "A2-1")["stateLine"], "已签收");
    assert_eq!(cell(&m.doc, "A1-2")["stateLine"], Value::Null);
    assert_eq!(cell_in(&m.doc, 1, "B1-1")["stateLine"], "支持");
    assert_eq!(
        state_line(&[
            PlanState {
                value: "已签收".into(),
                count: 1
            },
            PlanState {
                value: "decided".into(),
                count: 3
            }
        ])
        .as_deref(),
        Some("已签收 · decided 3")
    );
}

/// 行尾短词：叶子没签 · 底下 d/m · 交回等上一级（pending set 的代号 `source_delegated`）· 待定没人定。
#[test]
fn short_words_say_why_a_node_is_not_signed() {
    let m = made();
    let short = |k: &str, a: &[(&str, &str)]| copy_core::copy_text(k, a);
    assert_eq!(cell(&m.doc, "A1-2")["whyCode"], json!({"kind": "nosign"}));
    assert_eq!(
        cell(&m.doc, "A1-2")["whyShort"],
        json!({"text": short("plan.whyShort.nosign", &[]), "wait": false})
    );
    assert_eq!(
        cell(&m.doc, "A1")["whyCode"],
        json!({"kind": "inside", "done": 1, "of": 2})
    );
    assert_eq!(
        cell(&m.doc, "A1")["whyShort"]["text"],
        short("plan.whyShort.inside", &[("done", "1"), ("of", "2")])
    );
    assert_eq!(cell(&m.doc, "A2")["whyCode"], json!({"kind": "upper"}));
    assert_eq!(cell(&m.doc, "A2")["whyShort"]["wait"], true);
    assert_eq!(cell(&m.doc, "A1-1")["whyCode"], Value::Null);
    assert_eq!(
        cell(&m.doc, "A1-3")["whyCode"],
        Value::Null,
        "不算活的没有原因"
    );
    // 待定：还没 decided ⇒ 标出来、短词「待人定」；定了的不标。
    assert_eq!(cell(&m.doc, "A5")["undecided"], true);
    assert_eq!(cell(&m.doc, "A5")["whyCode"], json!({"kind": "undecided"}));
    assert_eq!(
        cell(&m.doc, "A5")["whyShort"]["text"],
        short("plan.whyShort.undecided", &[])
    );
    assert_eq!(cell(&m.doc, "A6")["undecided"], false);
    assert_eq!(cell(&m.doc, "A6")["whyCode"], Value::Null);
    // 交回那一条执行不了的原因不是「等上一级」⇒ 不标等收下。
    let mut d = dump("/w");
    d["repos"][0]["pending_sets"][1]["blocked_code"] = json!("rule");
    assert_eq!(
        cell(&make(&d, &who).doc, "A2")["whyCode"],
        json!({"kind": "inside", "done": 1, "of": 1})
    );
}

/// 权力范围从 `scopes[]` 来：根节点 · 负责人 · 站位 · 划出那一项还在不在 · 交接说明（不带认领码）· 给不了的原因；还没建的进 `unbuiltBlocks`。
#[test]
fn scopes_come_through_with_their_handoff() {
    let m = made();
    let b = &m.doc["slices"][0]["blocks"];
    assert_eq!(b[0]["id"], "project");
    assert_eq!(b[0]["cells"], json!(["project"]));
    assert_eq!(b[0]["owner"]["sid"], MAIN);
    assert_eq!(b[0]["at"], "A1-2");
    assert_eq!(b[0]["inside"], Value::Null, "project 不是图上的一格");
    assert!(b[0]["handoff"]
        .as_str()
        .unwrap()
        .contains("pb claim project <认领码>"));
    assert_eq!(b[1]["id"], "A2");
    assert_eq!(b[1]["phase"], "回看");
    assert_eq!(b[1]["listed"], true);
    assert_eq!(b[1]["inside"], json!({"done": 1, "of": 1}));
    assert_eq!(b[1]["handoffError"], Value::Null);
    let beta = &m.doc["slices"][1]["blocks"][0];
    assert_eq!(beta["handoff"], Value::Null);
    assert!(beta["handoffError"]
        .as_str()
        .unwrap()
        .contains("pb repo beta"));
    assert_eq!(
        m.doc["slices"][0]["unbuiltBlocks"],
        json!([{"id": "A7", "cells": ["A7"]}])
    );
}

/// 「新会话接手」那一格由后端判，只放码（字照旧在 handoff · handoffError）：
/// 划出了、没有负责人 ⇒ open；负责人那条会话停了 ⇒ open；交接说明拿不到 ⇒ no_line；负责人在跑 ⇒ running；
/// 负责人对不上这台的会话 ⇒ unsure（说不清停没停，不给）；划出那一项删了 ⇒ merging。
#[test]
fn each_scope_says_whether_a_new_session_may_take_it_over() {
    let takeover = |d: &Value, who: WhoPort, slice: usize, block: usize| {
        make(d, who).doc["slices"][slice]["blocks"][block]["takeover"].clone()
    };
    let d = dump("/w");
    // 负责人在跑（夹具里 MAIN · SUB 都活着、在等你）⇒ 不出。
    assert_eq!(takeover(&d, &who, 0, 0), "running");
    assert_eq!(takeover(&d, &who, 0, 1), "running");
    // 没有负责人、交接说明拿不到 ⇒ 出不了，写为什么。
    assert_eq!(takeover(&d, &who, 1, 0), "no_line");
    // 划出了、没有负责人 ⇒ 出，开口那句是 handoff。
    let mut free = dump("/w");
    free["repos"][0]["scopes"][1]["assignee"] = Value::Null;
    assert_eq!(takeover(&free, &who, 0, 1), "open");
    // 负责人那条会话停了（子 agent 看父会话）⇒ 出。
    let ended = |id: &str| match who(id) {
        crate::plan::Whose::Session { sid, .. } => crate::plan::Whose::Session { sid, live: None },
        crate::plan::Whose::Subagent {
            parent,
            parent_title,
            ..
        } => crate::plan::Whose::Subagent {
            parent,
            parent_title,
            live: None,
        },
        w => w,
    };
    assert_eq!(takeover(&d, &ended, 0, 1), "open");
    assert_eq!(takeover(&d, &ended, 0, 0), "open");
    // 负责人对不上这台 ⇒ 说不清。
    let mut stranger = dump("/w");
    stranger["repos"][0]["scopes"][1]["assignee"] = json!(STRANGER);
    assert_eq!(takeover(&stranger, &who, 0, 1), "unsure");
    // 划出那一项删了 ⇒ 不出（就算没有负责人）。
    free["repos"][0]["scopes"][1]["listed"] = json!(false);
    assert_eq!(takeover(&free, &who, 0, 1), "merging");
}

/// 还没执行的 sets：一片一张（带加 · 去与 pb 的原因、代号）· 每格自己写的那几条。
#[test]
fn pending_sets_carry_op_reason_and_code() {
    let m = made();
    let sl = &m.doc["slices"][0];
    assert_eq!(
        sl["pendingSets"][0],
        json!({"source": "A1-5", "target": "A1-2", "value": "已签收", "op": "add", "blockedReason": null, "blockedCode": null})
    );
    assert_eq!(sl["pendingSets"][1]["blockedCode"], "source_delegated");
    assert!(sl["pendingSets"][1]["blockedReason"]
        .as_str()
        .unwrap()
        .contains("还划出着"));
    assert_eq!(
        cell(&m.doc, "A1-5")["sets"],
        json!([{"target": "A1-2", "value": "已签收", "op": "add"}])
    );
    assert_eq!(cell(&m.doc, "A1-2")["sets"], json!([]));
    let mut d = dump("/w");
    node_mut(&mut d, "A1-5")["edges"]["sets"][0]["op"] = json!("remove");
    assert_eq!(cell(&make(&d, &who).doc, "A1-5")["sets"][0]["op"], "remove");
}

/// 校验四样：读不成的几处（哪一份 · 第几行 · 为什么）· 判不了的节点 · 不通过（该哪个权力范围改）· 判不了的规则。
#[test]
fn the_check_carries_read_errors_unparsed_nodes_failures_and_unknowns() {
    let m = made();
    let c = &m.doc["slices"][0]["check"];
    assert_eq!(c["unreadable"][0]["file"], ".planned-build/alpha/图.md");
    assert_eq!(c["unreadable"][0]["line"], 71);
    assert_eq!(c["unparsed"], json!(["A8"]));
    assert_eq!(c["red"][0]["rule"], "边指着图上的节点");
    assert_eq!(c["red"][0]["block"], "project");
    assert!(c["red"][0]["what"].as_str().unwrap().contains("A9"));
    assert_eq!(
        c["red"][0]["refs"],
        json!([{"start": 0, "end": 4, "ids": ["A1-2"]}])
    );
    assert_eq!(
        c["undecidable"],
        json!([{"rule": "无主文件", "why": "beta 的仓库不在"}])
    );
}

/// 无主文件：`paths` 照给；仓库读不出 ⇒ `paths` null、`why` 是 pb 的 `error`。
#[test]
fn unowned_files_and_why_they_cannot_be_told() {
    let m = made();
    assert_eq!(
        m.doc["slices"][0]["unowned"],
        json!({"paths": ["src/stray.txt"], "why": null})
    );
    assert_eq!(
        m.doc["slices"][1]["unowned"],
        json!({"paths": null, "why": "仓库 beta 不在盘上"})
    );
}

/// 领域与类：叫法从 `edge_labels` 取，阶段说明从 `description` 取。
#[test]
fn kinds_and_phases_take_the_shape_six_keys() {
    let m = made();
    let sl = &m.doc["slices"][0];
    assert_eq!(sl["kinds"][1]["name"], "模块");
    assert_eq!(sl["kinds"][1]["edgeWords"]["with"], "实现");
    assert_eq!(sl["kinds"][1]["values"], json!(["已签收", "decided"]));
    assert_eq!(
        sl["phases"][3],
        json!({"name": "回看", "does": "回看整体，决定下一步"})
    );
    let names: Vec<&str> = m.doc["domains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["研究", "软件"]);
    assert_eq!(m.doc["domains"][1]["kinds"][2]["edgeWords"]["to"], "测");
    let text = m.doc.to_string();
    for k in [
        "edge_labels",
        "replaced_by",
        "pending_sets",
        "covered_by",
        "plan_only",
        "blocked_reason",
        "evidenced_by",
    ] {
        assert!(!text.contains(k), "{k}");
    }
}

/// 每格带画哪一种（`look`）与右端那一句（`end`：状态那一行 ＋ 原因短词，用分隔连好）：两端照摆，不另判。
#[test]
fn every_cell_carries_its_look_and_end_line() {
    let m = made();
    let sep = copy_core::copy_text("kit.text.sep", &[]);
    let short = |k: &str, a: &[(&str, &str)]| copy_core::copy_text(k, a);
    assert_eq!(cell(&m.doc, "A1-1")["look"], "done");
    assert_eq!(cell(&m.doc, "A1-1")["end"], "已签收 2");
    assert_eq!(cell(&m.doc, "A1-2")["look"], "open");
    assert_eq!(
        cell(&m.doc, "A1-2")["end"],
        short("plan.whyShort.nosign", &[])
    );
    assert_eq!(
        cell(&m.doc, "A5")["look"],
        "undecided",
        "待定没人定另画一种"
    );
    assert_eq!(
        cell(&m.doc, "A6")["look"],
        "none",
        "类不许记收的值 ⇒ 不算活"
    );
    assert_eq!(cell(&m.doc, "A1-3")["look"], "none");
    let mut d = dump("/w");
    node_mut(&mut d, "A1")["state"] = json!([{"value": "decided", "count": 1}]);
    assert_eq!(
        cell(&make(&d, &who).doc, "A1")["end"],
        format!(
            "decided{sep}{}",
            short("plan.whyShort.inside", &[("done", "1"), ("of", "2")])
        ),
        "状态那一行与原因都有 ⇒ 用分隔连"
    );
    assert_eq!(cell_in(&m.doc, 1, "B1-1")["end"], "支持");
}

/// 状态那一节什么都没有时那一句（`quiet`）：算活、底下有格 ⇒ 状态跟着底下走；别的 ⇒ 没记状态。
/// 记着值 · 有指着它没执行的 sets ⇒ 不出（那一节有东西可摆）。
#[test]
fn a_cell_with_nothing_in_its_state_says_why() {
    let m = made();
    let t = |k: &str| copy_core::copy_text(k, &[]);
    assert_eq!(cell(&m.doc, "A1")["quiet"], t("plan.state.inner"));
    assert_eq!(
        cell(&m.doc, "A3")["quiet"],
        t("plan.state.none"),
        "不算活的"
    );
    assert_eq!(cell(&m.doc, "A1-1")["quiet"], Value::Null, "记着值");
    assert_eq!(cell(&m.doc, "A1-2")["stateLine"], Value::Null);
    assert_eq!(
        cell(&m.doc, "A1-2")["quiet"],
        Value::Null,
        "有指着它没执行的 sets"
    );
    // 同一格把那一条 sets 拿掉 ⇒ 没记状态。
    let mut d = dump("/w");
    d["repos"][0]["pending_sets"] = json!([]);
    assert_eq!(
        cell(&make(&d, &who).doc, "A1-2")["quiet"],
        t("plan.state.none")
    );
}

/// covers：写的照给，coveredBy 是 pb 算的（只留图上的格）。
#[test]
fn every_cell_carries_its_covers() {
    let m = made();
    assert_eq!(cell(&m.doc, "A3")["covers"], json!(["A1"]));
    assert_eq!(cell(&m.doc, "A4")["covers"], json!(["project"]));
    assert_eq!(cell(&m.doc, "A1-2")["coveredBy"], json!(["A3", "A4"]));
}

/// 凡出现编号界面一律写「编号 标题」：后端拼好一张表（图上每个节点 ＋ 归档里的）。
#[test]
fn every_slice_has_a_label_per_id_built_by_the_backend() {
    let m = made();
    let labels = &m.doc["slices"][0]["labels"];
    assert_eq!(labels["A1"], "A1 甲功能");
    assert_eq!(labels["A2-1"], "A2-1 乙的一半");
    assert_eq!(labels.as_object().unwrap().len(), 21);
    let mut d = dump("/w");
    d["repos"][0]["archived"] =
        json!([{"id": "A9", "title": "旧的", "kind": "能力", "replaced_by": "A7"}]);
    let m = make(&d, &who);
    assert_eq!(m.doc["slices"][0]["labels"]["A9"], "A9 旧的");
    assert_eq!(m.doc["slices"][0]["archived"][0]["replacedBy"], "A7");
    let mut d = dump("/w");
    node_mut(&mut d, "A1")["title"] = json!("");
    assert_eq!(make(&d, &who).doc["slices"][0]["labels"]["A1"], "A1");
}

/// pb 的 `start · end` 按 Unicode 码位；界面（JS）按 UTF-16 下标切 ⇒ 后端换好。
#[test]
fn ref_offsets_are_turned_from_code_points_into_utf16_units() {
    assert_eq!(utf16_at("ab", 1), 1);
    assert_eq!(utf16_at("😀A1", 1), 2);
    assert_eq!(utf16_at("x", 9), 1, "越界停在末尾");
    let m = made();
    assert_eq!(
        cell(&m.doc, "A1")["refs"]["body"],
        json!([{"start": 2, "end": 4, "ids": ["A3"]}, {"start": 15, "end": 21, "ids": ["A1-1", "A1-2"]}])
    );
    let mut d = dump("/w");
    node_mut(&mut d, "A1")["body"] = json!("😀照 A3 的公约");
    node_mut(&mut d, "A1")["refs"]["body"] = json!([{"start": 3, "end": 5, "ids": ["A3"]}]);
    d["repos"][0]["check"]["failures"][0]["message"] = json!("😀A1-2 --to--> A9");
    d["repos"][0]["check"]["failures"][0]["refs"] =
        json!([{"start": 1, "end": 5, "ids": ["A1-2"]}]);
    let m = make(&d, &who);
    let r = &cell(&m.doc, "A1")["refs"]["body"][0];
    assert_eq!((r["start"].as_u64(), r["end"].as_u64()), (Some(4), Some(6)));
    let red = &m.doc["slices"][0]["check"]["red"][0]["refs"][0];
    assert_eq!(
        (red["start"].as_u64(), red["end"].as_u64()),
        (Some(2), Some(6))
    );
}

#[test]
fn a_broken_slice_comes_out_as_name_domain_and_pbs_reason() {
    let m = make(&dump_broken("/w"), &who);
    let sl = &m.doc["slices"][0];
    assert_eq!(sl["name"], "alpha");
    assert_eq!(sl["error"], "图.md 第 3 行：元数据缺 id");
    assert_eq!(sl["bare"], true);
    assert_eq!(sl["cells"], json!([]));
    assert_eq!(sl["labels"], json!({}));
    assert_eq!(sl["openCells"], 0);
    assert_eq!(sl["unowned"], json!({"paths": null, "why": null}));
}

/// 会话头那一枚标要的反查表：会话 ⇒ 它负责的那个权力范围。自己负责的压过子 agent 替它负责的。
#[test]
fn by_session_maps_each_owner_session_to_its_scope_preferring_its_own_claim() {
    let m = made();
    let sep = copy_text("kit.text.sep", &[]);
    let top = copy_text("plan.head.top", &[]);
    let said = |name: &str, phase: &str| {
        let mark = ["alpha", name, phase].join(&sep);
        json!({"name": name, "mark": mark})
    };
    assert_eq!(
        m.doc["bySession"],
        json!({MAIN: {"slice": "alpha", "block": "project", "cell": "project", "title": null, "top": true, "phase": "开发", "at": "A1-2", "atTitle": "甲的写出", "via": "session",
                      "said": said(&top, "开发")}})
    );
    let mut d = dump("/w");
    d["repos"][0]["scopes"][0]["assignee"] = Value::Null;
    assert_eq!(
        make(&d, &who).doc["bySession"][MAIN],
        json!({"slice": "alpha", "block": "A2", "cell": "A2", "title": "乙功能", "top": false, "phase": "回看", "at": "A2-1", "atTitle": "乙的一半", "via": "subagent",
               "said": said("乙功能", "回看")})
    );
    let mut d = dump("/w");
    for b in [0, 1] {
        d["repos"][0]["scopes"][b]["assignee"] = json!(STRANGER);
    }
    assert_eq!(make(&d, &who).doc["bySession"], json!({}));
}

/// 时刻由后端写成给人看的字（界面照抄）；权力范围的 `at`（站在哪一格的编号）不是时刻 ⇒ 不添。
#[test]
fn time_fields_get_their_text_beside_them_and_cell_ids_do_not() {
    let now = 1_767_323_400_000_i64; // 2026-01-02T03:10:00Z
    let ms = |d: i64| u64::try_from(now - d).unwrap();
    let mut r = super::make(&dump("/w"), &who, "r0".into(), ms(60_000));
    r.stale = Some(cmd_wire::plan::PlanStale {
        said: Some("x".into()),
        raw: None,
        since: ms(86_400_000),
        since_text: None,
    });
    r.slices[0].cells[1].returned = Some(cmd_wire::plan::PlanReturned {
        at: ms(3_600_000),
        at_text: None,
        to: None,
        state: "returned".into(),
        by: None,
        child: None,
    });
    let mut shanghai = r.clone();
    with_time_texts(&mut r, now, &Default::default());
    let v = as_json(&r);
    assert_eq!(v["readAtText"], "03:09");
    assert_eq!(v["stale"]["sinceText"], "01-01 03:10");
    assert_eq!(v["slices"][0]["cells"][1]["returned"]["atText"], "02:10");
    assert!(v["slices"][0]["blocks"][1].get("atText").is_none());
    shanghai.read_at = u64::try_from(now).unwrap();
    with_time_texts(
        &mut shanghai,
        now,
        &crate::Tz::named("Asia/Shanghai").unwrap(),
    );
    assert_eq!(shanghai.read_at_text.as_deref(), Some("11:10"));
}

/// 每片的仓库位置照 pb 给的 `path`（相对工作区根），不拿名字补；读不成的也带；只做计划照 `plan_only`。
#[test]
fn every_slice_carries_pbs_repo_path_and_plan_only_as_given() {
    assert_eq!(made().doc["slices"][0]["path"], "repos/alpha");
    assert_eq!(made().doc["slices"][1]["path"], "beta");
    let at = make(&crate::plan::fixture::dump_at("/w", "x/y"), &who);
    assert_eq!(at.doc["slices"][0]["path"], "x/y");
    assert_eq!(
        make(&dump_broken("/w"), &who).doc["slices"][0]["path"],
        "repos/alpha"
    );
    let mut d = dump("/w");
    d["repos"][0].as_object_mut().unwrap().remove("path");
    assert_eq!(make(&d, &who).doc["slices"][0]["path"], Value::Null);
    assert_eq!(made().doc["slices"][0]["docsOnly"], false);
    assert_eq!(made().doc["slices"][1]["docsOnly"], true);
}

/// pb 那一句读不成的原因不是字：照它的原文写进 `error`，那一片照常出。
#[test]
fn a_reason_that_is_not_text_still_comes_through_as_its_raw_words() {
    let mut d = dump_broken("/w");
    d["repos"][0]["error"] = json!({"line": 3});
    d["domains"][0]["error"] = json!(7);
    let m = super::make(&d, &who, "r0".into(), 0);
    assert_eq!(m.slices[0].error.as_deref(), Some(r#"{"line":3}"#));
    assert!(m.slices[0].bare);
    assert_eq!(m.slices[1].name, "beta");
    assert_eq!(m.domains[0].error.as_deref(), Some("7"));
}
