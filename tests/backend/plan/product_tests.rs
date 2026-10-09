//! dump ⇒ 成品：摘 agent_view · 倒过来的边与文件 · id 对到会话 · 顶层进度 · 键名换成线上惯例。

use super::*;
use crate::plan::fixture::{dump, dump_broken, who, MAIN, STRANGER, SUB};

fn made() -> Made {
    make(&dump("/w"), &who)
}

fn cell<'a>(doc: &'a Value, id: &str) -> &'a Value {
    doc["slices"][0]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()
}

#[test]
fn agent_view_is_taken_off_every_cell_and_kept_aside() {
    let m = made();
    let text = m.doc.to_string();
    assert!(!text.contains("agent_view"), "成品里不许有 agent_view");
    assert!(!text.contains("── A1-1"), "agent_view 的原文不许跟着成品走");
    assert_eq!(
        m.views
            .get(&("alpha".into(), "A1-1".into()))
            .map(String::as_str),
        Some("── A1-1 甲的读入")
    );
    assert_eq!(m.views.len(), 4);
    assert_eq!(cell(&m.doc, "A1")["hasView"], true);
}

#[test]
fn edges_pointing_at_a_cell_are_indexed_backwards() {
    let m = made();
    assert_eq!(
        cell(&m.doc, "A1-1")["pointedBy"]["with"],
        serde_json::json!(["A1-2"])
    );
    assert_eq!(
        cell(&m.doc, "A1-1")["pointedBy"]["after"],
        serde_json::json!(["A1-2"])
    );
    assert_eq!(
        cell(&m.doc, "A1-2")["pointedBy"]["with"],
        serde_json::json!([])
    );
    assert_eq!(
        cell(&m.doc, "A1-2")["edges"]["with"],
        serde_json::json!(["A1-1"])
    );
}

#[test]
fn a_file_declared_by_two_cells_names_the_other_one() {
    let m = made();
    let f = &cell(&m.doc, "A1-1")["files"][0];
    assert_eq!(f["path"], "src/read.txt");
    assert_eq!(f["alsoBy"], serde_json::json!(["A1-2"]));
    let g = &cell(&m.doc, "A1-2")["files"];
    assert_eq!(g[0]["alsoBy"], serde_json::json!(["A1-1"]));
    assert_eq!(g[1]["alsoBy"], serde_json::json!([]));
    assert_eq!(g[1]["state"], "缺");
}

#[test]
fn owners_and_signers_are_resolved_to_sessions() {
    let m = made();
    let owner = &cell(&m.doc, "A1-1")["owner"];
    assert_eq!(owner["kind"], "subagent");
    assert_eq!(owner["sid"], MAIN);
    assert_eq!(owner["id"], SUB);
    assert_eq!(owner["alive"], true);
    assert_eq!(owner["activity"], "needs_you");
    assert_eq!(owner["needs"], "approve");
    let signs = &cell(&m.doc, "A1-1")["signs"];
    assert_eq!(signs[1]["by"]["kind"], "unknown");
    assert_eq!(signs[1]["by"]["id"], STRANGER);
    assert_eq!(signs[1]["by"]["sid"], Value::Null);
    // 签它的那一位：pb 还没给 ⇒ `null`；给了照样对到会话。
    assert_eq!(cell(&m.doc, "A1-1")["signer"], Value::Null);
    let mut d = dump("/w");
    d["slices"][0]["cells"][1]["signer"] = serde_json::json!(MAIN);
    let given = make(&d, &who);
    assert_eq!(cell(&given.doc, "A1-1")["signer"]["kind"], "session");
    let blocks = &m.doc["slices"][0]["blocks"];
    assert_eq!(blocks[0]["owner"]["kind"], "session");
    assert_eq!(blocks[0]["owner"]["sid"], MAIN);
    assert_eq!(blocks[1]["at"], "A1-2");
}

#[test]
fn top_level_progress_counts_pbs_statuses() {
    let m = made();
    let sl = &m.doc["slices"][0];
    assert_eq!(
        sl["progress"],
        serde_json::json!({"done": 0, "open": 1, "dropped": 1})
    );
    assert_eq!(sl["current"], true);
    assert_eq!(m.doc["auto"], true);
}

#[test]
fn keys_follow_the_wire_convention() {
    let m = made();
    let sl = &m.doc["slices"][0];
    assert_eq!(sl["kinds"][1]["edgeWords"]["with"], "实现");
    assert_eq!(sl["archived"][0]["replacedBy"], "A2");
    assert_eq!(sl["check"]["red"][0]["rule"], "悬空");
    assert!(!m.doc.to_string().contains("edge_words"));
    assert!(!m.doc.to_string().contains("replaced_by"));
}

#[test]
fn a_broken_slice_comes_out_as_name_domain_and_pbs_reason() {
    let m = make(&dump_broken("/w"), &who);
    let sl = &m.doc["slices"][0];
    assert_eq!(sl["name"], "alpha");
    assert_eq!(sl["error"], "图.md 第 3 行：元行缺 id");
    assert!(sl.get("cells").is_none());
    assert!(m.views.is_empty());
}

/// pb 的状态 · 原因 · 对账三样都是中文原话：成品另给一格码，界面按码画（认原话的那一处只住这里）。
#[test]
fn status_why_and_file_state_come_with_codes() {
    let m = made();
    assert_eq!(cell(&m.doc, "A1-1")["statusCode"], "done");
    assert_eq!(cell(&m.doc, "A1-2")["statusCode"], "open");
    assert_eq!(cell(&m.doc, "A2")["statusCode"], "dropped");
    assert_eq!(
        cell(&m.doc, "A1")["whyCode"],
        serde_json::json!({"kind": "inside", "done": 1, "of": 2})
    );
    assert_eq!(
        cell(&m.doc, "A1-2")["whyCode"],
        serde_json::json!({"kind": "nosign"})
    );
    assert_eq!(cell(&m.doc, "A1-1")["whyCode"], Value::Null);
    assert_eq!(cell(&m.doc, "A1-1")["files"][0]["stateCode"], "ok");
    assert_eq!(cell(&m.doc, "A1-2")["files"][1]["stateCode"], "missing");
}

#[test]
fn every_pb_word_maps_to_its_code_and_strangers_to_null() {
    assert_eq!(status_code(Some("做完了")), Some("done"));
    assert_eq!(status_code(Some("没做完")), Some("open"));
    assert_eq!(status_code(Some("不做了")), Some("dropped"));
    assert_eq!(status_code(Some("别的")), None);
    assert_eq!(why_code(Some("等上一级收下")), serde_json::json!({"kind": "upper"}));
    assert_eq!(why_code(Some("里面 0/3 做完了")), serde_json::json!({"kind": "inside", "done": 0, "of": 3}));
    assert_eq!(why_code(Some("别的原因")), Value::Null);
    assert_eq!(why_code(None), Value::Null);
    for (w, c) in [("在", "ok"), ("缺", "missing"), ("空", "empty"), ("坏", "broken")] {
        assert_eq!(file_code(Some(w)), Some(c));
    }
    assert_eq!(file_code(Some("?")), None);
}
