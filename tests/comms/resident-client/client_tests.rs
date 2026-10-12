//! 帧客户端在本 crate 里的两条（整套行为判据在 monitor `inbound_client_tests.rs`，对的是同一份代码）：
//! 时区经 `set_viewer_tz` 进每条信封 · 撤单补发。
use super::*;
use tokio::io::AsyncBufReadExt;

fn hello() -> BackendHello {
    let h = crate::attach::Hello::parse(
        r#"{"kind":"hello","v":1,"build_id":"b","commands":["ping","cancel"]}"#,
    )
    .expect("hello");
    BackendHello::from_hello_frame(&h).expect("见证")
}

#[tokio::test(start_paused = true)]
async fn the_viewer_tz_rides_on_every_request() {
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let client = park(mine).into_client(hello());
    client.set_viewer_tz(Some("Asia/Shanghai".into()));
    let mut peer = tokio::io::BufReader::new(theirs);
    let c = client.clone();
    let call = tokio::spawn(async move {
        c.call("ping", serde_json::json!({}), Duration::from_secs(5))
            .await
    });
    let mut line = String::new();
    peer.read_line(&mut line).await.unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["tz"], "Asia/Shanghai");
    assert!(client.route_reply(v["id"].as_str().unwrap(), true, None, None, None, None));
    assert_eq!(call.await.unwrap().unwrap(), None);
}

#[tokio::test(start_paused = true)]
async fn a_timed_out_call_sends_one_cancel() {
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let client = park(mine).into_client(hello());
    let mut peer = tokio::io::BufReader::new(theirs);
    let got = client
        .call("ping", serde_json::json!({}), Duration::from_millis(50))
        .await;
    assert!(matches!(got, Err(CallError::Timeout { .. })), "{got:?}");
    let mut first = String::new();
    peer.read_line(&mut first).await.unwrap();
    let mut second = String::new();
    peer.read_line(&mut second).await.unwrap();
    let v: Value = serde_json::from_str(&second).unwrap();
    assert_eq!(v["cmd"], "cancel");
}
