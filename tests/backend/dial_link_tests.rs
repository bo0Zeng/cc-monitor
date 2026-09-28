//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md`「链路四条」（链路的顺序 · 流控 · 收尾 · 拒收码）
//!
//! 核原文：「链路四条」逐字「下行逐链路信用 —— `link-open` 给初始窗口，后端发一块扣一块，扣不到就等」·
//! 「上行一次一块：`link-data` 的应答在那块**写进链路之后**才回」；`link-open` 小节逐字列出错误 code
//! 「`invalid_args` · `unsupported_use` · `duplicate_link` · `too_many_links`」—— 本族逐句判的就是这几句。
//! ⚠ `§42` 自己的机检只核字段名落在哪一节、不核行为；契约里**行为**那一半不漂，靠的是本族。
//! 为什么要有链路（本机一个常驻后端持 SSH 并复用）住 `设计/05 §13.4`。〔JA1 点址 2026-09-24〕
//!
//! 〔SR1a〕链路表的判据（`dial/link.rs`）。
//!
//! 大部分格子拿 `Table::install`（生产 `open` 也走它）喂一个**不拨号**的 `serve`（回声 / 造字节 / 永远挂着），
//! 钉的是链路自己的记账：顺序 · 流控 · 收尾 · 上限。最后几格走真的 `open`（真 russh，拨一个没人听的回环口），
//! 钉「链路上的字节 == C2 拨号代理 stdout 的形状」。真 sshd 上的读数见 `tests/evidence/SR1a-link-loopback.py`。

use super::*;
use crate::wire::{b64_decode, Frame};
use tokio::sync::mpsc;

/// 一个永远挂着、并**攥着两根管子**的 `serve`（不攥着的话管子当场被丢 ⇒ 链路立刻结束）。
async fn hang(up: tokio::io::DuplexStream, down: tokio::io::DuplexStream) {
    let _keep = (up, down);
    std::future::pending::<()>().await
}

/// 让别的任务跑到各自阻塞为止（单线程运行时上是确定的）。
async fn settle() {
    for _ in 0..256 {
        tokio::task::yield_now().await;
    }
}

/// 从应答通道里把**此刻已经到了**的帧全拿出来。
fn drain(rx: &mut mpsc::Receiver<Frame>) -> Vec<Frame> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        out.push(f);
    }
    out
}

/// 帧里某条链路的下行字节拼起来。
fn bytes_of(frames: &[Frame], link: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for f in frames {
        if let Frame::LinkData { link: l, data } = f {
            if l == link {
                out.extend(b64_decode(data).expect("下行块不是合法 base64"));
            }
        }
    }
    out
}

fn ended(frames: &[Frame], link: &str) -> Option<Option<String>> {
    frames.iter().find_map(|f| match f {
        Frame::LinkEnd { link: l, error } if l == link => Some(error.clone()),
        _ => None,
    })
}

fn pattern(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 7 % 251) as u8).collect()
}

/// 下行：`serve` 写多少、链路帧里就原样到多少（跨块、按序），然后恰好一帧 `link_end`（无 error），表里摘掉。
#[tokio::test]
async fn downstream_bytes_arrive_whole_and_in_order_then_the_link_ends() {
    let (tx, mut rx) = mpsc::channel::<Frame>(1024);
    let table = Table::new(tx);
    let want = pattern(3 * LINK_CHUNK_BYTES + 123);
    let payload = want.clone();
    let r = table.install(
        "t",
        "L".to_string(),
        MAX_WINDOW,
        move |_up, mut down| async move {
            use tokio::io::AsyncWriteExt;
            down.write_all(&payload).await.unwrap();
        },
    );
    assert!(matches!(r, Frame::Reply { ok: true, .. }));
    settle().await;
    let frames = drain(&mut rx);
    assert_eq!(bytes_of(&frames, "L"), want, "下行字节不齐或乱序");
    assert!(
        frames.iter().all(|f| match f {
            Frame::LinkData { data, .. } => b64_decode(data).unwrap().len() <= LINK_CHUNK_BYTES,
            _ => true,
        }),
        "有一块超过 LINK_CHUNK_BYTES"
    );
    assert_eq!(
        ended(&frames, "L"),
        Some(None),
        "没有恰好一帧正常的 link_end"
    );
    assert_eq!(
        frames
            .iter()
            .filter(|f| matches!(f, Frame::LinkEnd { .. }))
            .count(),
        1
    );
    assert_eq!(table.len(), 0, "链路结束了还留在表里");
}

/// ★ L4：流控。不还信用 ⇒ 发出去的字节**恰好等于**窗口；还一次 X ⇒ 再发**恰好** X。
///
/// ⚠ 窗口与还的信用**刻意不是块长的整数倍**：整数倍时「先读满一块、再等够一块的信用」那种泵也恰好凑满，
/// 本条就分不出它与「先等信用、再按信用读」（死值验 K1 首跑实测：窗口取两块时那一刀照样绿）。
#[tokio::test]
async fn without_credit_exactly_one_window_goes_out() {
    let (tx, mut rx) = mpsc::channel::<Frame>(1024);
    let table = Table::new(tx);
    let window = LINK_CHUNK_BYTES as u64 + 1000;
    let big = pattern(16 * LINK_CHUNK_BYTES);
    let r = table.install(
        "t",
        "W".to_string(),
        window,
        move |_up, mut down| async move {
            use tokio::io::AsyncWriteExt;
            let _ = down.write_all(&big).await;
        },
    );
    assert!(matches!(r, Frame::Reply { ok: true, .. }));
    settle().await;
    let first = drain(&mut rx);
    assert_eq!(
        bytes_of(&first, "W").len() as u64,
        window,
        "没还信用时发出去的字节不等于窗口"
    );
    assert_eq!(ended(&first, "W"), None);
    let more = 777;
    let r = table.credit("c", &serde_json::json!({"link": "W", "bytes": more}));
    assert!(matches!(r, Frame::Reply { ok: true, .. }));
    settle().await;
    let second = drain(&mut rx);
    assert_eq!(
        bytes_of(&second, "W").len() as u64,
        more,
        "还了 {more} 字节的信用，再发出去的不是恰好这么多"
    );
}

/// ★ L5：一条不读（不还信用）的链路不堵别的链路。
#[tokio::test]
async fn a_stalled_link_does_not_block_another() {
    let (tx, mut rx) = mpsc::channel::<Frame>(1024);
    let table = Table::new(tx);
    let window = LINK_CHUNK_BYTES as u64;
    let big = pattern(8 * LINK_CHUNK_BYTES);
    table.install(
        "t",
        "stuck".to_string(),
        window,
        move |_up, mut down| async move {
            use tokio::io::AsyncWriteExt;
            let _ = down.write_all(&big).await;
        },
    );
    let want = pattern(5 * LINK_CHUNK_BYTES);
    let payload = want.clone();
    table.install(
        "t",
        "free".to_string(),
        MAX_WINDOW,
        move |_up, mut down| async move {
            use tokio::io::AsyncWriteExt;
            down.write_all(&payload).await.unwrap();
        },
    );
    settle().await;
    let frames = drain(&mut rx);
    assert_eq!(bytes_of(&frames, "free"), want, "另一条链路被堵住了");
    assert_eq!(ended(&frames, "free"), Some(None));
    assert_eq!(bytes_of(&frames, "stuck").len(), LINK_CHUNK_BYTES);
}

/// 上行：`link-data` 的块按到达顺序进管子；**写进去之后**才回应答（ok）。回声 `serve` 把它们原样送回下行。
#[tokio::test]
async fn upstream_blocks_are_written_in_order_and_acked_after_the_write() {
    let (tx, mut rx) = mpsc::channel::<Frame>(1024);
    let table = Table::new(tx);
    table.install(
        "t",
        "E".to_string(),
        MAX_WINDOW,
        |mut up, mut down| async move {
            let _ = tokio::io::copy(&mut up, &mut down).await;
        },
    );
    let chunks: Vec<Vec<u8>> = (0..3).map(|i| pattern(1000 + i)).collect();
    for (i, c) in chunks.iter().enumerate() {
        let r = table.data(
            &format!("d{i}"),
            &serde_json::json!({"link": "E", "data": crate::wire::b64_encode(c)}),
        );
        assert!(
            r.is_none(),
            "上行块被当场回了应答（该由上行泵写完再回）：{r:?}"
        );
    }
    settle().await;
    let frames = drain(&mut rx);
    let acked: Vec<&str> = frames
        .iter()
        .filter_map(|f| match f {
            Frame::Reply { id, ok: true, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(acked, ["d0", "d1", "d2"], "上行块的应答不是按序一一对上");
    assert_eq!(
        bytes_of(&frames, "E"),
        chunks.concat(),
        "回声回来的字节不对"
    );
}

/// 上行队列满 ⇒ `link_busy`（对端不守「一次一块」时），不涨内存。单线程运行时里不让泵跑 ⇒ 第 5 块就满。
#[tokio::test]
async fn an_upstream_flood_is_refused_as_busy() {
    let (tx, _rx) = mpsc::channel::<Frame>(1024);
    let table = Table::new(tx);
    table.install("t", "B".to_string(), MAX_WINDOW, hang);
    let block = crate::wire::b64_encode(&[1u8; 8]);
    let codes: Vec<Option<String>> = (0..5)
        .map(|i| {
            table
                .data(
                    &format!("d{i}"),
                    &serde_json::json!({"link": "B", "data": block}),
                )
                .map(|f| match f {
                    Frame::Reply { code, .. } => code.unwrap_or_default(),
                    _ => String::new(),
                })
        })
        .collect();
    assert_eq!(
        codes,
        [None, None, None, None, Some("link_busy".to_string())],
        "上行队列的界不是 4"
    );
}

/// `link-close` ⇒ 三个任务都收掉（`serve` 手里的东西被丢）、表里摘掉；再关一次是幂等的。
#[tokio::test]
async fn close_aborts_the_link_and_is_idempotent() {
    let (tx, _rx) = mpsc::channel::<Frame>(16);
    let table = Table::new(tx);
    let (drop_tx, mut drop_rx) = mpsc::channel::<()>(1);
    table.install(
        "t",
        "C".to_string(),
        MAX_WINDOW,
        move |up, down| async move {
            let _keep = (up, down, DropSignal(drop_tx));
            std::future::pending::<()>().await
        },
    );
    settle().await;
    assert_eq!(table.len(), 1);
    let r = table.close("x", &serde_json::json!({"link": "C"}));
    assert!(matches!(r, Frame::Reply { ok: true, .. }));
    assert_eq!(table.len(), 0);
    settle().await;
    let gone = tokio::time::timeout(std::time::Duration::from_secs(5), drop_rx.recv()).await;
    assert!(
        matches!(gone, Ok(None)),
        "关了链路 5 秒，serve 手里的东西还没被丢"
    );
    let again = table.close("y", &serde_json::json!({"link": "C"}));
    assert!(
        matches!(again, Frame::Reply { ok: true, .. }),
        "再关一次不是幂等的"
    );
}

/// 表被丢（连接没了）⇒ 它开的链路一条不留。
#[tokio::test]
async fn dropping_the_table_aborts_every_link() {
    let (tx, _rx) = mpsc::channel::<Frame>(16);
    let table = Table::new(tx);
    let (drop_tx, mut drop_rx) = mpsc::channel::<()>(1);
    for i in 0..3 {
        let d = drop_tx.clone();
        table.install(
            "t",
            format!("T{i}"),
            MAX_WINDOW,
            move |up, down| async move {
                let _keep = (up, down, DropSignal(d));
                std::future::pending::<()>().await
            },
        );
    }
    drop(drop_tx);
    settle().await;
    drop(table);
    settle().await;
    let gone = tokio::time::timeout(std::time::Duration::from_secs(5), drop_rx.recv()).await;
    assert!(
        matches!(gone, Ok(None)),
        "表丢了 5 秒，还有链路的 serve 活着"
    );
}

/// 被丢的时候，它手里那个发送端随之关掉 ⇒ 接收端读到 `None`（「serve 手里的东西被丢了」的见证）。
struct DropSignal(#[allow(dead_code)] mpsc::Sender<()>);

/// 入参的几种坏形 ＋ 重复 id ＋ 子系统留口 ＋ 上限。
#[tokio::test]
async fn open_refuses_what_it_should_with_a_code() {
    let (tx, _rx) = mpsc::channel::<Frame>(16);
    let table = Table::new(tx);
    let code = |f: Frame| match f {
        Frame::Reply { code, .. } => code.unwrap_or_default(),
        _ => String::new(),
    };
    let dial = serde_json::json!({"machine": {"host":"127.0.0.1","port":1,"user":"u"}});
    assert_eq!(
        code(table.open("a", &serde_json::json!({}))),
        "invalid_args"
    );
    assert_eq!(
        code(table.open("a", &serde_json::json!({"link": ""}))),
        "invalid_args"
    );
    assert_eq!(
        code(table.open("a", &serde_json::json!({"link": "x"}))),
        "invalid_args"
    );
    assert_eq!(
        code(table.open("a", &serde_json::json!({"link": "x", "dial": {"host": 1}}))),
        "invalid_args"
    );
    // 窗口：缺席 / 小于一块 / 大于上限 ⇒ 拒收＋回错（不替对端夹）。
    for w in [
        serde_json::Value::Null,
        serde_json::json!(LINK_CHUNK_BYTES - 1),
        serde_json::json!(MAX_WINDOW + 1),
    ] {
        assert_eq!(
            code(table.open(
                "a",
                &serde_json::json!({"link": "w", "window": w, "dial": dial})
            )),
            "invalid_args",
            "窗口 {w} 该被拒"
        );
    }
    let mut sub = dial.clone();
    sub["use"] = "subsystem".into();
    assert_eq!(
        code(table.open("a", &serde_json::json!({"link": "x", "dial": sub}))),
        "unsupported_use",
        "子系统那一口该回 unsupported_use（留口不开）"
    );
    assert_eq!(table.len(), 0);
    table.install("t", "dup".to_string(), MAX_WINDOW, hang);
    assert_eq!(
        code(table.open(
            "a",
            &serde_json::json!({"link": "dup", "window": MAX_WINDOW, "dial": dial})
        )),
        "duplicate_link"
    );
    for i in 1..MAX_LINKS_PER_CONNECTION {
        table.install("t", format!("n{i}"), MAX_WINDOW, hang);
    }
    assert_eq!(table.len(), MAX_LINKS_PER_CONNECTION);
    assert_eq!(
        code(table.open(
            "a",
            &serde_json::json!({"link": "one-too-many", "window": MAX_WINDOW, "dial": dial})
        )),
        "too_many_links"
    );
    assert_eq!(
        code(table.credit("a", &serde_json::json!({"link": "nope", "bytes": 1}))),
        "no_such_link"
    );
    // 累计信用超过上限 ⇒ 拒收＋回错；恰好到上限 ⇒ 收。（表是满的，先腾一格。）
    table.close("a", &serde_json::json!({"link": "n1"}));
    let r = table.install("t", "cr".to_string(), LINK_CHUNK_BYTES as u64, hang);
    assert!(
        matches!(r, Frame::Reply { ok: true, .. }),
        "腾了一格还是装不上：{r:?}"
    );
    let room = MAX_WINDOW - LINK_CHUNK_BYTES as u64;
    assert_eq!(
        code(table.credit("a", &serde_json::json!({"link": "cr", "bytes": room + 1}))),
        "invalid_args",
        "多还一字节该被拒"
    );
    assert!(matches!(
        table.credit("a", &serde_json::json!({"link": "cr", "bytes": room})),
        Frame::Reply { ok: true, .. }
    ));
    let d = table.data("a", &serde_json::json!({"link": "nope", "data": "AAAA"}));
    assert_eq!(d.map(code).as_deref(), Some("no_such_link"));
    let bad = table.data("a", &serde_json::json!({"link": "dup", "data": "A"}));
    assert_eq!(
        bad.map(code).as_deref(),
        Some("invalid_args"),
        "坏 base64 没被拒"
    );
    let huge = crate::wire::b64_encode(&vec![0u8; LINK_CHUNK_BYTES + 1]);
    let too_big = table.data("a", &serde_json::json!({"link": "dup", "data": huge}));
    assert_eq!(
        too_big.map(code).as_deref(),
        Some("invalid_args"),
        "超块没被拒"
    );
}

/// ★ L1（不起 sshd 的那一半）：走**真的** `open` ＋ 真 russh，拨一个没人听的回环口 ⇒
/// 链路上恰好一行失败的 ack（形状与 C2 拨号代理的 stdout 同形：`ok:false` · `v` · `uses`），随后一帧 `link_end`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_dial_to_a_dead_port_answers_one_failed_ack_on_the_link() {
    // 拿一个此刻空着的口：绑上再放掉。
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let (tx, mut rx) = mpsc::channel::<Frame>(64);
    let table = Table::new(tx);
    let r = table.open(
        "o",
        &serde_json::json!({
            "link": "D",
            "window": 1 << 20,
            "dial": {"machine": {"host":"127.0.0.1","port":port,"user":"u"},
                     "use":"capture","command":"true",
                     "capture":{"max_bytes":16}}
        }),
    );
    assert!(
        matches!(r, Frame::Reply { ok: true, .. }),
        "open 没回 ok：{r:?}"
    );
    let mut frames = Vec::new();
    while ended(&frames, "D").is_none() {
        frames.push(rx.recv().await.expect("连接通道提前关了"));
    }
    let text = String::from_utf8(bytes_of(&frames, "D")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "链路上不是恰好一行（ack）：{text:?}");
    let ack: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(ack["ok"], false);
    assert_eq!(ack["v"], super::super::ACK_V);
    assert_eq!(ack["uses"], serde_json::json!(super::super::USES));
    assert!(
        ack["error"]
            .as_str()
            .unwrap_or_default()
            .contains("127.0.0.1"),
        "失败的 ack 没说清是哪个地址：{ack}"
    );
    assert_eq!(ended(&frames, "D"), Some(None));
    assert_eq!(table.len(), 0);
}
