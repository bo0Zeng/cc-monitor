//! **断线 → 重连 → 续上，数格**（桌面那一侧）：本机后端转来的那台那一串（`relayed`：hello · 帧 · 断了）× 收的那一头（[`consume`]）×
//! 旁路快照 × 续点 × 会话流，整串走生产代码；只有本机后端是替身（一条内存里的流，照那台那份记录答 `history-tail` / `history-read`）。
//!
//! 剧本：那台一份记录 3 行（0..2）、宣告读完了 ⇒ 快照拉 3 行；断了、重连，那一条推来实时第 3 行；断了；断着时又写了 2 行（4、5）；
//! 重连、再宣告 ⇒ 从续点只拉断线期间新到的那两行。订了这个会话的那条订阅：每一行恰好一格、不重不漏，
//! 看得见 / 看不见各在该在的位置。只数格、不看墙钟（等条件封顶 `guard_core::wall::CEILING`）。

use super::*;
use crate::chan::wire::Item;
use crate::event_replay::{EventReplay, ItemSink};
use cmd_wire::frame::Relayed;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncBufReadExt;

const ORIGIN: &str = "rc-box";
const SID: &str = "rc-s1";
const PATH: &str = "/srv/rc/.claude/projects/p/rc-s1.jsonl";

/// 那台的那份记录：第 i 行正文 `{"t":"r<i>"}`，末端偏移按字节累计。
#[derive(Default)]
struct Remote {
    lines: Mutex<Vec<String>>,
}

impl Remote {
    fn line(i: usize) -> String {
        format!(r#"{{"t":"r{i}"}}"#)
    }
    fn ends(&self) -> Vec<u64> {
        let mut at = 0u64;
        self.lines
            .lock()
            .unwrap()
            .iter()
            .map(|l| {
                at += l.len() as u64 + 1;
                at
            })
            .collect()
    }
    fn grow(&self, to: usize) {
        let mut l = self.lines.lock().unwrap();
        while l.len() < to {
            let i = l.len();
            l.push(Self::line(i));
        }
    }
    fn line_frame(&self, seq: usize) -> String {
        serde_json::json!({
            "kind": "line", "session_id": SID, "path": PATH, "seq": seq,
            "record": {"t": format!("r{seq}")}, "cwd": "/w", "byte_offset": self.ends()[seq],
        })
        .to_string()
    }
    fn primed(&self) -> String {
        serde_json::json!({"kind": "session_primed", "sid": SID, "path": PATH,
            "lines": self.lines.lock().unwrap().len()})
        .to_string()
    }
    /// 一问的答（只答旁路快照那两问；别的一律 `{}`）。
    fn answer(&self, cmd: &str, args: &serde_json::Value) -> serde_json::Value {
        let ends = self.ends();
        let start = |i: usize| if i == 0 { 0 } else { ends[i - 1] };
        match cmd {
            // 那台的判法只取这里用得到的那一支：交来的那一份同一路径、没越过 ⇒ 从它接着；否则尾段。第一页就是整段。
            "history-tail" => {
                let total = ends.len();
                let end = ends.last().copied().unwrap_or(0);
                let n = args["n"].as_u64().unwrap() as usize;
                let tail_from = total.saturating_sub(n);
                let held = &args["held"];
                let resume = held["path"] == PATH
                    && held["next"].as_u64().is_some_and(|k| k as usize <= total);
                let (how, from) = if resume {
                    ("resume", held["next"].as_u64().unwrap() as usize)
                } else {
                    ("tail", tail_from)
                };
                let rows: Vec<serde_json::Value> = (from..total)
                    .map(|i| serde_json::json!({"end": ends[i], "hash": i, "record": {"t": format!("r{i}")}, "cwd": "/w"}))
                    .collect();
                serde_json::json!({"total": total, "tail_from": tail_from, "split_at": start(tail_from), "end": end,
                    "how": how, "from": from, "rows": rows, "next": end, "eof": true,
                    "held": {"path": PATH, "next": total, "next_byte": end, "mark": ""}})
            }
            "history-read" => {
                let offset = args["offset"].as_u64().unwrap();
                let until = args["until"]
                    .as_u64()
                    .unwrap_or(ends.last().copied().unwrap_or(0));
                let rows: Vec<serde_json::Value> = (0..ends.len())
                    .filter(|&i| start(i) >= offset && ends[i] <= until)
                    .map(|i| serde_json::json!({"end": ends[i], "hash": i, "record": {"t": format!("r{i}")}, "cwd": "/w"}))
                    .collect();
                serde_json::json!({"rows": rows, "next": until, "eof": true})
            }
            _ => serde_json::json!({}),
        }
    }
}

/// 替身的握手：typed 那两条（`history-tail` · `history-read`）与帧表照这一版的形状指纹交（对不上的 monitor 事前就拒 / 不解）。
fn hello() -> String {
    let commands = ["ping", "cancel", "history-tail", "history-read"];
    let shapes: serde_json::Map<String, serde_json::Value> = commands
        .iter()
        .filter_map(|c| cmd_wire::shapes::shape_of(c).map(|h| (c.to_string(), h.into())))
        .collect();
    let frames: serde_json::Map<String, serde_json::Value> = cmd_wire::frame_shapes::FRAMES
        .iter()
        .map(|(k, h)| (k.to_string(), (*h).into()))
        .collect();
    serde_json::json!({"kind": "hello", "build_id": "rc-build", "host_arch": "x86_64",
        "commands": commands, "shapes": shapes, "frames": frames})
    .to_string()
}

/// 订阅收到的那几格：`line <seq>` · `seen` · `unseen` · 别的照名字。
#[derive(Default)]
struct Cells(Mutex<Vec<String>>);

impl ItemSink for Cells {
    fn deliver(&self, _: &str, _: u64, items: Vec<Item>) {
        let mut c = self.0.lock().unwrap();
        for i in items {
            c.push(match i {
                Item::Frame { body, .. } => {
                    let v: serde_json::Value = serde_json::from_slice(&body.0).unwrap();
                    // 一行那一格：`{line: {seq, record, catch_up, …}}`（正文就是第 seq 行，替身写的 `r<seq>`）。
                    match (v["line"]["seq"].as_u64(), v["line"]["record"]["t"].as_str()) {
                        (Some(n), Some(t)) if t == format!("r{n}") => format!("line {n}"),
                        _ => format!("frame {v}"),
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
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "等不到：{what}");
}

/// 本机后端那条通道的替身（登记在 `<local>`）：问那台的那几问（信封带 `machine`）经它转给那台 —— 这里就是照那台那份记录答。
/// 别台 · 不带 `machine` 的一问 ⇒ 判错（快照问那台只许走这一条）。
fn local_backend_for(remote: Arc<Remote>) -> Arc<crate::inbound_client::InboundClient> {
    let (mine, theirs) = tokio::io::duplex(256 * 1024);
    let hello = crate::inbound_client::witness_of(&crate::frame_fixtures::our_hello(&[]))
        .expect("是 Hello 帧");
    let client = crate::inbound_client::park(mine).into_client(hello);
    crate::inbound_client::register(client.clone());
    let c = client.clone();
    tokio::spawn(async move {
        let mut r = tokio::io::BufReader::new(theirs);
        let mut line = String::new();
        while r.read_line(&mut line).await.unwrap_or(0) > 0 {
            let v: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(v["machine"], ORIGIN, "问那台没经本机后端转：{v}");
            let data = remote.answer(v["cmd"].as_str().unwrap_or(""), &v["args"]);
            c.route_reply(
                v["id"].as_str().unwrap(),
                true,
                None,
                None,
                None,
                Some(data),
            );
            line.clear();
        }
    });
    client
}

/// 那台接上一条连接：本机后端先转它的 hello，再转这条连接上的那几帧。
async fn attach(tx: &tokio::sync::mpsc::Sender<LocalItem>, script: Vec<String>) {
    for line in std::iter::once(hello()).chain(script) {
        tx.send(LocalItem::Frame(Ordered::Relayed(Relayed::Line {
            machine: ORIGIN.into(),
            line,
        })))
        .await
        .unwrap();
    }
}

/// 那台这一条连接断了。
async fn drop_link(tx: &tokio::sync::mpsc::Sender<LocalItem>) {
    tx.send(LocalItem::Frame(Ordered::Relayed(Relayed::Ended {
        machine: ORIGIN.into(),
    })))
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_then_reconnect_then_resume_hands_each_line_over_exactly_once() {
    let _g = crate::inbound_client::local_origin_test_lock();
    let remote = Arc::new(Remote::default());
    let local = local_backend_for(Arc::clone(&remote));
    remote.grow(3);
    let replay = Arc::new(EventReplay::new());
    let cells = Arc::new(Cells::default());
    replay.attach_sink(cells.clone());
    replay.subscribe(
        "w",
        1,
        &crate::origin::Origin(ORIGIN.into()),
        &format!("{}/{SID}", crate::event_replay::SESSION_LINES_KIND),
        None,
        64,
    );
    let health: HealthOut = Arc::new(|_| Ok(()));
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let consumer = tokio::spawn(consume(rx, replay.clone(), health));
    let lines = |c: &Cells| {
        c.0.lock()
            .unwrap()
            .iter()
            .filter(|x| x.starts_with("line "))
            .count()
    };
    attach(&tx, vec![remote.primed()]).await;
    until("快照拉到那 3 行", || lines(&cells) == 3).await;
    // 断了、重连，那一条推来实时第 3 行。
    remote.grow(4);
    drop_link(&tx).await;
    attach(&tx, vec![remote.line_frame(3)]).await;
    until("断了一次、重连上推来实时第 3 行", || lines(&cells) == 4).await;
    // 断了；断着的时候那台又写了 2 行；重连、再宣告。
    drop_link(&tx).await;
    remote.grow(6);
    attach(&tx, vec![remote.primed()]).await;
    until("重连之后从续点续上那 2 行", || lines(&cells) == 6).await;
    let cells = cells.0.lock().unwrap().clone();
    let only_lines: Vec<&String> = cells.iter().filter(|c| c.starts_with("line ")).collect();
    assert_eq!(
        only_lines,
        ["line 0", "line 1", "line 2", "line 3", "line 4", "line 5"]
            .iter()
            .collect::<Vec<_>>(),
        "每一行恰好一格、按序（断线重连之后只续断线期间新到的）：{cells:?}"
    );
    let seen: Vec<&String> = cells
        .iter()
        .filter(|c| *c == "seen" || *c == "unseen")
        .collect();
    assert_eq!(
        seen,
        ["unseen", "seen", "unseen", "seen", "unseen", "seen"]
            .iter()
            .collect::<Vec<_>>(),
        "订阅起来时那台还没接上（看不见）→ 接上 → 断两回、各接回一回：{cells:?}"
    );
    drop(tx);
    let _ = tokio::time::timeout(guard_core::wall::CEILING, consumer).await;
    crate::inbound_client::unregister(&local);
}
