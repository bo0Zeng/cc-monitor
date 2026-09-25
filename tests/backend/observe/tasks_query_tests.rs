//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md` 的 `tasks-list` 节（线上契约）
//!
//! 核原文：`tasks-list` 节逐字「那个 sid **没有任务目录** ⇒ 空 `lines`（诚实的空）；目录**在但读不了** ⇒ `failed`（不说成「没有任务」）」——
//! 本族判这两条出口，以及同节写明的数字升序、对象原样透传、超限跳过并点名、sid 围栏。〔JA1 点址 2026-09-24〕
//!
//! 〔RM1b · 第四波〕`tasks-list` 本体的判据。
//!
//! 夹具只造**结构**（目录名、文件名、占位字段），不采任何真会话正文。
//! 口径逐条对着搬家前 monitor 那份直读实现的判据（跳旁文件 · 按数字排 · 半截 JSON 跳过 ·
//! 目录不在 = 空），外加本件新加的三条（sid 围栏 · 超限跳过 · 目录读不了 ≠ 空）。

use super::*;
use std::path::PathBuf;

fn home(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rm1b-tasks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put(home: &Path, sid: &str, name: &str, body: &str) {
    let dir = tasks_root(home).join(sid);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), body).unwrap();
}

fn task(id: &str) -> String {
    format!(r#"{{"id":"{id}","subject":"s{id}","status":"pending","blocks":[],"blockedBy":[]}}"#)
}

fn ids(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["id"].as_str().unwrap().to_string()
        })
        .collect()
}

#[test]
fn a_session_without_a_task_dir_is_an_honest_empty() {
    let h = home("empty");
    assert_eq!(
        session_task_lines(&h, "no-such-sid").unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn only_numbered_json_files_are_tasks_and_they_come_back_in_numeric_order() {
    let h = home("order");
    put(&h, "s", ".lock", "");
    put(&h, "s", ".highwatermark", "11");
    put(&h, "s", "notes.json", "{}");
    put(&h, "s", "7.txt", &task("7"));
    for id in ["10", "2", "1"] {
        put(&h, "s", &format!("{id}.json"), &task(id));
    }
    // 扩展名大小写不敏感（与搬家前同口径）。
    put(&h, "s", "3.JSON", &task("3"));
    let got = session_task_lines(&h, "s").unwrap();
    assert_eq!(ids(&got), vec!["1", "2", "3", "10"]);
}

#[test]
fn a_torn_file_is_skipped_and_the_rest_still_comes_back() {
    let h = home("torn");
    put(&h, "s", "3.json", "{\"id\":\"3\",\"sub");
    put(&h, "s", "4.json", &task("4"));
    // 不是对象的 JSON 也不是任务。
    put(&h, "s", "5.json", "[1,2]");
    // BOM 剥掉照读。
    put(&h, "s", "6.json", &format!("\u{feff}{}", task("6")));
    assert_eq!(ids(&session_task_lines(&h, "s").unwrap()), vec!["4", "6"]);
}

#[test]
fn every_field_goes_through_untouched() {
    let h = home("verbatim");
    let body = r#"{"id":"1","subject":"占位","description":"d","activeForm":"a","status":"in_progress","blocks":["2"],"blockedBy":["0"],"future":{"x":1}}"#;
    put(&h, "s", "1.json", body);
    let got = session_task_lines(&h, "s").unwrap();
    assert_eq!(got.len(), 1);
    let back: serde_json::Value = serde_json::from_str(&got[0]).unwrap();
    let want: serde_json::Value = serde_json::from_str(body).unwrap();
    // 两向：后端不认识的字段（`future`）也原样过去 —— 字段语义只在 monitor 一处。
    assert_eq!(back, want);
}

#[test]
fn an_oversized_file_is_skipped_not_the_whole_list() {
    let h = home("cap");
    put(&h, "s", "1.json", &task("1"));
    let pad = "x".repeat(TASK_FILE_CAP_BYTES as usize);
    put(
        &h,
        "s",
        "2.json",
        &format!(r#"{{"id":"2","subject":"{pad}","status":"pending"}}"#),
    );
    put(&h, "s", "3.json", &task("3"));
    assert_eq!(ids(&session_task_lines(&h, "s").unwrap()), vec!["1", "3"]);
}

#[test]
fn a_sid_that_would_walk_out_of_the_task_root_is_refused() {
    let h = home("fence");
    // 正控：一个普通 sid 过得去。
    assert!(session_task_lines(&h, "abc-123").is_ok());
    for bad in ["", ".", "..", "../x", "a/b", "a\\b", "a\0b"] {
        let err = session_task_lines(&h, bad).expect_err(bad);
        assert_eq!(err.0, "bad_args", "{bad:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_dir_that_is_there_but_unreadable_is_not_an_empty_list() {
    use std::os::unix::fs::PermissionsExt;
    let h = home("unreadable");
    put(&h, "s", "1.json", &task("1"));
    let dir = tasks_root(&h).join("s");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
    // root 跑测试时权限位拦不住读 —— 那一格判不了，如实跳过而不是假绿。
    let perms_bite = std::fs::File::open(&dir).is_err();
    let got = session_task_lines(&h, "s");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    if !perms_bite {
        return;
    }
    assert_eq!(got.expect_err("读不了的目录回了 Ok").0, "failed");
}
