//! 这台书签那份后端自有文件：切换 / 删 / 加的列表逐格 · 两个写者同时加都在 · 读不懂不覆盖 · 不在 ⇒ 空。
//!
//! ⚠ 家目录一律是临时目录（`Own::issue_at`），真 `~/.cc-monitor/` 一个字节都不碰。

use super::*;
use cmd_wire::bookmarks::{BookmarkOp, Bookmarks, BookmarksEditReq, BookmarksReadReq};
use std::collections::BTreeSet;

fn home(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-bookmarks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn own(h: &std::path::Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_at(h)
}

fn edit(o: &crate::platform::fs::Own, op: BookmarkOp, dirs: &[&str]) -> Vec<String> {
    answer_edit(
        o,
        &BookmarksEditReq {
            op,
            dirs: dirs.iter().map(|s| s.to_string()).collect(),
            at: Vec::new(),
        },
    )
    .expect("写")
    .dirs
}

fn read(o: &crate::platform::fs::Own) -> Result<Bookmarks, Fail> {
    answer_read(o, &BookmarksReadReq::default())
}

fn read_at(o: &crate::platform::fs::Own, at: &str) -> Option<u32> {
    answer_read(
        o,
        &BookmarksReadReq {
            at: vec![at.into()],
        },
    )
    .expect("读")
    .here
    .get(at)
    .copied()
}

fn edit_at(o: &crate::platform::fs::Own, op: BookmarkOp, dir: &str, at: &[&str]) -> Bookmarks {
    answer_edit(
        o,
        &BookmarksEditReq {
            op,
            dirs: vec![dir.into()],
            at: at.iter().map(|s| s.to_string()).collect(),
        },
    )
    .expect("写")
}

fn here_of(pairs: &[(&str, u32)]) -> std::collections::BTreeMap<String, u32> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// 🔴 先红的那一条：问的时候带上此刻的目录 ⇒ 回包答它在书签里的第几条（按归一之后的形比）。
#[test]
fn a_read_with_the_current_dir_answers_where_it_sits() {
    let o = own(&home("here"));
    edit(&o, BookmarkOp::Add, &["/a/b", "/c"]);
    assert_eq!(read_at(&o, "/a//b/"), Some(0));
    assert_eq!(read_at(&o, "/c/"), Some(1));
    assert_eq!(read_at(&o, "/nope"), None, "不在书签里 ⇒ 缺");
    assert_eq!(read_at(&o, ""), None, "空路径 ⇒ 缺");
    assert!(read(&o).unwrap().here.is_empty(), "没问 ⇒ 空");
}

/// 几栏各在一个目录 ⇒ 一问各答一个下标（原样作键，不在的不列）。
#[test]
fn a_read_with_several_dirs_answers_each_of_them() {
    let o = own(&home("here-many"));
    edit(&o, BookmarkOp::Add, &["/a", "/b"]);
    let got = answer_read(
        &o,
        &BookmarksReadReq {
            at: vec!["/b/".into(), "/a".into(), "/nope".into()],
        },
    )
    .unwrap();
    assert_eq!(got.here, here_of(&[("/a", 0), ("/b/", 1)]));
}

/// 改完的那一份也答各栏的目录：★ 加上 ⇒ 有下标；删一条 ⇒ 别的那栏照改完那一份挪；存盘那一份不带它。
#[test]
fn an_edit_with_the_current_dir_answers_where_it_sits_after_the_edit() {
    let h = home("here-edit");
    let o = own(&h);
    edit(&o, BookmarkOp::Add, &["/first"]);
    let on = edit_at(&o, BookmarkOp::Toggle, "/srv/", &["/srv", "/first"]);
    assert_eq!(on.dirs, ["/first", "/srv"]);
    assert_eq!(on.here, here_of(&[("/first", 0), ("/srv", 1)]));
    let off = edit_at(&o, BookmarkOp::Remove, "/first", &["//srv/", "/first"]);
    assert_eq!(off.dirs, ["/srv"]);
    assert_eq!(off.here, here_of(&[("//srv/", 0)]));
    let raw = std::fs::read_to_string(h.join(relay_route_core::BOOKMARKS_REL)).unwrap();
    assert!(!raw.contains("here"), "存盘那一份不带「此刻」：{raw}");
}

#[test]
fn absent_reads_as_empty_and_writes_land_where_the_contract_says() {
    let h = home("absent");
    let o = own(&h);
    assert!(read(&o).unwrap().dirs.is_empty(), "文件不在 ⇒ 空的一份");
    edit(&o, BookmarkOp::Add, &["/srv/"]);
    let raw = std::fs::read_to_string(h.join(relay_route_core::BOOKMARKS_REL)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        v,
        serde_json::json!({ "version": 1, "dirs": ["/srv"] }),
        "存的是归一之后的形，顶层带声明表那一版"
    );
    assert_eq!(read(&o).unwrap().dirs, vec!["/srv".to_string()]);
}

#[test]
fn toggle_remove_and_add_follow_the_old_panel() {
    let o = own(&home("ops"));
    assert_eq!(edit(&o, BookmarkOp::Toggle, &["/home/pi"]), ["/home/pi"]);
    // 第二下是「取消」；`/home/pi/` 与 `/home/pi` 是同一条。
    assert!(edit(&o, BookmarkOp::Toggle, &["/home/pi/"]).is_empty());
    edit(&o, BookmarkOp::Toggle, &["/home/pi/"]);
    assert_eq!(
        edit(&o, BookmarkOp::Toggle, &["/var/log"]),
        ["/home/pi", "/var/log"]
    );
    assert_eq!(edit(&o, BookmarkOp::Remove, &["/home/pi/"]), ["/var/log"]);
    assert_eq!(
        edit(&o, BookmarkOp::Remove, &["/nope"]),
        ["/var/log"],
        "删一条不在的 ⇒ 不动"
    );
    // 加：在就不动（迁移重来一次不翻转）；空路径不加。
    assert_eq!(
        edit(&o, BookmarkOp::Add, &["/var/log/", "", "/srv", "/srv"]),
        ["/var/log", "/srv"]
    );
    assert_eq!(edit(&o, BookmarkOp::Toggle, &[""]), ["/var/log", "/srv"]);
}

/// 🔴 先红的那一条：两个写者同时加，两边加的都在（读—改—写在锁里）。
#[test]
fn two_writers_adding_at_once_lose_nothing() {
    let h = home("two");
    let n = 40usize;
    let spawn = |who: &'static str| {
        let h = h.clone();
        std::thread::spawn(move || {
            let o = own(&h);
            for i in 0..n {
                edit(&o, BookmarkOp::Add, &[&format!("/{who}/{i}")]);
            }
        })
    };
    let (a, b) = (spawn("a"), spawn("b"));
    a.join().unwrap();
    b.join().unwrap();
    let got: BTreeSet<String> = read(&own(&h)).unwrap().dirs.into_iter().collect();
    let want: BTreeSet<String> = ["a", "b"]
        .iter()
        .flat_map(|w| (0..n).map(move |i| format!("/{w}/{i}")))
        .collect();
    assert_eq!(got, want, "两个写者加的都要在");
}

#[test]
fn an_unreadable_file_is_said_and_never_overwritten() {
    let h = home("bad");
    let f = h.join(relay_route_core::BOOKMARKS_REL);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, b"{ not json").unwrap();
    let o = own(&h);
    assert_eq!(read(&o).unwrap_err().code, "bookmarks_unreadable");
    let e = answer_edit(
        &o,
        &BookmarksEditReq {
            op: BookmarkOp::Add,
            dirs: vec!["/x".into()],
            at: Vec::new(),
        },
    )
    .unwrap_err();
    assert_eq!(e.code, "bookmarks_unreadable");
    assert_eq!(std::fs::read(&f).unwrap(), b"{ not json", "读不懂 ⇒ 不覆盖");
}

#[test]
fn a_newer_file_is_said_and_never_overwritten() {
    let h = home("newer");
    let f = h.join(relay_route_core::BOOKMARKS_REL);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    let newer = br#"{"version":2,"dirs":["/a"]}"#;
    std::fs::write(&f, newer).unwrap();
    let o = own(&h);
    assert_eq!(read(&o).unwrap_err().code, "bookmarks_unreadable");
    let e = answer_edit(
        &o,
        &BookmarksEditReq {
            op: BookmarkOp::Add,
            dirs: vec!["/x".into()],
            at: Vec::new(),
        },
    )
    .unwrap_err();
    assert_eq!(e.code, "bookmarks_unreadable");
    assert_eq!(std::fs::read(&f).unwrap(), newer, "比这一版新 ⇒ 不覆盖");
}
