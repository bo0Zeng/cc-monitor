//! `history-find` 的结果头 `head` 由后端照「这一页之后已交多少 · 全量多少 · 搜的哪一类」写好那一句，界面只摆。夹具只造结构。

use super::*;
use copy_core::copy_text;
use std::path::PathBuf;

fn scratch(tag: &str) -> (PathBuf, String) {
    let d = std::env::temp_dir().join(format!("ccm-find-head-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let dir = d.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    let body: String = (1..=3)
        .map(|i| {
            format!(r#"{{"type":"user","uuid":"hit-{i}","message":{{"content":"zqx {i}"}}}}"#)
                + "\n"
        })
        .collect();
    std::fs::write(&p, body).unwrap();
    let path = p.to_string_lossy().into_owned();
    (d, path)
}

fn find(home: &std::path::Path, path: &str, query: &str, user: bool, skip: u64) -> HistoryFind {
    let req: HistoryFindReq = serde_json::from_value(serde_json::json!({
        "path": path,
        "query": query,
        "limit": 2,
        "skip": skip,
        "scope": if user { Some("user") } else { None },
    }))
    .unwrap();
    find_at(home, req, &Default::default()).unwrap()
}

#[test]
fn the_head_says_how_many_are_shown_after_this_page() {
    let (home, path) = scratch("pages");
    let first = find(&home, &path, "zqx", false, 0);
    assert_eq!((first.hits.len(), first.total), (2, 3));
    assert_eq!(
        first.head,
        copy_text("findStrip.status.truncated", &[("n", "2"), ("total", "3")])
    );
    assert!(!first.widen);
    let second = find(&home, &path, "zqx", false, 2);
    assert_eq!(
        second.head,
        copy_text("findStrip.status.count", &[("n", "3")])
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn no_hit_says_so_and_only_the_user_scope_offers_to_widen() {
    let (home, path) = scratch("none");
    let all = find(&home, &path, "nothing", false, 0);
    assert_eq!(
        (all.head, all.widen),
        (copy_text("kit.menu.noMatch", &[("q", "nothing")]), false)
    );
    let user = find(&home, &path, "nothing", true, 0);
    assert_eq!(
        (user.head, user.widen),
        (
            copy_text("sessionFind.status.noneScoped", &[("q", "nothing")]),
            true
        )
    );
    let _ = std::fs::remove_dir_all(&home);
}
