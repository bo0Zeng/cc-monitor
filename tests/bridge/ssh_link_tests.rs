//! 〔C2 · `设计/05 §13`〕拨号应答客户端（`ssh_link`）的判据。
//!
//! 买到：阶段行按到达顺序交出、ack 读得出 · 拨不通带着指纹回来 · 老代理（不认所请求的用法）出声 ·
//! 没回应答 / 形状不对 / 行太长各落各的错 · 收全结果与转发计数读得出。
//! **异源**：下面那几行「代理的输出」逐字抄自对真回环 sshd 跑出来的读数（`tests/evidence/C2-dial-loopback.py`
//! 那一趟的 stdout），不是本侧序列化出来再读回去。阶段 `kind` 的六个字面量另由后端那侧
//! （`tests/backend/dial_tests.rs::the_ack_and_the_stage_lines_have_the_shape_the_monitor_reads`）从它的类型序列化出来核一遍。
//! **买不到**：真代理进程（那一圈归宿主 `dial_host` 与读数脚本）。

use super::*;

/// 逐字抄自真 sshd 读数（竞速一个死端口 ＋ 一个活的，`stages=true`，`use=capture`）。
const REAL_OUTPUT: &str = concat!(
    r#"{"stage":{"endpoint":"127.0.0.1:1","kind":"dialing"}}"#,
    "\n",
    r#"{"stage":{"endpoint":"127.0.0.1:22422","kind":"dialing"}}"#,
    "\n",
    r#"{"stage":{"endpoint":"127.0.0.1:1","kind":"failed","reason":"[tcp] Connection refused (os error 111)"}}"#,
    "\n",
    r#"{"stage":{"endpoint":"127.0.0.1:22422","fingerprint":"SHA256:t3uliopxPzh9UPGAywEklG+BprfkJP07toWVhzBCwB4","kind":"hostKey"}}"#,
    "\n",
    r#"{"stage":{"endpoint":"127.0.0.1:22422","kind":"won"}}"#,
    "\n",
    r#"{"stage":{"detail":null,"kind":"auth","ok":true}}"#,
    "\n",
    r#"{"stage":{"kind":"established"}}"#,
    "\n",
    r#"{"ok":true,"error":null,"fingerprint":"SHA256:t3uliopxPzh9UPGAywEklG+BprfkJP07toWVhzBCwB4","endpoint":"127.0.0.1:22422","v":2,"uses":["stream","capture","forward"]}"#,
    "\n",
    r#"{"stdout":"out\n","stderr":"err\n","exit_status":7}"#,
    "\n",
);

const CAP: u64 = 64 * 1024;

#[tokio::test]
async fn stages_come_out_in_order_then_the_ack_then_the_captured_line() {
    let mut r = tokio::io::BufReader::new(REAL_OUTPUT.as_bytes());
    let mut seen: Vec<ConnectStage> = Vec::new();
    let ack = handshake(&mut r, "capture", CAP, &mut |s| seen.push(s))
        .await
        .expect("真读数读不动");
    assert_eq!(
        seen,
        vec![
            ConnectStage::Dialing {
                endpoint: "127.0.0.1:1".into()
            },
            ConnectStage::Dialing {
                endpoint: "127.0.0.1:22422".into()
            },
            ConnectStage::Failed {
                endpoint: "127.0.0.1:1".into(),
                reason: "[tcp] Connection refused (os error 111)".into()
            },
            ConnectStage::HostKey {
                endpoint: "127.0.0.1:22422".into(),
                fingerprint: "SHA256:t3uliopxPzh9UPGAywEklG+BprfkJP07toWVhzBCwB4".into()
            },
            ConnectStage::Won {
                endpoint: "127.0.0.1:22422".into()
            },
            ConnectStage::Auth {
                ok: true,
                detail: None
            },
            ConnectStage::Established,
        ]
    );
    assert_eq!(ack.endpoint.as_deref(), Some("127.0.0.1:22422"));
    assert_eq!(ack.v, 2);
    let got = captured(&mut r, CAP).await.expect("结果行读不动");
    assert_eq!(
        (got.stdout.as_str(), got.stderr.as_str(), got.exit_status),
        ("out\n", "err\n", Some(7))
    );
}

/// 拨不通 ⇒ `Refused`，带着看到过的指纹（真读数：严格指纹失配）。
#[tokio::test]
async fn a_refusal_comes_back_with_the_fingerprint_it_saw() {
    let out = concat!(
        r#"{"ok":false,"error":"所有地址连接失败: 127.0.0.1:22422 Unknown server key（竞速：127.0.0.1:22422）","fingerprint":"SHA256:t3uliopxPzh9UPGAywEklG+BprfkJP07toWVhzBCwB4","endpoint":null,"v":2,"uses":["stream","capture","forward"]}"#,
        "\n"
    );
    let mut r = tokio::io::BufReader::new(out.as_bytes());
    let e = handshake(&mut r, "stream", CAP, &mut |_| {})
        .await
        .expect_err("拨不通被读成了通");
    assert_eq!(
        e,
        LinkError::Refused {
            why: "所有地址连接失败: 127.0.0.1:22422 Unknown server key（竞速：127.0.0.1:22422）"
                .into(),
            fingerprint: Some("SHA256:t3uliopxPzh9UPGAywEklG+BprfkJP07toWVhzBCwB4".into()),
        }
    );
}

/// 🔴 老代理（`K-P6b` 那一版的 ack：没有 `v`、没有 `uses`）对 `capture` 请求 ⇒ `TooOld`，**不去解后面那些字节**。
/// 它会把 `capture` 当成长流、在 ack 之后吐原始 stdout —— 读成 JSON 就是一句没人看得懂的解析错。
#[tokio::test]
async fn an_old_proxy_is_named_not_misread() {
    let out = "{\"ok\":true,\"error\":null,\"fingerprint\":\"SHA256:x\"}\nout\n";
    let mut r = tokio::io::BufReader::new(out.as_bytes());
    let e = handshake(&mut r, "capture", CAP, &mut |_| {})
        .await
        .expect_err("老代理被当成新代理了");
    assert_eq!(
        e,
        LinkError::TooOld {
            wanted: "capture".into(),
            v: 0
        }
    );
    assert!(e.to_string().contains("本机后端太旧"));
    // 同一个 ack，请求的是长流 ⇒ **照样出声**（见下面那句为什么）
    let mut r = tokio::io::BufReader::new(out.as_bytes());
    let e = handshake(&mut r, "stream", CAP, &mut |_| {}).await;
    assert_eq!(
        e,
        Err(LinkError::TooOld {
            wanted: "stream".into(),
            v: 0
        }),
        "v1 代理的 ack 没有 `uses`：连 stream 也认不出 —— 这是刻意的（界面与本机后端同一次构建，错配只出在没 bump 的开发树上，出声比猜强）"
    );
}

/// 没回应答 / 形状不对 / 一行太长 —— 三种各落各的错。
#[tokio::test]
async fn silence_garbage_and_an_endless_line_are_three_different_errors() {
    let mut r = tokio::io::BufReader::new(&b""[..]);
    assert_eq!(
        handshake(&mut r, "stream", CAP, &mut |_| {}).await,
        Err(LinkError::Silent)
    );
    let mut r = tokio::io::BufReader::new(&b"not json\n"[..]);
    assert!(matches!(
        handshake(&mut r, "stream", CAP, &mut |_| {}).await,
        Err(LinkError::Garbled(_))
    ));
    let long = format!("{{\"ok\":true,\"pad\":\"{}\"}}", "x".repeat(100));
    let mut r = tokio::io::BufReader::new(long.as_bytes());
    assert!(matches!(
        handshake(&mut r, "stream", 32, &mut |_| {}).await,
        Err(LinkError::LineTooLong(_))
    ));
    // 正控：同一行、上限够 ⇒ 读得过（它是 ok 但缺 `uses` ⇒ 落到 TooOld，不是行太长）
    let mut r = tokio::io::BufReader::new(long.as_bytes());
    assert!(matches!(
        handshake(&mut r, "stream", CAP, &mut |_| {}).await,
        Err(LinkError::TooOld { .. })
    ));
}

/// 转发计数：真读数两行 ＋ 管子关了 ⇒ `None`。
#[tokio::test]
async fn forward_counts_read_until_the_pipe_closes() {
    let out = "{\"accepted\":1}\n{\"accepted\":2}\n";
    let mut r = tokio::io::BufReader::new(out.as_bytes());
    assert_eq!(accepted(&mut r, CAP).await, Ok(Some(1)));
    assert_eq!(accepted(&mut r, CAP).await, Ok(Some(2)));
    assert_eq!(accepted(&mut r, CAP).await, Ok(None));
}
