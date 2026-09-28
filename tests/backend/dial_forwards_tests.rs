//! 设计/99 §2.1 ⑬「端口转发 3 条进本机后端」—— `dial/forwards.rs` 的判据：围栏在查表之前 · 查不到那台不拨 ·
//! ack 不成不进账 · 停 = 链路那一侧被收 · 列照抄计数与状态。链路那一侧用替身（不起真 SSH）。
use super::*;
use std::sync::atomic::AtomicUsize;
use tokio::io::AsyncWriteExt;

fn table_with(origin: &str) -> remote_ask::Table {
    let t: remote_ask::Table = Mutex::new(BTreeMap::new());
    remote_ask::register(
        &t,
        &json!({"origin": origin, "dial": {"machine": {"host": "10.0.0.2", "port": 22, "user": "u", "keyPath": "/k"}, "use": "capture"}}),
    )
    .unwrap();
    t
}

fn args(origin: &str, local: u64, host: &str, remote: u64) -> Value {
    json!({"origin": origin, "localPort": local, "remoteHost": host, "remotePort": remote})
}

/// 丢掉它 ⇒ 记一笔（替身链路那一侧被收的证据）。
struct DropMark(Arc<AtomicUsize>);
impl Drop for DropMark {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// 〔IV1 · V121〕要求住址：`INVARIANTS §47`（外部值交给对端之前本侧先过放行判定）；①形。
/// 原 monitor `port_forward_tests.rs` 那两条（规格三句 · 「围栏在任何 I/O 之前」）合成一条搬来：
/// 非法规格 ⇒ `bad_spec`，**替身一次都没被叫到**（连可达表里有没有那台都不问 —— 用一张空表，围栏没了就会报 `unreachable`）。
#[tokio::test]
async fn the_spec_fence_stands_before_any_lookup_or_dial() {
    let empty: remote_ask::Table = Mutex::new(BTreeMap::new());
    let ledger = Ledger::new();
    let calls = Arc::new(AtomicUsize::new(0));
    for bad in [
        args("dev", 0, "localhost", 5432),
        args("dev", 15432, "", 5432),
        args("dev", 15432, "   ", 5432),
        args("dev", 15432, "localhost", 0),
    ] {
        let c = Arc::clone(&calls);
        let err = start_with(&bad, &empty, &ledger, move |_req, _up, _down| {
            c.fetch_add(1, Ordering::SeqCst);
            async {}
        })
        .await
        .expect_err("非法规格竟然没被拒");
        assert_eq!(
            err.0, "bad_spec",
            "拒了，但不是围栏拒的（{err:?}）—— 围栏在查表之后了"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(parse_spec(&args("dev", 15432, "localhost", 5432)).is_ok());
    assert_eq!(
        parse_spec(
            &json!({"origin": "dev", "localPort": 70000, "remoteHost": "h", "remotePort": 1})
        )
        .unwrap_err()
        .0,
        "invalid_args"
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
        move |_r, _u, _d| {
            c.fetch_add(1, Ordering::SeqCst);
            async {}
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, "unreachable");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(list_with(&ledger)["forwards"], json!([]));

    let req = forward_request(
        &json!({"machine": {"host": "10.0.0.2", "port": 22, "user": "u", "keyPath": "/k"}, "use": "capture"}),
        &parse_spec(&args("dev", 15432, "db.internal", 5432)).unwrap(),
    )
    .unwrap();
    assert_eq!(req.use_, crate::dial::Use::Forward);
    let f = req.forward.expect("没带转发规格");
    assert_eq!(
        (f.local_port, f.remote_host.as_str(), f.remote_port),
        (15432, "db.internal", 5432)
    );
    assert_eq!(
        (req.host.as_str(), req.key_path.as_deref()),
        ("10.0.0.2", Some("/k"))
    );
}

/// ack 说不成（口绑不上 / 连不上）⇒ `failed` 带原话、**不进账**；说成 ⇒ 进账，计数照抄链路那一侧报的数。
/// **停 = 收**：摘掉那一条之后链路那一侧被丢（替身手里的 [`DropMark`] 记了一笔），再列就没有它；再停 ⇒ `not_found`。
#[tokio::test]
async fn a_failed_ack_never_enters_the_ledger_and_stop_drops_the_link() {
    let reach = table_with("dev");
    let ledger = Ledger::new();

    let err = start_with(
        &args("dev", 15432, "localhost", 5432),
        &reach,
        &ledger,
        |_r, _u, mut down| async move {
            let _ = down
                .write_all(b"{\"v\":2,\"ok\":false,\"error\":\"port 15432 in use\",\"uses\":[]}\n")
                .await;
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, "failed");
    assert!(
        err.1.contains("port 15432 in use"),
        "原话没带回来：{}",
        err.1
    );
    assert_eq!(list_with(&ledger)["forwards"], json!([]));

    let dropped = Arc::new(AtomicUsize::new(0));
    let mark = DropMark(Arc::clone(&dropped));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let got = start_with(
        &args("dev", 15432, "localhost", 5432),
        &reach,
        &ledger,
        move |_r, _up, mut down| async move {
            let _mark = mark;
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
    // 跨语言金样：界面那一侧（`src/port-forward-reads.ts::decodeForwards`）严格收的就是这一份。
    let golden: Value =
        serde_json::from_str(include_str!("../__fixtures__/forward-list.golden.json")).unwrap();
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
        stop_with(&json!({"id": id}), &ledger).unwrap_err().0,
        "not_found"
    );
}

/// 链路那一侧自己收工（远端断了 / accept 失败）⇒ 那一条留在账上、状态读成 `error`（用户看得见、自己停掉）。
#[tokio::test]
async fn a_link_that_ends_on_its_own_reads_as_error() {
    let reach = table_with("dev");
    let ledger = Ledger::new();
    start_with(
        &args("dev", 15433, "localhost", 22),
        &reach,
        &ledger,
        |_r, _up, mut down| async move {
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

/// 〔MIG-1 续 · 主会话裁：流没起的远端不许拒〕可达表里没有那台、界面一并交来了它的配置 ⇒ 按配置自己组请求去拨（`dial/machine.rs`），
/// 交给链路那一侧的是 `use: forward` ＋ 这条规格 ＋ 配置里的那台；配置也没交 ⇒ 才是 `unreachable`。
#[tokio::test]
async fn a_machine_that_never_streamed_is_dialled_from_its_config() {
    let empty: remote_ask::Table = Mutex::new(BTreeMap::new());
    let ledger = Ledger::new();
    let seen = Arc::new(Mutex::new(None::<(String, u16, crate::dial::Use)>));
    let s = Arc::clone(&seen);
    let mut a = args("dev", 15434, "localhost", 5432);
    a["machine"] = json!({"host": "10.9.9.9", "label": "dev", "port": 2201, "user": "u"});
    start_with(&a, &empty, &ledger, move |req, _up, mut down| {
        *s.lock().unwrap() = Some((req.host.clone(), req.port, req.use_));
        async move {
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
        Some(("10.9.9.9".to_string(), 2201, crate::dial::Use::Forward))
    );
    assert_eq!(list_with(&ledger)["forwards"][0]["origin"], "dev");
}
