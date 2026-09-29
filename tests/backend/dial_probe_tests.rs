//! 设计/99 §2.1 ⑬ 主会话裁「`test_remote_connection`：后端组拨号请求、拨一次、回结局（后端持有全部 SSH）」——
//! `dial/probe.rs` 的三步各自的结局（链路那一侧用替身）：握手不成 ⇒ `sshOk:false` 且不回指纹 · 通了没 hello · 通了有 hello、控制通道往返。
//! 〔MIG-1 收尾 · 主会话裁「进度不许倒退」〕进度边拨边推：每条判据同时钉**推了哪几格、什么顺序**（结局是最后一格）。
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

fn args() -> Value {
    json!({"ticket": "t-1", "machine": {"host": "10.0.0.2", "label": "aya", "user": "u"}})
}

/// 跑一趟探针，收下它推的每一格（票都得是交来的那张）；回 `(结局, 结局之前那几格)`。
async fn cells_of<F, Fut>(args: &Value, serve: F) -> (Value, Vec<Value>)
where
    F: FnOnce(crate::dial::DialRequest, tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    probe_with(args, serve, &tx).await.unwrap();
    drop(tx);
    let mut cells = Vec::new();
    while let Some(f) = rx.recv().await {
        match f {
            Frame::Probe { ticket, cell } => {
                assert_eq!(ticket, "t-1", "进度格要带交来的那张票");
                cells.push(cell);
            }
            other => panic!("探针只该推进度格：{other:?}"),
        }
    }
    let end = cells.pop().expect("至少有结局那一格");
    assert_eq!(
        end.as_object().map(|o| o.len()),
        Some(1),
        "每一格恰好一个键：{end}"
    );
    (end["end"].clone(), cells)
}

#[tokio::test]
async fn a_refused_handshake_reports_ssh_down_with_its_stages_and_no_fingerprint() {
    let (r, cells) = cells_of(&args(), |req, _up, mut down| async move {
        assert!(req.probe && req.stages, "测试连接是短命探活、要阶段行");
        let _ = down.write_all(b"{\"stage\":{\"kind\":\"dialing\",\"endpoint\":\"10.0.0.2:22\"}}\n").await;
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":false,\"error\":\"host key mismatch\",\"fingerprint\":\"SHA256:x\",\"uses\":[]}\n")
            .await;
    })
    .await;
    assert_eq!(r["sshOk"], false);
    assert_eq!(
        r["fingerprint"],
        Value::Null,
        "失败时不回指纹（免得被固化）"
    );
    assert!(r["message"].as_str().unwrap().contains("host key mismatch"));
    assert_eq!(
        cells,
        vec![json!({"stage": {"kind": "dialing", "endpoint": "10.0.0.2:22"}})],
        "握手那几行逐条推成 `stage` 格；没过握手 ⇒ 没有 `reached`"
    );
    assert!(r.get("stages").is_none(), "阶段行不再塞进结局");
}

#[tokio::test]
async fn ssh_up_but_no_hello_is_its_own_verdict() {
    let (r, cells) = cells_of(&args(), |_req, _up, mut down| async move {
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":true,\"fingerprint\":\"SHA256:k\",\"endpoint\":\"10.0.0.2:22\",\"uses\":[\"stream\"]}\nnot json\n")
            .await;
    })
    .await;
    assert_eq!(
        cells,
        vec![json!({"reached": "ssh"})],
        "握手过了、hello 没到 ⇒ 只走完 ssh 那一段"
    );
    assert_eq!(
        (r["sshOk"].as_bool(), r["backendOk"].as_bool()),
        (Some(true), Some(false))
    );
    assert_eq!(r["fingerprint"], "SHA256:k");
    assert_eq!(r["message"], copy_text("beProbe.test.noHello", &[]));
}

/// 设计/91 §6 第 9 条 · 99 §2.2 R1 认形状 ＋ 主会话 09-28 裁：机器页「测试连接」那一行不露 `v=… build=… caps=[…]` 日志行，
/// 露三格（版本 · 能用几项 / 做不到几项 · 往返毫秒），做不到的那几类按码分类交给界面；键值对那一形只进日志。
fn is_not_a_log_line(s: &str) -> bool {
    !s.contains('=') && !s.contains('[')
}

#[tokio::test]
async fn a_hello_that_answers_ping_is_all_green() {
    let (r, cells) = cells_of(&args(), |_req, up, mut down| async move {
        let _ = down.write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n").await;
        let _ = down
            .write_all(b"{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b1\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/h/.claude\",\"capabilities\":[\"stream\"],\"commands\":[\"ping\",\"kill\",\"files-chmod\"],\"unavailable\":[{\"command\":\"kill\",\"code\":\"no_tmux\"}]}\n")
            .await;
        // 真去读那一问、按它的 id 回（证明往返真走了这条流）。
        let mut lines = tokio::io::BufReader::new(up).lines();
        let asked: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(asked["cmd"], "ping");
        let reply = json!({"kind": "reply", "id": asked["id"], "ok": true});
        let _ = down.write_all(format!("{reply}\n").as_bytes()).await;
    })
    .await;
    assert_eq!(
        cells,
        vec![
            json!({"reached": "ssh"}),
            json!({"reached": "hello"}),
            json!({"reached": "control"})
        ],
        "三段按序各一格，结局最后"
    );
    assert_eq!(r["backendOk"], true);
    assert_eq!(r["message"], copy_text("beProbe.test.ok", &[]));
    let hello = r["backendHello"].as_str().unwrap();
    // 三格：版本 b1 · 能用 2 项（ping · files-chmod）、做不到 1 项（kill）· 往返 N 毫秒（N 是现量的，只核是数）。
    let want = copy_text(
        "beProbe.hello.ok",
        &[
            ("build", "b1"),
            ("usable", "2"),
            ("gaps", "1"),
            ("ms", "\u{0}"),
        ],
    );
    let (pre, post) = want.split_once('\u{0}').unwrap();
    let ms = hello
        .strip_prefix(pre)
        .and_then(|t| t.strip_suffix(post))
        .unwrap_or_else(|| panic!("不是那三格：{hello}"));
    assert!(
        !ms.is_empty() && ms.chars().all(|c| c.is_ascii_digit()),
        "{hello}"
    );
    // 做不到的按码分类交出去（那句人话归 monitor，`control-said.ts::unavailableReason`）。
    assert_eq!(r["backendGaps"], json!([{"code": "no_tmux", "count": 1}]));
    assert!(is_not_a_log_line(hello), "机器页那一行又是日志行：{hello}");
}

#[tokio::test]
async fn an_old_backend_without_commands_says_too_old() {
    // 〔从 monitor `the_control_probe_writes_nothing_to_an_old_backend`〔散文墓碑〕 搬来〕旧后端（没声明命令）：一个字节都不发给它。
    let (tx, rx) = tokio::sync::oneshot::channel::<tokio::io::DuplexStream>();
    let (r, cells) = cells_of(&args(), |_req, up, mut down| async move {
        let _ = down
            .write_all(
                b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n{\"kind\":\"hello\",\"v\":1}\n",
            )
            .await;
        // 上行读端交给判据自己读（本任务随探针返回被收，读端不能跟着它没了）；下行随本任务收尾关掉
        // ⇒ 探针若错发了 ping 也等不住（读到 EOF、落「不能起会话」），不会挂住判据。
        let _ = tx.send(up);
    })
    .await;
    assert_eq!(
        cells,
        vec![json!({"reached": "ssh"}), json!({"reached": "hello"})],
        "没往返 ⇒ 没有 control 那一格"
    );
    let mut up = rx.await.unwrap();
    let mut got = Vec::new();
    let _ = tokio::io::AsyncReadExt::read_to_end(&mut up, &mut got).await;
    assert!(
        got.is_empty(),
        "对旧后端发了字节：{:?}",
        String::from_utf8_lossy(&got)
    );
    assert_eq!(r["backendOk"], true);
    assert_eq!(r["message"], copy_text("beProbe.test.noControl", &[]));
    let unknown = copy_text("beProbe.hello.noBuild", &[]);
    assert_eq!(
        r["backendHello"],
        json!(copy_text("beProbe.hello.tooOld", &[("build", &unknown)]))
    );
    assert!(is_not_a_log_line(r["backendHello"].as_str().unwrap()));
    assert_eq!(r["backendGaps"], json!([]));
}

#[tokio::test]
async fn a_probe_without_a_valid_ticket_is_refused_before_dialling() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    for bad in [json!(null), json!(""), json!("a b"), json!("x".repeat(65))] {
        let mut a = args();
        a["ticket"] = bad.clone();
        let r = probe_with(
            &a,
            |_req, _up, _down| async move { panic!("票不对就不该拨") },
            &tx,
        )
        .await;
        assert_eq!(r.map_err(|(c, _)| c), Err("invalid_args"), "票 {bad} 该拒");
    }
    drop(tx);
    assert!(rx.recv().await.is_none(), "拒了就一格都不推");
}
