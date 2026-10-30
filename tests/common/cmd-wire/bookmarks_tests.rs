//! [`super`] 的判据：目录归一（逐格 · 生成语料对拍）· 两条命令的线上形。

use super::*;

/// 书签路径 `normalize` 的原样例，外加几格手推。
#[test]
fn normalize_matches_the_old_panel_case_by_case() {
    let cases: &[(&str, &str)] = &[
        ("/a//b/", "/a/b"),
        ("/", "/"),
        ("//", "/"),
        ("/home/pi", "/home/pi"),
        ("/home/pi/", "/home/pi"),
        ("///srv///data///", "/srv/data"),
        ("", ""),
        ("rel/dir/", "rel/dir"),
        ("/文档/项目/", "/文档/项目"),
    ];
    for (input, want) in cases {
        assert_eq!(normalize_dir(input), *want, "normalize_dir({input:?})");
    }
}

/// 两步替换的对照写法（反复 `replace` 到不动为止、再去尾）：与生产那一趟扫描刻意不同写法。
fn old_two_step(path: &str) -> String {
    let mut c = path.to_string();
    while c.contains("//") {
        c = c.replace("//", "/");
    }
    if c == "/" {
        return c;
    }
    c.strip_suffix('/').map(str::to_string).unwrap_or(c)
}

fn corpus() -> Vec<String> {
    let alphabet = ['/', 'a', '文', '.'];
    (0..4096u32)
        .map(|n| {
            let mut s = String::new();
            let mut k = n;
            for _ in 0..6 {
                s.push(alphabet[(k % 4) as usize]);
                k /= 4;
            }
            s
        })
        .collect()
}

#[test]
fn normalize_agrees_with_the_two_step_rewrite_on_a_generated_corpus() {
    let all = corpus();
    for s in &all {
        assert_eq!(normalize_dir(s), old_two_step(s), "路径 {s:?}");
    }
    assert_eq!(all.len(), 4096);
    // 语料自检：三种要紧的形都真在人群里，否则这条对拍是空转的。
    assert!(all.iter().any(|s| s.contains("//")));
    assert!(all
        .iter()
        .any(|s| s.len() > 1 && s.ends_with('/') && !s.ends_with("//")));
    assert!(all.iter().any(|s| s.chars().all(|c| c == '/')));
}

#[test]
fn the_two_commands_have_the_wire_shape_both_ends_share() {
    use crate::Call;
    assert_eq!(BookmarksReadReq::NAME, "bookmarks-read");
    assert_eq!(BookmarksEditReq::NAME, "bookmarks-edit");
    let edit = BookmarksEditReq {
        op: BookmarkOp::Toggle,
        dirs: vec!["/srv".into()],
        at: None,
    };
    assert_eq!(
        serde_json::to_value(&edit).unwrap(),
        serde_json::json!({ "op": "toggle", "dirs": ["/srv"] }),
        "没带此刻的目录 ⇒ 线上不出那一格"
    );
    assert!(
        serde_json::from_value::<BookmarksReadReq>(serde_json::json!({ "x": 1 })).is_err(),
        "多一个键 ⇒ 拒"
    );
    let r: Bookmarks = serde_json::from_value(serde_json::json!({ "dirs": ["/a"] })).unwrap();
    assert_eq!((r.dirs, r.here), (vec!["/a".to_string()], None));
}

/// 此刻的目录（`at`）两条都可缺；回包的 `here` 可缺、有就是下标。
#[test]
fn the_current_dir_and_its_answer_are_optional_cells() {
    let read: BookmarksReadReq = serde_json::from_value(serde_json::json!({})).unwrap();
    assert_eq!(read.at, None);
    let read: BookmarksReadReq =
        serde_json::from_value(serde_json::json!({ "at": "/a//b/" })).unwrap();
    assert_eq!(read.at.as_deref(), Some("/a//b/"));
    let edit = BookmarksEditReq {
        op: BookmarkOp::Remove,
        dirs: vec!["/x".into()],
        at: Some("/y".into()),
    };
    assert_eq!(
        serde_json::to_value(&edit).unwrap(),
        serde_json::json!({ "op": "remove", "dirs": ["/x"], "at": "/y" })
    );
    let here = Bookmarks {
        dirs: vec!["/a".into()],
        here: Some(0),
    };
    assert_eq!(
        serde_json::to_value(&here).unwrap(),
        serde_json::json!({ "dirs": ["/a"], "here": 0 })
    );
    assert_eq!(
        serde_json::to_value(Bookmarks::default()).unwrap(),
        serde_json::json!({ "dirs": [] }),
        "没答 ⇒ 线上不出那一格（存盘那一份也不带它）"
    );
}
