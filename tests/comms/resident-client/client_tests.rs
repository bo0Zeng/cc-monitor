//! 帧客户端在本 crate 里的两条（整套行为判据在 monitor `inbound_client_tests.rs`，对的是同一份代码）：
//! 时区经 `set_viewer_tz` 进每条信封 · 撤单补发。
use super::*;
use tokio::io::AsyncBufReadExt;

/// 同一版的后端：交它认的那两条与它们的形状指纹（跟本侧编进来的同一份）。
fn hello() -> BackendHello {
    let shape = |c: &str| cmd_wire::shapes::shape_of(c).expect("typed 命令");
    let h = crate::attach::Hello::parse(&format!(
        r#"{{"kind":"hello","v":1,"build_id":"b","commands":["ping","cancel"],"shapes":{{"ping":"{}","cancel":"{}"}}}}"#,
        shape("ping"),
        shape("cancel")
    ))
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

/// 关写半边交回的那一点：对端见了 EOF 它也不先完成；读这条连接的那一方说「读到底了」（对端收掉了这条流）才完成。
#[tokio::test(start_paused = true)]
async fn close_write_is_done_when_the_reader_says_the_stream_ended() {
    use std::future::Future;
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let client = park(mine).into_client(hello());
    let mut peer = tokio::io::BufReader::new(theirs);
    let ended = client.close_write();
    let mut rest = String::new();
    assert_eq!(peer.read_line(&mut rest).await.unwrap(), 0, "对端该见 EOF");
    let mut ended = std::pin::pin!(ended);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(
        ended.as_mut().poll(&mut cx).is_pending(),
        "读端还没说读到底就完成了"
    );
    client.note_read_end();
    tokio::time::timeout(Duration::from_secs(5), ended)
        .await
        .expect("说了读到底还没完成");
}
