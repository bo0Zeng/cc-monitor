use super::*;
use crate::agents::claudecode::paths::projects_root;
use std::path::PathBuf;

fn fixture_project(root: &Path, dir_name: &str) -> PathBuf {
    let dir = root.join("projects").join(dir_name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, lines.join("\n")).unwrap();
    p
}

#[test]
fn session_paths_outside_projects_are_refused() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-ro-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-b");
    write_jsonl(&dir, "ok.jsonl", &[r#"{"type":"user"}"#]);
    // projects 外的真实文件 → 拒绝
    let outside = tmp.join("secret.jsonl");
    std::fs::write(&outside, "nope").unwrap();
    assert!(validate_session_path(&tmp, &outside.to_string_lossy()).is_err());
    // 非 jsonl → 拒绝
    let txt = dir.join("note.txt");
    std::fs::write(&txt, "x").unwrap();
    assert!(validate_session_path(&tmp, &txt.to_string_lossy()).is_err());
    std::fs::remove_dir_all(&tmp).ok();
}

// ── 按路径读会话的围栏也认各家合成历史面给的记录根 ─────────────────────
//
// 守的要求：「历史 / 账号 / tmux / MCP 四个面，本机与远端走同一条代码路径」——
// 本机冷读也改走后端的 `history-read` 之后，历史清单列得出的 Codex 会话必须经同一道围栏打得开。

fn loc1b_tree(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let tmp = std::env::temp_dir().join(format!("ccm-loc1b-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&tmp).ok();
    let home = tmp.join("claude");
    fixture_project(&home, "p");
    let codex_root = tmp.join("codex").join("sessions");
    std::fs::create_dir_all(codex_root.join("2026")).unwrap();
    (tmp, home, codex_root)
}

/// 两向：另外那几个根下的会话 ⇒ 放行；两类根都不在 ⇒ 拒（回 `projects/` 那一句）。
#[test]
fn loc1b_the_fence_admits_a_registered_history_root_and_nothing_else() {
    let (tmp, home, codex_root) = loc1b_tree("fence");
    let inside = write_jsonl(&codex_root.join("2026"), "rollout-x.jsonl", &["{}"]);
    let outside_dir = tmp.join("elsewhere");
    std::fs::create_dir_all(&outside_dir).unwrap();
    let outside = write_jsonl(&outside_dir, "rollout-y.jsonl", &["{}"]);
    let roots = vec![codex_root.clone()];

    let ok = validate_session_path_among(&home, &roots, inside.to_str().unwrap());
    assert_eq!(ok.unwrap(), inside.canonicalize().unwrap());

    let refused = validate_session_path_among(&home, &roots, outside.to_str().unwrap());
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("outside projects dir")),
        "{refused:?}"
    );
    // 正控：不给另外那个根 ⇒ 同一份也拒（放行确实来自那个根，不是围栏本来就漏）。
    assert!(validate_session_path_among(&home, &[], inside.to_str().unwrap()).is_err());
    std::fs::remove_dir_all(&tmp).ok();
}

/// 另外那几个根不收相对路径（相对路径的意思只在 `projects/` 下有定义）；非 `.jsonl` 照拒；符号链接逃出根照拒。
#[test]
fn loc1b_the_extra_roots_take_absolute_jsonl_paths_that_stay_inside() {
    let (tmp, home, codex_root) = loc1b_tree("shape");
    let roots = vec![codex_root.clone()];
    write_jsonl(&codex_root, "rel.jsonl", &["{}"]);
    assert!(validate_session_path_among(&home, &roots, "rel.jsonl").is_err());

    let txt = write_jsonl(&codex_root, "notes.txt", &["x"]);
    let e = validate_session_path_among(&home, &roots, txt.to_str().unwrap()).unwrap_err();
    assert!(e.contains("non-jsonl"), "{e}");

    #[cfg(unix)]
    {
        let secret_dir = tmp.join("secret");
        std::fs::create_dir_all(&secret_dir).unwrap();
        let secret = write_jsonl(&secret_dir, "s.jsonl", &["{}"]);
        let link = codex_root.join("link.jsonl");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        assert!(validate_session_path_among(&home, &roots, link.to_str().unwrap()).is_err());
    }
    std::fs::remove_dir_all(&tmp).ok();
}
