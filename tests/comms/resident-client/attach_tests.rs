//! 握手零件：拒绝行 · 不是我们的 · 认不得新一形 attach 行的是旧的 · 成了（新旧照回话；紧跟 attach 回话的帧留在读半边里）·
//! 要了 deflate：回认 ⇒ 边读边解、逐字节相同且线上不到三成，没认 ⇒ 照原样。
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

const HELLO: &str =
    r#"{"kind":"hello","build_id":"p10g-x","host_arch":"x86_64","commands":["ping"]}"#;

async fn peer(lines: &'static [&'static str]) -> tokio::io::DuplexStream {
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let mut r = tokio::io::BufReader::new(theirs);
        for l in lines {
            r.get_mut()
                .write_all(format!("{l}\n").as_bytes())
                .await
                .unwrap();
            if l.contains("hello") {
                let mut got = String::new();
                r.read_line(&mut got).await.unwrap();
                assert!(got.contains("\"attach\":true"), "attach 行：{got}");
            }
        }
        // 留着不关，让读那一头自己决定。
        std::future::pending::<()>().await;
    });
    mine
}

#[tokio::test(start_paused = true)]
async fn a_good_handshake_keeps_the_frame_that_follows_the_reply() {
    let s = peer(&[
        HELLO,
        r#"{"attach":"ok","relation":"newer"}"#,
        r#"{"kind":"sessions_replayed"}"#,
    ])
    .await;
    let mut a = handshake(
        s,
        Some("Asia/Shanghai"),
        Some("p10f-y"),
        None,
        false,
        Duration::from_secs(5),
    )
    .await
    .map_err(|e| format!("{e:?}"))
    .expect("握手成");
    assert_eq!(a.hello.build_id, "p10g-x");
    assert_eq!(a.relation, Relation::Newer, "新旧照回话");
    let mut next = String::new();
    a.reader.read_line(&mut next).await.unwrap();
    assert!(
        next.contains("sessions_replayed"),
        "紧跟的那一帧丢了：{next:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_relay_refusal_and_a_stranger_are_told_apart() {
    let s = peer(&[r#"{"attach":"refused","reason":"absent"}"#]).await;
    assert_eq!(
        handshake(s, None, None, None, false, Duration::from_secs(5))
            .await
            .err(),
        Some(AttachFail::Refused("absent".into()))
    );
    let s = peer(&["SSH-2.0 whatever"]).await;
    assert!(matches!(
        handshake(s, None, None, None, false, Duration::from_secs(5)).await.err(),
        Some(AttachFail::NotOurs(l)) if l == "SSH-2.0 whatever"
    ));
}

/// 那台认不得这一形 attach 行（hello 之后回 `malformed-attach`）⇒ 旧；回话不是这一形 ⇒ 那一句。
#[tokio::test(start_paused = true)]
async fn a_backend_that_refuses_the_attach_line_is_old() {
    let s = peer(&[HELLO, r#"{"attach":"refused","reason":"malformed-attach"}"#]).await;
    assert_eq!(
        handshake(s, None, Some("p10f-y"), None, false, Duration::from_secs(5))
            .await
            .err(),
        Some(AttachFail::Old)
    );
    let s = peer(&[HELLO, r#"{"kind":"sessions_replayed"}"#]).await;
    assert!(matches!(
        handshake(s, None, None, None, false, Duration::from_secs(5))
            .await
            .err(),
        Some(AttachFail::Reply(_))
    ));
}

#[tokio::test(start_paused = true)]
async fn a_silent_peer_is_cut_at_the_handshake_deadline() {
    let s = peer(&[]).await;
    assert!(matches!(
        handshake(s, None, None, None, false, Duration::from_secs(20))
            .await
            .err(),
        Some(AttachFail::Cut { hello: true, .. })
    ));
}

/// 照中那份记录的形状造的一串 `line` 帧（每帧都带同一条 `path` · `session_id`，正文是小词表里拼的话；不放真正文）。
fn mid_shaped_frames(n: usize) -> Vec<String> {
    const WORDS: &[&str] = &[
        "read", "the", "file", "then", "edit", "test", "run", "cargo", "build", "frame", "line",
        "stream", "path", "offset", "reply", "page", "error", "ok", "fix", "check", "node",
        "graph", "commit", "branch", "merge", "deflate", "flush", "write", "async", "tokio",
        "serde", "json",
    ];
    let sid = "6f1c0d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f";
    let path = format!("/home/u/.claude/projects/-home-u-work-repo/{sid}.jsonl");
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut offset = 0u64;
    (0..n)
        .map(|i| {
            let words = 20 + (next() % 200) as usize;
            let text: Vec<&str> = (0..words)
                .map(|_| WORDS[(next() % WORDS.len() as u64) as usize])
                .collect();
            let record = serde_json::json!({
                "type": if i % 2 == 0 { "assistant" } else { "user" },
                "uuid": format!("{:032x}", next() as u128 * 7),
                "timestamp": format!("2026-10-28T08:{:02}:{:02}.000Z", (i / 60) % 60, i % 60),
                "message": {"role": "assistant", "content": [{"type": "text", "text": text.join(" ")}]},
            });
            let frame = serde_json::json!({
                "kind": "line", "session_id": sid, "path": path, "seq": i, "byte_offset": offset, "record": record,
            });
            offset += 700;
            frame.to_string()
        })
        .collect()
}

/// 那台认了 deflate：写完回话那一行之后整条走一个 raw deflate 流，每帧 sync flush（级别 6）。回 `(原样字节, 线上字节)`。
async fn deflating_peer(
    theirs: tokio::io::DuplexStream,
    frames: Vec<String>,
    ack: bool,
) -> (usize, usize) {
    use std::io::Write;
    let mut r = tokio::io::BufReader::new(theirs);
    r.get_mut()
        .write_all(format!("{HELLO}\n").as_bytes())
        .await
        .unwrap();
    let mut got = String::new();
    r.read_line(&mut got).await.unwrap();
    assert!(
        got.contains("\"deflate\":true"),
        "attach 行要带 deflate：{got}"
    );
    let reply = if ack {
        r#"{"attach":"ok","deflate":true}"#
    } else {
        r#"{"attach":"ok"}"#
    };
    r.get_mut()
        .write_all(format!("{reply}\n").as_bytes())
        .await
        .unwrap();
    let (mut raw, mut wire) = (0usize, 0usize);
    let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(6));
    for f in frames {
        let line = format!("{f}\n");
        raw += line.len();
        let out = if ack {
            z.write_all(line.as_bytes()).unwrap();
            z.flush().unwrap();
            std::mem::take(z.get_mut())
        } else {
            line.into_bytes()
        };
        wire += out.len();
        r.get_mut().write_all(&out).await.unwrap();
    }
    r.get_mut().flush().await.unwrap();
    (raw, wire)
}

async fn read_frames<R: tokio::io::AsyncBufRead + Unpin>(r: &mut R, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = Vec::new();
    for _ in 0..n {
        match comms_inward::line::read_capped_line(r, &mut buf, crate::line::BACKEND_FRAME_LINE_CAP)
            .await
            .unwrap()
        {
            comms_inward::line::CappedLine::Line => {
                out.push(String::from_utf8(buf.clone()).unwrap())
            }
            other => panic!("第 {} 帧：{other:?}", out.len()),
        }
    }
    out
}

/// attach 行声明收 deflate、那台回认 ⇒ 读出来的帧与原帧逐字节相同，线上字节 ≤ 原样的 0.3。
#[tokio::test]
async fn a_backend_that_acks_deflate_is_read_back_byte_for_byte_at_under_a_third() {
    let frames = mid_shaped_frames(400);
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let peer = tokio::spawn(deflating_peer(theirs, frames.clone(), true));
    let mut a = handshake(mine, None, None, None, true, Duration::from_secs(5))
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("握手成");
    let got = read_frames(&mut a.reader, frames.len()).await;
    let (raw, wire) = peer.await.unwrap();
    assert!(got == frames, "解出来的帧与原帧不同");
    assert!(
        wire * 10 <= raw * 3,
        "线上 {wire} 字节，原样 {raw} 字节：没压到三成以下"
    );
}

/// 要了 deflate、那台回话里没认 ⇒ 照原样读。
#[tokio::test]
async fn a_backend_that_does_not_ack_deflate_is_read_plain() {
    let frames = mid_shaped_frames(20);
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let peer = tokio::spawn(deflating_peer(theirs, frames.clone(), false));
    let mut a = handshake(mine, None, None, None, true, Duration::from_secs(5))
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("握手成");
    assert!(read_frames(&mut a.reader, frames.len()).await == frames);
    let (raw, wire) = peer.await.unwrap();
    assert_eq!(raw, wire);
}
