use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

/// 每个测试独占的临时目录。仓库约定不引 tempfile，用 process_id + 全局计数器
/// 保证唯一性，TestDir::drop 时清理（cargo test 多线程并发跑也安全）。
struct TestDir(PathBuf);

impl TestDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p =
            std::env::temp_dir().join(format!("ccm-tasks-test-{}-{tag}-{n}", std::process::id(),));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        TestDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[test]
fn read_empty_session_returns_empty() {
    let dir = TestDir::new("empty");
    let got = read_session_tasks(dir.path(), "any-sid");
    assert!(got.is_empty());
}

#[test]
fn read_skips_lock_and_highwatermark_and_non_digit_names() {
    let dir = TestDir::new("skip");
    let sid = "abc";
    let sdir = dir.path().join(sid);
    fs::create_dir_all(&sdir).unwrap();
    write(&sdir.join(".lock"), "");
    write(&sdir.join(".highwatermark"), "5");
    write(&sdir.join("notes.json"), "{}");
    write(
        &sdir.join("1.json"),
        r#"{"id":"1","subject":"t1","status":"pending","blocks":[],"blockedBy":[]}"#,
    );
    let got = read_session_tasks(dir.path(), sid);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, "1");
}

#[test]
fn read_sorts_by_numeric_id() {
    let dir = TestDir::new("sort");
    let sid = "s";
    let sdir = dir.path().join(sid);
    fs::create_dir_all(&sdir).unwrap();
    for id in ["10", "2", "1"] {
        write(
            &sdir.join(format!("{id}.json")),
            &format!(
                r#"{{"id":"{id}","subject":"t{id}","status":"completed","blocks":[],"blockedBy":[]}}"#
            ),
        );
    }
    let got = read_session_tasks(dir.path(), sid);
    let ids: Vec<&str> = got.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["1", "2", "10"]);
}

#[test]
fn read_tolerates_partial_json_during_lock() {
    let dir = TestDir::new("lock");
    let sid = "s";
    let sdir = dir.path().join(sid);
    fs::create_dir_all(&sdir).unwrap();
    // 半截 JSON（写者持锁中途读）
    write(&sdir.join("3.json"), "{\"id\":\"3\",\"sub");
    // 完整 JSON
    write(
        &sdir.join("4.json"),
        r#"{"id":"4","subject":"ok","status":"in_progress","blocks":[],"blockedBy":[]}"#,
    );
    let got = read_session_tasks(dir.path(), sid);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, "4");
}

#[test]
fn read_parses_optional_fields() {
    let dir = TestDir::new("opt");
    let sid = "s";
    let sdir = dir.path().join(sid);
    fs::create_dir_all(&sdir).unwrap();
    write(
        &sdir.join("1.json"),
        r##"{
                "id":"1",
                "subject":"#1a 前端 priority queue",
                "description":"按 session 分组 + 优先 active",
                "activeForm":"实现 priority queue",
                "status":"in_progress",
                "blocks":["2"],
                "blockedBy":["0"]
            }"##,
    );
    let got = read_session_tasks(dir.path(), sid);
    assert_eq!(got.len(), 1);
    let t = &got[0];
    assert_eq!(t.subject, "#1a 前端 priority queue");
    assert_eq!(
        t.description.as_deref(),
        Some("按 session 分组 + 优先 active")
    );
    assert_eq!(t.active_form.as_deref(), Some("实现 priority queue"));
    assert_eq!(t.status, "in_progress");
    assert_eq!(t.blocks, vec!["2"]);
    assert_eq!(t.blocked_by, vec!["0"]);
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
