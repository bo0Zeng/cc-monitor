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
use crate::dial::remote_ask::{Remote, Said, Table};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

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
            self.run(dial, command, stdin).await.map_err(|_| Said {
                code: Some("resident_absent".into()),
                message: "该机后端未在运行".into(),
            })
        })
    }
}

fn table(machine: &str) -> Table {
    let t = Table::default();
    crate::dial::remote_ask::insert(&t, machine, json!({"host": "h"})).unwrap();
    t
}

async fn ask(
    peer: &Peer,
    copies: &Copies,
    table: &Table,
    fresh: bool,
) -> Result<Listing, (&'static str, String)> {
    listing_with(
        "devbox",
        fresh,
        &crate::Tz::default(),
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
    assert_eq!(e.0, "resident_absent", "{e:?}");
    assert!(e.1.contains("devbox"), "那一句没说是哪台：{e:?}");
    // 没登记过的那台 ⇒ 够不着，照实说。
    let fresh_copies = Copies::default();
    let e = listing_with(
        "elsewhere",
        false,
        &crate::Tz::default(),
        crate::platform::child::Deadline::secs(5),
        &t,
        &peer,
        &fresh_copies,
    )
    .await
    .unwrap_err();
    assert_eq!(e.0, "unreachable", "{e:?}");
}

/// ★ 判据 8：手上那份照读，不去问那台；同步没成记下那一句，再成了清掉。
#[tokio::test]
async fn what_is_held_reads_without_asking_and_remembers_a_failed_sync() {
    let mut peer = Peer::new(vec![row("s1", "一", 30)]);
    let (copies, t) = (Copies::default(), table("devbox"));
    assert!(matches!(kept_in(&copies, "devbox"), Kept::Unread));
    ask(&peer, &copies, &t, true).await.unwrap();
    let Kept::Got(l) = kept_in(&copies, "devbox") else {
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
    } = kept_in(&copies, "devbox")
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
        &crate::Tz::default(),
        crate::platform::child::Deadline::secs(5),
        &t,
        &peer,
        &fresh_copies,
    )
    .await
    .unwrap_err();
    assert!(matches!(kept_in(&fresh_copies, "devbox"), Kept::Failed(_)));
    peer.absent = false;
    ask(&peer, &copies, &t, true).await.unwrap();
    assert!(
        matches!(kept_in(&copies, "devbox"), Kept::Got(_)),
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
