//! 留上一次好的：某片读不成给旧的 ＋ 原因 ＋ 时刻；整次失败给整份旧的；摘要只随输出变；不写工作区一个字节。

use super::*;
use crate::plan::fixture::{dump, dump_broken, fake_pb, scratch, who, workspace};

fn ran(doc: &Value) -> Ran {
    let raw = doc.to_string().into_bytes();
    Ran::Dump {
        doc: doc.clone(),
        raw,
    }
}

#[test]
fn a_slice_that_cannot_be_read_now_shows_the_last_good_one_with_reason_and_time() {
    let b = Book::default();
    let dir = Path::new("/w");
    let first = b.take(ran(&dump("/w")), dir, &who, 1_000).unwrap();
    assert_eq!(first["slices"][0]["stale"], Value::Null);
    let second = b.take(ran(&dump_broken("/w")), dir, &who, 2_000).unwrap();
    let sl = &second["slices"][0];
    assert_eq!(
        sl["cells"].as_array().unwrap().len(),
        4,
        "给的是上一次好的那一份"
    );
    assert_eq!(sl["stale"]["said"], "图.md 第 3 行：元行缺 id");
    assert_eq!(sl["stale"]["since"], 1_000);
    assert_eq!(
        b.view("/w", "alpha", "A1").as_deref(),
        Some("── A1 甲功能（能力 · 没做完）\n> id: A1")
    );
}

#[test]
fn a_slice_never_read_well_stays_an_error() {
    let b = Book::default();
    let out = b
        .take(ran(&dump_broken("/w")), Path::new("/w"), &who, 1)
        .unwrap();
    assert_eq!(out["slices"][0]["error"], "图.md 第 3 行：元行缺 id");
    assert_eq!(out["slices"][0]["stale"], Value::Null);
}

#[test]
fn a_failed_run_gives_the_whole_last_good_one_or_a_miss() {
    let b = Book::default();
    let dir = Path::new("/w/alpha");
    let fail = || Ran::Failed {
        said: "超时".into(),
        raw: None,
    };
    assert!(matches!(
        b.take(fail(), dir, &who, 1),
        Err(Miss::Failed { .. })
    ));
    b.take(ran(&dump("/w")), dir, &who, 5).unwrap();
    let out = b.take(fail(), dir, &who, 9).unwrap();
    assert_eq!(out["stale"]["said"], "超时");
    assert_eq!(out["stale"]["since"], 5);
    assert_eq!(out["slices"][0]["cells"].as_array().unwrap().len(), 4);
}

#[test]
fn the_rev_moves_only_with_the_output() {
    let b = Book::default();
    let dir = Path::new("/w");
    let a = b.take(ran(&dump("/w")), dir, &who, 1).unwrap();
    let again = b.take(ran(&dump("/w")), dir, &who, 2).unwrap();
    assert_eq!(a["rev"], again["rev"]);
    let mut changed = dump("/w");
    changed["slices"][0]["cells"][1]["status"] = serde_json::json!("没做完");
    let c = b.take(ran(&changed), dir, &who, 3).unwrap();
    assert_ne!(a["rev"], c["rev"]);
    assert_eq!(b.rev("/w"), c["rev"].as_str().map(str::to_string));
}

#[test]
fn dirs_are_remembered_both_ways() {
    let b = Book::default();
    b.take(Ran::NotWorkspace("x".into()), Path::new("/plain"), &who, 1)
        .unwrap_err();
    assert_eq!(b.known_dir(Path::new("/plain")), Some(None));
    b.take(ran(&dump("/w")), Path::new("/w/alpha"), &who, 1)
        .unwrap();
    assert_eq!(b.known_dir(Path::new("/w/alpha")), Some(Some("/w".into())));
    assert_eq!(b.workspaces(), vec!["/w".to_string()]);
}

/// 工作区的样子：夹具建的每个目录的修改时刻（目录里多一项、少一项都会动它）＋ 每份文件的字节。不遍历目录。
fn snapshot(root: &Path) -> Vec<(PathBuf, Option<std::time::SystemTime>, Vec<u8>)> {
    let dirs = [
        "",
        ".planned-build",
        ".planned-build/alpha",
        "alpha",
        "alpha/src",
    ];
    let files = [".env", "dump.json"];
    let mut out: Vec<_> = dirs
        .iter()
        .map(|d| {
            let p = root.join(d);
            let m = std::fs::metadata(&p).unwrap().modified().ok();
            (p, m, Vec::new())
        })
        .collect();
    out.extend(files.iter().map(|f| {
        let p = root.join(f);
        let m = std::fs::metadata(&p).unwrap().modified().ok();
        let b = std::fs::read(&p).unwrap();
        (p, m, b)
    }));
    out
}

/// 真起假 pb 读一次：工作区（计划仓 ＋ `.env` ＋ 仓库）跑前跑后逐字节不变、目录一项没多没少。
#[test]
fn reading_a_workspace_writes_nothing_in_it() {
    let d = scratch("book-readonly");
    let entry = fake_pb(&d.join("pb"), crate::plan::locate::PLUGIN_NAME, None);
    let ws = workspace(&d.join("ws"), &dump("/synthetic"));
    let before = snapshot(&ws);
    let b = Book::default();
    let out = b.read(&entry, &ws, &who, 1).unwrap();
    assert_eq!(out["slices"][0]["name"], "alpha");
    assert_eq!(snapshot(&ws), before);
}
