//! 握手零件：拒绝行 · 不是我们的 · 成了（紧跟 attach 回话的帧留在读半边里）。
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

const HELLO: &str = r#"{"kind":"hello","v":1,"build_id":"p10g-x","host_arch":"x86_64","claude_dir":"/c","commands":["ping"]}"#;

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
        tokio::time::sleep(Duration::from_secs(3600)).await;
    });
    mine
}

#[tokio::test(start_paused = true)]
async fn a_good_handshake_keeps_the_frame_that_follows_the_reply() {
    let s = peer(&[
        HELLO,
        r#"{"attach":"ok"}"#,
        r#"{"kind":"sessions_replayed"}"#,
    ])
    .await;
    let mut a = handshake(s, Some("Asia/Shanghai"), None, Duration::from_secs(5))
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("握手成");
    assert_eq!(a.hello.build_id, "p10g-x");
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
        handshake(s, None, None, Duration::from_secs(5)).await.err(),
        Some(AttachFail::Refused("absent".into()))
    );
    let s = peer(&["SSH-2.0 whatever"]).await;
    assert!(matches!(
        handshake(s, None, None, Duration::from_secs(5)).await.err(),
        Some(AttachFail::NotOurs(l)) if l == "SSH-2.0 whatever"
    ));
}

#[tokio::test(start_paused = true)]
async fn a_silent_peer_is_cut_at_the_handshake_deadline() {
    let s = peer(&[]).await;
    assert!(matches!(
        handshake(s, None, None, Duration::from_secs(20))
            .await
            .err(),
        Some(AttachFail::Cut { hello: true, .. })
    ));
}
