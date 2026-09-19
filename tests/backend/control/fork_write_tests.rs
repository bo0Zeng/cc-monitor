use super::*;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-fork-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(p.join("projects").join("proj")).unwrap();
    p
}

fn seed(dir: &Path, sid: &str) {
    let rows = [
        serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"timestamp":"t1","sessionId":sid}),
        serde_json::json!({"type":"assistant","uuid":"u2","parentUuid":"u1","timestamp":"t2","sessionId":sid}),
        serde_json::json!({"type":"user","uuid":"u3","parentUuid":"u2","timestamp":"t3","sessionId":sid}),
    ];
    let body: String = rows
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<Vec<_>>()
        .concat();
    std::fs::write(
        dir.join("projects")
            .join("proj")
            .join(format!("{sid}.jsonl")),
        body,
    )
    .unwrap();
}

#[test]
fn fork_writes_new_file_and_leaves_source_untouched() {
    let root = tmp("ok");
    seed(&root, "srcsid");
    let src = root.join("projects").join("proj").join("srcsid.jsonl");
    let before = std::fs::read(&src).unwrap();

    let res = run_inner(&root, &sargs(&["--fork-session", "srcsid", "u2"])).unwrap();

    assert_eq!(std::fs::read(&src).unwrap(), before, "源文件被改动了");
    let out = PathBuf::from(&res.jsonl_path);
    assert!(out.exists());
    assert_eq!(
        out.parent().unwrap(),
        src.parent().unwrap(),
        "应落在源同目录"
    );
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(&out)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // 走的是共享变换：祖先链 u1→u2，sessionId 换新
    let uuids: Vec<&str> = rows.iter().map(|r| r["uuid"].as_str().unwrap()).collect();
    assert_eq!(uuids, vec!["u1", "u2"]);
    assert_eq!(rows[0]["sessionId"].as_str().unwrap(), res.session_id);
    std::fs::remove_dir_all(&root).ok();
}

/// ★ 并发/覆盖：`O_EXCL` 必须让第二次写落到错误上，而不是盖掉第一份。
#[test]
fn write_new_file_refuses_existing_target() {
    let root = tmp("excl");
    let p = root.join("projects").join("proj").join("dup.jsonl");
    std::fs::write(&p, "PREEXISTING\n").unwrap();
    let err = write_new_file(&p, &[serde_json::json!({"x":1})]).unwrap_err();
    assert!(err.contains("create"), "got: {err}");
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "PREEXISTING\n",
        "已存在的文件被覆盖了"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 找那一份走的是共享 crate —— 这里钉的是**这条路真的经过它**。
fn find_here(root: &Path, sid: &str) -> Result<PathBuf, String> {
    branch_core::find_session_file(&projects_root(root), sid)
}

#[test]
fn rejects_path_traversal_sid() {
    let root = tmp("trav");
    for bad in ["../../etc/passwd", "a/b", "..", "a\\b", ""] {
        assert!(find_here(&root, bad).is_err(), "sid {bad:?} 应被拒");
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn missing_session_is_an_error_not_a_panic() {
    let root = tmp("missing");
    let err = find_here(&root, "nope").unwrap_err();
    assert!(err.contains("not found"), "got: {err}");
    std::fs::remove_dir_all(&root).ok();
}

/// ★★ `KR88D2` 第三刀（daemon 这一侧）：**给一个查不到的 sid，处置是报错，
/// 不是「树上有什么就拿什么」。**
///
/// 树上**真的有两份**别的会话 —— 少了这一步，下面那条断言在空树上也绿，
/// 而「静默取第一个」正是它要逮的那一形。
/// monitor 侧的同形判据是 `history·rs::an_unknown_session_id_is_refused_not_silently_substituted`，
/// 两条读的是同一份实现。
#[test]
fn an_unknown_session_id_is_refused_not_silently_substituted() {
    let root = tmp("unknown");
    seed(&root, "aaa");
    seed(&root, "bbb");
    // 反向自检：树上真有东西可被「随手挑」。
    assert!(find_here(&root, "aaa").is_ok(), "夹具没造出可被挑中的会话");
    let err = run_inner(&root, &sargs(&["--fork-session", "ccc", "u2"])).unwrap_err();
    assert!(
        err.contains("not found") && err.contains("ccc"),
        "查不到的 sid 应当报错并点名，实得：{err}"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// ★ 符号链接不算命中 —— 记录树里一条指向界外的链接，**分叉不到它**。
///
/// 这一半原先只有 monitor 那条路有（靠 canonicalize 两边比前缀）；
/// 收成一份之后两侧同时拿到。
#[cfg(unix)]
#[test]
fn a_symlink_inside_the_tree_is_not_a_hit() {
    let root = tmp("symlink");
    let outside = root.join("secret.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    let link = root.join("projects").join("proj").join("linked.jsonl");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let err = find_here(&root, "linked").unwrap_err();
    assert!(err.contains("not found"), "got: {err}");
    assert!(outside.exists(), "界外那份被动过了");
    std::fs::remove_dir_all(&root).ok();
}

/// 子 agent 记录不可分叉 —— 这条判据在共享 crate 里，这里钉住 daemon 真的走了它。
#[test]
fn sidechain_reject_reaches_daemon_path() {
    let root = tmp("side");
    seed(&root, "sc");
    let f = root.join("projects").join("proj").join("sc.jsonl");
    let mut body = std::fs::read_to_string(&f).unwrap();
    body.push_str(
        &serde_json::json!({"type":"assistant","uuid":"s1","parentUuid":"u2",
            "timestamp":"t9","sessionId":"sc","isSidechain":true})
        .to_string(),
    );
    body.push('\n');
    std::fs::write(&f, body).unwrap();
    let err = run_inner(&root, &sargs(&["--fork-session", "sc", "s1"])).unwrap_err();
    assert!(err.contains("sidechain"), "got: {err}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn new_session_id_has_uuid_shape_and_varies() {
    let a = new_session_id("x");
    assert_eq!(a.len(), 36, "{a}");
    assert_eq!(a.split('-').count(), 5, "{a}");
    // 同一输入连续两次也应不同（含 nanos）——但唯一性真正靠 O_EXCL，不靠这个。
    assert_ne!(a, new_session_id("x"));
}

fn sargs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}
