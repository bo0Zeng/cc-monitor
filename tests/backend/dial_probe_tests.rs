//! 设计/99 §2.1 ⑬ 主会话裁「`test_remote_connection`：后端组拨号请求、拨一次、回结局（后端持有全部 SSH）」——
//! `dial/probe.rs` 的三步各自的结局（链路那一侧用替身）：握手不成 ⇒ `sshOk:false` 且不回指纹 · 通了没 hello · 通了有 hello、控制通道往返。
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

fn args() -> Value {
    json!({"machine": {"host": "10.0.0.2", "label": "devbox", "user": "u"}})
}

#[tokio::test]
async fn a_refused_handshake_reports_ssh_down_with_its_stages_and_no_fingerprint() {
    let r = probe_with(&args(), |req, _up, mut down| async move {
        assert!(req.probe && req.stages, "测试连接是短命探活、要阶段行");
        let _ = down.write_all(b"{\"stage\":{\"kind\":\"dialing\",\"endpoint\":\"10.0.0.2:22\"}}\n").await;
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":false,\"error\":\"host key mismatch\",\"fingerprint\":\"SHA256:x\",\"uses\":[]}\n")
            .await;
    })
    .await
    .unwrap();
    assert_eq!(r["sshOk"], false);
    assert_eq!(
        r["fingerprint"],
        Value::Null,
        "失败时不回指纹（免得被固化）"
    );
    assert!(r["message"].as_str().unwrap().contains("host key mismatch"));
    assert_eq!(
        r["stages"],
        json!([{"kind": "dialing", "endpoint": "10.0.0.2:22"}])
    );
}

#[tokio::test]
async fn ssh_up_but_no_hello_is_its_own_verdict() {
    let r = probe_with(&args(), |_req, _up, mut down| async move {
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":true,\"fingerprint\":\"SHA256:k\",\"endpoint\":\"10.0.0.2:22\",\"uses\":[\"stream\"]}\nnot json\n")
            .await;
    })
    .await
    .unwrap();
    assert_eq!(
        (r["sshOk"].as_bool(), r["backendOk"].as_bool()),
        (Some(true), Some(false))
    );
    assert_eq!(r["fingerprint"], "SHA256:k");
    assert_eq!(r["message"], copy_text("beProbe.test.noHello", &[]));
}

#[tokio::test]
async fn a_hello_that_answers_ping_is_all_green() {
    let r = probe_with(&args(), |_req, up, mut down| async move {
        let _ = down.write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n").await;
        let _ = down
            .write_all(b"{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b1\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/h/.claude\",\"commands\":[\"ping\"]}\n")
            .await;
        // 真去读那一问、按它的 id 回（证明往返真走了这条流）。
        let mut lines = tokio::io::BufReader::new(up).lines();
        let asked: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(asked["cmd"], "ping");
        let reply = json!({"kind": "reply", "id": asked["id"], "ok": true});
        let _ = down.write_all(format!("{reply}\n").as_bytes()).await;
    })
    .await
    .unwrap();
    assert_eq!(r["backendOk"], true);
    assert_eq!(r["message"], copy_text("beProbe.test.ok", &[]));
    let hello = r["backendHello"].as_str().unwrap();
    assert!(
        hello.starts_with("v=1 build=b1 arch=x86_64 home=/h/.claude"),
        "{hello}"
    );
    assert!(
        hello.contains("control=ok(") && hello.ends_with("ms)"),
        "{hello}"
    );
}

#[tokio::test]
async fn an_old_backend_without_commands_says_too_old() {
    // 〔从 monitor `the_control_probe_writes_nothing_to_an_old_backend`〔散文墓碑〕 搬来〕旧后端（没声明命令）：一个字节都不发给它。
    let (tx, rx) = tokio::sync::oneshot::channel::<tokio::io::DuplexStream>();
    let r = probe_with(&args(), |_req, up, mut down| async move {
        let _ = down
            .write_all(
                b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n{\"kind\":\"hello\",\"v\":1}\n",
            )
            .await;
        // 上行读端交给判据自己读（本任务随探针返回被收，读端不能跟着它没了）；下行随本任务收尾关掉
        // ⇒ 探针若错发了 ping 也等不住（读到 EOF、落「不能起会话」），不会挂住判据。
        let _ = tx.send(up);
    })
    .await
    .unwrap();
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
}
