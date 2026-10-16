//! 拨号核心拆出来时新加的几样（手机要、桌面从前没有）：多跳 · 密码 · 外部签名器（Keystore 那一形）· PTY ＋ 改窗口 ·
//! 写的规矩（宿主装、线上改不动）。都对一台**进程内**的 SSH 服务端（[`Door`]，回环口上，测试档不许出网）真拨一趟：
//! 走生产那条 `uses::run`（拨号 → 鉴权 → 开通道 → 阶段行 ＋ ack ＋ 字节），不另写一份拨号。

use crate::guard_support::{register, remembered, test_host, TestCred};
use crate::{DialRequest, StageSink};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// 进程内的一台 SSH 服务端：按配置收密码 / 认一把公钥；session 通道都答应；PTY · shell · 改窗口 · exec 都记一笔；
/// direct-tcpip 照拨本机回环上那个口（跳板那一形）。
#[derive(Clone)]
struct Door {
    password: Option<String>,
    key: Option<russh::keys::PublicKey>,
    log: Arc<Mutex<Vec<String>>>,
    /// 开了 shell 的那几条通道（只回显它们：russh 服务端把隧道通道上的字节也报给 `data`，回显那些会把隧道搅坏）。
    shells: Arc<Mutex<Vec<russh::ChannelId>>>,
    /// 有它 ⇒ 收到密码先等它放行再答（判据要停在「认证」那一步之前看进展行）。
    gate: Option<Arc<tokio::sync::Notify>>,
}

impl Door {
    fn note(&self, s: String) {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).push(s);
    }
}

impl russh::server::Handler for Door {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        _user: &str,
        password: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        if let Some(g) = &self.gate {
            g.notified().await;
        }
        Ok(if self.password.as_deref() == Some(password) {
            russh::server::Auth::Accept
        } else {
            russh::server::Auth::reject()
        })
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        key: &russh::keys::PublicKey,
    ) -> Result<russh::server::Auth, Self::Error> {
        // russh 先核过签名才问到这里：认的是「这把公钥、而且对面真持有它的私钥」。
        Ok(
            if self.key.as_ref().map(|k| k.key_data()) == Some(key.key_data()) {
                self.note("publickey ok".to_string());
                russh::server::Auth::Accept
            } else {
                russh::server::Auth::reject()
            },
        )
    }

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<russh::server::Msg>,
        _session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: russh::ChannelId,
        term: &str,
        cols: u32,
        rows: u32,
        _pw: u32,
        _ph: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.note(format!("pty {term} {cols}x{rows}"));
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: russh::ChannelId,
        session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.note("shell".to_string());
        self.shells
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(channel);
        session.channel_success(channel)?;
        session.data(channel, b"$ ".to_vec())
    }

    async fn window_change_request(
        &mut self,
        channel: russh::ChannelId,
        cols: u32,
        rows: u32,
        _pw: u32,
        _ph: u32,
        session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.note(format!("resize {cols}x{rows}"));
        // 改完回一句，判据据它知道这一下到了（不靠睡）。
        session.data(channel, format!("[{cols}x{rows}]").into_bytes())
    }

    async fn data(
        &mut self,
        channel: russh::ChannelId,
        data: &[u8],
        session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        // shell 那几条回显（像一个 PTY 那样）；别的（隧道）不碰。
        if self
            .shells
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&channel)
        {
            session.data(channel, data.to_vec())?;
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: russh::ChannelId,
        data: &[u8],
        session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.note(format!("exec {}", String::from_utf8_lossy(data)));
        session.channel_success(channel)?;
        session.data(channel, b"ran\n".to_vec())?;
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: russh::Channel<russh::server::Msg>,
        host: &str,
        port: u32,
        _oa: &str,
        _op: u32,
        _session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        self.note(format!("tcpip {host}:{port}"));
        let Ok(port) = u16::try_from(port) else {
            return Ok(false);
        };
        assert!(
            upstream_url_core::upstream_is_loopback(host),
            "测试档不许出网：跳板被要求开到 {host}"
        );
        let Ok(mut tcp) = tokio::net::TcpStream::connect((host, port)).await else {
            return Ok(false);
        };
        tokio::spawn(async move {
            let mut ch = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut ch, &mut tcp).await;
        });
        Ok(true)
    }
}

/// 在回环口上起一台 [`Door`]，回 `(口, 它的记账)`。每进一条 TCP 起一次服务端会话；进程收尾时随运行时一起走。
async fn door(
    password: Option<&str>,
    key: Option<russh::keys::PublicKey>,
) -> (u16, Arc<Mutex<Vec<String>>>) {
    let (port, log, _) = door_with_sessions(password, key).await;
    (port, log)
}

/// 同 [`door`]，另回每条进来的服务端会话的收场把手（判据要从服务端那一侧掐断连接）。
#[allow(clippy::type_complexity)]
async fn door_with_sessions(
    password: Option<&str>,
    key: Option<russh::keys::PublicKey>,
) -> (
    u16,
    Arc<Mutex<Vec<String>>>,
    Arc<Mutex<Vec<russh::server::Handle>>>,
) {
    door_keyed(password, key, None).await
}

/// 同 [`door_with_sessions`]，host key 由判据给（缺 ⇒ 按口派生一把 ed25519）。
#[allow(clippy::type_complexity)]
async fn door_keyed(
    password: Option<&str>,
    key: Option<russh::keys::PublicKey>,
    host_keys: Option<Vec<russh::keys::PrivateKey>>,
) -> (
    u16,
    Arc<Mutex<Vec<String>>>,
    Arc<Mutex<Vec<russh::server::Handle>>>,
) {
    door_full(password, key, host_keys, None).await
}

/// 收密码先等 `gate` 放行的那一台（只要口）。
async fn door_gated(password: &str, gate: Arc<tokio::sync::Notify>) -> u16 {
    door_full(Some(password), None, None, Some(gate)).await.0
}

#[allow(clippy::type_complexity)]
async fn door_full(
    password: Option<&str>,
    key: Option<russh::keys::PublicKey>,
    host_keys: Option<Vec<russh::keys::PrivateKey>>,
    gate: Option<Arc<tokio::sync::Notify>>,
) -> (
    u16,
    Arc<Mutex<Vec<String>>>,
    Arc<Mutex<Vec<russh::server::Handle>>>,
) {
    test_host();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("绑回环口");
    let port = listener.local_addr().expect("口").port();
    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let handler = Door {
        password: password.map(str::to_string),
        key,
        log: Arc::clone(&log),
        shells: Arc::default(),
        gate,
    };
    let keys = host_keys.unwrap_or_else(|| {
        vec![russh::keys::PrivateKey::from(
            russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[port as u8; 32]),
        )]
    });
    let cfg = Arc::new(russh::server::Config {
        keys,
        auth_rejection_time: std::time::Duration::ZERO,
        auth_rejection_time_initial: Some(std::time::Duration::ZERO),
        ..Default::default()
    });
    let sessions: Arc<Mutex<Vec<russh::server::Handle>>> = Arc::default();
    let seen = Arc::clone(&sessions);
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let cfg = Arc::clone(&cfg);
            let h = handler.clone();
            h.note("accept".to_string());
            let seen = Arc::clone(&seen);
            tokio::spawn(async move {
                if let Ok(running) = russh::server::run_stream(cfg, tcp, h).await {
                    seen.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(running.handle());
                    let _ = running.await;
                }
            });
        }
    });
    (port, log, sessions)
}

fn request(v: serde_json::Value) -> DialRequest {
    serde_json::from_value(v).expect("拨号请求读不动")
}

/// 跑一趟 `uses::run`，读阶段行之后的那一行 ack；回 `(ack, 之后的读端, 上行写端, 跑着的那一趟)`。
async fn open(
    req: DialRequest,
    control: crate::uses::Control,
) -> (
    serde_json::Value,
    BufReader<tokio::io::DuplexStream>,
    tokio::io::DuplexStream,
    tokio::task::JoinHandle<()>,
) {
    let (up_w, up_r) = tokio::io::duplex(64 * 1024);
    let (mut down_w, down_r) = tokio::io::duplex(64 * 1024);
    let run = tokio::spawn(async move {
        let stages = StageSink::new(req.stages);
        crate::uses::run_with(&req, &stages, up_r, &mut down_w, control).await;
    });
    let mut rd = BufReader::new(down_r);
    loop {
        let mut line = String::new();
        let n = tokio::time::timeout(guard_core::wall::CEILING, rd.read_line(&mut line))
            .await
            .expect("等 ack 超时")
            .expect("读 ack");
        assert!(n > 0, "没等到 ack 链路就断了");
        let v: serde_json::Value = serde_json::from_str(&line).expect("ack 不是 JSON");
        if v.get("stage").is_none() {
            return (v, rd, up_w, run);
        }
    }
}

/// 读一行（期限内）。
async fn line_of(rd: &mut BufReader<tokio::io::DuplexStream>) -> serde_json::Value {
    let mut line = String::new();
    let n = tokio::time::timeout(guard_core::wall::CEILING, rd.read_line(&mut line))
        .await
        .expect("等一行超时")
        .expect("读一行");
    assert!(n > 0, "链路断了");
    serde_json::from_str(&line).expect("不是 JSON")
}

/// 跑一趟带 `progress` 的拨号，逐行读：进展行的那一步记下、到 ack 为止；`at_step` 读到第几步时调一次 `poke`。
async fn steps_until_ack(
    req: DialRequest,
    mut poke: impl FnMut(&[String]),
) -> (Vec<String>, serde_json::Value) {
    let (_up_w, up_r) = tokio::io::duplex(64 * 1024);
    let (mut down_w, down_r) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let stages = StageSink::new(req.stages);
        crate::uses::run(&req, &stages, up_r, &mut down_w).await;
    });
    let mut rd = BufReader::new(down_r);
    let mut steps = Vec::new();
    loop {
        let v = line_of(&mut rd).await;
        match v.get("progress").and_then(serde_json::Value::as_str) {
            Some(st) => {
                steps.push(st.to_string());
                poke(&steps);
            }
            None => return (steps, v),
        }
    }
}

/// ★ **进展行**（`progress: true`）：拨号每过一步当场写一行 `{"progress":…}`（不攒到 ack）—— 停在认证之前（服务端还没答密码）时
/// 对面已经读到 TCP 连上 · 收到 SSH 头 · host key 过了那三步；放行之后认证过 · 通道开了，再是 ack。不要它的请求一行都不出。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_step_of_a_dial_is_written_the_moment_it_passes() {
    let gate = Arc::new(tokio::sync::Notify::new());
    let port = door_gated("p", Arc::clone(&gate)).await;
    register("live-p", TestCred::Password("p".into()));
    let req = request(serde_json::json!({
        "host": "127.0.0.1", "port": port, "user": "u", "key_path": "live-p", "probe": true,
        "use": "capture", "command": "uname", "capture": {"max_bytes": 1024}, "progress": true
    }));
    let (steps, ack) = steps_until_ack(req, |s| {
        if s.len() == 3 {
            assert_eq!(s, ["tcp", "banner", "hostkey"], "认证之前那几步要当场到");
            gate.notify_one();
        }
    })
    .await;
    assert_eq!(ack["ok"], true, "{ack}");
    assert_eq!(steps, ["tcp", "banner", "hostkey", "auth", "channel"]);
    let (port, _) = door(Some("p"), None).await;
    let quiet = request(serde_json::json!({
        "host": "127.0.0.1", "port": port, "user": "u", "key_path": "live-p", "probe": true,
        "use": "capture", "command": "uname", "capture": {"max_bytes": 1024}
    }));
    let (steps, ack) = steps_until_ack(quiet, |_| {}).await;
    assert!(
        steps.is_empty() && ack["ok"] == true,
        "没要进展行却出了：{steps:?} {ack}"
    );
}

/// 经跳板：跳板那一台与目标那一台各走一遍（中间多一步「隧道开出来了」）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_jumped_dial_reports_each_hop_and_the_tunnel() {
    let (p_hop, _) = door(Some("h"), None).await;
    let (p_target, _) = door(Some("t"), None).await;
    register("step-h", TestCred::Password("h".into()));
    register("step-t", TestCred::Password("t".into()));
    let req = request(serde_json::json!({
        "host": "127.0.0.1", "port": p_target, "user": "u", "key_path": "step-t", "probe": true,
        "use": "capture", "command": "uname", "capture": {"max_bytes": 1024}, "progress": true,
        "jump": {"host": "127.0.0.1", "port": p_hop, "user": "u", "key_path": "step-h", "label": "甲"}
    }));
    let (steps, ack) = steps_until_ack(req, |_| {}).await;
    assert_eq!(ack["ok"], true, "{ack}");
    assert_eq!(
        steps,
        ["tcp", "banner", "hostkey", "auth", "tunnel", "banner", "hostkey", "auth", "channel"]
    );
}

/// 读到 `want` 出现为止（期限内）。
async fn read_until(rd: &mut BufReader<tokio::io::DuplexStream>, want: &str) -> String {
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    let deadline = guard_core::wall::CEILING;
    while !String::from_utf8_lossy(&got).contains(want) {
        let n = tokio::time::timeout(deadline, rd.read(&mut buf))
            .await
            .unwrap_or_else(|_| {
                panic!("等 {want:?} 超时，已收 {:?}", String::from_utf8_lossy(&got))
            })
            .expect("读");
        assert!(
            n > 0,
            "等 {want:?} 时链路断了，已收 {:?}",
            String::from_utf8_lossy(&got)
        );
        got.extend_from_slice(&buf[..n]);
    }
    String::from_utf8_lossy(&got).into_owned()
}

/// ★ 密码鉴权：钥匙那一格交的是一个编号、宿主把它换成密码（手机那一形）。对的密码登上、跑一次 capture；
/// 错的密码 ⇒ ack 失败、原因码 `auth`（不是「私钥读不出来」）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_password_from_the_host_logs_in_and_a_wrong_one_is_refused() {
    let (port, log) = door(Some("s3cret"), None).await;
    register("pw-good", TestCred::Password("s3cret".into()));
    register("pw-bad", TestCred::Password("nope".into()));
    let req = |key: &str| {
        request(serde_json::json!({
            "host": "127.0.0.1", "port": port, "user": "u", "key_path": key,
            "use": "capture", "command": "echo", "capture": {"max_bytes": 1024}, "stages": true
        }))
    };
    let (ack, mut rd, _up, _run) = open(req("pw-good"), crate::uses::Control::none()).await;
    assert_eq!(ack["ok"], true, "对的密码没登上：{ack}");
    let mut line = String::new();
    rd.read_line(&mut line).await.expect("读结果行");
    let got: serde_json::Value = serde_json::from_str(&line).expect("结果行");
    assert_eq!(got["stdout"], "ran\n");
    assert_eq!(got["exit_status"], 0);
    assert!(log.lock().unwrap().iter().any(|l| l == "exec echo"));

    let (ack, ..) = open(req("pw-bad"), crate::uses::Control::none()).await;
    assert_eq!(ack["ok"], false, "错的密码居然登上了：{ack}");
    assert_eq!(ack["reason"], "auth", "{ack}");
}

/// 一个替我们签名的外部签名器（同手机 Keystore：私钥不出它、回 DER）。这里拿 `ring` 现生成一把 P-256。
struct RingSigner {
    pair: ring::signature::EcdsaKeyPair,
    public: russh::keys::PublicKey,
    signed: Arc<Mutex<usize>>,
}

impl RingSigner {
    fn new() -> Arc<RingSigner> {
        let rng = ring::rand::SystemRandom::new();
        let alg = &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING;
        let pkcs8 = ring::signature::EcdsaKeyPair::generate_pkcs8(alg, &rng).expect("生成钥匙");
        let pair =
            ring::signature::EcdsaKeyPair::from_pkcs8(alg, pkcs8.as_ref(), &rng).expect("读回钥匙");
        use ring::signature::KeyPair as _;
        let point = pair.public_key().as_ref().to_vec();
        let public = russh::keys::PublicKey::from(
            russh::keys::ssh_key::public::EcdsaPublicKey::from_sec1_bytes(&point).expect("公钥点"),
        );
        Arc::new(RingSigner {
            pair,
            public,
            signed: Arc::default(),
        })
    }
}

impl crate::env::ExternalSigner for RingSigner {
    fn public_key(&self) -> russh::keys::PublicKey {
        self.public.clone()
    }

    fn sign(
        &self,
        data: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, crate::Said>> + Send>>
    {
        *self.signed.lock().unwrap() += 1;
        let rng = ring::rand::SystemRandom::new();
        let der = self
            .pair
            .sign(&rng, &data)
            .map(|s| s.as_ref().to_vec())
            .map_err(|e| crate::Said::from(format!("sign: {e}")));
        Box::pin(async move { der })
    }
}

/// ★ 外部签名器鉴权（Keystore 那一形）：私钥不出签名器，回的 DER 由本 crate 换成 SSH 的签名格式；服务端核过签名才放行。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_external_signer_logs_in_and_its_der_signature_is_converted() {
    let signer = RingSigner::new();
    let (port, log) = door(None, Some(signer.public.clone())).await;
    register("keystore-a", TestCred::Signer(signer.clone()));
    let req = request(serde_json::json!({
        "host": "127.0.0.1", "port": port, "user": "u", "key_path": "keystore-a",
        "use": "capture", "command": "id", "capture": {"max_bytes": 1024}
    }));
    let (ack, ..) = open(req, crate::uses::Control::none()).await;
    assert_eq!(ack["ok"], true, "外部签名器没登上：{ack}");
    assert!(*signer.signed.lock().unwrap() >= 1, "签名器一次都没被叫到");
    assert!(log.lock().unwrap().iter().any(|l| l == "publickey ok"));
}

/// DER ⇒ SSH 的 ECDSA 签名本体：两个 INTEGER 原样成两个 mpint（最高位是 1 的那一格 DER 已经补了 0x00，mpint 也要它）；
/// 认不出的 DER ⇒ `None`。期望逐字节手写（异源于实现）。
#[test]
fn a_der_ecdsa_signature_becomes_two_mpints() {
    // SEQUENCE(len 8) { INTEGER 01 02 · INTEGER 00 80 01 }
    let der = [
        0x30, 0x09, 0x02, 0x02, 0x01, 0x02, 0x02, 0x03, 0x00, 0x80, 0x01,
    ];
    assert_eq!(
        crate::connect::ecdsa_der_to_ssh(&der),
        Some(vec![0, 0, 0, 2, 0x01, 0x02, 0, 0, 0, 3, 0x00, 0x80, 0x01])
    );
    assert_eq!(
        crate::connect::ecdsa_der_to_ssh(&der[..der.len() - 1]),
        None
    );
    assert_eq!(crate::connect::ecdsa_der_to_ssh(&[0x31, 0x00]), None);
}

/// ★ 多跳：本机 → 甲 → 乙 → 目标，每一台都是一台真服务端（各自的密码）；逐台经上一台的 direct-tcpip 开到下一台，
/// 跳板那几台的指纹进 `jump_fingerprints`、目标那一台的进 `fingerprints`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_jumps_reach_the_target_through_each_other() {
    let (p_outer, log_outer) = door(Some("a"), None).await;
    let (p_inner, log_inner) = door(Some("b"), None).await;
    let (p_target, log_target) = door(Some("c"), None).await;
    register("hop-a", TestCred::Password("a".into()));
    register("hop-b", TestCred::Password("b".into()));
    register("hop-c", TestCred::Password("c".into()));
    let req = request(serde_json::json!({
        "host": "127.0.0.1", "port": p_target, "user": "u", "key_path": "hop-c",
        "use": "capture", "command": "uname", "capture": {"max_bytes": 1024},
        "jump": {
            "host": "127.0.0.1", "port": p_inner, "user": "u", "key_path": "hop-b", "label": "乙",
            "jump": {"host": "127.0.0.1", "port": p_outer, "user": "u", "key_path": "hop-a", "label": "甲"}
        }
    }));
    let chain: Vec<&str> = crate::connect::hop_chain(&req)
        .iter()
        .map(|h| h.label.as_str())
        .collect();
    assert_eq!(chain, vec!["甲", "乙"], "跳板链要从最外一台起");
    let (ack, mut rd, _up, _run) = open(req, crate::uses::Control::none()).await;
    assert_eq!(ack["ok"], true, "两跳没连上：{ack}");
    let mut line = String::new();
    rd.read_line(&mut line).await.expect("读结果行");
    assert!(line.contains("ran"), "{line}");
    assert!(
        log_outer
            .lock()
            .unwrap()
            .contains(&format!("tcpip 127.0.0.1:{p_inner}")),
        "外一台没替我们开到内一台"
    );
    assert!(
        log_inner
            .lock()
            .unwrap()
            .contains(&format!("tcpip 127.0.0.1:{p_target}")),
        "内一台没替我们开到目标"
    );
    assert!(log_target.lock().unwrap().iter().any(|l| l == "exec uname"));
    assert_eq!(
        ack["jump_fingerprints"].as_object().map(|m| m.len()),
        Some(2),
        "两台跳板的指纹该各记一格：{ack}"
    );
    assert_eq!(
        ack["fingerprints"].as_object().map(|m| m.len()),
        Some(1),
        "{ack}"
    );
}

/// 多跳的连接身份：外一台进了身份（换了外一台就是另一条连接）；只一跳时身份与从前同形（`[主机, 口, 用户, 钥匙, 指纹]`）。
#[test]
fn the_pool_identity_carries_every_hop() {
    let one = request(serde_json::json!({
        "host": "t", "port": 22, "user": "u",
        "jump": {"host": "j", "port": 22, "user": "ju"}
    }));
    let two = request(serde_json::json!({
        "host": "t", "port": 22, "user": "u",
        "jump": {"host": "j", "port": 22, "user": "ju", "jump": {"host": "o", "port": 2200, "user": "ou"}}
    }));
    let id1 = crate::pool::identity(&one);
    assert!(id1.contains(r#"["j",22,"ju",null,null]"#), "{id1}");
    let id2 = crate::pool::identity(&two);
    assert_ne!(id1, id2, "换了外一台跳板还是同一个身份");
    assert!(id2.contains(r#"["o",2200,"ou",null,null]"#), "{id2}");
}

/// ★ PTY：要一个 PTY（终端名 ＋ 列行）开 shell，之后原样对拷；改窗口走 `Control` 那根管子、远端收到 window-change。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shell_gets_its_pty_and_follows_resizes() {
    let (port, log) = door(Some("p"), None).await;
    register("pty-pw", TestCred::Password("p".into()));
    let (resize_tx, resize_rx) = tokio::sync::mpsc::unbounded_channel();
    let req = request(serde_json::json!({
        "host": "127.0.0.1", "port": port, "user": "u", "key_path": "pty-pw",
        "use": "shell", "pty": {"term": "xterm-256color", "cols": 100, "rows": 30}
    }));
    let control = crate::uses::Control {
        resize: Some(resize_rx),
    };
    let (ack, mut rd, mut up, _run) = open(req, control).await;
    assert_eq!(ack["ok"], true, "shell 没开成：{ack}");
    read_until(&mut rd, "$ ").await;
    up.write_all(b"hello").await.unwrap();
    read_until(&mut rd, "hello").await;
    resize_tx.send((120, 40)).unwrap();
    read_until(&mut rd, "[120x40]").await;
    let log = log.lock().unwrap().clone();
    assert!(
        log.contains(&"pty xterm-256color 100x30".to_string()),
        "{log:?}"
    );
    assert!(log.contains(&"shell".to_string()), "{log:?}");
    assert!(log.contains(&"resize 120x40".to_string()), "{log:?}");
    assert!(crate::USES.contains(&"shell"));
}

/// 写的规矩只认宿主装的那一种（[`crate::sftp::WriteRules`]）：没装 ⇒ 一个字节都不许写（缺省宁可写不成）。
#[test]
fn the_bare_host_writes_nothing_remote() {
    assert_eq!(
        crate::env::Env::bare().files,
        crate::sftp::WriteRules::ReadOnly
    );
}

/// ★ 握着一条连接（`use: hold`，手机那一形：连上之后别的链路都复用它）：ack 之后链路不收工；同一身份的下一条链路
/// 复用这条连接（服务端只进来过一条 TCP）；服务端那头断开 ⇒ 握着的链路当场收工（事件驱动，不靠钟）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_connection_is_reused_and_its_link_ends_when_the_peer_goes() {
    let (port, log, sessions) = door_with_sessions(Some("h"), None).await;
    register("hold-pw", TestCred::Password("h".into()));
    let req = |u: &str| {
        request(serde_json::json!({
            "host": "127.0.0.1", "port": port, "user": "u", "key_path": "hold-pw",
            "use": u, "command": "echo", "capture": {"max_bytes": 1024}
        }))
    };
    let (ack, mut held, _up, run) = open(req("hold"), crate::uses::Control::none()).await;
    assert_eq!(ack["ok"], true, "{ack}");
    let (ack2, mut rd, _up2, _run2) = open(req("capture"), crate::uses::Control::none()).await;
    assert_eq!(ack2["ok"], true, "{ack2}");
    let mut line = String::new();
    rd.read_line(&mut line).await.unwrap();
    assert!(line.contains("ran"), "{line}");
    let accepts = log
        .lock()
        .unwrap()
        .iter()
        .filter(|l| *l == "accept")
        .count();
    assert_eq!(accepts, 1, "第二条链路没复用握着的那条连接");
    assert!(!run.is_finished(), "握着的链路 ack 之后就收工了");
    let handles: Vec<russh::server::Handle> = sessions.lock().unwrap().clone();
    for h in handles {
        let _ = h
            .disconnect(russh::Disconnect::ByApplication, "bye".into(), "en".into())
            .await;
    }
    let mut rest = Vec::new();
    let n = tokio::time::timeout(guard_core::wall::CEILING, held.read_to_end(&mut rest))
        .await
        .expect("服务端断开之后握着的链路一直没收工");
    assert!(n.is_ok());
    tokio::time::timeout(guard_core::wall::CEILING, run)
        .await
        .expect("那一趟没收场")
        .unwrap();
    assert!(crate::USES.contains(&"hold"));
}

/// 一把 host key 的 OpenSSH 那一行前两段（算法 ＋ base64）。
fn key_line(k: &russh::keys::PrivateKey) -> String {
    let line = k.public_key().to_openssh().unwrap();
    line.split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

fn ecdsa_key() -> russh::keys::PrivateKey {
    russh::keys::PrivateKey::random(
        &mut rand::rng(),
        russh::keys::Algorithm::Ecdsa {
            curve: russh::keys::EcdsaCurve::NistP256,
        },
    )
    .unwrap()
}

fn ed25519_key(seed: u8) -> russh::keys::PrivateKey {
    russh::keys::PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(
        &[seed; 32],
    ))
}

async fn dial_with_known(port: u16, known: Vec<String>) -> serde_json::Value {
    register("known-pw", TestCred::Password("k".into()));
    let req = request(serde_json::json!({
        "host": "127.0.0.1", "port": port, "user": "u", "key_path": "known-pw",
        "use": "capture", "command": "echo", "capture": {"max_bytes": 64},
        "known_keys": known
    }));
    open(req, crate::uses::Control::none()).await.0
}

/// ★ 同 OpenSSH：记过的类型排在协商前头 ⇒ 服务端两种都给时协商出记过的那一种、按那一把严格比、过。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_known_key_type_is_negotiated_first_and_passes() {
    let ec = ecdsa_key();
    let (port, _l, _s) = door_keyed(Some("k"), None, Some(vec![ed25519_key(9), ec.clone()])).await;
    let ack = dial_with_known(port, vec![key_line(&ec)]).await;
    assert_eq!(ack["ok"], true, "记过的那一种没被协商出来 / 被拒了：{ack}");
}

/// ★ 服务端只给一种这台没记过的类型（而别的类型记过）⇒ 拒，报「钥匙变了」带类型；同类型但不是那一把 ⇒ 也拒。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unstored_key_type_or_another_key_is_refused() {
    let (port, _l, _s) = door_keyed(Some("k"), None, Some(vec![ed25519_key(11)])).await;
    let ack = dial_with_known(port, vec![key_line(&ecdsa_key())]).await;
    assert_eq!(ack["ok"], false, "{ack}");
    assert_eq!(ack["reason"], "host_key", "{ack}");
    assert_eq!(ack["key_type"], "ssh-ed25519", "{ack}");
    let ack = dial_with_known(port, vec![key_line(&ed25519_key(12))]).await;
    assert_eq!(ack["ok"], false, "{ack}");
    assert_eq!(ack["reason"], "host_key", "{ack}");
    // 正控：记的正是那一把 ⇒ 过。
    let ack = dial_with_known(port, vec![key_line(&ed25519_key(11))]).await;
    assert_eq!(ack["ok"], true, "{ack}");
}

/// 赛跑里另拨的那一条用的那一份（[`StageSink::quiet`]）：阶段 · 原因码不进这一趟，进展照报到这一趟的那一头
/// （这一趟在等它：它一步一步在动，循环就不该把这一拨当成「没人回话」丢下）。
#[test]
fn the_quiet_sink_of_a_raced_dial_still_reports_progress() {
    let (live, mut steps) = StageSink::new(true).reporting();
    let quiet = live.quiet();
    quiet.step(crate::step::TCP);
    quiet.note_why(crate::Why::Timeout);
    quiet.emit(crate::Stage::Established);
    assert_eq!(steps.try_recv().ok(), Some(crate::step::TCP));
    assert_eq!(live.why(), None, "另拨那一条的原因码不进这一趟");
}

/// ★ 首见的 host key 鉴权过了才交宿主记：鉴权被拒的那一趟握手照样过了核钥匙那一步，但那把钥匙不进信任库
/// （冒充的端点鉴权不过，写不进终端那一跳要认的那份库）；对的密码再连一次 ⇒ 记下正好那一把、一笔。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_first_seen_key_is_remembered_only_after_login() {
    let host_key = ed25519_key(31);
    let (port, _l, _s) = door_keyed(Some("right"), None, Some(vec![host_key.clone()])).await;
    register("tofu-bad", TestCred::Password("wrong".into()));
    register("tofu-good", TestCred::Password("right".into()));
    let req = |key: &str| {
        request(serde_json::json!({
            "host": "127.0.0.1", "port": port, "user": "u", "key_path": key,
            "use": "capture", "command": "echo", "capture": {"max_bytes": 64}
        }))
    };
    let (ack, ..) = open(req("tofu-bad"), crate::uses::Control::none()).await;
    assert_eq!(ack["ok"], false, "{ack}");
    assert_eq!(ack["reason"], serde_json::json!(cmd_wire::machine::Why::Auth), "{ack}");
    assert!(
        ack["fingerprint"].is_string(),
        "握手要先过核钥匙那一步：{ack}"
    );
    assert_eq!(remembered(port), vec![], "鉴权没过的那一趟把钥匙记下了");
    let (ack, ..) = open(req("tofu-good"), crate::uses::Control::none()).await;
    assert_eq!(ack["ok"], true, "{ack}");
    assert_eq!(
        remembered(port),
        vec![("127.0.0.1".to_string(), port, key_line(&host_key))]
    );
}

/// ★ 经跳板：每一台鉴权过了才记它自己那一把 —— 外一台过了、记下；内一台鉴权被拒、不记；目标没走到、不记。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_hop_is_remembered_only_after_its_own_login() {
    let (p_outer, _) = door(Some("a"), None).await;
    let (p_inner, _) = door(Some("b"), None).await;
    let (p_target, _) = door(Some("c"), None).await;
    register("rem-a", TestCred::Password("a".into()));
    register("rem-b-wrong", TestCred::Password("x".into()));
    register("rem-c", TestCred::Password("c".into()));
    let req = request(serde_json::json!({
        "host": "127.0.0.1", "port": p_target, "user": "u", "key_path": "rem-c",
        "use": "capture", "command": "uname", "capture": {"max_bytes": 64},
        "jump": {
            "host": "127.0.0.1", "port": p_inner, "user": "u", "key_path": "rem-b-wrong", "label": "乙",
            "jump": {"host": "127.0.0.1", "port": p_outer, "user": "u", "key_path": "rem-a", "label": "甲"}
        }
    }));
    let (ack, ..) = open(req, crate::uses::Control::none()).await;
    assert_eq!(ack["ok"], false, "{ack}");
    assert_eq!(remembered(p_outer).len(), 1, "外一台鉴权过了却没记");
    assert_eq!(remembered(p_inner), vec![], "内一台鉴权没过却记下了");
    assert_eq!(remembered(p_target), vec![], "目标没走到却记下了");
}
