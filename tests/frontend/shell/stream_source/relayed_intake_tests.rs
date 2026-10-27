//! **本机那条流上转来的别台**（`relayed`）：收的那一头按那台分到它自己那一份收口。
//!
//! 喂本机通道一串 `relayed`（那台的 hello · 一个活会话 · 一行 · 丢了一行 · 断了），订了那台的订阅依次收
//! `Seen` · 行 · `Gap` · `Unseen`，会话账在断了那一刻把那台的成品作废（`LinkLost`）；本机那条流断了 ⇒ 经它转来的各台一并作废。
//! 只数格、不看墙钟（等条件封顶 `guard_core::wall::CEILING`）。

use super::*;
use crate::chan::wire::Item;
use crate::event_replay::ItemSink;
use cmd_wire::frame::Relayed;
use std::sync::Mutex;

const BOX: &str = "rl-box";
const SID: &str = "rl-s1";

/// 订阅收到的那几格：`line <seq>` · `seen` · `unseen` · `gap` · 别的照名字。
#[derive(Default)]
struct Cells(Mutex<Vec<String>>);

impl ItemSink for Cells {
    fn deliver(&self, _: &str, _: u64, items: Vec<Item>) {
        let mut c = self.0.lock().unwrap();
        for i in items {
            c.push(match i {
                Item::Frame { body, .. } => {
                    let v: serde_json::Value = serde_json::from_slice(&body.0).unwrap();
                    match v["line"]["seq"].as_u64() {
                        Some(n) => format!("line {n}"),
                        None => "frame".into(),
                    }
                }
                Item::Seen { .. } => "seen".into(),
                Item::Unseen { .. } => "unseen".into(),
                Item::Gap { .. } => "gap".into(),
                Item::Closed { .. } => "closed".into(),
            });
        }
    }
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    let waited = tokio::time::timeout(guard_core::wall::CEILING, async {
        while !f() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "等不到：{what}");
}

fn hello() -> String {
    serde_json::to_string(&crate::frame_fixtures::our_hello(&[])).unwrap()
}

fn added() -> String {
    let f: Frame = cmd_wire::frame::Frame::SessionAdded(crate::frame_fixtures::session_added(
        SID,
        serde_json::json!({}),
    ));
    serde_json::to_string(&f).unwrap()
}

fn line(seq: u64) -> String {
    serde_json::json!({
        "kind": "line", "session_id": SID, "path": "/w/rl-s1.jsonl", "seq": seq,
        "record": {"t": "x"}, "cwd": "/w", "byte_offset": 10 * (seq + 1),
    })
    .to_string()
}

fn relay(r: Relayed) -> LocalItem {
    LocalItem::Frame(Ordered::Relayed(r))
}

fn from_box(line: String) -> LocalItem {
    from(BOX, line)
}

fn from(machine: &str, line: String) -> LocalItem {
    relay(Relayed::Line {
        machine: machine.into(),
        line,
    })
}

/// 起一个收的那一头，订那台那个会话的内容流。
fn harness(
    machine: &str,
) -> (
    tokio::sync::mpsc::Sender<LocalItem>,
    Arc<Cells>,
    tokio::task::JoinHandle<()>,
) {
    let replay = Arc::new(EventReplay::new());
    let cells = Arc::new(Cells::default());
    replay.attach_sink(cells.clone());
    replay.subscribe(
        "w",
        1,
        &crate::origin::Origin::named(machine),
        &format!("{}/{SID}", crate::event_replay::SESSION_LINES_KIND),
        None,
        64,
    );
    let health: HealthOut = Arc::new(|_| Ok(()));
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let consumer = tokio::spawn(consume(rx, replay, health));
    (tx, cells, consumer)
}

fn got(c: &Cells) -> Vec<String> {
    c.0.lock().unwrap().clone()
}

fn live(sid: &str) -> bool {
    live_at(BOX, sid)
}

fn live_at(machine: &str, sid: &str) -> bool {
    crate::session_book::book()
        .read()
        .is_live(&crate::origin::Origin::named(machine), sid)
}

/// 那台这一条连接：hello ⇒ `Seen`；行 ⇒ 那台的订阅收到；丢了一行 ⇒ `Gap`；断了 ⇒ `Unseen`、那台成品作废。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_relayed_connection_opens_feeds_and_ends_that_machines_intake() {
    // 本机那一格是进程内全局的：那台接上时名单台经它报（别的用例在那一格上登记的替身会收到）。
    let _g = crate::inbound_client::local_origin_test_lock();
    let (tx, cells, consumer) = harness(BOX);
    for item in [from_box(hello()), from_box(added()), from_box(line(0))] {
        tx.send(item).await.unwrap();
    }
    until("那台的那一行到了", || {
        got(&cells).contains(&"line 0".into())
    })
    .await;
    until("那台的会话进了会话账", || live(SID)).await;
    tx.send(relay(Relayed::Gap {
        machine: BOX.into(),
    }))
    .await
    .unwrap();
    tx.send(relay(Relayed::Ended {
        machine: BOX.into(),
    }))
    .await
    .unwrap();
    until("那台断了", || {
        got(&cells).iter().filter(|c| *c == "unseen").count() == 2
    })
    .await;
    let seen: Vec<String> = got(&cells).into_iter().filter(|c| c != "frame").collect();
    assert_eq!(
        seen,
        ["unseen", "seen", "line 0", "gap", "unseen"],
        "订阅起来时那台还没接上 → hello ⇒ 看得见 → 那一行 → 丢了一行 → 断了：{seen:?}"
    );
    assert!(!live(SID), "那台断了，它的成品还当活的留着");
    // 断了之后再来的一行（没有新的 hello）不进收口。
    tx.send(from_box(line(1))).await.unwrap();
    drop(tx);
    tokio::time::timeout(guard_core::wall::CEILING, consumer)
        .await
        .expect("收的那一头没收摊")
        .unwrap();
    assert!(
        !got(&cells).contains(&"line 1".into()),
        "断了之后、新连接的 hello 之前来的一行被收了"
    );
}

/// 本机那条流断了 ⇒ 经它转来的那台一并收：订阅收 `Unseen`、那台成品作废。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_local_stream_ending_ends_every_relayed_machine() {
    let _g = crate::inbound_client::local_origin_test_lock();
    const OTHER: &str = "rl-s2";
    const BOX2: &str = "rl-box2";
    let (tx, cells, consumer) = harness(BOX2);
    let added_other = {
        let f: Frame = cmd_wire::frame::Frame::SessionAdded(crate::frame_fixtures::session_added(
            OTHER,
            serde_json::json!({}),
        ));
        serde_json::to_string(&f).unwrap()
    };
    for item in [from(BOX2, hello()), from(BOX2, added_other)] {
        tx.send(item).await.unwrap();
    }
    until("那台的会话进了会话账", || live_at(BOX2, OTHER)).await;
    tx.send(LocalItem::StreamEnded).await.unwrap();
    until("那台跟着本机那条流断了", || {
        got(&cells).iter().filter(|c| *c == "unseen").count() == 2
    })
    .await;
    assert!(
        !live_at(BOX2, OTHER),
        "本机那条流断了，经它转来的那台的成品还当活的留着"
    );
    drop(tx);
    tokio::time::timeout(guard_core::wall::CEILING, consumer)
        .await
        .expect("收的那一头没收摊")
        .unwrap();
}
