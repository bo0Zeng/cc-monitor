//! 握手零件：拒绝行 · 不是我们的 · 认不得新一形 attach 行的是旧的 · 成了（新旧照回话；紧跟 attach 回话的帧留在读半边里）。
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
        handshake(s, None, None, Duration::from_secs(5)).await.err(),
        Some(AttachFail::Refused("absent".into()))
    );
    let s = peer(&["SSH-2.0 whatever"]).await;
    assert!(matches!(
        handshake(s, None, None, Duration::from_secs(5)).await.err(),
        Some(AttachFail::NotOurs(l)) if l == "SSH-2.0 whatever"
    ));
}

/// 那台认不得这一形 attach 行（hello 之后回 `malformed-attach`）⇒ 旧；回话不是这一形 ⇒ 那一句。
#[tokio::test(start_paused = true)]
async fn a_backend_that_refuses_the_attach_line_is_old() {
    let s = peer(&[HELLO, r#"{"attach":"refused","reason":"malformed-attach"}"#]).await;
    assert_eq!(
        handshake(s, None, Some("p10f-y"), Duration::from_secs(5))
            .await
            .err(),
        Some(AttachFail::Old)
    );
    let s = peer(&[HELLO, r#"{"kind":"sessions_replayed"}"#]).await;
    assert!(matches!(
        handshake(s, None, None, Duration::from_secs(5)).await.err(),
        Some(AttachFail::Reply(_))
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
