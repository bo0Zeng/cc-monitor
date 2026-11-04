//! 帧客户端在本 crate 里的两条（整套行为判据在 monitor `inbound_client_tests.rs`，对的是同一份代码）：
//! 时区经 `set_viewer_tz` 进每条信封 · 撤单补发。
use super::*;
use tokio::io::AsyncBufReadExt;

/// 同一版的后端：交它认的那两条与它们的形状指纹（跟本侧编进来的同一份）。
fn hello() -> BackendHello {
    let shape = |c: &str| cmd_wire::shapes::shape_of(c).expect("typed 命令");
    crate::attach::Hello::parse(&format!(
        r#"{{"kind":"hello","build_id":"b","host_arch":"x86_64","commands":["ping","cancel"],"shapes":{{"ping":"{}","cancel":"{}"}}}}"#,
        shape("ping"),
        shape("cancel")
    ))
    .expect("hello")
    .witness()
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

/// 转一问（本机后端把别人的请求交那台）：信封里的时区是发起方那一份，不是这条连接登记的那一份；
/// 带不带出口声明都一样带。
#[tokio::test(start_paused = true)]
async fn a_relayed_request_carries_the_askers_tz() {
    for view in [
        None,
        Some(serde_json::json!({"omit": {"entries": ["size"]}})),
    ] {
        let (mine, theirs) = tokio::io::duplex(64 * 1024);
        let client = park(mine).into_client(hello());
        client.set_viewer_tz(Some("Asia/Shanghai".into()));
        let mut peer = tokio::io::BufReader::new(theirs);
        let c = client.clone();
        let v2 = view.clone();
        let call = tokio::spawn(async move {
            c.relay_until(
                "ping",
                serde_json::json!({}),
                v2.as_ref(),
                "America/Los_Angeles",
                tokio::time::Instant::now() + Duration::from_secs(5),
            )
            .await
        });
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["tz"], "America/Los_Angeles", "view={view:?}");
        assert_eq!(v.get("view"), view.as_ref());
        assert!(client.route_reply(v["id"].as_str().unwrap(), true, None, None, None, None));
        assert_eq!(call.await.unwrap().unwrap(), None);
    }
}

/// 这一端怎么称呼这条连接通往的那台（登记时记下的名字）进每条信封的 `called`：远端照名字，本机照「本机」那个词；
/// 没登记 ⇒ 不带（那台不编名字）。问的是它持有的别台（`call_at`）⇒ 不带：那一问的称呼归转过去那一跳的客户端。
#[tokio::test(start_paused = true)]
async fn the_name_this_side_calls_the_peer_rides_on_its_own_requests_only() {
    async fn first_line(
        client: &Arc<InboundClient>,
        theirs: tokio::io::DuplexStream,
        at: Option<&'static str>,
    ) -> Value {
        let mut peer = tokio::io::BufReader::new(theirs);
        let c = client.clone();
        let call = tokio::spawn(async move {
            let until = tokio::time::Instant::now() + Duration::from_secs(5);
            match at {
                Some(m) => {
                    c.call_at(m, "ping", serde_json::json!({}), None, until)
                        .await
                }
                None => c.call_until("ping", serde_json::json!({}), until).await,
            }
        });
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert!(client.route_reply(v["id"].as_str().unwrap(), true, None, None, None, None));
        call.await.unwrap().unwrap();
        v
    }
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let remote = park(mine).into_client(hello());
    remote.name_machine("devbox");
    assert_eq!(first_line(&remote, theirs, None).await["called"], "devbox");

    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let local = park(mine).into_client(hello());
    local.name_machine(cmd_wire::LOCAL);
    assert_eq!(
        first_line(&local, theirs, None).await["called"],
        copy_core::local_machine()
    );

    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let local = park(mine).into_client(hello());
    local.name_machine(cmd_wire::LOCAL);
    let v = first_line(&local, theirs, Some("devbox")).await;
    assert!(
        v.get("called").is_none(),
        "问别台那一问不带这一端对本机的称呼：{v}"
    );

    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let unnamed = park(mine).into_client(hello());
    let v = first_line(&unnamed, theirs, None).await;
    assert!(v.get("called").is_none(), "没登记名字 ⇒ 不带：{v}");
}

/// ② 问那台的一问经本机后端转：信封多一格 `machine`，`view` 照带；这一端连的是本机后端，
/// 那台认不认这条 · 形状对不对由本机后端连那台的客户端判 ⇒ 这里不拿本机后端的命令表拦（它不认的照发）。
#[tokio::test(start_paused = true)]
async fn a_call_for_another_machine_names_it_in_the_envelope_and_is_not_gated_here() {
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let client = park(mine).into_client(hello());
    let mut peer = tokio::io::BufReader::new(theirs);
    let c = client.clone();
    let view = serde_json::json!({"omit": {"record": ["blocks"]}});
    let v2 = view.clone();
    let call = tokio::spawn(async move {
        c.call_at(
            "devbox",
            "history-tail",
            serde_json::json!({"path": "/p"}),
            Some(&v2),
            tokio::time::Instant::now() + Duration::from_secs(5),
        )
        .await
    });
    let mut line = String::new();
    peer.read_line(&mut line).await.unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["machine"], "devbox");
    assert_eq!(v["cmd"], "history-tail", "本机后端的命令表里没有它也照发");
    assert_eq!(v["view"], view);
    let data = serde_json::json!({"rows": []});
    assert!(client.route_reply(
        v["id"].as_str().unwrap(),
        true,
        None,
        None,
        None,
        Some(data.clone())
    ));
    assert_eq!(call.await.unwrap().unwrap(), Some(data));
    // 不带那一台的照旧不出这一格。
    assert!(!encode_request("i", "ping", &Value::Null, None, None, None).contains("machine"));
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

/// 照类型问（`ask::<C>`）：名字取 `C::NAME`、入参照类型写进信封、回包照 `C::Reply` 解；回包对不上类型 ⇒ 里层那一格是解码器那一句，不当成没成。
#[tokio::test(start_paused = true)]
async fn ask_sends_the_calls_name_and_decodes_its_reply_type() {
    use cmd_wire::files::{FilesPathReply, HomeReq, WirePath};
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let client = park(mine).into_client(hello());
    let mut peer = tokio::io::BufReader::new(theirs);
    let deadline = || tokio::time::Instant::now() + Duration::from_secs(5);
    let want = FilesPathReply {
        path: WirePath(b"/home/u".to_vec()),
    };
    for (data, decodes) in [
        (serde_json::to_value(&want).unwrap(), true),
        (serde_json::json!({"path": 7}), false),
    ] {
        let c = client.clone();
        let asked =
            tokio::spawn(async move { c.ask(Some("devbox"), &HomeReq {}, None, deadline()).await });
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["cmd"], "files-home");
        assert_eq!(v["machine"], "devbox");
        assert_eq!(v["args"], serde_json::json!({}));
        assert!(client.route_reply(
            v["id"].as_str().unwrap(),
            true,
            None,
            None,
            None,
            Some(data)
        ));
        let got = asked.await.unwrap().expect("这一问成了");
        match (got, decodes) {
            (Ok(r), true) => assert_eq!(r, want),
            (Err(_), false) => {}
            (other, _) => panic!("解码结局不对：{other:?}"),
        }
    }
}
