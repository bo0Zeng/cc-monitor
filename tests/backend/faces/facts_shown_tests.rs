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
