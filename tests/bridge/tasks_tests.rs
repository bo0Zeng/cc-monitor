//! 〔RM1b · 第四波〕读任务文件那一段搬进了后端（`src/backend/observe/tasks_query.rs`，
//! 它的判据在 `tests/backend/observe/tasks_query_tests.rs`：跳旁文件 · 按数字排 · 半截跳过 ·
//! 目录不在 = 空 · 目录读不了 ≠ 空 · sid 围栏 · 超限跳过）。本文件只剩 monitor 这一侧的三件：
//! 字段语义（`parse_task_lines`）· watcher 反推 sid · 线上 camelCase 契约。
//! 夹具只造结构（占位字段），不采任何真会话正文。

use super::*;
use std::path::PathBuf;

fn line(id: &str, status: &str) -> String {
    format!(r#"{{"id":"{id}","subject":"s{id}","status":"{status}","blocks":[],"blockedBy":[]}}"#)
}

#[test]
fn parse_keeps_the_backend_order_and_drops_lines_that_are_not_tasks() {
    let lines = vec![
        line("1", "completed"),
        // 后端只保证「是一个对象」：缺 `subject` / `status` 的对象在这一侧挡下。
        r#"{"id":"2"}"#.to_string(),
        "not json".to_string(),
        line("10", "pending"),
    ];
    let got = parse_task_lines(&lines);
    let ids: Vec<&str> = got.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["1", "10"]);
    // 反向：一行都不是任务 ⇒ 空（不是 panic、不是错）。
    assert!(parse_task_lines(&["[]".to_string()]).is_empty());
}

#[test]
fn parse_reads_the_optional_fields() {
    let lines = vec![r##"{
        "id":"1",
        "subject":"占位 subject",
        "description":"占位 description",
        "activeForm":"占位 activeForm",
        "status":"in_progress",
        "blocks":["2"],
        "blockedBy":["0"],
        "unknownFutureField": true
    }"##
    .to_string()];
    let got = parse_task_lines(&lines);
    assert_eq!(got.len(), 1);
    let t = &got[0];
    assert_eq!(t.subject, "占位 subject");
    assert_eq!(t.description.as_deref(), Some("占位 description"));
    assert_eq!(t.active_form.as_deref(), Some("占位 activeForm"));
    assert_eq!(t.status, "in_progress");
    assert_eq!(t.blocks, vec!["2"]);
    assert_eq!(t.blocked_by, vec!["0"]);
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
