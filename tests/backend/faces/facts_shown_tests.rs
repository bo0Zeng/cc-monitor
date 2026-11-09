//! 会话事实的目录显示形（`cwdShown`）：那台按自己的家目录把项目目录写成 `~/…`（写法同 `AccountRow.configDirShown`），两端照抄；
//! 「在哪」那一句（`whereLine`）里的目录也是这一形。不在家目录底下 ⇒ 原样；项目目录还没读到 ⇒ 不出现。
use super::*;
use std::path::PathBuf;

/// 一份最小的会话记录，开头带项目目录（`cwd`）；`cwd = None` ⇒ 开头不带目录。
fn record(tag: &str, cwd: Option<&str>) -> (PathBuf, PathBuf) {
    let home = test_home::scratch(tag);
    let dir = home.join("projects").join("-u-me-proj");
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

/// `(cwdShown, whereLine)`，用户家目录是 `/u/me`。
fn shown(home: &Path, path: &Path, called: Option<&str>) -> (serde_json::Value, serde_json::Value) {
    let reply = facts_in(
        home,
        Some(Path::new("/u/me")),
        HistoryFactsReq::Path(FactsByPath {
            path: path.to_string_lossy().into_owned(),
        }),
        &crate::observe::recognize::facts::ContextLimits::default(),
        &crate::common::time::Tz::default(),
        called,
    )
    .expect("事实该答得出");
    let v = serde_json::to_value(reply).unwrap();
    (v["cwdShown"].clone(), v["whereLine"].clone())
}

#[test]
fn a_directory_under_home_is_shown_from_the_tilde() {
    let (home, p) = record("shown-under", Some("/u/me/proj"));
    let (cwd, at) = shown(&home, &p, Some("devbox"));
    assert_eq!(cwd, serde_json::json!("~/proj"));
    assert_eq!(
        at,
        serde_json::json!(copy_core::copy_text(
            "terminal.head.where",
            &[("machine", "devbox"), ("dir", "~/proj")]
        )),
        "「在哪」那一句里的目录也是显示形"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn home_itself_is_the_tilde_and_elsewhere_stays_as_it_is() {
    let (home, p) = record("shown-home", Some("/u/me"));
    assert_eq!(shown(&home, &p, None).0, serde_json::json!("~"));
    let _ = std::fs::remove_dir_all(&home);
    let (home, p) = record("shown-else", Some("/u/meow/x"));
    assert_eq!(
        shown(&home, &p, None),
        (
            serde_json::json!("/u/meow/x"),
            serde_json::json!("/u/meow/x")
        ),
        "前缀相同、不在家目录底下 ⇒ 原样"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn without_a_directory_there_is_no_shown_form() {
    let (home, p) = record("shown-none", None);
    let (cwd, _) = shown(&home, &p, Some("devbox"));
    assert!(cwd.is_null(), "项目目录还没读到 ⇒ 不出现：{cwd}");
    let _ = std::fs::remove_dir_all(&home);
}

/// 项目名（`projectName`）：项目目录的最后一段，与历史清单 · 搜索行同一个函数（`plain_path_core::last_segment`）；
/// Windows 目录在 Linux 上的后端也切得开；项目目录还没读到 ⇒ 不出现。
#[test]
fn the_project_name_is_the_last_segment_of_the_project_dir() {
    let name = |home: &Path, path: &Path| {
        let reply = facts_in(
            home,
            Some(Path::new("/u/me")),
            HistoryFactsReq::Path(FactsByPath {
                path: path.to_string_lossy().into_owned(),
            }),
            &crate::observe::recognize::facts::ContextLimits::default(),
            &crate::common::time::Tz::default(),
            None,
        )
        .expect("事实该答得出");
        serde_json::to_value(reply).unwrap()["projectName"].clone()
    };
    let (home, p) = record("name-win", Some(r"C:\\w\\proj"));
    assert_eq!(name(&home, &p), serde_json::json!("proj"));
    let _ = std::fs::remove_dir_all(&home);
    let (home, p) = record("name-posix", Some("/u/me/proj/"));
    assert_eq!(name(&home, &p), serde_json::json!("proj"));
    let _ = std::fs::remove_dir_all(&home);
    let (home, p) = record("name-none", None);
    assert!(name(&home, &p).is_null(), "项目目录还没读到 ⇒ 不出现");
    let _ = std::fs::remove_dir_all(&home);
}

/// 搜索行的项目名也是那一种切法（清单行那一半在 `history/search_rules_tests.rs`）：Windows 目录在 Linux 上的后端也切得开，
/// 与会话事实的 `projectName` 同一个值。
#[test]
fn a_windows_project_dir_names_the_same_project_in_search_rows() {
    let (home, p) = record("name-search", Some(r"C:\\w\\proj"));
    let opts = crate::observe::search_query::SearchOpts {
        include_tools: false,
        scope: None,
        after_ms: 0,
        limit: 10,
        titles: false,
    };
    let mut searched = Vec::new();
    crate::observe::search_query::search_into(&home, "q", &opts, &mut |h| {
        searched.push(serde_json::to_value(h).unwrap()["projectName"].clone());
        Ok(())
    })
    .expect("搜得动");
    let _ = std::fs::remove_dir_all(&home);
    let _ = p;
    assert_eq!(searched, [serde_json::json!("proj")]);
}
