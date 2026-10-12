//! **断线 → 重连 → 续上，数格**（桌面那一侧）：同一条循环（`resident_client::feed::Feed`）× monitor 收事那一头（[`consume_remote`]）×
//! 旁路快照 × 续点 × 会话流，整串走生产代码；只有那台远端是替身（一条内存里的流，答 `history-tail` / `history-read`）。
//!
//! 剧本：那台一份记录 3 行（0..2）、宣告读完了 ⇒ 快照拉 3 行；实时来第 3 行；断网；断着时又写了 2 行（4、5）；
//! 重连、再宣告 ⇒ 从续点只拉断线期间新到的那两行。订了这个会话的那条订阅：每一行恰好一格、不重不漏，
//! 看得见 / 看不见各在该在的位置。只数格、不看墙钟（等条件封顶 `guard_core::wall::CEILING`）。

use super::*;
use crate::chan::wire::Item;
use crate::event_replay::ItemSink;
use std::collections::VecDeque;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

const ORIGIN: &str = "rc-box";
const SID: &str = "rc-s1";
const PATH: &str = "/home/rc/.claude/projects/p/rc-s1.jsonl";

/// 那台的那份记录：第 i 行正文 `{"t":"r<i>"}`，末端偏移按字节累计。
#[derive(Default)]
struct Remote {
    lines: Mutex<Vec<String>>,
    /// 每条连接接上之后先发的那几帧（一条连接取一份）。
    scripts: Mutex<VecDeque<Vec<String>>>,
    /// 拨它 ⇒ 接着的那一条当场断（断网）。
    drop_link: tokio::sync::Notify,
    attaches: std::sync::atomic::AtomicUsize,
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
            "history-tail" => {
                let total = ends.len();
                let n = args["n"].as_u64().unwrap() as usize;
                let tail_from = total.saturating_sub(n);
                serde_json::json!({"total": total, "tail_from": tail_from,
                    "split_at": start(tail_from), "end": ends.last().copied().unwrap_or(0)})
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

const HELLO: &str = r#"{"kind":"hello","v":1,"build_id":"rc-build","host_arch":"x86_64","claude_dir":"/c","commands":["ping","cancel","history-tail","history-read"]}"#;

async fn serve(s: tokio::io::DuplexStream, remote: Arc<Remote>) {
    let mut r = tokio::io::BufReader::new(s);
    let _ = r.get_mut().write_all(format!("{HELLO}\n").as_bytes()).await;
    let mut line = String::new();
    if r.read_line(&mut line).await.unwrap_or(0) == 0 {
        return;
    }
    let _ = r.get_mut().write_all(b"{\"attach\":\"ok\"}\n").await;
    let script = remote
        .scripts
        .lock()
        .unwrap()
        .pop_front()
        .unwrap_or_default();
    for f in script {
        let _ = r.get_mut().write_all(format!("{f}\n").as_bytes()).await;
    }
    loop {
        line.clear();
        let got = tokio::select! {
            n = r.read_line(&mut line) => n.unwrap_or(0),
            () = remote.drop_link.notified() => 0,
        };
        if got == 0 {
            return;
        }
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        let data = remote.answer(v["cmd"].as_str().unwrap_or(""), &v["args"]);
        let reply = serde_json::json!({"kind":"reply","id":v["id"],"ok":true,"data":data});
        let _ = r.get_mut().write_all(format!("{reply}\n").as_bytes()).await;
    }
}

/// 那台远端的替身：链路是一条内存里的流；装得了后端（版本门问「本机」：恒接）。
struct Fake(Arc<Remote>);

impl Transport for Fake {
    type Stream = tokio::io::DuplexStream;
    fn stream(&self, command: String) -> Boxed<'_, Result<Self::Stream, Problem>> {
        let remote = Arc::clone(&self.0);
        Box::pin(async move {
            assert!(command.ends_with("--resident-attach"), "{command}");
            remote
                .attaches
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let (mine, theirs) = tokio::io::duplex(256 * 1024);
            tokio::spawn(serve(theirs, remote));
            Ok(mine)
        })
    }
    fn capture(&self, _command: String) -> Boxed<'_, Result<Exec, Problem>> {
        Box::pin(async {
            Ok(Exec {
                exit_status: Some(0),
                stdout: "{}".into(),
                stderr: String::new(),
            })
        })
    }
    fn installer(&self) -> Option<&dyn Install> {
        Some(self)
    }
}

impl Install for Fake {
    fn verdict<'a>(
        &'a self,
        _mine: &'a str,
        _theirs: &'a str,
        _replaced: bool,
    ) -> Boxed<'a, Result<resident_client::gate::Verdict, Problem>> {
        Box::pin(async {
            Ok(resident_client::gate::Verdict {
                replace: false,
                older: false,
            })
        })
    }
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_then_reconnect_then_resume_hands_each_line_over_exactly_once() {
    let remote = Arc::new(Remote::default());
    remote.grow(3);
    remote
        .scripts
        .lock()
        .unwrap()
        .push_back(vec![remote.primed()]);
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
    let cfg: RemoteConfig = serde_json::from_value(serde_json::json!({
        "host": "rc.invalid", "label": ORIGIN, "user": "rc",
    }))
    .unwrap();
    assert_eq!(cfg.origin_label(), ORIGIN);
    let (tx, rx) = tokio::sync::mpsc::channel(EVENT_QUEUE);
    let feed = Feed::start(
        feed_config(ORIGIN),
        Some(Arc::new(Fake(Arc::clone(&remote)))),
        queue_sink(tx),
    );
    let consumer = tokio::spawn(consume_remote(cfg, rx, replay.clone(), health));
    let got = |c: &Cells| c.0.lock().unwrap().clone();
    let lines = |c: &Cells| {
        c.0.lock()
            .unwrap()
            .iter()
            .filter(|x| x.starts_with("line "))
            .count()
    };
    until("快照拉到那 3 行", || lines(&cells) == 3).await;
    // 实时来第 3 行（写进那份记录、沿流推来）。
    remote.grow(4);
    let live = remote.line_frame(3);
    // 这条连接上推一帧：替身没有「之后再推」的口，换成下一条连接的剧本里补不了实时那一格 ⇒ 用一条只推它的连接：
    //   先断网，重连那一条的剧本是「实时第 3 行 ＋ 再断」。
    remote.scripts.lock().unwrap().push_back(vec![live]);
    remote.drop_link.notify_waiters();
    until("断了一次、重连上推来实时第 3 行", || {
        lines(&cells) == 4
    })
    .await;
    // 断网；断着的时候那台又写了 2 行。
    remote.grow(6);
    remote
        .scripts
        .lock()
        .unwrap()
        .push_back(vec![remote.primed()]);
    remote.drop_link.notify_waiters();
    until("重连之后从续点续上那 2 行", || {
        lines(&cells) == 6
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let cells = got(&cells);
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
    assert_eq!(
        remote.attaches.load(std::sync::atomic::Ordering::SeqCst),
        3,
        "三条连接：起来一条、断后各重连一条"
    );
    feed.close();
    consumer.abort();
}
