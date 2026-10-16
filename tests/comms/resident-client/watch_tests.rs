//! 名单台：新连接没带过此刻那份 ⇒ 报一次、带过 ⇒ 不报 · 名单没变不报 · 连着改几次只报到最新那份、按序 · 应答 `from[]` 交宿主 ·
//! 读清单的宿主交回手上那份（attach 行与报的那一问同一个读法）。替身后端只认 `stream-watch`。
use super::*;
use crate::client::{park, BackendHello};
use tokio::io::AsyncBufReadExt;

fn hello() -> BackendHello {
    let shape = |c: &str| cmd_wire::shapes::shape_of(c).expect("typed 命令");
    crate::attach::Hello::parse(&format!(
        r#"{{"kind":"hello","build_id":"b","host_arch":"x86_64","commands":["cancel","stream-watch"],"shapes":{{"cancel":"{}","stream-watch":"{}"}}}}"#,
        shape("cancel"),
        shape("stream-watch")
    ))
    .expect("hello")
    .witness()
}

/// 替身那一头收到的每一问的入参；`froms` ＝ 每答一次取一份（没有 ⇒ 空）；`hold` 有值 ⇒ 每答之前等一个放行。
#[derive(Default)]
struct Peer {
    asks: Mutex<Vec<serde_json::Value>>,
    froms: Mutex<Vec<serde_json::Value>>,
    hold: Option<Arc<Notify>>,
}

/// 一条替身连接：答 `stream-watch`（交回 `from`）。
fn link(peer: &Arc<Peer>) -> Arc<InboundClient> {
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let client = park(mine).into_client(hello());
    let (c, p) = (Arc::clone(&client), Arc::clone(peer));
    tokio::spawn(async move {
        let mut r = tokio::io::BufReader::new(theirs);
        let mut line = String::new();
        while r.read_line(&mut line).await.unwrap_or(0) > 0 {
            let v: serde_json::Value = serde_json::from_str(&line).unwrap();
            line.clear();
            if v["cmd"] != "stream-watch" {
                continue;
            }
            p.asks.lock().unwrap().push(v["args"].clone());
            if let Some(h) = &p.hold {
                h.notified().await;
            }
            let from = {
                let mut f = p.froms.lock().unwrap();
                if f.is_empty() {
                    serde_json::json!([])
                } else {
                    f.remove(0)
                }
            };
            c.route_reply(
                v["id"].as_str().unwrap(),
                true,
                None,
                None,
                None,
                Some(serde_json::json!({ "from": from })),
            );
        }
    });
    client
}

fn set(xs: &[&str]) -> BTreeSet<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

fn sids(a: &serde_json::Value) -> serde_json::Value {
    a["sids"].clone()
}

/// 让发送任务跑完手上的（虚拟时钟：转几圈 ＋ 推一点时间）。
async fn settle() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
}

#[tokio::test(start_paused = true)]
async fn a_new_link_is_told_only_what_its_attach_line_did_not_carry() {
    let peer = Arc::new(Peer::default());
    let desk = Desk::new(|_| vec![], |_| {});
    desk.set(set(&["s1"]));
    // 没有 attach 行的载体（stdio）：接上即报。
    desk.up(&link(&peer), None);
    settle().await;
    assert_eq!(peer.asks.lock().unwrap().len(), 1);
    assert_eq!(
        sids(&peer.asks.lock().unwrap()[0]),
        serde_json::json!(["s1"])
    );
    // 名单没变、连接没换 ⇒ 不报。
    desk.set(set(&["s1"]));
    settle().await;
    assert_eq!(peer.asks.lock().unwrap().len(), 1, "没变不报");
    // 新连接的 attach 行带过此刻那份 ⇒ 不报。
    let carried = desk.ask_for_attach();
    assert_eq!(carried.sids, vec!["s1".to_string()]);
    desk.up(&link(&peer), Some(&carried));
    settle().await;
    assert_eq!(peer.asks.lock().unwrap().len(), 1, "attach 行带过的不再问");
    // 握手中名单变了（attach 行写出之后、接上之前）⇒ 接上后补报一次、是最新那份。
    let carried = desk.ask_for_attach();
    desk.set(set(&["s1", "s2"]));
    desk.up(&link(&peer), Some(&carried));
    settle().await;
    let asks = peer.asks.lock().unwrap().clone();
    assert_eq!(asks.len(), 2, "{asks:?}");
    assert_eq!(sids(&asks[1]), serde_json::json!(["s1", "s2"]));
}

/// 连着改三次、第一问还没答 ⇒ 至多再报一次、是最新那份；按序（同一台只一个发送任务）。
#[tokio::test(start_paused = true)]
async fn quick_changes_end_on_the_latest_and_never_overtake() {
    let gate = Arc::new(Notify::new());
    let peer = Arc::new(Peer {
        hold: Some(Arc::clone(&gate)),
        ..Default::default()
    });
    let desk = Desk::new(|_| vec![], |_| {});
    desk.up(&link(&peer), None);
    settle().await;
    assert_eq!(peer.asks.lock().unwrap().len(), 1, "接上即报（空名单）");
    desk.set(set(&["a"]));
    desk.set(set(&["a", "b"]));
    desk.set(set(&["a", "b", "c"]));
    settle().await;
    assert_eq!(
        peer.asks.lock().unwrap().len(),
        1,
        "第一问没答之前不发第二问"
    );
    gate.notify_one();
    settle().await;
    gate.notify_one();
    settle().await;
    let asks = peer.asks.lock().unwrap().clone();
    assert_eq!(
        asks.iter().map(sids).collect::<Vec<_>>(),
        vec![serde_json::json!([]), serde_json::json!(["a", "b", "c"])],
        "中间那几份不发"
    );
}

/// 应答里的 `from[]` 原样交宿主；空的不交。
#[tokio::test(start_paused = true)]
async fn the_from_rows_go_to_the_host() {
    let peer = Arc::new(Peer::default());
    peer.froms
        .lock()
        .unwrap()
        .push(serde_json::json!([{ "sid": "a", "path": "/p/a.jsonl", "seq": 7 }]));
    let got: Arc<Mutex<Vec<Vec<WatchFrom>>>> = Arc::default();
    let g = Arc::clone(&got);
    let desk = Desk::new(|_| vec![], move |rows| g.lock().unwrap().push(rows));
    desk.set(set(&["a"]));
    desk.up(&link(&peer), None);
    settle().await;
    desk.set(set(&["a", "b"]));
    settle().await;
    assert_eq!(peer.asks.lock().unwrap().len(), 2);
    assert_eq!(
        *got.lock().unwrap(),
        vec![vec![WatchFrom {
            sid: "a".into(),
            path: "/p/a.jsonl".into(),
            seq: 7
        }]],
        "第二问答的是空的：不交"
    );
}

/// 读清单的宿主：attach 行与报的那一问都带上名单里各条手上那份（读在那一刻）；不读的交空的。
#[tokio::test(start_paused = true)]
async fn the_branch_lists_held_ride_along_on_both_paths() {
    let peer = Arc::new(Peer::default());
    let desk = Desk::new(
        |sids| {
            sids.iter()
                .map(|s| BranchHeld {
                    sid: s.clone(),
                    path: Some(format!("/p/{s}.jsonl")),
                    at: 12,
                    sum: Some("00ff".into()),
                })
                .collect()
        },
        |_| {},
    );
    desk.set(set(&["a"]));
    assert_eq!(
        serde_json::to_value(desk.ask_for_attach()).unwrap(),
        serde_json::json!({ "sids": ["a"], "branch": [
            { "sid": "a", "path": "/p/a.jsonl", "at": 12, "sum": "00ff" },
        ]})
    );
    desk.up(&link(&peer), None);
    settle().await;
    assert_eq!(
        peer.asks.lock().unwrap()[0]["branch"],
        serde_json::json!([{ "sid": "a", "path": "/p/a.jsonl", "at": 12, "sum": "00ff" }])
    );
    let bare = Desk::new(|_| vec![], |_| {});
    bare.set(set(&["a"]));
    assert_eq!(
        serde_json::to_value(bare.ask_for_attach()).unwrap(),
        serde_json::json!({ "sids": ["a"], "branch": [] })
    );
    assert_eq!(
        serde_json::to_value(BranchHeld::nothing("b")).unwrap(),
        serde_json::json!({ "sid": "b", "at": 0 })
    );
}

/// 报名单只此一处：后端以外的 Rust 生产码里，`stream-watch` 这个命令名与 [`WATCH_CMD`] 只在本文件出现（正控：本文件里两样都在）。
/// 宿主（桌面壳 · 手机核心）只交名单；哪一处又自己发这一问，这一条红。
#[test]
fn only_the_desk_says_stream_watch() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let me = "src/comms/resident-client/watch.rs";
    let mine = guard_core::production_code(&std::fs::read_to_string(repo.join(me)).unwrap());
    assert!(
        mine.contains("\"stream-watch\"") && mine.contains("WATCH_CMD"),
        "正控：本文件里没有那两样了"
    );
    let mut hits = Vec::new();
    let mut seen = 0usize;
    for root in [
        "src/comms",
        "src/frontend/shell/src",
        "src/frontend/filewin/src",
        "src/mobile/core-rs/rust",
    ] {
        for rel in guard_core::files_by_extension(&repo.join(root), "rs") {
            let path = format!("{root}/{}", rel.replace('\\', "/"));
            if path == me {
                continue;
            }
            seen += 1;
            let src =
                guard_core::production_code(&std::fs::read_to_string(repo.join(&path)).unwrap());
            for word in ["\"stream-watch\"", "WATCH_CMD"] {
                if src.contains(word) {
                    hits.push(format!("{path}: {word}"));
                }
            }
        }
    }
    assert!(seen > 50, "扫到的文件太少（{seen}）：根写错了");
    assert!(hits.is_empty(), "别处又自己报名单：{hits:?}");
}
