//! 「端口转发 3 条进本机后端」—— `dial/forwards.rs` 的判据：围栏在查表之前 · 查不到那台不拨 ·
//! ack 不成不进账 · 停 = 链路那一侧被收 · 列照抄计数与状态。链路那一侧用替身（不起真 SSH）。
use super::*;
use std::sync::atomic::AtomicUsize;
use tokio::io::AsyncWriteExt;

// 线上那一份入参经 typed 那一处收（同分派），再交本体；回包写成线上那一份（判据照线上形比）。
async fn start_with<F, Fut>(
    args: &Value,
    reach: &remote_ask::Table,
    ledger: &Ledger,
    serve: F,
) -> Result<Value, crate::common::said::Said>
where
    F: FnOnce(DialRequest, tokio::net::TcpListener, DuplexStream, DuplexStream) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let req: ForwardStartReq = crate::wire::typed::take(args)?;
    super::start_with(&req, reach, ledger, serve)
        .await
        .map(|r| serde_json::to_value(r).unwrap())
}

fn parse_spec(args: &Value) -> Result<Spec, crate::common::said::Said> {
    let req: ForwardStartReq = crate::wire::typed::take(args)?;
    spec_of(&req)
}

fn list_with(ledger: &Ledger) -> Value {
    serde_json::to_value(super::list_with(ledger)).unwrap()
}

fn stop_with(args: &Value, ledger: &Ledger) -> Result<Value, crate::common::said::Said> {
    let req: ForwardId = crate::wire::typed::take(args)?;
    super::stop_with(req, ledger).map(|r| serde_json::to_value(r).unwrap())
}

fn table_with(origin: &str) -> remote_ask::Table {
    remote_ask::Table::of(vec![<crate::dial::Host as serde::Deserialize>::deserialize(
        &json!({"label": origin, "host": "192.0.2.2", "port": 22, "user": "u", "keyPath": "/k"}),
    )
    .unwrap()])
}

/// 此刻空着的一个回环口（现绑现放：起转发那一下要真绑它）。
fn free_port() -> u16 {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    l.local_addr().unwrap().port()
}

/// 两个端口照界面交的那一形：用户敲进去的原文（数字也写成字）。
fn args(
    origin: &str,
    local: impl std::fmt::Display,
    host: &str,
    remote: impl std::fmt::Display,
) -> Value {
    json!({"origin": origin, "localPort": local.to_string(), "remoteHost": host, "remotePort": remote.to_string()})
}

/// 丢掉它 ⇒ 记一笔（替身链路那一侧被收的证据）。
struct DropMark(Arc<AtomicUsize>);
impl Drop for DropMark {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// 要求住址：`INVARIANTS §47`（外部值交给对端之前本侧先过放行判定）；①形。
/// 原 monitor `port_forward_tests.rs` 那两条（规格三句 · 「围栏在任何 I/O 之前」）合成一条搬来：
/// 非法规格 ⇒ `bad_spec`，**替身一次都没被叫到**（连可达表里有没有那台都不问 —— 用一张空表，围栏没了就会报 `unreachable`）。
#[tokio::test]
async fn the_spec_fence_stands_before_any_lookup_or_dial() {
    let empty = crate::dial::remote_ask::test_peers::empty();
    let ledger = Ledger::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let local = copy_text("beForwards.spec.localPort", &[]);
    let host = copy_text("beForwards.spec.remoteHost", &[]);
    let remote = copy_text("beForwards.spec.remotePort", &[]);
    let origin = copy_text("beForwards.spec.origin", &[]);
    for (bad, said) in [
        (args("dev", 0, "localhost", 5432), &local),
        (args("dev", "", "localhost", 5432), &local),
        (args("dev", "abc", "localhost", 5432), &local),
        (args("dev", 70000, "localhost", 5432), &local),
        (args("dev", -1, "localhost", 5432), &local),
        (args("dev", 15432, "", 5432), &host),
        (args("dev", 15432, "   ", 5432), &host),
        (args("dev", 15432, "localhost", 0), &remote),
        (args("dev", 15432, "localhost", " "), &remote),
        (args("dev", 15432, "localhost", 65536), &remote),
        (args("", 15432, "localhost", 5432), &origin),
    ] {
        let c = Arc::clone(&calls);
        let err = start_with(&bad, &empty, &ledger, move |_req, _port, _up, _down| {
            c.fetch_add(1, Ordering::SeqCst);
            async {}
        })
        .await
        .expect_err("非法规格竟然没被拒");
        assert_eq!(
            err.code.as_deref(),
            Some("bad_spec"),
            "拒了，但不是围栏拒的（{err:?}）—— 围栏在查表之后了"
        );
        assert_eq!(&err.said, said, "{bad} 拒的那一句不对");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let ok = parse_spec(&args("dev", " 15432 ", "localhost", 5432)).unwrap();
    assert_eq!((ok.local_port, ok.remote_port), (15432, 5432));
    // 形状不对（端口交成数字）仍是解入参那一步拒。
    assert_eq!(
        parse_spec(&json!({"origin": "dev", "localPort": 1, "remoteHost": "h", "remotePort": "1"}))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
}

/// 可达表里没有那台 ⇒ `unreachable`、一次都不拨、不进账；有 ⇒ 交给链路那一侧的请求是「那份拨号请求 ＋ `use: forward` ＋ 这条规格」。
#[tokio::test]
async fn an_unknown_machine_is_not_dialled_and_a_known_one_gets_a_forward_request() {
    let ledger = Ledger::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&calls);
    let err = start_with(
        &args("nope", 15432, "localhost", 5432),
        &table_with("dev"),
        &ledger,
        move |_r, _port, _u, _d| {
            c.fetch_add(1, Ordering::SeqCst);
            async {}
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("unreachable"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(list_with(&ledger)["forwards"], json!([]));

    let req = forward_request(
        &table_with("dev").dial("dev").unwrap(),
        &parse_spec(&args("dev", 15432, "db.internal", 5432)).unwrap(),
    )
    .unwrap();
    assert_eq!(req.use_, crate::dial::Use::Forward);
    let f = req.forward.expect("没带转发规格");
    assert_eq!(
        (f.remote_host.as_str(), f.remote_port),
        ("db.internal", 5432)
    );
    assert_eq!(
        (req.host.as_str(), req.key_path.as_deref()),
        ("192.0.2.2", Some("/k"))
    );
}

/// ack 说不成（口绑不上 / 断开）⇒ `failed` 带原话、**不进账**；说成 ⇒ 进账，计数照抄链路那一侧报的数。
/// **停 = 收**：摘掉那一条之后链路那一侧被丢（替身手里的 [`DropMark`] 记了一笔），再列就没有它；再停 ⇒ `not_found`。
#[tokio::test]
async fn a_failed_ack_never_enters_the_ledger_and_stop_drops_the_link() {
    let reach = table_with("dev");
    let ledger = Ledger::new();

    let err = start_with(
        &args("dev", free_port(), "localhost", 5432),
        &reach,
        &ledger,
        |_r, _port, _u, mut down| async move {
            // 照拨号核心写的那一形回：`DialAck::of_fail` 序列化成一行。
            let ack = ssh_dial::DialAck::of_fail(ssh_dial::DialFail {
                said: "port 15432 in use".to_string(),
                detail: None,
                fingerprint: None,
                reason: None,
                key_type: None,
                expected_fingerprint: None,
            });
            let mut row = serde_json::to_vec(&ack).unwrap();
            row.push(b'\n');
            let _ = down.write_all(&row).await;
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("failed"));
    assert!(
        err.said.contains("port 15432 in use"),
        "原话没带回来：{}",
        err.said
    );
    assert_eq!(list_with(&ledger)["forwards"], json!([]));

    let dropped = Arc::new(AtomicUsize::new(0));
    let mark = DropMark(Arc::clone(&dropped));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let local = free_port();
    let got = start_with(
        &args("dev", local, "localhost", 5432),
        &reach,
        &ledger,
        move |_r, port, _up, mut down| async move {
            let _mark = mark;
            let _port = port;
            let _ = down
                .write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"forward\"]}\n")
                .await;
            let _ = down
                .write_all(b"{\"accepted\":1}\n{\"accepted\":2}\n")
                .await;
            let _ = tx.send(());
            std::future::pending::<()>().await;
        },
    )
    .await
    .unwrap();
    let id = got["id"].as_str().unwrap().to_string();
    rx.await.unwrap();
    // 计数那两行被另一个任务读 —— 让出几次，直到它照抄到 2（不睡、不定时）。
    for _ in 0..1000 {
        if list_with(&ledger)["forwards"][0]["connCount"] == json!(2) {
            break;
        }
        tokio::task::yield_now().await;
    }
    // 跨语言金样：界面那一侧（`src/frontend/ui/port-forward-reads.ts::decodeForwards`）严格收的就是这一份。
    let mut golden: Value =
        serde_json::from_str(include_str!("../__fixtures__/forward-list.golden.json")).unwrap();
    // 本机口是这一趟现找的空口（真绑）：金样里那一格换成它，别的照比。
    golden["forwards"][0]["localPort"] = json!(local);
    assert_eq!(id, "fwd-1");
    assert_eq!(list_with(&ledger), golden);
    assert_eq!(dropped.load(Ordering::SeqCst), 0);

    stop_with(&json!({"id": id}), &ledger).unwrap();
    for _ in 0..1000 {
        if dropped.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        1,
        "停了，链路那一侧却没被收（本地口还占着）"
    );
    assert_eq!(list_with(&ledger)["forwards"], json!([]));
    assert_eq!(
        stop_with(&json!({"id": id}), &ledger)
            .unwrap_err()
            .code
            .as_deref(),
        Some("not_found")
    );
}

/// 本机口被占 ⇒ 当场回绑口失败那一句（`failed`），**链路那一侧一次都没被叫到**（不拨号）、不进账：
/// 口由本后端先绑好再交给拨号核心，绑不上就不去拨。
#[tokio::test]
async fn a_taken_local_port_fails_before_any_dial() {
    let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = taken.local_addr().unwrap().port();
    let ledger = Ledger::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&calls);
    let err = start_with(
        &args("dev", port, "localhost", 5432),
        &table_with("dev"),
        &ledger,
        move |_r, _port, _u, _d| {
            c.fetch_add(1, Ordering::SeqCst);
            async {}
        },
    )
    .await
    .expect_err("口被占着也起来了");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "口被占着还去拨了");
    assert_eq!(err.code.as_deref(), Some("failed"));
    let why = copy_text(
        "beForwards.start.bindFailed",
        &[
            ("port", &port.to_string()),
            ("why", &copy_core::io_reason(std::io::ErrorKind::AddrInUse)),
        ],
    );
    assert_eq!(
        err.said,
        copy_text(
            "beForwards.start.failed",
            &[("machine", "dev"), ("why", &why)]
        )
    );
    assert_eq!(list_with(&ledger)["forwards"], json!([]));
    drop(taken);
}

/// 拨号核心不绑口：`use: forward` 不带宿主绑好的口（走 `open`）、或别的用法带了口（走 `run_forward`）⇒ 请求格式不对，不拨。
/// 那台指着本机一个没人听的口：真去拨了就是「连接被拒」那一句，不是格式不对。
#[tokio::test]
async fn the_dial_core_takes_the_port_only_from_the_host_and_only_for_forward() {
    let malformed = copy_text("beContract.malformed.say", &[]);
    let dial = json!({"host": "127.0.0.1", "port": 1, "user": "u", "use": "stream"});
    let spec = parse_spec(&args("dev", 1, "localhost", 5432)).unwrap();
    let fwd = forward_request(&dial, &spec).unwrap();
    let said =
        match crate::dial::uses::open(&fwd, &crate::dial::StageSink::new(false), |_| {}).await {
            Err(f) => f.said,
            Ok(_) => panic!("没交口的转发开起来了"),
        };
    assert!(said.starts_with(&malformed), "没交口的转发去拨了：{said}");

    let stream = crate::dial::read_request(&dial).unwrap();
    let port = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let (_up_w, up_r) = tokio::io::duplex(64);
    let mut down = Vec::new();
    ssh_dial::uses::run_forward(
        &stream,
        port,
        &crate::dial::StageSink::new(false),
        up_r,
        &mut down,
    )
    .await;
    let ack: Value = serde_json::from_slice(&down).unwrap();
    assert_eq!(ack["ok"], json!(false));
    let said = ack["said"].as_str().unwrap();
    assert!(said.starts_with(&malformed), "不是转发也收了口：{said}");
}

/// 链路那一侧自己收工（远端断了 / accept 失败）⇒ 那一条留在账上、状态读成 `error`（用户看得见、自己停掉）。
#[tokio::test]
async fn a_link_that_ends_on_its_own_reads_as_error() {
    let reach = table_with("dev");
    let ledger = Ledger::new();
    start_with(
        &args("dev", free_port(), "localhost", 22),
        &reach,
        &ledger,
        |_r, _port, _up, mut down| async move {
            let _ = down
                .write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"forward\"]}\n")
                .await;
        },
    )
    .await
    .unwrap();
    for _ in 0..1000 {
        if list_with(&ledger)["forwards"][0]["state"] == json!("error") {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(list_with(&ledger)["forwards"][0]["state"], json!("error"));
}

/// 〔流没起的远端不许拒〕只交名字：怎么拨照这一端的机器表组（`ssh-dial/machine.rs`），流起没起来都一样；
/// 交给链路那一侧的是 `use: forward` ＋ 这条规格 ＋ 表里的那台；表里没有 ⇒ 才是 `unreachable`。
#[tokio::test]
async fn a_machine_that_never_streamed_is_dialled_from_the_table() {
    let empty = crate::dial::remote_ask::test_peers::empty();
    let ledger = Ledger::new();
    let local = free_port();
    let a = args("dev", local, "localhost", 5432);
    let e = start_with(&a, &empty, &ledger, |_req, _port, _up, _down| async {})
        .await
        .expect_err("表里没有那台也拨了");
    assert_eq!(e.code.as_deref(), Some("unreachable"));
    let table = remote_ask::Table::of(vec![
        <crate::dial::Host as serde::Deserialize>::deserialize(
            &json!({"host": "198.51.100.9", "label": "dev", "port": 2201, "user": "u"}),
        )
        .unwrap(),
    ]);
    let seen = Arc::new(Mutex::new(None::<(String, u16, crate::dial::Use)>));
    let bound = Arc::new(Mutex::new(None::<std::net::SocketAddr>));
    let s = Arc::clone(&seen);
    let b = Arc::clone(&bound);
    start_with(&a, &table, &ledger, move |req, port, _up, mut down| {
        *s.lock().unwrap() = Some((req.host.clone(), req.port, req.use_));
        *b.lock().unwrap() = port.local_addr().ok();
        async move {
            let _port = port;
            let _ = down
                .write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"forward\"]}\n")
                .await;
            std::future::pending::<()>().await;
        }
    })
    .await
    .expect("流没起的那台照样开得起转发");
    assert_eq!(
        seen.lock().unwrap().clone(),
        Some(("198.51.100.9".to_string(), 2201, crate::dial::Use::Forward))
    );
    // 交进链路那一侧的口是本后端绑好的：回环 · 界面要的那个号。
    assert_eq!(
        *bound.lock().unwrap(),
        Some(std::net::SocketAddr::from(([127, 0, 0, 1], local)))
    );
    assert_eq!(list_with(&ledger)["forwards"][0]["origin"], "dev");
}
