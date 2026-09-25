//! # 要求住址：`INVARIANTS §18`（`TaskEntry` 宽容解析）＋ `INVARIANTS §40`（本机也走后端那条路）
//!
//! 核原文：`INVARIANTS §18` 逐字「`tasks/<sid>/<id>.json` (`tasks::TaskEntry`) — 已经按宽容处理」—— 解析那两条判它；
//! `§40` 逐字「一条路径，transport 是它唯一的差异」—— 本模块不再自己读任务文件那一条判它。
//! 由变更路径反推 sid 与 camelCase 那几条是普通模块行为，没有逐字原文。〔JA1 点址 2026-09-24〕
//!
//! 〔RM1b · 第四波〕读任务文件那一段搬进了后端（`src/backend/observe/tasks_query.rs`，
//! 它的判据在 `tests/backend/observe/tasks_query_tests.rs`：跳旁文件 · 按数字排 · 半截跳过 ·
//! 目录不在 = 空 · 目录读不了 ≠ 空 · sid 围栏 · 超限跳过）。
//! 〔LOC1a · 第四波 4D · C4e 批 4〕字段语义也搬过去了（后端 `task_entry` 出成品，`设计/05 §14.3`「业务解释只有一个家」；
//! 旧口径那几档 —— 缺必填 / 类型不对 / 多余键 —— 由跨语言金样 `tests/__fixtures__/tasks-list.golden.json` 在后端那侧判）。
//! 本文件只剩 monitor 这一侧的三件：成品按形状严格收（`decode_tasks`）· watcher 反推 sid · 线上 camelCase 契约。
//! 夹具只造结构（占位字段），不采任何真会话正文。

use super::*;
use std::path::PathBuf;

fn golden() -> serde_json::Value {
    let raw = std::fs::read_to_string(
        crate::guard_support::repo_root().join("tests/__fixtures__/tasks-list.golden.json"),
    )
    .expect("读金样");
    serde_json::from_str(&raw).expect("金样是 JSON")
}

/// 金样的成品这一侧收得下，逐格读得出来（异源：金样由后端测试从生产路径现算核过）。
#[test]
fn the_golden_product_decodes_on_this_side() {
    let got = decode_tasks("本机", golden()["product"].clone()).expect("成品收得下");
    let ids: Vec<&str> = got.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["1", "2", "3"]);
    assert_eq!(got[0].description.as_deref(), Some("说明"));
    assert_eq!(got[1].active_form.as_deref(), Some("占位乙中"));
    assert_eq!(got[1].blocked_by, vec!["1"]);
    assert!(got[2].description.is_none() && got[2].blocks.is_empty());
    // 空清单 ⇒ 空（诚实的空，不是错）。
    assert!(decode_tasks("本机", serde_json::json!({"tasks": []})).unwrap().is_empty());
}

/// 按形状**严格**收：多一格 / 缺一格 / 类型不对 / 顶层不对 ⇒ `Err`「两端契约对不上」，**不跳过那一条**
/// （跳过是字段语义，那一份只住后端）。
#[test]
fn a_product_of_the_wrong_shape_is_refused_not_skimmed() {
    let ok = serde_json::json!({"id":"1","subject":"s","status":"pending","blocks":[],"blockedBy":[]});
    let with = |k: &str, v: serde_json::Value| {
        let mut t = ok.clone();
        t[k] = v;
        serde_json::json!({ "tasks": [t] })
    };
    let mut missing = ok.clone();
    missing.as_object_mut().unwrap().remove("status");
    for bad in [
        with("extra", serde_json::json!(1)),
        with("id", serde_json::json!(1)),
        with("blocks", serde_json::json!(null)),
        serde_json::json!({ "tasks": [missing] }),
        serde_json::json!({ "lines": [] }),
        serde_json::json!({ "tasks": [], "more": 1 }),
        serde_json::json!([]),
    ] {
        let e = decode_tasks("本机", bad.clone()).expect_err("该拒");
        assert!(e.contains("两端契约对不上"), "{bad} ⇒ {e}");
    }
}

/// ★ 本机读实现真的退役了：生产段里一处 `read_dir` / `read_to_string` 都不许有
/// （零命中 ＋ 正控：同一把尺子对搬家前那份逐字源码数得出来）。
#[test]
fn this_module_no_longer_reads_task_files_itself() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/tasks.rs"));
    // 针运行时拼：字面量会让本文件自己变成「裸遍历目录」那一族扫描判据的语料。
    let needles = [
        format!("read_{}(", "dir"),
        format!("read_to_{}(", "string"),
        format!("fs::{}(", "read"),
    ];
    let hits: Vec<&String> = needles
        .iter()
        .filter(|n| prod.contains(n.as_str()))
        .collect();
    assert!(hits.is_empty(), "tasks.rs 生产段又自己读盘了：{hits:?}");
    // 正控：搬家前那一段的形状，同一把尺子必须数得到。
    let before = format!(
        "let entries = match std::fs::read_{}(&session_dir) {{ let raw = match std::fs::read_to_{}(&path) {{",
        "dir", "string"
    );
    assert!(
        needles
            .iter()
            .filter(|n| before.contains(n.as_str()))
            .count()
            == 2,
        "尺子瞎了"
    );
}

#[test]
fn session_id_from_change_strips_root() {
    let root = PathBuf::from("/x/tasks");
    let got = session_id_from_change(&PathBuf::from("/x/tasks/sid-xyz/15.json"), &root).unwrap();
    assert_eq!(got, "sid-xyz");
}

#[test]
fn session_id_from_change_handles_lock_files() {
    let root = PathBuf::from("/x/tasks");
    let got = session_id_from_change(&PathBuf::from("/x/tasks/sid-xyz/.lock"), &root).unwrap();
    assert_eq!(got, "sid-xyz");
}

#[test]
fn session_id_from_change_returns_none_outside_root() {
    let root = PathBuf::from("/x/tasks");
    let got = session_id_from_change(&PathBuf::from("/y/other.json"), &root);
    assert!(got.is_none());
}

#[test]
fn camel_case_serialization_matches_frontend_contract() {
    // 验证 serde 输出 activeForm/blockedBy（不是 active_form/blocked_by）
    let t = TaskEntry {
        id: "1".into(),
        subject: "s".into(),
        description: None,
        active_form: Some("af".into()),
        status: "pending".into(),
        blocks: vec![],
        blocked_by: vec!["0".into()],
    };
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"activeForm\":\"af\""));
    assert!(json.contains("\"blockedBy\":[\"0\"]"));
    // description: None 时不应该出现（skip_serializing_if）
    assert!(!json.contains("description"));
}
