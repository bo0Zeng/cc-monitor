//! dump（形状 4，pb 0.3.8 对合成工作区真跑的）⇒ 成品：倒过来的边与文件 · id 对到会话 · 顶层进度（凭据不进分母）·
//! 0.3.8 新给的那几样照搬成线上惯例 · 「编号 标题」后端拼 · refs 的下标换成 UTF-16。

use super::*;
use crate::plan::fixture::{dump, dump_broken, who, MAIN, STRANGER, SUB};

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
        .unwrap()
}

/// 整份 dump 里没有 agent_view（0.3.8 起点到一格再 `--cell` 取）：成品也不带、不给「有没有」那一格。
#[test]
fn the_product_carries_no_agent_view_and_no_has_view_flag() {
    let m = made();
    let text = m.doc.to_string();
    assert!(!text.contains("agent_view") && !text.contains("hasView"));
}

#[test]
fn edges_pointing_at_a_cell_are_indexed_backwards() {
    let m = made();
    assert_eq!(cell(&m.doc, "A1-1")["pointedBy"]["after"], json!(["A1-2"]));
    assert_eq!(
        cell(&m.doc, "A1")["pointedBy"]["with"],
        json!(["A1-1", "A1-2", "A3", "A4"])
    );
    assert_eq!(cell(&m.doc, "A1-2")["edges"]["after"], json!(["A1-1"]));
}

#[test]
fn a_file_declared_by_two_cells_names_the_other_one() {
    let mut d = dump("/w");
    d["slices"][0]["cells"][2]["files"] =
        json!([{"path": "src/read.txt", "state": "在", "note": "3 字"}]);
    let m = make(&d, &who);
    assert_eq!(cell(&m.doc, "A1-1")["files"][0]["alsoBy"], json!(["A1-2"]));
    assert_eq!(cell(&m.doc, "A1-2")["files"][0]["alsoBy"], json!(["A1-1"]));
    assert_eq!(cell(&made().doc, "A1-2")["files"][0]["state"], "缺");
}

#[test]
fn owners_signers_and_sign_executors_are_resolved_to_sessions() {
    let m = made();
    let owner = &cell(&m.doc, "A2-1")["owner"];
    assert_eq!(owner["kind"], "subagent");
    assert_eq!(owner["sid"], MAIN);
    assert_eq!(owner["id"], SUB);
    assert_eq!(owner["face"]["live"]["activity"], "needs_you");
    assert_eq!(owner["needs"], "approve");
    let signs = &cell(&m.doc, "A1-1")["signs"];
    assert_eq!(signs[1]["by"]["kind"], "unknown");
    assert_eq!(signs[1]["by"]["id"], STRANGER);
    // signer ＝ 谁的 update 执行得了收下它的 sets：A2-1 的上一级格在 A2 那一块 ⇒ 那一块的接手（子 agent）。
    assert_eq!(cell(&m.doc, "A2-1")["signer"]["id"], SUB);
    assert_eq!(cell(&m.doc, "A1-2")["signer"]["kind"], "session");
    // 凭据类的 · 有子的格：pb 给 null。
    assert_eq!(cell(&m.doc, "A1-3")["signer"], Value::Null);
    assert_eq!(cell(&m.doc, "A1")["signer"], Value::Null);
    let blocks = &m.doc["slices"][0]["blocks"];
    assert_eq!(blocks[0]["owner"]["sid"], MAIN);
    assert_eq!(blocks[1]["at"], "A2-1");
}

/// 签收多了值 · 凭 · 这一笔算不算做完；早先 `pb sign` 那一笔：值收下、凭 null、理由照给。
#[test]
fn signs_carry_value_via_and_done() {
    let m = made();
    let s = &cell(&m.doc, "A1-1")["signs"];
    assert_eq!(
        (&s[0]["value"], &s[0]["via"], &s[0]["done"]),
        (&json!("收下"), &Value::Null, &json!(true))
    );
    assert_eq!(s[0]["reason"], "第一次，照 A1 看过");
    assert_eq!(
        (&s[1]["value"], &s[1]["via"]),
        (&json!("收下"), &json!("A1-5"))
    );
    // 研究那一片：值「正」记了，不算做完。
    let b = &cell_in(&m.doc, 1, "B1-2")["signs"][0];
    assert_eq!((&b["value"], &b["done"]), (&json!("正"), &json!(false)));
    assert_eq!(cell_in(&m.doc, 1, "B1-2")["counts"], "正 1（B1-1）");
    assert_eq!(cell(&m.doc, "A1-1")["counts"], Value::Null);
}

/// 凭据类的格：标出来、码是 `evidence`、不进任何一个分母（顶层进度 · 块根格的子 · 没做完的格数）。
#[test]
fn evidence_cells_are_marked_and_left_out_of_every_count() {
    let m = made();
    let sl = &m.doc["slices"][0];
    for id in ["A1-3", "A1-4", "A1-5", "A1-6"] {
        assert_eq!(cell(&m.doc, id)["evidence"], true, "{id}");
        assert_eq!(cell(&m.doc, id)["statusCode"], "evidence", "{id}");
        assert_eq!(cell(&m.doc, id)["whyShort"], Value::Null, "{id}");
    }
    assert_eq!(cell(&m.doc, "A1-2")["evidence"], false);
    // 没做完的：A1 · A1-2 · A2 · A2-1 · A3 · A4 · A5（A1-1 做完了；四格凭据不算）。
    assert_eq!(sl["openCells"], 7);
    assert_eq!(sl["progress"], json!({"done": 0, "open": 5, "dropped": 0}));
    // 一个块根格底下挂着凭据：不进 d/m。
    let mut d = dump("/w");
    d["slices"][0]["blocks"][1]["cells"] = json!(["A1"]);
    assert_eq!(
        make(&d, &who).doc["slices"][0]["blocks"][1]["inside"],
        json!({"done": 1, "of": 2})
    );
    // 顶层一格是凭据：不进顶层进度。
    let mut d = dump("/w");
    d["slices"][0]["cells"][10]["evidence"] = json!(true);
    d["slices"][0]["cells"][10]["status"] = d["slices"][0]["cells"][3]["status"].clone();
    assert_eq!(
        make(&d, &who).doc["slices"][0]["progress"],
        json!({"done": 0, "open": 4, "dropped": 0})
    );
}

/// 0.3.8 新给的那几样照搬，键换成小驼峰。
#[test]
fn the_new_slice_and_top_level_fields_come_through_in_wire_keys() {
    let m = made();
    let sl = &m.doc["slices"][0];
    assert_eq!(sl["unbuiltBlocks"], json!([{"id": "A5", "cells": ["A5"]}]));
    assert_eq!(
        sl["unowned"],
        json!({"paths": ["src/stray.txt"], "why": null})
    );
    assert_eq!(
        sl["unsplit"],
        json!([{"id": "A2", "path": "src/shared.txt"}])
    );
    assert_eq!(
        sl["pendingSets"][0],
        json!({"source": "A1-4", "target": "A1-2", "value": "收下", "whyNot": null})
    );
    assert!(sl["pendingSets"][1]["whyNot"]
        .as_str()
        .unwrap()
        .contains("两头落进同一块"));
    assert_eq!(sl["records"][0]["n"], 1);
    assert_eq!(sl["records"][0]["summary"], "读写分两半");
    assert_eq!(
        sl["records"][0]["quotes"],
        json!(["「分开做」", "「先读后写」"])
    );
    assert_eq!(sl["records"][0]["by"]["kind"], "unknown");
    assert_eq!(sl["kinds"][2]["evidence"], true);
    assert_eq!(sl["kinds"][2]["values"], json!(["收下"]));
    assert_eq!(
        sl["check"]["red"][0]["refs"],
        json!([{"start": 0, "end": 4, "ids": ["A1-2"]}])
    );
    // 仓库读不出：paths null、why 照给（判不了，不是一份都没有）。
    let beta = &m.doc["slices"][1];
    assert_eq!(beta["unowned"]["paths"], Value::Null);
    assert!(beta["unowned"]["why"].as_str().is_some());
    let names: Vec<&str> = m.doc["domains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["小说", "研究", "软件"]);
    assert_eq!(m.doc["domains"][2]["kinds"][2]["edgeWords"]["with"], "测");
    assert_eq!(m.doc["domains"][2]["phases"][3]["marks"], json!(["看全局"]));
    let text = m.doc.to_string();
    for k in [
        "edge_words",
        "replaced_by",
        "unbuilt_blocks",
        "pending_sets",
        "why_not",
        "covered_by",
        "docs_only",
    ] {
        assert!(!text.contains(k), "{k}");
    }
}

/// 每格新给的：住哪一块 · covers（写的）· coveredBy（pb 算的）· 没执行的 sets。
#[test]
fn every_cell_carries_block_covers_and_its_own_pending_sets() {
    let m = made();
    assert_eq!(cell(&m.doc, "A2-1")["block"], "A2");
    assert_eq!(cell(&m.doc, "A1-2")["block"], "project");
    assert_eq!(cell(&m.doc, "A3")["covers"], json!(["A1"]));
    assert_eq!(cell(&m.doc, "A4")["covers"], json!(["project"]));
    assert_eq!(cell(&m.doc, "A1-2")["coveredBy"], json!(["A3", "A4"]));
    assert_eq!(
        cell(&m.doc, "A1-4")["sets"],
        json!([{"target": "A1-2", "value": "收下"}])
    );
    assert_eq!(cell(&m.doc, "A1-2")["sets"], json!([]));
}

/// 凡出现编号界面一律写「编号 标题」：后端拼好一张表（活图上的格 ＋ 归档里的）。
#[test]
fn every_slice_has_a_label_per_id_built_by_the_backend() {
    let m = made();
    let labels = &m.doc["slices"][0]["labels"];
    assert_eq!(labels["A1"], "A1 甲功能");
    assert_eq!(labels["A2-1"], "A2-1 乙的一半");
    assert_eq!(labels.as_object().unwrap().len(), 12);
    let mut d = dump("/w");
    d["slices"][0]["archived"] =
        json!([{"id": "A6", "title": "旧的", "kind": "能力", "replaced_by": "A5"}]);
    let m = make(&d, &who);
    assert_eq!(m.doc["slices"][0]["labels"]["A6"], "A6 旧的");
    assert_eq!(m.doc["slices"][0]["archived"][0]["replacedBy"], "A5");
    // 没有标题的一格：只写编号。
    let mut d = dump("/w");
    d["slices"][0]["cells"][0]["title"] = Value::Null;
    assert_eq!(make(&d, &who).doc["slices"][0]["labels"]["A1"], "A1");
}

/// pb 的 `start · end` 按 Unicode 码位；界面（JS）按 UTF-16 下标切 ⇒ 后端换好。码位外的字（emoji）在前面才看得出差别。
#[test]
fn ref_offsets_are_turned_from_code_points_into_utf16_units() {
    assert_eq!(utf16_at("ab", 1), 1);
    assert_eq!(utf16_at("😀A1", 1), 2);
    assert_eq!(utf16_at("😀A1", 3), 4);
    assert_eq!(utf16_at("x", 9), 1, "越界停在末尾");
    let mut d = dump("/w");
    d["slices"][0]["cells"][0]["body"] = json!("😀照 A3 的公约");
    d["slices"][0]["cells"][0]["refs"]["body"] = json!([{"start": 3, "end": 5, "ids": ["A3"]}]);
    d["slices"][0]["cells"][1]["signs"][0]["reason"] = json!("😀😀 A1");
    d["slices"][0]["cells"][1]["signs"][0]["refs"] = json!([{"start": 3, "end": 5, "ids": ["A1"]}]);
    d["slices"][0]["check"]["red"][0]["what"] = json!("😀A1-2 --to--> A9");
    d["slices"][0]["check"]["red"][0]["refs"] = json!([{"start": 1, "end": 5, "ids": ["A1-2"]}]);
    let m = make(&d, &who);
    let body: Vec<u16> = cell(&m.doc, "A1")["body"]
        .as_str()
        .unwrap()
        .encode_utf16()
        .collect();
    let r = &cell(&m.doc, "A1")["refs"]["body"][0];
    assert_eq!((r["start"].as_u64(), r["end"].as_u64()), (Some(4), Some(6)));
    assert_eq!(String::from_utf16(&body[4..6]).unwrap(), "A3");
    let s = &cell(&m.doc, "A1-1")["signs"][0]["refs"][0];
    assert_eq!((s["start"].as_u64(), s["end"].as_u64()), (Some(5), Some(7)));
    let red = &m.doc["slices"][0]["check"]["red"][0]["refs"][0];
    assert_eq!(
        (red["start"].as_u64(), red["end"].as_u64()),
        (Some(2), Some(6))
    );
}

#[test]
fn keys_follow_the_wire_convention() {
    let m = made();
    let sl = &m.doc["slices"][0];
    assert_eq!(sl["kinds"][1]["edgeWords"]["with"], "实现");
    assert_eq!(sl["check"]["red"][0]["rule"], "悬空");
}

#[test]
fn a_broken_slice_comes_out_as_name_domain_and_pbs_reason() {
    let m = make(&dump_broken("/w"), &who);
    let sl = &m.doc["slices"][0];
    assert_eq!(sl["name"], "alpha");
    assert_eq!(sl["error"], "图.md 第 3 行：元行缺 id");
    assert_eq!(sl["bare"], true);
    assert_eq!(sl["cells"], json!([]));
    assert_eq!(sl["labels"], json!({}));
    assert_eq!(sl["pendingSets"], json!([]));
    assert_eq!(sl["unowned"], json!({"paths": null, "why": null}));
}

/// pb 的状态 · 原因 · 对账三样都是中文原话：成品另给一格码，界面按码画（认原话的那一处只住这里）。
#[test]
fn status_why_and_file_state_come_with_codes() {
    let m = made();
    assert_eq!(cell(&m.doc, "A1-1")["statusCode"], "done");
    assert_eq!(cell(&m.doc, "A1-2")["statusCode"], "open");
    assert_eq!(
        cell(&m.doc, "A1")["whyCode"],
        json!({"kind": "inside", "done": 1, "of": 2})
    );
    assert_eq!(cell(&m.doc, "A1-2")["whyCode"], json!({"kind": "nosign"}));
    assert_eq!(cell(&m.doc, "A1-1")["whyCode"], Value::Null);
    assert_eq!(cell(&m.doc, "A1-1")["files"][0]["stateCode"], "ok");
    assert_eq!(cell(&m.doc, "A1-2")["files"][0]["stateCode"], "missing");
}

#[test]
fn every_pb_word_maps_to_its_code_and_strangers_to_null() {
    assert_eq!(status_code(Some("做完了")), Some("done"));
    assert_eq!(status_code(Some("没做完")), Some("open"));
    assert_eq!(status_code(Some("不做了")), Some("dropped"));
    assert_eq!(status_code(Some("别的")), None);
    assert_eq!(why_code(Some("等上一级收下")), json!({"kind": "upper"}));
    assert_eq!(
        why_code(Some("里面 0/3 做完了")),
        json!({"kind": "inside", "done": 0, "of": 3})
    );
    assert_eq!(why_code(Some("别的原因")), Value::Null);
    assert_eq!(why_code(None), Value::Null);
    for (w, c) in [
        ("在", "ok"),
        ("缺", "missing"),
        ("空", "empty"),
        ("坏", "broken"),
    ] {
        assert_eq!(file_code(Some(w)), Some(c));
    }
    assert_eq!(file_code(Some("?")), None);
}

/// 会话头那一枚标要的反查表：会话 ⇒ 它接手的那一块。自己接手的压过子 agent 替它接的。
#[test]
fn by_session_maps_each_owner_session_to_its_block_preferring_its_own_claim() {
    let m = made();
    assert_eq!(
        m.doc["bySession"],
        json!({MAIN: {"slice": "alpha", "block": "project", "cell": "project", "title": null, "top": true, "phase": "回看", "at": null, "atTitle": null, "via": "session"}})
    );
    let mut d = dump("/w");
    d["slices"][0]["blocks"][0]["owner"] = Value::Null;
    d["slices"][1]["blocks"][0]["owner"] = Value::Null;
    assert_eq!(
        make(&d, &who).doc["bySession"][MAIN],
        json!({"slice": "alpha", "block": "A2", "cell": "A2", "title": "乙功能", "top": false, "phase": "执行", "at": "A2-1", "atTitle": "乙的一半", "via": "subagent"})
    );
    let mut d = dump("/w");
    for (s, b) in [(0, 0), (0, 1), (1, 0)] {
        d["slices"][s]["blocks"][b]["owner"] = json!(STRANGER);
    }
    assert_eq!(make(&d, &who).doc["bySession"], json!({}));
}

/// 时刻由后端写成给人看的字（界面照抄）；块的 `at`（站在哪一格的编号）不是时刻 ⇒ 不添。
#[test]
fn time_fields_get_their_text_beside_them_and_cell_ids_do_not() {
    let now = 1_767_323_400_000_i64; // 2026-01-02T03:10:00Z
    let mut v = json!({
        "readAt": now - 60_000,
        "stale": {"said": "x", "since": now - 86_400_000},
        "slices": [{
            "blocks": [{"id": "B", "at": "A1-2"}],
            "cells": [{"signs": [{"at": "2026-01-02T00:00:00Z"}], "returned": {"at": now - 3_600_000}}]
        }]
    });
    with_time_texts(&mut v, now, &Default::default());
    assert_eq!(v["readAtText"], "03:09");
    assert_eq!(v["stale"]["sinceText"], "01-01 03:10");
    assert_eq!(v["slices"][0]["cells"][0]["signs"][0]["atText"], "00:00");
    assert_eq!(v["slices"][0]["cells"][0]["returned"]["atText"], "02:10");
    assert!(v["slices"][0]["blocks"][0].get("atText").is_none());
    let mut w = json!({"readAt": now});
    with_time_texts(&mut w, now, &crate::Tz::named("Asia/Shanghai").unwrap());
    assert_eq!(w["readAtText"], "11:10");
}

/// 每片的仓库位置照 pb 给的 `path`（相对工作区根），不拿片名补；读不成的片也带。
#[test]
fn every_slice_carries_pbs_repo_path_as_given() {
    assert_eq!(made().doc["slices"][0]["path"], "repos/alpha");
    assert_eq!(made().doc["slices"][1]["path"], "beta");
    let at = make(&crate::plan::fixture::dump_at("/w", "x/y"), &who);
    assert_eq!(at.doc["slices"][0]["path"], "x/y");
    assert_eq!(
        make(&dump_broken("/w"), &who).doc["slices"][0]["path"],
        "repos/alpha"
    );
    let mut d = dump("/w");
    d["slices"][0].as_object_mut().unwrap().remove("path");
    assert_eq!(make(&d, &who).doc["slices"][0]["path"], Value::Null);
}

#[test]
fn every_slice_says_whether_it_only_writes_docs() {
    assert_eq!(made().doc["slices"][0]["docsOnly"], false);
    assert_eq!(made().doc["slices"][1]["docsOnly"], true);
}

/// 计划页照画的几样由这里数好、排好、写好：块根格的直接子做完几格 · 行尾那一个原因短词 · 签收新的在上。
#[test]
fn counts_order_and_short_words_come_ready_for_the_page() {
    let m = made();
    let sl = &m.doc["slices"][0];
    assert_eq!(sl["blocks"][1]["inside"], json!({"done": 0, "of": 1}));
    assert_eq!(sl["blocks"][0]["inside"], Value::Null);
    assert_eq!(
        cell(&m.doc, "A1")["whyShort"],
        json!({"text": copy_core::copy_text("plan.whyShort.inside", &[("done", "1"), ("of", "2")]), "wait": false})
    );
    assert_eq!(
        cell(&m.doc, "A1-2")["whyShort"],
        json!({"text": copy_core::copy_text("plan.whyShort.nosign", &[]), "wait": false})
    );
    let mut d = dump("/w");
    d["slices"][0]["cells"][0]["why"] = json!("等上一级收下");
    d["slices"][0]["cells"][2]["why"] = json!("pb 新说法");
    let m2 = make(&d, &who);
    assert_eq!(
        cell(&m2.doc, "A1")["whyShort"],
        json!({"text": copy_core::copy_text("plan.whyShort.upper", &[]), "wait": true})
    );
    assert_eq!(
        cell(&m2.doc, "A1-2")["whyShort"],
        json!({"text": "pb 新说法", "wait": false})
    );
    // 签收新的在上：A1-1 签过两次，后签的那条在前。
    assert_eq!(
        sl["recentSigns"],
        json!([{"cell": "A1-1", "i": 1}, {"cell": "A1-1", "i": 0}])
    );
    let broken = make(&dump_broken("/w"), &who);
    assert_eq!(broken.doc["slices"][0]["openCells"], 0);
    assert_eq!(broken.doc["slices"][0]["recentSigns"], json!([]));
}

/// 读不出时刻的签收排在最后（同为读不出的照原来的先后）。
#[test]
fn signs_without_a_readable_time_go_last() {
    let mut d = dump("/w");
    d["slices"][0]["cells"][0]["signs"] = json!([
        {"at": "不是时刻", "by": null, "value": "收下", "via": null, "done": true, "reason": "a", "refs": []},
        {"at": "2025-12-31T00:00:00Z", "by": null, "value": "收下", "via": null, "done": true, "reason": "b", "refs": []}
    ]);
    assert_eq!(
        make(&d, &who).doc["slices"][0]["recentSigns"],
        json!([{"cell": "A1-1", "i": 1}, {"cell": "A1-1", "i": 0}, {"cell": "A1", "i": 1}, {"cell": "A1", "i": 0}])
    );
}
