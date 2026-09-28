//! 要求住址：主会话 09-28 裁 MIG-3b ①「`HostScope::Client` 那几行照旧由 monitor 答」（`设计/05 §14.3` E 组「足迹里 monitor 自己那几行」）。
//!
//! monitor 这一侧只答事实：环境三格 · 交来的绝对路径逐条 stat（文件 / 目录带一层名字 / 不在）。判定在后端（`tests/backend/footprint/`）。

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-fpclient-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

/// ★ 文件 · 目录（带一层名字）· 不在 三种事实分得开；环境原样；没给路径 ⇒ 只回环境。
#[test]
fn facts_tell_file_dir_and_absent_apart() {
    let d = temp_dir("kinds");
    std::fs::write(d.join("f"), "12345").unwrap();
    std::fs::create_dir(d.join("sub")).unwrap();
    std::fs::write(d.join("sub").join("a"), "").unwrap();
    let paths: Vec<String> = ["f", "sub", "nope"]
        .iter()
        .map(|n| d.join(n).display().to_string())
        .collect();
    let got = facts(
        Some(d.clone()),
        Some(d.join(".claude")),
        Some("/usr/bin".into()),
        &paths,
    )
    .unwrap();
    assert_eq!(got["stat"][&paths[0]], json!({ "kind": "file", "size": 5 }));
    assert_eq!(
        got["stat"][&paths[1]],
        json!({ "kind": "dir", "size": 0, "entries": ["a"] })
    );
    assert!(got["stat"][&paths[2]].is_null(), "不在的路径应当是 null");
    assert_eq!(got["env"]["home"], d.display().to_string());
    assert_eq!(got["env"]["path"], "/usr/bin");
    let bare = facts(Some(d.clone()), Some(d.join(".claude")), None, &[]).unwrap();
    assert_eq!(bare["stat"], json!({}));
    assert!(
        bare["env"]["path"].is_null(),
        "PATH 取不到 ⇒ null（查不动，不是空）"
    );
    assert!(
        facts(None, None, None, &[]).is_err(),
        "没有家目录就解不了 `~/…`，不猜"
    );
    let _ = std::fs::remove_dir_all(&d);
}
