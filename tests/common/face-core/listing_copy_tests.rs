//! 远端清单副本（`listing_copy.rs`，两端同一份）的判据：落盘那一形装得回 · 读不懂当没有 · 一台一个文件名。
//! 问那台 · 照答的那一半见后端 `remote_listing_tests.rs`（那里有替身远端）。
use super::*;
use std::collections::HashMap;
use std::sync::Arc;

fn row(sid: &str) -> RawRow {
    serde_json::from_value(serde_json::json!({
        "agent": "claude", "sessionId": sid, "projectPath": "/w/a", "projectName": "a",
        "aiTitle": sid, "firstUserExcerpt": "", "title": sid,
        "startedAt": 1, "updatedAt": 1, "jsonlPath": format!("/r/{sid}.jsonl"),
        "life": {"is": "ended"}, "messageCountApprox": 3, "isBg": false,
        "starred": false, "customTitle": null, "projectDir": "-w-a", "group": "claude:/w/a",
    }))
    .expect("一行")
}

/// 整份的一答（记号 `h1`）。
fn whole(sids: &[&str]) -> Patch<ListingHead, RawRow> {
    serde_json::from_value(serde_json::json!({
        "held": "h1", "same": false, "whole": true,
        "order": sids,
        "up": sids.iter().map(|s| (s.to_string(), serde_json::to_value(row(s)).unwrap())).collect::<serde_json::Map<_, _>>(),
        "head": null,
    }))
    .expect("一答")
}

/// 内存里的落盘处（几张副本表共用一个 ＝ 同一个目录）。
#[derive(Clone, Default)]
struct Mem(Arc<Mutex<HashMap<String, Vec<u8>>>>);

impl Shelf for Mem {
    fn load(&self, machine: &str) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(machine).cloned()
    }
    fn store(&self, machine: &str, body: &[u8]) {
        self.0.lock().unwrap().insert(machine.into(), body.to_vec());
    }
}

fn sids(l: &Listing) -> Vec<String> {
    l.rows.iter().map(|r| r.session_id.clone()).collect()
}

/// ★ 同步成了整份写进落盘处；换一张空副本表读同一个落盘处 ⇒ 手上那份 · 记号 · 落下的那一刻都回来了。
#[test]
fn a_new_table_on_the_same_shelf_gets_the_copy_back() {
    let shelf = Mem::default();
    let a = Copies::on(shelf.clone());
    a.took("devbox", whole(&["s1", "s2"]), 1234);
    let b = Copies::on(shelf.clone());
    match b.to_ask("devbox", true) {
        Ask::With(Some(h)) => assert_eq!(h.0, "h1"),
        other => panic!("重来之后没带落盘那份的记号：{other:?}"),
    }
    b.missed("devbox", "够不着".into());
    let Kept::Stale { listing, at_ms, .. } = b.kept("devbox") else {
        panic!("同步没成：落盘那份要照出、标明是旧的")
    };
    assert_eq!(sids(&listing), ["s1", "s2"]);
    assert_eq!(at_ms, 1234, "落下的那一刻要跟着落盘");
}

/// ★ 读不懂 · 版本对不上 ⇒ 当没有（不迁移）；没有落盘处 ⇒ 只在内存。
#[test]
fn an_unreadable_or_other_version_copy_counts_as_none() {
    let shelf = Mem::default();
    Copies::on(shelf.clone()).took("devbox", whole(&["s1"]), 1);
    let body = shelf.load("devbox").unwrap();
    let mut v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["version"], VERSION, "落盘那一形顶层带 version");
    v["version"] = (VERSION + 1).into();
    shelf.store("devbox", &serde_json::to_vec(&v).unwrap());
    assert!(matches!(
        Copies::on(shelf.clone()).kept("devbox"),
        Kept::Unread
    ));
    shelf.store("devbox", b"{");
    assert!(matches!(
        Copies::on(shelf.clone()).to_ask("devbox", false),
        Ask::With(None)
    ));
    let mem = Copies::default();
    mem.took("devbox", whole(&["s1"]), 1);
    assert!(matches!(mem.to_ask("devbox", false), Ask::Held(_)));
}

/// 文件名：一台一个、拼不出路径（`/` · `..` · 打头的 `.` 都写成 `%XX`）。
#[test]
fn a_machine_name_never_becomes_a_path() {
    assert_eq!(file_stem("devbox-2_a"), "devbox-2_a");
    assert_eq!(file_stem("../x"), "%2E%2E%2Fx");
    assert_eq!(file_stem("a.b"), "a%2Eb");
    assert_ne!(file_stem("a.b"), file_stem("a_b"));
    assert_eq!(file_stem(""), "%");
}
