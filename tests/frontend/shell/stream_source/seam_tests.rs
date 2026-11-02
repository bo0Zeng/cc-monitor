// hello 解冻写半边并登记 · 应答路由回等待者 · 「重新连接」只叫醒那一台：随重连循环住 `resident-client`
//   （`feed.rs`：握手换出客户端、应答经 `route` 交回；`feed_tests.rs` 的问答与叫醒那几条）；那台的常驻流归本机后端（`control/feeds.rs`），
//   壳收的那一头读它转来的那一串（`relayed_intake_tests.rs` · `reconnect_cells_tests.rs`）。

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

/// 传输 · 测试连接只开在本机后端那条流上：远端来了 ⇒ 分派第一层照实说一句、丢掉（不交、也不当有序的那几种往下送）。
#[test]
fn a_local_only_frame_from_a_remote_is_said_and_dropped() {
    let warns = Warns::default();
    let sink = warns.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || sink.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let origin = crate::origin::Origin::named("devbox");
    let got = tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        [
            r#"{"kind":"transfer","id":"x","got":1,"total":2}"#,
            r#"{"kind":"probe","ticket":"t","cell":{"reached":"ssh"}}"#,
        ]
        .iter()
        .map(|l| {
            let f = super::decode(l).unwrap();
            super::route(&origin, None, f).is_some()
        })
        .collect::<Vec<_>>()
    });
    assert_eq!(got, [false; 2], "远端来的传输 / 测试连接帧被往下送了");
    let log = String::from_utf8(warns.0.lock().unwrap().clone()).unwrap();
    assert_eq!(
        log.matches("[devbox] 远端后端发来了只开在本机后端上的帧")
            .count(),
        2,
        "远端来的那几帧没有照实说：\n{log}"
    );
}

/// 本机那条流上来一帧 `overflow` ⇒ 告诉用户一条：那台写的那一句原样带上、前面排「本机」（与远端同一处）。
#[tokio::test]
async fn an_overflow_on_the_local_stream_is_said() {
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let consumer = tokio::spawn(super::consume(
        rx,
        std::sync::Arc::new(crate::event_replay::EventReplay::new()),
    ));
    let f = super::decode(r#"{"kind":"overflow","said":"那台写的一句"}"#).unwrap();
    let item =
        super::route(&crate::origin::Origin::local(), None, f).expect("overflow 是有序的那几种");
    let said = crate::ui_error::tests::during_async(async {
        tx.send(super::LocalItem::Frame(item)).await.unwrap();
        drop(tx);
        tokio::time::timeout(guard_core::wall::CEILING, consumer)
            .await
            .expect("本机消费者没收摊")
            .unwrap();
    })
    .await;
    assert_eq!(
        said.len(),
        1,
        "本机那条流上的拥塞丢帧没说、或说了不止一条：{said:?}"
    );
    crate::ui_error::tests::assert_is_copy_not_log(&said[0]);
    assert_eq!(
        said[0]["said"],
        copy_core::copy_text(
            "rsUiError.overflow.said",
            &[
                ("host", &copy_core::copy_text("detail.label.local", &[])),
                ("said", "那台写的一句")
            ]
        ),
        "本机那条流上的拥塞丢帧没原样说出来"
    );
}
