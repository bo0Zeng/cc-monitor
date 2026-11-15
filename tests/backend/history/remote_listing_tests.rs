//! 远端清单进本机后端（`history/remote_listing.rs`）的判据。
//!
//! # 守的要求（住址）
//!
//! 看远端那台的历史页时，本机后端自己去问那台的**常驻**（`--resident-call`，沿已有 SSH 连接），带着手上那份的记号问 `history-listing`，
//! 只拿变了的那几行，副本住本机后端；界面只问本机后端。那台常驻不在 ⇒ 照实失败，不在那台起一个冷进程顶上。
//!
//! # 判据
//!
//! 1. 副本从无到有 ⇒ 那台答一次整份；本机副本拼回来就是那台此刻的清单。
//! 2. 再 `fresh`、那台没变 ⇒ 那台答 `same`（回包里一行都不带）、本机副本不动。
//! 3. 那台一条会话变了 ⇒ 回包只带那一行。
//! 4. 那台重启过（另一份备忘）⇒ 整份。
//! 5. 不 `fresh`、手上有副本 ⇒ 不问那台。
//! 7. 本机后端交给那台的期限就是发起方给的那个（`--within-ms` 原样往下带）。
//! 8. 手上那份不去问就能读（历史页几台并成一列那一问用）：没问成过 ⇒ 说没读过；上一次同步没成 ⇒ 那一句 ＋ 旧副本 ＋ 它落下的时刻（照出、标明是旧的）；
//!    从没读到过又没成 ⇒ 只有那一句；再问成了 ⇒ 清掉。
//! 6. 问的永远是那台的常驻（命令行是 `--resident-call`，命令名与入参走 stdin）；那台常驻不在 ⇒ `resident_absent`、那句说的是那台的名字。
//!
//! # 买不到
//!
//! - 🔴 真 SSH 与真远端常驻（对面是替身：照生产那一半 [`answer_with`] 现答）。

use super::*;
use crate::common::said::Said;
use crate::dial::remote_ask::{Remote, Table};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

/// 落在 `own` 那个数据目录底下的副本表（一台一份）：换一张空副本表读同一个目录 ＝ 重启过。
fn disk_copies(own: Own) -> Copies {
    Copies::on(DiskShelf(own))
}

/// 落在给定数据目录底下的那一个落盘处（同生产那一处的读写）。
struct DiskShelf(Own);

impl Shelf for DiskShelf {
    fn load(&self, machine: &str) -> Option<Vec<u8>> {
        load_at(&self.0, machine)
    }
    fn store(&self, machine: &str, body: &[u8]) {
        store_at(&self.0, machine, body);
    }
}

fn row(sid: &str, title: &str, at: i64) -> RawRow {
    serde_json::from_value(json!({
        "agent": "claude", "sessionId": sid, "projectPath": "/w/a", "projectName": "a",
        "aiTitle": title, "firstUserExcerpt": "", "title": title,
        "startedAt": at, "updatedAt": at, "jsonlPath": format!("/r/{sid}.jsonl"),
        "life": {"is": "ended"}, "messageCountApprox": 3, "isBg": false,
        "starred": false, "customTitle": null, "projectDir": "-w-a", "group": "claude:/w/a",
    }))
    .expect("一行")
}

fn there(rows: Vec<RawRow>) -> Listing {
    Listing {
        rows,
        failed: vec![cmd_wire::listing::FailedDir {
            agent: crate::agents::default_kind().into(),
            project_dir: "-bad".into(),
            error: "读不了".into(),
        }],
        annotations: Some(Default::default()),
        annotations_notice: None,
        rules: Default::default(),
    }
}

/// 那台：一份备忘（换一份 ＝ 重启过）＋ 此刻的清单；记下每一问的命令行与回包。
struct Peer {
    views: Mutex<Views>,
    now: Mutex<Listing>,
    absent: bool,
    asked: Mutex<Vec<(String, Value)>>,
    said: Mutex<Vec<Value>>,
}

impl Peer {
    fn new(rows: Vec<RawRow>) -> Peer {
        Peer {
            views: Mutex::new(Views::new(1)),
            now: Mutex::new(there(rows)),
            absent: false,
            asked: Mutex::new(Vec::new()),
            said: Mutex::new(Vec::new()),
        }
    }
    fn last(&self) -> Value {
        self.said.lock().unwrap().last().cloned().expect("问过")
    }
    fn times(&self) -> usize {
        self.asked.lock().unwrap().len()
    }
}

impl Remote for Peer {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            let line: Value =
                serde_json::from_str(stdin.as_deref().unwrap_or("")).expect("stdin 一行");
            self.asked.lock().unwrap().push((command, line.clone()));
            if self.absent {
                return Err("absent".into());
            }
            let req: HistoryListingReq =
                serde_json::from_value(line["args"].clone()).expect("入参照类型");
            let patch = answer_with(
                &self.views.lock().unwrap(),
                self.now.lock().unwrap().clone(),
                req.held.as_ref(),
            );
            let v = serde_json::to_value(&patch).unwrap();
            self.said.lock().unwrap().push(v.clone());
            Ok(v.to_string())
        })
    }

    fn run_coded<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, Said>> + Send + 'a>> {
        Box::pin(async move {
            self.run(dial, command, stdin)
                .await
                .map_err(|_| Said::new("resident_absent", "该机器的后端未在运行".to_string()))
        })
    }
}

fn table(machine: &str) -> Table {
    crate::dial::remote_ask::test_peers::named(&[machine])
}

async fn ask(
    peer: &Peer,
    copies: &Copies,
    table: &Table,
    fresh: bool,
) -> Result<Listing, crate::common::said::Said> {
    listing_with(
        "devbox",
        fresh,
        &crate::common::time::Tz::default(),
        crate::platform::child::Deadline::secs(5),
        table,
        peer,
        copies,
    )
    .await
}

fn sids(l: &Listing) -> Vec<String> {
    l.rows.iter().map(|r| r.session_id.clone()).collect()
}

/// ★ 判据 1 · 2 · 3 · 5。
#[tokio::test]
async fn the_copy_starts_whole_then_only_changes_come_over() {
    let peer = Peer::new(vec![
        row("s1", "一", 30),
        row("s2", "二", 20),
        row("s3", "三", 10),
    ]);
    let (copies, t) = (Copies::default(), table("devbox"));
    let first = ask(&peer, &copies, &t, false).await.expect("头一次");
    assert_eq!(
        peer.last()["whole"],
        true,
        "副本从无到有那一问得是整份：{}",
        peer.last()
    );
    assert_eq!(sids(&first), ["s1", "s2", "s3"]);
    assert_eq!(first.failed.len(), 1, "头里那几格也拼回来");
    // 不 fresh ⇒ 不问那台。
    ask(&peer, &copies, &t, false).await.unwrap();
    assert_eq!(peer.times(), 1, "手上有副本、不 fresh，却又问了那台");
    // fresh、那台没变 ⇒ same，一行不带，副本不动。
    let again = ask(&peer, &copies, &t, true).await.unwrap();
    assert_eq!(peer.times(), 2);
    assert_eq!(peer.last()["same"], true, "{}", peer.last());
    assert_eq!(peer.last()["up"], json!({}));
    assert_eq!(sids(&again), sids(&first));
    // 那台一条会话变了 ⇒ 只带那一行。
    peer.now.lock().unwrap().rows[1] = row("s2", "二改", 20);
    let changed = ask(&peer, &copies, &t, true).await.unwrap();
    let said = peer.last();
    let up: Vec<&String> = said["up"].as_object().unwrap().keys().collect();
    assert_eq!(up, ["s2"], "{said}");
    assert_eq!(peer.last()["whole"], false);
    assert_eq!(changed.rows[1].title, "二改");
    assert_eq!(sids(&changed), ["s1", "s2", "s3"]);
}

/// ★ 判据 4：那台重启过（另一份备忘，记号对不上）⇒ 整份。
#[tokio::test]
async fn a_restarted_peer_answers_whole() {
    let peer = Peer::new(vec![row("s1", "一", 30)]);
    let (copies, t) = (Copies::default(), table("devbox"));
    ask(&peer, &copies, &t, true).await.unwrap();
    ask(&peer, &copies, &t, true).await.unwrap();
    assert_eq!(peer.last()["same"], true);
    *peer.views.lock().unwrap() = Views::new(1);
    let l = ask(&peer, &copies, &t, true).await.unwrap();
    assert_eq!(
        peer.last()["whole"],
        true,
        "那台重启过却没给整份：{}",
        peer.last()
    );
    assert_eq!(sids(&l), ["s1"]);
}

/// ★ 判据 6：问的是那台的常驻（`--resident-call`，命令名与入参在 stdin 那一行），不是那台的一次性 CLI 面；
/// 那台常驻不在 ⇒ `resident_absent`，那一句说的是那台的名字；副本不凭空造。
#[tokio::test]
async fn it_asks_the_resident_and_an_absent_one_fails_plainly() {
    let mut peer = Peer::new(vec![row("s1", "一", 30)]);
    let (copies, t) = (Copies::default(), table("devbox"));
    ask(&peer, &copies, &t, false).await.unwrap();
    let (line, stdin) = peer.asked.lock().unwrap()[0].clone();
    assert!(line.contains("'--resident-call'"), "{line}");
    assert!(
        line.contains("'--within-ms' '5000'"),
        "期限没照发起方的往下带：{line}"
    );
    assert!(
        !line.contains("--history-list"),
        "问了那台的一次性 CLI 面：{line}"
    );
    assert_eq!(stdin["cmd"], "history-listing");
    assert!(stdin["args"]["held"].is_null(), "头一问不带记号：{stdin}");
    peer.absent = true;
    let e = ask(&peer, &copies, &t, true).await.unwrap_err();
    assert_eq!(e.code.as_deref(), Some("resident_absent"), "{e:?}");
    assert!(e.said.contains("devbox"), "那一句没说是哪台：{e:?}");
    // 没登记过的那台 ⇒ 够不着，照实说。
    let fresh_copies = Copies::default();
    let e = listing_with(
        "elsewhere",
        false,
        &crate::common::time::Tz::default(),
        crate::platform::child::Deadline::secs(5),
        &t,
        &peer,
        &fresh_copies,
    )
    .await
    .unwrap_err();
    assert_eq!(e.code.as_deref(), Some("unreachable"), "{e:?}");
}

/// ★ 判据 8：手上那份照读，不去问那台；同步没成记下那一句，再成了清掉。
#[tokio::test]
async fn what_is_held_reads_without_asking_and_remembers_a_failed_sync() {
    let mut peer = Peer::new(vec![row("s1", "一", 30)]);
    let (copies, t) = (Copies::default(), table("devbox"));
    assert!(matches!(copies.kept_seen("devbox").0, Kept::Unread));
    ask(&peer, &copies, &t, true).await.unwrap();
    let Kept::Got(l) = copies.kept_seen("devbox").0 else {
        panic!("问成了却读不到手上那份")
    };
    assert_eq!(sids(&l), ["s1"]);
    assert_eq!(peer.times(), 1, "读手上那份去问了那台");
    let landed = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    peer.absent = true;
    ask(&peer, &copies, &t, true).await.unwrap_err();
    let Kept::Stale {
        listing,
        why,
        at_ms,
    } = copies.kept_seen("devbox").0
    else {
        panic!("同步没成：手上那份要照出、标明是旧的")
    };
    assert!(why.contains("devbox"), "{why}");
    assert_eq!(sids(&listing), ["s1"], "旧副本的行要照出");
    assert!(
        (landed - at_ms).abs() < 60_000,
        "标的时刻得是副本落下那一刻：{at_ms} vs {landed}"
    );
    // 从没读到过、同步又没成 ⇒ 只有那一句，没有行可出。
    let fresh_copies = Copies::default();
    let _ = listing_with(
        "devbox",
        true,
        &crate::common::time::Tz::default(),
        crate::platform::child::Deadline::secs(5),
        &t,
        &peer,
        &fresh_copies,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        fresh_copies.kept_seen("devbox").0,
        Kept::Failed(_)
    ));
    peer.absent = false;
    ask(&peer, &copies, &t, true).await.unwrap();
    assert!(
        matches!(copies.kept_seen("devbox").0, Kept::Got(_)),
        "再问成了那一句没清掉"
    );
}

/// 那台答整份：键是会话 ID、先后照清单、头带读不了的目录与注解。
#[test]
fn the_peer_half_keys_rows_by_session_and_keeps_the_head() {
    let views = Views::new(1);
    let p = answer_with(
        &views,
        there(vec![row("b", "乙", 2), row("a", "甲", 1)]),
        None,
    );
    assert!(p.whole);
    assert_eq!(
        p.order.as_deref(),
        Some(&["b".to_string(), "a".to_string()][..])
    );
    let head = p.head.expect("整份带头");
    assert_eq!(head.total, 2);
    assert_eq!(head.info.failed[0].project_dir, "-bad");
}

/// 一个数据目录（判据自己的，不碰真的家）上的写句柄。
fn own_at(tag: &str) -> (std::path::PathBuf, crate::platform::fs::Own) {
    let tmp = std::env::temp_dir().join(format!("ccm-remote-listing-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("data")).unwrap();
    let (data, home) = (
        tmp.join("data").to_string_lossy().into_owned(),
        tmp.to_string_lossy().into_owned(),
    );
    let own = crate::platform::fs::Own::issue_with(&|k| match k {
        relay_route_core::DATA_DIR_ENV => Some(data.clone()),
        "HOME" => Some(home.clone()),
        _ => None,
    });
    (tmp, own)
}

/// ★ 判据 9：副本落盘（`cache/remote-listing/<机器>.json`）；换一张空副本表读同一个目录（＝ 本机后端重启过）⇒
/// 头一问带的是盘上那份的记号、手上那份不问就读得到；盘上那份读不懂 ⇒ 当没有（不带记号问、整份）。
#[tokio::test]
async fn the_copy_lands_on_disk_and_a_restarted_backend_asks_with_it() {
    let (tmp, own) = own_at("restart");
    let peer = Peer::new(vec![row("s1", "一", 30), row("s2", "二", 20)]);
    let t = table("devbox");
    let before = disk_copies(own.clone());
    ask(&peer, &before, &t, true).await.unwrap();
    let held = peer.last()["held"].clone();
    assert!(held.is_string(), "{}", peer.last());
    let file = tmp.join("data/cache/remote-listing/devbox.json");
    assert!(file.is_file(), "同步成了却没落盘：{}", file.display());

    let after = disk_copies(own.clone());
    let Kept::Got(l) = after.kept_seen("devbox").0 else {
        panic!("重启之后读不到盘上那份")
    };
    assert_eq!(sids(&l), ["s1", "s2"]);
    ask(&peer, &after, &t, true).await.unwrap();
    let (_, stdin) = peer.asked.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        stdin["args"]["held"], held,
        "重启之后头一问没带盘上那份的记号：{stdin}"
    );
    assert_eq!(peer.last()["same"], true, "{}", peer.last());

    std::fs::write(&file, b"{\"version\":1,\"hand\":").unwrap();
    let broken = disk_copies(own);
    assert!(matches!(broken.kept_seen("devbox").0, Kept::Unread));
    ask(&peer, &broken, &t, true).await.unwrap();
    let (_, stdin) = peer.asked.lock().unwrap().last().cloned().unwrap();
    assert!(
        stdin["args"]["held"].is_null(),
        "读不懂的那份当了真：{stdin}"
    );
    assert_eq!(peer.last()["whole"], true);
    let _ = std::fs::remove_dir_all(&tmp);
}

/// 副本表那一半（[`Copies`]：不问那台、不碰文件系统）的判据：落盘那一形装得回 · 读不懂当没有 · 一台一个文件名。
mod copies {
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
        let Kept::Stale { listing, at_ms, .. } = b.kept_seen("devbox").0 else {
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
            Copies::on(shelf.clone()).kept_seen("devbox").0,
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
}

/// 副本的代号：从没换过 ⇒ 没有；每换一次（问成了 · 问不成记下那一句）就严格变大；代号与交出去的那一份同一把锁里读。
/// 界面照它认同一台哪一包新（几问回包先后不定，旧的那一包到了不盖新的）。
#[tokio::test]
async fn the_copy_seen_mark_grows_with_every_change() {
    let peer = Peer::new(vec![row("s1", "一", 30)]);
    let (copies, t) = (Copies::default(), table("devbox"));
    assert!(matches!(copies.kept_seen("devbox"), (Kept::Unread, None)));
    ask(&peer, &copies, &t, false).await.expect("头一次");
    let (k, a) = copies.kept_seen("devbox");
    assert!(matches!(k, Kept::Got(_)), "{k:?}");
    let a = a.expect("换过一次就有代号");
    ask(&peer, &copies, &t, true).await.expect("再问一次");
    let b = copies.kept_seen("devbox").1.unwrap();
    assert!(b > a, "问成了一次代号没变大：{a} → {b}");
    copies.missed("devbox", "断了".into());
    let (k, c) = copies.kept_seen("devbox");
    assert!(matches!(k, Kept::Stale { .. }), "{k:?}");
    assert!(c.unwrap() > b, "问不成记下那一句，代号没变大");
}
