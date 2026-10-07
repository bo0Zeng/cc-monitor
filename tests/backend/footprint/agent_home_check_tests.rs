//! 换 agent 家目录存之前那一问：在不在 · 是不是目录 · 像不像（有没有那一家的记录树），一条命令判、回码。

use super::*;
use serde_json::json;

struct TmpDir(std::path::PathBuf);
impl TmpDir {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tmp() -> TmpDir {
    let p = std::env::temp_dir().join(format!(
        "st8-agenthome-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    TmpDir(p)
}

#[test]
fn 有记录树才算像() {
    let d = tmp();
    std::fs::create_dir_all(crate::agents::records_root(d.path()).unwrap()).unwrap();
    assert_eq!(verdict(d.path()), Verdict::Ok);
    assert_eq!(
        answer_check(&json!({"path": d.path()})).unwrap(),
        json!({"state": "ok"})
    );
}

#[test]
fn 不在_不是目录_没有记录树_各一个码() {
    let d = tmp();
    assert_eq!(verdict(&d.path().join("nope")), Verdict::Missing);
    std::fs::write(d.path().join("a.txt"), "x").unwrap();
    assert_eq!(verdict(&d.path().join("a.txt")), Verdict::NotDir);
    assert_eq!(verdict(d.path()), Verdict::NoRecords);
    // 记录树那个名字是个文件也不算
    std::fs::write(crate::agents::records_root(d.path()).unwrap(), "x").unwrap();
    assert_eq!(verdict(d.path()), Verdict::NoRecords);
    assert_eq!(
        answer_check(&json!({"path": d.path().join("nope")})).unwrap(),
        json!({"state": "missing"})
    );
}

#[test]
fn 路径缺席_不是串_相对路径_拒() {
    for a in [
        json!({}),
        json!({"path": 3}),
        json!({"path": ""}),
        json!({"path": "rel/x"}),
    ] {
        assert_eq!(answer_check(&a).unwrap_err().0, "bad_args", "{a}");
    }
}

#[test]
fn 波浪号按这台家目录展开() {
    let d = tmp();
    std::fs::create_dir_all(crate::agents::records_root(&d.path().join(".agent-x")).unwrap())
        .unwrap();
    let home = d.path().to_path_buf();
    let got = answer_check_with(&json!({"path": "~/.agent-x"}), Some(home)).unwrap();
    assert_eq!(got, json!({"state": "ok"}));
}
