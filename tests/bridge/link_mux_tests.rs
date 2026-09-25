//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md`「链路四条」（monitor 这一侧：还信用 · 关链路 · 解帧）
//!
//! 核原文：`link-credit` 小节逐字「monitor 的做法：链路的读者每读走半个窗口就还一次（`link_mux.rs`）」·
//! `link-close` 小节逐字「monitor 侧的链路句柄被丢时自动发这一条」· `link_data` 行逐字
//! 「`data` = base64，标准字母表带补位；解码后 ≤ 32 KiB」—— 本族判的正是这几句。
//! 对端超窗当场判坏并出声：契约没写 monitor 怎么反应，那一半的要求是 `设计/01 §5 D4`（一条都不许静默忽略）。〔JA1 点址 2026-09-24〕
//!
//! 〔SR1a〕链路的 monitor 这一侧（`link_mux.rs`）的判据。
//!
//! 台架：一条内存管道两头 —— monitor 这头是**真的** `InboundClient`（经 `park → into_client`）＋
//! **真的**本机吸收点（`local_backend::absorb_local_frame`），对面是一个会说链路协议的小假后端
//! （记下每条请求、每条都回 `ok`，下行帧由用例自己塞）。后端那一半的真实现另有 `dial_link_tests`，
//! 两半接在一起的真进程读数见 `tests/evidence/SR1a-link-loopback.py`。
//!
//! ⚠ 链路表是**进程内全局**的 ⇒ 本文件的用例一律先拿 [`serial`] 那把锁（`fail_owned_by` 只结束同一个客户端开的，
//! 而每个用例各有自己的台架客户端 —— 锁是再加一道保险，不是靠它才对）。

use super::*;
use crate::backend::control::inbound_client::{park, BackendHello};
use crate::ssh_source::{parse_frame, InboundFrame};
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

fn serial() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

/// 台架：真 client ＋ 真吸收点 ＋ 假后端。
struct Rig {
    client: Arc<InboundClient>,
    /// 假后端收到的每一条请求（`{id, cmd, args}`）。
    seen: mpsc::UnboundedReceiver<Value>,
    /// 往 monitor 那头塞一行后端帧（不带换行）。
    to_monitor: mpsc::UnboundedSender<String>,
}

fn rig_fresh() -> Rig {
    rig()
}

fn rig() -> Rig {
    let (mon_w, be_r) = tokio::io::duplex(1 << 20);
    let (be_w, mon_r) = tokio::io::duplex(1 << 20);
    let hello = InboundFrame::Hello {
        v: 1,
        build_id: "t".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/tmp".into(),
        homes: vec![],
        capabilities: vec![],
        commands: ["link-open", "link-data", "link-credit", "link-close"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    let witness = BackendHello::from_hello_frame(&hello).expect("是 hello");
    let client = park(mon_w).into_client(witness);
    // monitor 这头的读循环：解帧 → **真的**吸收点。
    let c2 = Arc::clone(&client);
    tauri::async_runtime::spawn(async move {
        let mut lines = tokio::io::BufReader::new(mon_r).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            if let Some(f) = parse_frame(&l) {
                crate::backend::control::local_backend::absorb_local_frame(f, Some(&c2));
            }
        }
    });
    // 假后端的写出口（应答与用例塞的帧走同一个口，保序）。
    let (to_monitor, mut outq) = mpsc::unbounded_channel::<String>();
    tauri::async_runtime::spawn(async move {
        let mut w = be_w;
        while let Some(l) = outq.recv().await {
            if w.write_all(format!("{l}\n").as_bytes()).await.is_err() {
                break;
            }
        }
    });
    // 假后端的读口：记下请求，每条都回 ok。
    let (seen_tx, seen) = mpsc::unbounded_channel::<Value>();
    let reply = to_monitor.clone();
    tauri::async_runtime::spawn(async move {
        let mut lines = tokio::io::BufReader::new(be_r).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            let v: Value = serde_json::from_str(&l).expect("请求不是 JSON");
            let id = v["id"].as_str().unwrap_or_default().to_string();
            let _ = reply.send(format!(r#"{{"kind":"reply","id":{id:?},"ok":true}}"#));
            let _ = seen_tx.send(v);
        }
    });
    Rig {
        client,
        seen,
        to_monitor,
    }
}

impl Rig {
    async fn next(&mut self, cmd: &str) -> Value {
        loop {
            let v = tokio::time::timeout(Duration::from_secs(5), self.seen.recv())
                .await
                .unwrap_or_else(|_| panic!("5s 没等到 `{cmd}`"))
                .expect("假后端的请求口关了");
            if v["cmd"] == cmd {
                return v;
            }
        }
    }

    fn send_data(&self, link: &str, bytes: &[u8]) {
        let _ = self.to_monitor.send(format!(
            r#"{{"kind":"link_data","link":{link:?},"data":{:?}}}"#,
            b64_encode(bytes)
        ));
    }

    fn send_end(&self, link: &str, error: Option<&str>) {
        let line = match error {
            None => format!(r#"{{"kind":"link_end","link":{link:?}}}"#),
            Some(e) => format!(r#"{{"kind":"link_end","link":{link:?},"error":{e:?}}}"#),
        };
        let _ = self.to_monitor.send(line);
    }
}

fn pattern(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 13 % 251) as u8).collect()
}

const BUDGET: Duration = Duration::from_secs(5);

/// base64 对 **RFC 4648 §10** 的七条标准向量（与后端 `wire_tests::b64_matches_the_rfc_4648_test_vectors` 同一组，异源 = RFC）。
#[test]
fn the_link_codec_matches_the_rfc_4648_test_vectors() {
    let vectors = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];
    for (plain, enc) in vectors {
        assert_eq!(b64_encode(plain.as_bytes()), enc, "编 {plain:?}");
        assert_eq!(b64_decode(enc).unwrap(), plain.as_bytes(), "解 {enc:?}");
    }
    // 字母表最后两位（`+` `/`，RFC 4648 §4 表 1 的 62 / 63）：§10 那七条向量一个都用不到它们 ——
    // 死值验 K20 首跑实测：把字母表尾巴换成 URL 安全版（`-_`）七条照样绿。0xfb 0xff ⇒ 62 · 63 · 60。
    assert_eq!(b64_encode(&[0xfb, 0xff]), "+/8=");
    assert_eq!(b64_decode("+/8=").unwrap(), [0xfb, 0xff]);
    let all: Vec<u8> = (0..=255u8).collect();
    assert_eq!(b64_decode(&b64_encode(&all)).unwrap(), all);
    for bad in ["A", "AA=", "A===", "Zg==Zg==", "Zm9v!A==", "===="] {
        assert!(b64_decode(bad).is_err(), "{bad:?} 该被拒");
    }
}

/// 上行步长与后端一块的上限同一个数；窗口落在后端肯收的区间里（`[LINK_CHUNK_BYTES, MAX_WINDOW]`，出了区间后端拒开）。
/// 两个数从**后端源码**现抠（异源：另一个 crate 的常量定义）。
#[test]
fn the_chunk_cap_is_the_same_number_on_both_sides() {
    let src = include_str!("../../src/backend/dial/link.rs");
    let grab = |name: &str| -> u64 {
        let key = format!("pub const {name}: ");
        let at = src.find(&key).unwrap_or_else(|| panic!("后端没有 {name}"));
        let rest = &src[at + key.len()..];
        let expr = rest[rest.find('=').unwrap() + 1..rest.find(';').unwrap()].trim();
        // 只认本文件真在用的三种写法：`a * b` · `1 << k` · `k << m`。
        if let Some((a, b)) = expr.split_once('*') {
            a.trim().parse::<u64>().unwrap() * b.trim().parse::<u64>().unwrap()
        } else if let Some((a, b)) = expr.split_once("<<") {
            a.trim().parse::<u64>().unwrap() << b.trim().parse::<u64>().unwrap()
        } else {
            expr.parse()
                .unwrap_or_else(|_| panic!("{name} = {expr} 认不出"))
        }
    };
    assert_eq!(grab("LINK_CHUNK_BYTES"), LINK_STEP as u64);
    let max = grab("MAX_WINDOW");
    assert!(
        (LINK_STEP as u64..=max).contains(&LINK_WINDOW_BYTES),
        "monitor 的窗口 {LINK_WINDOW_BYTES} 不在后端肯收的区间 [{LINK_STEP}, {max}] 里 —— 后端会拒开每一条链路"
    );
}

/// ★ M4（下行 ＋ 信用）：假后端先发**恰好一窗**，等到信用再发剩下的；读出来的字节逐字相等，
/// 还回去的信用**恰好**是读走的那么多（按半窗取整），然后 `link_end` ⇒ EOF、表里摘掉。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn downstream_bytes_arrive_and_credit_goes_back_exactly() {
    let _g = serial();
    let mut rig = rig();
    let open = LinkStream::open(
        Arc::clone(&rig.client),
        serde_json::json!({"host": "h"}),
        BUDGET,
    );
    let (link, req) = tokio::join!(open, rig.next("link-open"));
    let mut link = link.expect("开链路");
    let id = req["args"]["link"].as_str().unwrap().to_string();
    assert_eq!(id, link.id);
    assert_eq!(req["args"]["window"], LINK_WINDOW_BYTES);
    assert_eq!(req["args"]["dial"]["host"], "h", "拨号请求没原样交过去");

    let total = LINK_WINDOW_BYTES as usize + LINK_WINDOW_BYTES as usize / 2;
    let want = pattern(total);
    // 先发恰好一窗（按块切）。
    for c in want[..LINK_WINDOW_BYTES as usize].chunks(LINK_STEP) {
        rig.send_data(&id, c);
    }
    let mut got = vec![0u8; LINK_WINDOW_BYTES as usize];
    link.read_exact(&mut got).await.expect("读第一窗");
    // 读走了一整窗 ⇒ 还了恰好两次半窗。
    let c1 = rig.next("link-credit").await;
    let c2 = rig.next("link-credit").await;
    assert_eq!(
        c1["args"]["bytes"].as_u64().unwrap() + c2["args"]["bytes"].as_u64().unwrap(),
        LINK_WINDOW_BYTES,
        "读走一窗，还回去的信用不等于一窗"
    );
    for c in want[LINK_WINDOW_BYTES as usize..].chunks(LINK_STEP) {
        rig.send_data(&id, c);
    }
    rig.send_end(&id, None);
    let mut rest = Vec::new();
    link.read_to_end(&mut rest).await.expect("读到 EOF");
    got.extend(rest);
    assert_eq!(got, want, "下行字节不齐或乱序");
    assert!(!lock().contains_key(&id), "链路收尾了还留在表里");
}

/// ★ M4（上行）：写进去的字节按块（≤ 一块上限）、按序变成 `link-data`，拼起来逐字相等；一次只有一块在途。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upstream_writes_become_ordered_link_data_blocks() {
    let _g = serial();
    let mut rig = rig();
    let open = LinkStream::open(Arc::clone(&rig.client), serde_json::json!({}), BUDGET);
    let (link, _) = tokio::join!(open, rig.next("link-open"));
    let mut link = link.expect("开链路");
    let want = pattern(3 * LINK_STEP + 77);
    let payload = want.clone();
    let writer = tokio::spawn(async move {
        link.write_all(&payload).await.expect("写");
        link.flush().await.expect("flush");
        link
    });
    let mut got = Vec::new();
    while got.len() < want.len() {
        let v = rig.next("link-data").await;
        let block = b64_decode(v["args"]["data"].as_str().unwrap()).unwrap();
        assert!(block.len() <= LINK_STEP, "一块超过上限");
        got.extend(block);
    }
    assert_eq!(got, want);
    drop(writer.await.unwrap());
}

/// 丢掉链路 ⇒ 发 `link-close`、表里摘掉（C2 那一版：丢掉子进程句柄 = 代理被收掉）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_link_sends_link_close_and_unregisters() {
    let _g = serial();
    let mut rig = rig();
    let open = LinkStream::open(Arc::clone(&rig.client), serde_json::json!({}), BUDGET);
    let (link, _) = tokio::join!(open, rig.next("link-open"));
    let link = link.expect("开链路");
    let id = link.id.clone();
    assert!(lock().contains_key(&id));
    drop(link);
    let v = rig.next("link-close").await;
    assert_eq!(v["args"]["link"], id.as_str());
    assert!(!lock().contains_key(&id));
}

/// `shutdown` ＝ 关链路：发 `link-close`，读端随即 EOF（不再等后端）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_closes_the_link_and_the_read_side_ends() {
    let _g = serial();
    let mut rig = rig();
    let open = LinkStream::open(Arc::clone(&rig.client), serde_json::json!({}), BUDGET);
    let (link, _) = tokio::join!(open, rig.next("link-open"));
    let mut link = link.expect("开链路");
    link.shutdown().await.expect("shutdown");
    let v = rig.next("link-close").await;
    assert_eq!(v["args"]["link"], link.id.as_str());
    let mut buf = Vec::new();
    link.read_to_end(&mut buf).await.expect("读端该给 EOF");
    assert!(buf.is_empty());
    assert!(link.write_all(b"x").await.is_err(), "关了之后还能写");
}

/// 本机那条流断了（`fail_owned_by`）⇒ **经它开的**在飞链路读到带原因的错，不是干等；
/// 别的客户端开的链路**不受影响**（本机后端重连之后，新流上的链路不许被旧流的收尾带走）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dead_stream_ends_only_the_links_it_opened() {
    let _g = serial();
    let mut rig = rig();
    let mut other = rig_fresh();
    let open = LinkStream::open(Arc::clone(&rig.client), serde_json::json!({}), BUDGET);
    let (link, _) = tokio::join!(open, rig.next("link-open"));
    let mut mine = link.expect("开链路");
    let open = LinkStream::open(Arc::clone(&other.client), serde_json::json!({}), BUDGET);
    let (link, _) = tokio::join!(open, other.next("link-open"));
    let theirs = link.expect("开链路");
    fail_owned_by(&rig.client, "本机后端的流断了（台架）");
    let mut buf = Vec::new();
    let e = mine.read_to_end(&mut buf).await.expect_err("该读到错");
    assert!(e.to_string().contains("台架"), "原因没带过来：{e}");
    assert!(
        lock().contains_key(&theirs.id),
        "别的流开的链路被一起带走了"
    );
}

/// `link_end` 带 `error` ⇒ 读端拿到那句话；对端超窗 ⇒ 当场判坏、读端拿到「协议对不上」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_error_end_and_an_over_window_flood_both_surface_as_read_errors() {
    let _g = serial();
    let mut rig = rig();
    let open = LinkStream::open(Arc::clone(&rig.client), serde_json::json!({}), BUDGET);
    let (link, _) = tokio::join!(open, rig.next("link-open"));
    let mut a = link.expect("开链路");
    rig.send_end(&a.id, Some("读链路下行失败：x"));
    let mut buf = Vec::new();
    let e = a
        .read_to_end(&mut buf)
        .await
        .expect_err("带 error 的收尾该是错");
    assert!(e.to_string().contains("读链路下行失败"));

    let open = LinkStream::open(Arc::clone(&rig.client), serde_json::json!({}), BUDGET);
    let (link, _) = tokio::join!(open, rig.next("link-open"));
    let mut b = link.expect("开链路");
    // 不读、直接灌一窗多一块（不守约：没等信用）。等它被判坏（表里摘掉）之后才开始读 ——
    // 边灌边读的话读者会还信用，那就不是「超窗」了。
    let block = pattern(LINK_STEP);
    for _ in 0..(LINK_WINDOW_BYTES as usize / LINK_STEP + 1) {
        rig.send_data(&b.id, &block);
    }
    let t0 = std::time::Instant::now();
    while lock().contains_key(&b.id) {
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "灌了一窗多一块，5s 还没判坏"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut sink = Vec::new();
    let e = b.read_to_end(&mut sink).await.expect_err("超窗该判坏");
    assert!(e.to_string().contains("协议对不上"), "超窗的说法不对：{e}");
    assert!(
        sink.len() as u64 <= LINK_WINDOW_BYTES,
        "判坏之前交出去的字节超过了一窗：{}",
        sink.len()
    );
}

/// 解帧：`link_data` 的 base64 在解帧这一步解开；解不开 ⇒ 整帧 `None`（不交一段猜出来的字节）。
#[test]
fn parse_frame_decodes_link_frames_and_refuses_bad_base64() {
    match parse_frame(r#"{"kind":"link_data","link":"L","data":"aGkK"}"#) {
        Some(InboundFrame::LinkData { link, data }) => {
            assert_eq!(link, "L");
            assert_eq!(data, b"hi\n");
        }
        other => panic!("解出来的不对：{other:?}"),
    }
    assert!(parse_frame(r#"{"kind":"link_data","link":"L","data":"a!"}"#).is_none());
    assert_eq!(
        parse_frame(r#"{"kind":"link_end","link":"L","error":"e"}"#),
        Some(InboundFrame::LinkEnd {
            link: "L".into(),
            error: Some("e".into())
        })
    );
    assert_eq!(
        parse_frame(r#"{"kind":"link_end","link":"L"}"#),
        Some(InboundFrame::LinkEnd {
            link: "L".into(),
            error: None
        })
    );
}
