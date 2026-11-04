//! 会话事实的「机器 · 目录」那一句（`whereLine`）：那台照信封里看的这一端对它的称呼（`called`）写；
//! 没交称呼 ⇒ 只写目录，不编名字；目录还没读到 ⇒ 只写名字；两样都没有 ⇒ 不带。
use super::*;
use std::path::PathBuf;

/// 一份最小的会话记录，开头带项目目录（`cwd`）；`cwd = None` ⇒ 开头不带目录。
fn record(tag: &str, cwd: Option<&str>) -> (PathBuf, PathBuf) {
    let home = test_home::scratch(tag);
    let dir = home.join("projects").join("-w-proj");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("0b1c2d3e-aaaa-4bbb-8ccc-1234567890ab.jsonl");
    let head = match cwd {
        Some(c) => format!(
            r#"{{"type":"user","uuid":"u-1","timestamp":"t0","cwd":"{c}","message":{{"content":"q"}}}}"#
        ),
        None => r#"{"type":"user","uuid":"u-1","timestamp":"t0","message":{"content":"q"}}"#.to_string(),
    };
    std::fs::write(&p, format!("{head}\n")).unwrap();
    (home, p)
}

fn where_line(home: &Path, path: &Path, called: Option<&str>) -> serde_json::Value {
    let limits = crate::observe::recognize::facts::ContextLimits::default();
    let req = HistoryFactsReq::Path(FactsByPath {
        path: path.to_string_lossy().into_owned(),
    });
    let reply = facts_at(home, req, &limits, &crate::common::time::Tz::default(), called)
        .expect("事实该答得出");
    serde_json::to_value(reply).unwrap()["whereLine"].clone()
}

#[test]
fn the_line_names_the_machine_the_way_the_viewer_calls_it() {
    let (home, p) = record("named", Some("/w/proj"));
    assert_eq!(
        where_line(&home, &p, Some("devbox")),
        serde_json::json!(copy_core::copy_text(
            "beFacts.where.line",
            &[("machine", "devbox"), ("dir", "/w/proj")]
        ))
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn without_a_name_the_line_is_the_directory_alone() {
    let (home, p) = record("unnamed", Some("/w/proj"));
    assert_eq!(where_line(&home, &p, None), serde_json::json!("/w/proj"));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn without_a_directory_the_line_is_the_name_alone_and_with_neither_there_is_none() {
    let (home, p) = record("nodir", None);
    assert_eq!(where_line(&home, &p, Some("devbox")), serde_json::json!("devbox"));
    assert!(where_line(&home, &p, None).is_null());
    let _ = std::fs::remove_dir_all(&home);
}
