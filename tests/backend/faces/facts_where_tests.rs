//! 会话事实里由那台起好的两句：「在哪」（`whereLine`，与终端页头同一句「<目录> · <机器>」）照信封里看的这一端对它的称呼（`called`）写，
//! 没交称呼 ⇒ 只写目录、不编名字，目录还没读到 ⇒ 只写名字，两样都没有 ⇒ 不带；「分叉自」（`forkedLine`）见文件后半。
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
        None => {
            r#"{"type":"user","uuid":"u-1","timestamp":"t0","message":{"content":"q"}}"#.to_string()
        }
    };
    std::fs::write(&p, format!("{head}\n")).unwrap();
    (home, p)
}

fn where_line(home: &Path, path: &Path, called: Option<&str>) -> serde_json::Value {
    let limits = crate::observe::recognize::facts::ContextLimits::default();
    let req = HistoryFactsReq::Path(FactsByPath {
        path: path.to_string_lossy().into_owned(),
    });
    let reply = facts_at(
        home,
        req,
        &limits,
        &crate::common::time::Tz::default(),
        called,
    )
    .expect("事实该答得出");
    serde_json::to_value(reply).unwrap()["whereLine"].clone()
}

#[test]
fn the_line_names_the_machine_the_way_the_viewer_calls_it() {
    let (home, p) = record("named", Some("/w/proj"));
    let got = where_line(&home, &p, Some("devbox"));
    assert_eq!(
        got,
        serde_json::json!(copy_core::copy_text(
            "terminal.head.where",
            &[("machine", "devbox"), ("dir", "/w/proj")]
        ))
    );
    let s = got.as_str().unwrap_or_default();
    assert!(
        s.contains("devbox") && s.contains("/w/proj"),
        "那一句里有那台的称呼与目录：{got}"
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
    assert_eq!(
        where_line(&home, &p, Some("devbox")),
        serde_json::json!("devbox")
    );
    assert!(where_line(&home, &p, None).is_null());
    let _ = std::fs::remove_dir_all(&home);
}

// ---- 「分叉自」那一句（`forkedLine`）：那台照父会话那份记录起名；记录不在 ⇒ 退到父会话号头 8 位；不是分叉来的 ⇒ 不带 ----

const PARENT: &str = "7a8b9c0d-1111-4222-8333-444455556666";

/// 一份分叉来的记录（开头那条带 `forkedFrom`）；`parent_said = Some(..)` ⇒ 同目录里再放一份父会话的记录，第一句是它。
fn forked(tag: &str, parent_said: Option<&str>) -> (PathBuf, PathBuf) {
    let home = test_home::scratch(tag);
    let dir = home.join("projects").join("-w-proj");
    std::fs::create_dir_all(&dir).unwrap();
    if let Some(said) = parent_said {
        let head = format!(
            r#"{{"type":"user","uuid":"p-1","timestamp":"t0","cwd":"/w/proj","message":{{"content":"{said}"}}}}"#
        );
        std::fs::write(dir.join(format!("{PARENT}.jsonl")), format!("{head}\n")).unwrap();
    }
    let p = dir.join("0b1c2d3e-bbbb-4bbb-8ccc-1234567890ab.jsonl");
    let head = format!(
        r#"{{"type":"user","uuid":"f-1","timestamp":"t1","cwd":"/w/proj","forkedFrom":{{"sessionId":"{PARENT}","messageUuid":"p-1"}},"message":{{"content":"q"}}}}"#
    );
    std::fs::write(&p, format!("{head}\n")).unwrap();
    (home, p)
}

fn forked_line(home: &Path, path: &Path) -> serde_json::Value {
    let limits = crate::observe::recognize::facts::ContextLimits::default();
    let req = HistoryFactsReq::Path(FactsByPath {
        path: path.to_string_lossy().into_owned(),
    });
    let reply = facts_at(
        home,
        req,
        &limits,
        &crate::common::time::Tz::default(),
        None,
    )
    .expect("事实该答得出");
    serde_json::to_value(reply).unwrap()["forkedLine"].clone()
}

#[test]
fn a_fork_is_named_after_the_parent_record() {
    let (home, p) = forked("fork-named", Some("整理一下登录页"));
    let got = forked_line(&home, &p);
    assert_eq!(
        got,
        serde_json::json!(copy_core::copy_text(
            "newSession.fork.fromBare",
            &[("title", "整理一下登录页")]
        )),
        "那一句照父会话那份记录起的名字"
    );
    assert!(
        !got.as_str().unwrap_or_default().contains("7a8b9c0d"),
        "父记录在 ⇒ 不写会话号：{got}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn without_the_parent_record_the_name_falls_back_to_the_first_eight_of_its_id() {
    let (home, p) = forked("fork-gone", None);
    assert_eq!(
        forked_line(&home, &p),
        serde_json::json!(copy_core::copy_text(
            "newSession.fork.fromBare",
            &[("title", "7a8b9c0d")]
        ))
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_session_that_is_not_a_fork_carries_no_line() {
    let (home, p) = record("not-fork", Some("/w/proj"));
    assert!(forked_line(&home, &p).is_null());
    let _ = std::fs::remove_dir_all(&home);
}
