//! **本机那条流上转来的别台**（`relayed`）：收的那一头按那台分到它自己那一份收口。
//!
//! 喂本机通道一串 `relayed`（那台的 hello · 一个活会话 · 一行 · 丢了一行 · 断了），订了那台的订阅依次收
//! `Seen` · 行 · `Gap` · `Unseen`；那台各条的说不清是本机后端在 `ended` 之前转来的 `session_left`，壳一条都不合成；
//! 本机那条流断了 ⇒ 经它转来的各台的收口一并收，会话账照留（没有谁说说不清，这一侧也不说）。
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

/// 本机后端紧跟在那台一行会话帧后面转来的那一格成品（`seats`：那一条此刻的整条；`control/feeds.rs` 那一侧折，壳不折）。
fn seat_after(machine: &str, line: &str) -> LocalItem {
    let (sid, live, left) = match cmd_wire::frame::decode::<serde_json::Value>(line).unwrap() {
        cmd_wire::frame::Frame::SessionAdded(m) => (m.sid.clone(), Some(m), None),
        cmd_wire::frame::Frame::SessionLeft(l) => (l.sid.clone(), None, Some(l)),
        other => panic!("夹具只有宣告 · 离开：{other:?}"),
    };
    relay(Relayed::Seats {
        machine: machine.into(),
        seats: cmd_wire::frame::SessionSeats {
            seats: vec![cmd_wire::frame::SessionSeat::of(
                &sid,
                live.as_ref(),
                left.as_ref(),
                None,
                None,
            )],
            listed: false,
            whole: false,
        },
    })
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
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let consumer = tokio::spawn(consume(rx, replay));
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

/// 那台这一条连接：hello ⇒ `Seen`；行 ⇒ 那台的订阅收到；丢了一行 ⇒ `Gap`；断了 ⇒ `Unseen`；那台的会话照本机后端那一帧说不清落账。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_relayed_connection_opens_feeds_and_ends_that_machines_intake() {
    // 本机那一格是进程内全局的：那台接上时名单台经它报（别的用例在那一格上登记的替身会收到）。
    let _g = crate::inbound_client::local_origin_test_lock();
    let (tx, cells, consumer) = harness(BOX);
    for item in [
        from_box(hello()),
        from_box(added()),
        seat_after(BOX, &added()),
        from_box(line(0)),
    ] {
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
    // 本机后端在断的那一刻先转那台各条的说不清（`control/feeds.rs`）。
    let unclear = {
        let f: Frame = cmd_wire::frame::Frame::SessionLeft(crate::frame_fixtures::session_left(
            SID, "unclear", "T",
        ));
        serde_json::to_string(&f).unwrap()
    };
    tx.send(from_box(unclear.clone())).await.unwrap();
    tx.send(seat_after(BOX, &unclear)).await.unwrap();
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
    assert!(!live(SID), "本机后端说了说不清，账里还当活的留着");
    assert_eq!(
        crate::session_book::book()
            .read()
            .replay(&[(SID.to_string(), BOX.to_string())])
            .after
            .iter()
            .filter_map(|o| match o {
                crate::session_book::Out::Left { left, .. } => Some(left.face.life.clone()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![cmd_wire::frame::Life::Unclear],
        "F5 照本机后端那一帧再说说不清"
    );
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

/// 本机那条流断了 ⇒ 经它转来的那台一并收：订阅收 `Unseen`；会话账一条都不在这一侧改（本机后端没了，没有谁说说不清）。
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
    for item in [
        from(BOX2, hello()),
        from(BOX2, added_other.clone()),
        seat_after(BOX2, &added_other),
    ] {
        tx.send(item).await.unwrap();
    }
    until("那台的会话进了会话账", || live_at(BOX2, OTHER)).await;
    tx.send(LocalItem::StreamEnded).await.unwrap();
    until("那台跟着本机那条流断了", || {
        got(&cells).iter().filter(|c| *c == "unseen").count() == 2
    })
    .await;
    assert!(
        live_at(BOX2, OTHER),
        "本机那条流断了，壳自己把那台的会话改了（说不清只由本机后端说）"
    );
    drop(tx);
    tokio::time::timeout(guard_core::wall::CEILING, consumer)
        .await
        .expect("收的那一头没收摊")
        .unwrap();
}

/// ★ 本机那一台也只照本机后端折好的成品进会话账：本机那条流上一帧宣告原样走，紧跟的 `session_seats` 才让它算活。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_local_machine_enters_the_book_only_through_its_seats() {
    let _g = crate::inbound_client::local_origin_test_lock();
    const MINE: &str = "rl-local-1";
    let (tx, _cells, consumer) = harness("<local>");
    let meta = crate::frame_fixtures::session_added(MINE, serde_json::json!({}));
    let local = crate::origin::Origin::local();
    let is_live = || crate::session_book::book().read().is_live(&local, MINE);
    tx.send(LocalItem::Frame(Ordered::SessionAdded(meta.clone())))
        .await
        .unwrap();
    tx.send(LocalItem::Frame(Ordered::SessionSeats(
        cmd_wire::frame::SessionSeats {
            seats: vec![cmd_wire::frame::SessionSeat::of(
                MINE,
                Some(&meta),
                None,
                None,
                None,
            )],
            listed: false,
            whole: false,
        },
    )))
    .await
    .unwrap();
    until("本机的成品进了会话账", is_live).await;
    drop(tx);
    tokio::time::timeout(guard_core::wall::CEILING, consumer)
        .await
        .expect("收的那一头没收摊")
        .unwrap();
}

/// 订阅收到的那几格活卡的体（`live` 那一格）。
#[derive(Default)]
struct LiveBodies(Mutex<Vec<serde_json::Value>>, Mutex<u32>);

impl ItemSink for LiveBodies {
    fn deliver(&self, _: &str, _: u64, items: Vec<Item>) {
        for i in items {
            match i {
                Item::Frame { body, .. } => {
                    let v: serde_json::Value = serde_json::from_slice(&body.0).unwrap();
                    self.0.lock().unwrap().push(v["live"].clone());
                }
                Item::Unseen { .. } => *self.1.lock().unwrap() += 1,
                _ => {}
            }
        }
    }
}

/// ★ 本机那条流整行丢了（丢的可能是一格 `live_now`）⇒ 订了本机活卡的，那一处下一格交带来的整份、不接尾巴。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_local_line_makes_the_next_live_card_go_out_whole() {
    let _g = crate::inbound_client::local_origin_test_lock();
    let local = crate::origin::Origin::local();
    let replay = Arc::new(EventReplay::new());
    let got = Arc::new(LiveBodies::default());
    replay.attach_sink(got.clone());
    replay.subscribe(
        "w",
        1,
        &local,
        crate::event_replay::SESSION_LIVE_CARD_KIND,
        None,
        64,
    );
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let consumer = tokio::spawn(consume(rx, replay.clone()));
    let whole = serde_json::json!({"sid": "rl-live", "cards": []});
    let tail: cmd_wire::frame::LiveNow = serde_json::from_value(serde_json::json!({
        "change": {"t": "tail", "tail": {"sid": "rl-live", "card": 0, "keep": 0, "blocks": []}},
        "whole": whole,
    }))
    .unwrap();
    tx.send(LocalItem::LineLost).await.unwrap();
    // 排在它后面的那一件（流结束 ⇒ 订阅收 `Unseen`）到了 ⇒ 丢行那一格已经处理过。
    tx.send(LocalItem::StreamEnded).await.unwrap();
    // 订上那一刻那台还看不见（一格 `Unseen`），流结束再一格。
    until("流结束那一格到了", || *got.1.lock().unwrap() > 1).await;
    replay.on_live_now(&local, tail.clone());
    replay.on_live_now(&local, tail);
    let bodies = got.0.lock().unwrap().clone();
    assert_eq!(
        bodies
            .iter()
            .map(|b| b["t"].as_str().unwrap_or("").to_string())
            .collect::<Vec<_>>(),
        ["whole", "tail"],
        "丢过一行之后那一处先交整份：{bodies:?}"
    );
    drop(tx);
    tokio::time::timeout(guard_core::wall::CEILING, consumer)
        .await
        .expect("收的那一头没收摊")
        .unwrap();
}
