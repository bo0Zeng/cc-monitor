use super::*;
use crate::inbound_client;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

fn hello_line(commands: &str) -> String {
    format!(
        r#"{{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d","commands":{commands}}}"#
    )
}

/// MU13 的回归钉：收到 hello ⇒ 写半边解冻 + 客户端登记进注册表。
#[tokio::test]
async fn a_hello_frame_thaws_the_write_half_and_registers_the_client() {
    let (mine, _theirs) = tokio::io::duplex(4096);
    let mut parked = Some(inbound_client::park(mine));
    let origin = "seam-attach-origin";

    // 非 hello 帧不解冻。
    let not_hello = parse_frame(r#"{"kind":"overflow","dropped":1}"#);
    assert!(
        attach_inbound_client(origin, &mut parked, not_hello.as_ref()).is_none(),
        "非 hello 帧居然把写半边解冻了"
    );
    assert!(parked.is_some(), "写半边被误消耗了");
    assert!(inbound_client::client_for(origin).is_none());

    // hello ⇒ 解冻 + 登记，且 daemon 声明的命令集透传到客户端。
    let hello = parse_frame(&hello_line(r#"["ping"]"#));
    let client =
        attach_inbound_client(origin, &mut parked, hello.as_ref()).expect("hello 应当换出客户端");
    assert!(client.accepts("ping"));
    assert!(!client.accepts("launch"));
    assert!(parked.is_none(), "写半边应当已被 take 走");
    assert!(
        inbound_client::client_for(origin).is_some_and(|c| std::sync::Arc::ptr_eq(&c, &client)),
        "客户端没登记进注册表 —— 2b 的 launch 会取不到"
    );

    // 第二次 hello（不该有）：静默跳过，不会再造一个客户端。
    let again = parse_frame(&hello_line(r#"["ping"]"#));
    assert!(attach_inbound_client(origin, &mut parked, again.as_ref()).is_none());

    inbound_client::unregister(origin, &client);
    assert!(inbound_client::client_for(origin).is_none());
}

/// ★★ **有不可恢复的丢失时，不许再对用户说「重开会话可看完整历史」**〔audit-0805 F21，E4〕。
///
/// 那句话对**内容帧**是真的（行还在远端 jsonl 里），对**状态增量帧**是**假的** ——
/// 它是一次差分的结果、别处不存在。这句 message 是用户唯一看得见的东西，
/// 所以它对不对本身就是本件的正题。
#[test]
fn the_overflow_message_stops_lying_when_state_was_lost() {
    let lost = vec![
        super::LostFrameInfo {
            kind: "session_removed".into(),
            subject: Some("sid-a".into()),
        },
        super::LostFrameInfo {
            kind: "session_added".into(),
            subject: Some("sid-b".into()),
        },
    ];
    let m = super::overflow_health_message("box1", 7, &lost, false);
    assert!(
        !m.contains("重开该会话可看完整历史"),
        "丢了状态增量帧还说「重开会话可看完整历史」—— 那是假话。实得：{m}"
    );
    assert!(
        m.contains("sid-a") && m.contains("sid-b"),
        "要点名受影响的会话：{m}"
    );
    assert!(m.contains("补不回来"), "要说清这部分补不回来：{m}");
    assert!(!m.contains("清单**不全**"), "没截断就别说截断：{m}");
}

/// 只丢内容帧时**逐字沿用老说法** —— 旧 daemon（`p1x` 之前）不发 `lost`，
/// 也落这一档，行为必须与从前一字不差。
#[test]
fn the_overflow_message_is_byte_identical_when_only_lines_were_lost() {
    let m = super::overflow_health_message("box1", 3, &[], false);
    assert_eq!(
        m,
        "远端 [box1] 管道拥塞，可能丢失约 3 条实时行；重开该会话可看完整历史。"
    );
}

/// 身份表被截断时要**再说一句**（暗示理性做法是整体重取）。
#[test]
fn the_overflow_message_says_so_when_the_identity_list_was_truncated() {
    let lost = vec![super::LostFrameInfo {
        kind: "session_removed".into(),
        subject: Some("sid-a".into()),
    }];
    let m = super::overflow_health_message("box1", 99, &lost, true);
    assert!(m.contains("不全"), "截断了就要说出来：{m}");
}

/// ★ **旧 daemon 的 overflow 帧必须照旧能解析**〔additive 的真正代价在这里〕。
///
/// 把 `lost` 当必需字段会让整帧变成坏帧、**连 `dropped` 都丢掉** —— 比不认识新字段更糟。
#[test]
fn overflow_from_an_old_daemon_still_parses() {
    match parse_frame(r#"{"kind":"overflow","dropped":5}"#) {
        Some(InboundFrame::Overflow {
            dropped,
            lost,
            lost_truncated,
        }) => {
            assert_eq!(dropped, 5);
            assert!(lost.is_empty(), "旧 daemon 不发 lost ⇒ 空集");
            assert!(!lost_truncated);
        }
        other => panic!("旧 daemon 的 overflow 解析不出来了：{other:?}"),
    }
}

/// 新 daemon 的 `lost` / `lost_truncated` 要真的被读进来。
#[test]
fn overflow_identity_fields_are_actually_parsed() {
    let json = r#"{"kind":"overflow","dropped":2,"lost":[{"kind":"session_removed","subject":"sid-x"},{"kind":"tmux_sessions"}],"lost_truncated":true}"#;
    match parse_frame(json) {
        Some(InboundFrame::Overflow {
            dropped,
            lost,
            lost_truncated,
        }) => {
            assert_eq!(dropped, 2);
            assert!(lost_truncated);
            assert_eq!(lost.len(), 2, "两条身份都要收进来");
            assert_eq!(lost[0].kind, "session_removed");
            assert_eq!(lost[0].subject.as_deref(), Some("sid-x"));
            assert_eq!(lost[1].subject, None, "没有 subject 的那条也要留住 kind");
        }
        other => panic!("带身份的 overflow 解析失败：{other:?}"),
    }
}

/// MU12 的回归钉：`reply` / `cancelled` 真的被路由回等待者。
#[tokio::test]
async fn reply_and_cancelled_frames_reach_the_waiting_caller() {
    let (mine, theirs) = tokio::io::duplex(4096);
    let mut peer = BufReader::new(theirs);
    let mut parked = Some(inbound_client::park(mine));
    let origin = "seam-route-origin";
    let hello = parse_frame(&hello_line(r#"["ping","cancel"]"#));
    let client = attach_inbound_client(origin, &mut parked, hello.as_ref()).expect("客户端");

    let c = client.clone();
    let caller = tokio::spawn(async move {
        c.call("ping", serde_json::Value::Null, Duration::from_secs(5))
            .await
    });
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(5), peer.read_line(&mut line))
        .await
        .expect("等请求行超时")
        .expect("读到请求行");
    let id = serde_json::from_str::<serde_json::Value>(line.trim_end()).expect("JSON")["id"]
        .as_str()
        .expect("id")
        .to_string();

    // 走的是 stream_loop 真正调的那个函数，不是直接调 route_reply。
    let frame = parse_frame(&format!(
        r#"{{"kind":"reply","id":"{id}","ok":true,"data":{{"pong":1}}}}"#
    ))
    .expect("reply 帧");
    assert!(
        route_inbound_frame(origin, Some(&client), frame),
        "应答没被路由回等待者"
    );
    assert_eq!(
        caller.await.expect("task").expect("call 成功"),
        Some(serde_json::json!({ "pong": 1 }))
    );

    inbound_client::unregister(origin, &client);
}

/// 没有客户端时（daemon 在 hello 之前回应答）不 panic、返回 false。
#[test]
fn routing_without_a_client_is_reported_not_panicked() {
    let frame = parse_frame(r#"{"kind":"reply","id":"x","ok":true}"#).expect("reply");
    assert!(!route_inbound_frame("no-client-origin", None, frame));
    // 非入方向帧误传进来也不 panic。
    let other = parse_frame(r#"{"kind":"overflow","dropped":3}"#).expect("overflow");
    assert!(!route_inbound_frame("no-client-origin", None, other));
}

/// MU14 的回归钉：`probe_control_channel` 必须**真发一条 ping 并等应答**。
///
/// 用内存双工管道扮 daemon：读到请求行就回一条 `reply`。
#[tokio::test]
async fn the_control_probe_really_sends_a_ping_and_measures_the_round_trip() {
    // mon_w → dae_r：monitor 写 / 假 daemon 读；dae_w → mon_r：假 daemon 写 / monitor 读。
    let (mon_w, mut dae_r) = tokio::io::duplex(4096);
    let (mut dae_w, mon_r) = tokio::io::duplex(4096);
    let fake_daemon = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let mut rd = BufReader::new(&mut dae_r);
        let mut line = String::new();
        rd.read_line(&mut line).await.expect("读到请求");
        let req: serde_json::Value =
            serde_json::from_str(line.trim_end()).expect("请求是合法 JSON");
        let id = req["id"].as_str().expect("id").to_string();
        dae_w
            .write_all(format!("{{\"kind\":\"reply\",\"id\":\"{id}\",\"ok\":true}}\n").as_bytes())
            .await
            .expect("回应答");
        dae_w.flush().await.expect("flush");
        line
    });

    let hello = parse_frame(&hello_line(r#"["ping"]"#)).expect("hello");
    let out =
        probe_control_channel(inbound_client::park(mon_w), BufReader::new(mon_r), &hello).await;
    assert!(
        out.starts_with("control=ok("),
        "探测应当报成功往返，实得：{out}"
    );
    let sent = fake_daemon.await.expect("假 daemon task");
    assert!(
        sent.contains(r#""cmd":"ping""#),
        "假 daemon 收到的不是 ping：{sent}"
    );
}

/// 旧 daemon（`commands` 空集）：不发任何字节，直接报 unsupported。
#[tokio::test]
async fn the_control_probe_writes_nothing_to_an_old_daemon() {
    let (mon_w, mut dae_r) = tokio::io::duplex(4096);
    let (_dae_w, mon_r) = tokio::io::duplex(4096);
    let hello = parse_frame(&hello_line("[]")).expect("hello");
    let out =
        probe_control_channel(inbound_client::park(mon_w), BufReader::new(mon_r), &hello).await;
    assert!(out.starts_with("control=unsupported"), "实得：{out}");
    let mut buf = [0u8; 16];
    let read = tokio::time::timeout(
        Duration::from_millis(80),
        tokio::io::AsyncReadExt::read(&mut dae_r, &mut buf),
    )
    .await;
    // 只可能是「超时没数据」或「对端已关（0 字节）」——**不许有真数据**。
    match read {
        Err(_) => {}
        Ok(Ok(0)) => {}
        Ok(other) => panic!("对旧 daemon 发了字节：{other:?}"),
    }
}
