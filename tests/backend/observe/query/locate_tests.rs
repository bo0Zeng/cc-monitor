use super::*;
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
// 本机冷读也改走后端的 `history-read` 之后，历史清单列得出的合成历史那几家的会话必须经同一道围栏打得开。
// 替身家（`fake::runs::history`）的两个根：`<家>/chats/` 与归档根 `<家>/shelf/`。

use crate::agents::fake::runs::history as chats;

fn loc1b_tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ccm-loc1b-{tag}-{}", std::process::id()))
}

fn fam_fence() -> Option<PathBuf> {
    Some(loc1b_tmp("fence").join("fam"))
}

fn fam_shape() -> Option<PathBuf> {
    Some(loc1b_tmp("shape").join("fam"))
}

fn fam_shelf() -> Option<PathBuf> {
    Some(loc1b_tmp("shelf").join("fam"))
}

/// `(临时目录, 记录树那一家的家, 替身家的家, 只有替身家那一行的注册表)`；替身家两个根都建好。
fn loc1b_tree(
    tag: &str,
    fam: fn() -> Option<PathBuf>,
) -> (PathBuf, PathBuf, PathBuf, [crate::agents::Adapter; 1]) {
    let tmp = loc1b_tmp(tag);
    std::fs::remove_dir_all(&tmp).ok();
    let home = tmp.join("claude");
    fixture_project(&home, "p");
    let fam_home = fam().unwrap();
    std::fs::create_dir_all(chats::records_root(&fam_home)).unwrap();
    std::fs::create_dir_all(chats::shelf_root(&fam_home)).unwrap();
    (tmp, home, fam_home, [chats::row(fam)])
}

/// 两向：替身家根下的会话 ⇒ 放行；两类根都不在 ⇒ 拒（回 `projects/` 那一句）。
#[test]
fn loc1b_the_fence_admits_a_registered_history_root_and_nothing_else() {
    let (tmp, home, fam, reg) = loc1b_tree("fence", fam_fence);
    let inside = write_jsonl(&chats::records_root(&fam), &chats::file_name("a"), &["{}"]);
    let outside_dir = tmp.join("elsewhere");
    std::fs::create_dir_all(&outside_dir).unwrap();
    let outside = write_jsonl(&outside_dir, &chats::file_name("b"), &["{}"]);

    let ok = validate_session_path_among(&reg, &home, inside.to_str().unwrap());
    assert_eq!(ok.unwrap(), inside.canonicalize().unwrap());

    let refused = validate_session_path_among(&reg, &home, outside.to_str().unwrap());
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("outside projects dir")),
        "{refused:?}"
    );
    // 正控：注册表里没有那一家 ⇒ 同一份也拒（放行确实来自那一家的根，不是围栏本来就漏）。
    assert!(validate_session_path_among(&[], &home, inside.to_str().unwrap()).is_err());
    std::fs::remove_dir_all(&tmp).ok();
}

/// 另外那几个根不收相对路径（相对路径的意思只在 `projects/` 下有定义）；不是那一家的会话文件照拒；符号链接逃出根照拒。
#[test]
fn loc1b_the_extra_roots_take_absolute_session_paths_that_stay_inside() {
    let (tmp, home, fam, reg) = loc1b_tree("shape", fam_shape);
    let root = chats::records_root(&fam);
    write_jsonl(&root, &chats::file_name("rel"), &["{}"]);
    assert!(validate_session_path_among(&reg, &home, &chats::file_name("rel")).is_err());

    let txt = write_jsonl(&root, "notes.txt", &["x"]);
    let e = validate_session_path_among(&reg, &home, txt.to_str().unwrap()).unwrap_err();
    assert!(e.contains("non-jsonl"), "{e}");

    #[cfg(unix)]
    {
        let secret_dir = tmp.join("secret");
        std::fs::create_dir_all(&secret_dir).unwrap();
        let secret = write_jsonl(&secret_dir, &chats::file_name("s"), &["{}"]);
        let link = root.join(chats::file_name("link"));
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        assert!(validate_session_path_among(&reg, &home, link.to_str().unwrap()).is_err());
    }
    std::fs::remove_dir_all(&tmp).ok();
}

/// 那一家一个家报两个根（日期树之外还有归档根）⇒ 归档根底下那一份按路径照样读得开。
#[test]
fn an_archived_record_under_the_second_root_opens_by_path() {
    let (tmp, home, fam, reg) = loc1b_tree("shelf", fam_shelf);
    let shelved = write_jsonl(&chats::shelf_root(&fam), &chats::file_name("c"), &["{}"]);
    let got = validate_session_path_among(&reg, &home, shelved.to_str().unwrap());
    assert_eq!(got, Ok(shelved.canonicalize().unwrap()));
    std::fs::remove_dir_all(&tmp).ok();
}

/// ★ 活会话的状态一句照配上挂着请求的那一份写（同会话事实那一处）：pidfile 没说在等、只挂着一条答得了的 ⇒ 种类那一句、
/// 交「答得了」；挂着的是挡着终端的 ⇒ 不交；没挂 ⇒ 不交。
#[test]
fn the_live_line_is_written_from_the_need_dressed_with_the_hung_ask() {
    use crate::agents::asks::{specimen, Asked};
    let live = crate::history::live_table::LiveTable::default();
    let at = |clear: bool| {
        let mut a = Asked {
            ask: specimen(),
            since_ms: 1_000,
        };
        if clear {
            a.ask.choices.clear();
        }
        Some(a)
    };
    let line = |hung: Option<Asked>| {
        let mut f = crate::observe::recognize::facts::SessionFacts::default();
        line_with(
            crate::agents::SessionActivity::NeedsYou,
            &mut f,
            &live,
            "s",
            std::path::Path::new("/p/s.jsonl"),
            hung,
            4_000,
        )
        .expect("在等人就有那一句")
    };
    let answerable = line(at(false));
    assert!(answerable.asks);
    assert_eq!(
        answerable.text.0,
        copy_core::copy_text("beSession.needs.answer", &[])
    );
    assert!(!line(at(true)).asks);
    let none = line(None);
    assert!(!none.asks);
    assert_eq!(
        none.text.0,
        copy_core::copy_text("beSession.needs.unknown", &[])
    );
}
