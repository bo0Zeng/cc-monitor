use crate::copy_table::copy_text;

fn overflow(
    dropped: u64,
    lost: &[(&str, &str)],
    lost_truncated: bool,
) -> cmd_wire::frame::Overflow {
    cmd_wire::frame::Overflow {
        dropped,
        lost: lost
            .iter()
            .map(|(k, s)| cmd_wire::frame::LostFrame {
                kind: k.to_string(),
                subject: Some(s.to_string()),
            })
            .collect(),
        lost_truncated,
    }
}

/// ★★ **有不可恢复的丢失时，不许再对用户说「重开会话可看完整历史」**〔audit-0805 F21，E4〕。
///
/// 那句话对**内容帧**是真的（行还在远端 jsonl 里），对**状态增量帧**是**假的** ——
/// 它是一次差分的结果、别处不存在。这句 message 是用户唯一看得见的东西，
/// 所以它对不对本身就是正题。
#[test]
fn the_overflow_message_stops_lying_when_state_was_lost() {
    let o = overflow(
        7,
        &[("session_removed", "sid-a"), ("session_added", "sid-b")],
        false,
    );
    let m = super::overflow_health_message("box1", &o);
    assert!(
        !copy_core::copy_matches("rsSshSource.health.overflowLines", &m),
        "丢了状态增量帧还说「重开会话可看完整历史」—— 那是假话。实得：{m}"
    );
    assert!(
        m.contains("sid-a") && m.contains("sid-b"),
        "要点名受影响的会话：{m}"
    );
    assert!(
        copy_core::copy_matches("rsSshSource.health.overflowLost", &m),
        "要说清这部分补不回来：{m}"
    );
    assert!(!m.contains("清单**不全**"), "没截断就别说截断：{m}");
}

/// 只丢内容帧时说「重开会话可看完整历史」那一句（按文案键）。
#[test]
fn the_overflow_message_is_byte_identical_when_only_lines_were_lost() {
    let m = super::overflow_health_message("box1", &overflow(3, &[], false));
    assert_eq!(
        m,
        copy_text(
            "rsSshSource.health.overflowLines",
            &[("host", "box1"), ("dropped", "3")]
        )
    );
}

/// 身份表被截断时要**再说一句**（暗示理性做法是整体重取）。
#[test]
fn the_overflow_message_says_so_when_the_identity_list_was_truncated() {
    let m = super::overflow_health_message(
        "box1",
        &overflow(99, &[("session_removed", "sid-a")], true),
    );
    assert!(
        m.contains(copy_core::copy_static!(
            "rsSshSource.health.overflowTruncated"
        )),
        "截断了就要说出来：{m}"
    );
}

// hello 解冻写半边并登记 · 应答路由回等待者 · 「重新连接」只叫醒那一台：随重连循环住 `resident-client`
//   （`feed.rs`：握手换出客户端、应答经 `route` 交回；`feed_tests.rs` 的问答与叫醒那几条）；登记在桌面收事那一头
//   （`stream_source/run.rs::on_attached`），`reconnect_cells_tests.rs` 走真的那一串。

// 控制通道往返探测那两条（真发 ping 并等应答 · 对旧后端一个字节不发）随测试连接搬进后端：
//   `tests/backend/dial_probe_tests.rs`（`a_hello_that_answers_ping_is_all_green` · `an_old_backend_without_commands_says_too_old`）。

/// 抓本线程上 warn 级以上的日志。
#[derive(Clone, Default)]
struct Warns(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for Warns {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 链路 · 传输 · 测试连接只开在本机后端那条流上：远端来了 ⇒ 分派第一层照实说一句、丢掉（不交、也不当有序的那几种往下送）。
#[test]
fn a_link_frame_from_a_remote_is_said_and_dropped() {
    let warns = Warns::default();
    let sink = warns.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || sink.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let origin = crate::origin::Origin("devbox".into());
    let got = tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        [
            r#"{"kind":"link_data","link":"L","data":"aGk="}"#,
            r#"{"kind":"link_end","link":"L"}"#,
            r#"{"kind":"transfer","id":"x","got":1,"total":2}"#,
            r#"{"kind":"probe","ticket":"t","cell":{"reached":"ssh"}}"#,
        ]
        .iter()
        .map(|l| {
            let f = super::decode(l).unwrap();
            super::route(super::Via::Remote, &origin, None, f).is_some()
        })
        .collect::<Vec<_>>()
    });
    assert_eq!(
        got, [false; 4],
        "远端来的链路 / 传输 / 测试连接帧被往下送了"
    );
    let log = String::from_utf8(warns.0.lock().unwrap().clone()).unwrap();
    assert_eq!(
        log.matches("[devbox] 远端后端发来了只开在本机后端上的帧")
            .count(),
        4,
        "远端来的那几帧没有照实说：\n{log}"
    );
}

/// 本机那条流上来一帧 `overflow` ⇒ 健康信息说一句（与远端同一句、同一处）。
#[tokio::test]
async fn an_overflow_on_the_local_stream_is_said() {
    let said = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
    let s2 = said.clone();
    let health: super::HealthOut = std::sync::Arc::new(move |p| {
        s2.lock().unwrap().push((p.origin, p.kind));
        Ok(())
    });
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let consumer = tokio::spawn(super::consume_local(
        crate::origin::LOCAL.to_string(),
        rx,
        std::sync::Arc::new(crate::event_replay::EventReplay::new()),
        health,
    ));
    let f = super::decode(r#"{"kind":"overflow","dropped":3}"#).unwrap();
    let item = super::route(super::Via::Local, &crate::origin::Origin::local(), None, f)
        .expect("overflow 是有序的那几种");
    tx.send(super::LocalItem::Frame(item)).await.unwrap();
    drop(tx);
    tokio::time::timeout(guard_core::wall::CEILING, consumer)
        .await
        .expect("本机消费者没收摊")
        .unwrap();
    assert_eq!(
        *said.lock().unwrap(),
        [(crate::origin::LOCAL.to_string(), "overflow".to_string())],
        "本机那条流上的拥塞丢帧没说出来"
    );
}

/// 链路没开成交给循环的那一份：后端给的原因码与那台出示的指纹随那一句走（断开时机器状态按它定那一态与修法）；
/// 读应答这一跳坏了 ⇒ 两格空。
#[test]
fn a_refused_link_hands_its_reason_and_seen_key_to_the_loop() {
    let refused = crate::dial_host::Unopened {
        said: crate::dial_host::refused_said("指纹变了".into(), Some("命令：dial")),
        why: Some(cmd_wire::machine::Why::HostKey),
        seen_key: Some("SHA256:seen".into()),
    };
    let p = super::problem_of(refused);
    assert_eq!(
        (p.text.as_str(), p.detail.as_str()),
        ("指纹变了", "命令：dial")
    );
    assert_eq!(
        (p.why, p.seen_key.as_deref()),
        (Some(cmd_wire::machine::Why::HostKey), Some("SHA256:seen"))
    );
    let broken = super::problem_of(crate::detail::Said::written("读不动".to_string()).into());
    assert_eq!((broken.why, broken.seen_key), (None, None));
}
