//! 〔C2 · `设计/05 §13`〕拨号代理宿主（`dial_host`）的判据。
//!
//! 买到：请求按蛇形键写（与后端 `dial::DialRequest` 读的那一侧逐键对拍，异源：后端源码）·
//! 只放私钥**路径**不放本体 · 竞速顺序 last-good 排首 · 跳板环当场拒 · **没有进程内回落**
//! （本文件生产段零 `russh`、恰好一处起进程）。
//! **买不到**：真代理进程与真 sshd（读数脚本 `tests/evidence/C2-dial-loopback.py`）。

use super::*;

fn cfg(label: &str) -> RemoteConfig {
    RemoteConfig {
        host: "h.example".into(),
        label: label.into(),
        port: 2222,
        user: "u".into(),
        key_path: Some("/home/u/.ssh/id_ed25519".into()),
        backend_path: "/b".into(),
        host_key_fingerprint: Some("SHA256:abc".into()),
        addresses: vec!["10.0.0.9".into(), "[::1]:22".into()],
        jump: None,
    }
}

/// 请求的键 == 后端 `DialRequest` 读的键（两向，只看本侧会写的那些）。异源：从后端源码里现抠字段名。
#[test]
fn the_request_keys_are_the_ones_the_proxy_reads() {
    let req = request(
        &cfg("c2-dial-host-a"),
        "stream",
        serde_json::json!({"command": "x"}),
    )
    .expect("造不出请求");
    let written: std::collections::BTreeSet<String> =
        req.as_object().unwrap().keys().cloned().collect();
    let backend = include_str!("../../src/backend/dial/mod.rs");
    let body = &backend[backend
        .find("pub struct DialRequest {")
        .expect("后端没有 DialRequest 了")..];
    let body = &body[..body.find("\n}\n").unwrap()];
    // 后端字段名（`use_` 线上叫 `use`）
    let read: std::collections::BTreeSet<String> = body
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub "))
        .filter_map(|l| l.split(':').next())
        .map(|f| {
            if f == "use_" {
                "use".to_string()
            } else {
                f.to_string()
            }
        })
        .collect();
    for k in &written {
        assert!(
            read.contains(k),
            "本侧写了 `{k}`，后端 DialRequest 不读它 —— 两端契约漂了"
        );
    }
    for must in [
        "host",
        "port",
        "user",
        "key_path",
        "host_key_fingerprint",
        "command",
        "endpoints",
        "use",
    ] {
        assert!(written.contains(must), "请求里缺 `{must}`");
    }
    // 🔴 凭据面 `K11`：只放路径，不放私钥本体
    assert_eq!(req["key_path"], "/home/u/.ssh/id_ed25519");
    assert!(
        !req.to_string().contains("PRIVATE KEY"),
        "请求里出现了私钥本体的样子"
    );
}

/// 竞速顺序：last-good 排首，其余保序（地址解析与 `RemoteConfig::endpoints` 同一份）。
#[test]
fn the_race_order_puts_last_good_first() {
    let c = cfg("c2-dial-host-b");
    let req = request(&c, "stream", serde_json::json!({})).unwrap();
    let order = |r: &serde_json::Value| -> Vec<String> {
        r["endpoints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| format!("{}:{}", e["host"].as_str().unwrap(), e["port"]))
            .collect()
    };
    assert_eq!(order(&req), ["h.example:2222", "10.0.0.9:2222", "::1:22"]);
    crate::ssh_source::record_last_good(
        &c.origin_label(),
        &crate::ssh_source::Endpoint {
            host: "10.0.0.9".into(),
            port: 2222,
        },
    );
    let req = request(&c, "stream", serde_json::json!({})).unwrap();
    assert_eq!(order(&req), ["10.0.0.9:2222", "h.example:2222", "::1:22"]);
}

/// 跳板指向自己 ⇒ 当场拒（fail-closed，不去拨）。
#[test]
fn a_jump_to_itself_is_refused_before_dialing() {
    let mut c = cfg("c2-dial-host-c");
    c.jump = Some("c2-dial-host-c".into());
    assert_eq!(
        request(&c, "stream", serde_json::json!({})).unwrap_err(),
        "跳板配置指向自己（环）"
    );
}

/// 🔴 **没有退路**（`D11`）：宿主生产段里零 `russh`、恰好一处起进程（起的就是拨号代理），
/// 且拿链路的四个入口都经同一个 `open(`。进程内拨号只许住 `inproc_dial.rs`（只剩 SFTP）。
#[test]
fn the_host_never_dials_in_process() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/dial_host.rs"));
    assert!(
        !prod.contains("russh"),
        "宿主里出现了 russh —— 进程内拨号长回来了"
    );
    guard_core::find_pinned(&prod, "tokio::process::Command::new(&bin)")
        .expect("起拨号代理那一处不是恰好一处");
    assert_eq!(
        prod.matches("Command::new(").count(),
        1,
        "宿主里起进程的地方不止一处"
    );
    for entry in [
        "pub(crate) async fn open_stream(",
        "pub(crate) async fn capture(",
        "pub(crate) async fn probe(",
        "pub(crate) async fn forward(",
    ] {
        let at = prod
            .find(entry)
            .unwrap_or_else(|| panic!("找不到入口 `{entry}`"));
        let body = &prod[at..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert_eq!(
            body.matches("open(cfg, &req,").count(),
            1,
            "`{entry}` 没有恰好一次经 `open` 拿链路"
        );
    }
}

/// 〔C2 · 读数，不进门禁〕**界面这一侧对着真回环 sshd 走一遍**：宿主起真代理、成员读真应答。
///
/// 由 `tests/evidence/C2-dial-loopback.py --monitor` 起 sshd 之后带着环境变量 `C2_LOOPBACK`
/// （一份 JSON：`{host,port,user,key_path,proxy}`）来跑；没有那个变量就**明说跳过**（`#[ignore]`，默认不跑）。
/// 买到：`connect_and_exec_cmd` 的字节流 · `connect_and_exec_capture` 的退出码 · 测试连接的阶段 ＋ 指纹 ·
/// 端口转发的计数 —— 全部经 `dial_host`，界面进程里零 russh。
#[tokio::test]
#[ignore = "要真 sshd：由 tests/evidence/C2-dial-loopback.py --monitor 带环境变量来跑"]
async fn loopback_roundtrip_through_the_proxy() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let raw = std::env::var("C2_LOOPBACK").expect("没有 C2_LOOPBACK —— 这条只该由读数脚本来跑");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    // 宿主按 `CCM_DIAL_PROXY` 找代理：读数脚本同时设了它（本条不去改进程环境）。
    assert_eq!(
        std::env::var(DIAL_PROXY_ENV).ok().as_deref(),
        v["proxy"].as_str(),
        "读数脚本没把 CCM_DIAL_PROXY 指到它编出来的那份代理"
    );
    let cfg = RemoteConfig {
        host: v["host"].as_str().unwrap().into(),
        label: "c2-loopback".into(),
        port: v["port"].as_u64().unwrap() as u16,
        user: v["user"].as_str().unwrap().into(),
        key_path: v["key_path"].as_str().map(String::from),
        backend_path: "cat".into(),
        host_key_fingerprint: None,
        addresses: vec![],
        jump: None,
    };
    // ① 字节流：远端 `head -n1`，写进去什么回来什么；它读完一行自己退 ⇒ 下行 EOF ⇒ 链路收工
    let mut s = crate::ssh_source::connect_and_exec_cmd(&cfg, "head -n1")
        .await
        .expect("开不了流");
    s.write_all(b"hello\n").await.unwrap();
    s.flush().await.unwrap();
    let mut got = String::new();
    s.read_to_string(&mut got).await.unwrap();
    assert_eq!(got, "hello\n");
    // ①b 界面关了写半边 ⇒ 代理收工（`D3③`：界面走了代理跟着走）—— 远端 `cat` 永不自己退
    let mut s = crate::ssh_source::connect_and_exec_cmd(&cfg, "cat")
        .await
        .expect("开不了流");
    s.shutdown().await.unwrap();
    let mut rest = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), s.read_to_end(&mut rest))
        .await
        .expect("关了写半边 10 秒代理还没收工 —— 它挂在一条没人收的下行上了")
        .unwrap();
    // ② 收全：stdout / stderr / 退出码
    let ex = crate::ssh_source::connect_and_exec_capture(&cfg, "echo o; echo e >&2; exit 5", None)
        .await
        .expect("收全失败");
    assert_eq!(
        (ex.stdout.as_str(), ex.stderr.as_str(), ex.exit_status),
        ("o\n", "e\n", Some(5))
    );
    // ③ 测试连接那一趟：阶段按序、ack 带指纹与胜出地址
    let mut kinds: Vec<String> = Vec::new();
    let (_link, ack) = probe(&cfg, "true", &mut |s| {
        kinds.push(
            serde_json::to_value(&s).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string(),
        )
    })
    .await
    .expect("探活失败");
    assert_eq!(kinds, ["dialing", "hostKey", "won", "auth", "established"]);
    assert!(ack
        .fingerprint
        .as_deref()
        .is_some_and(|f| f.starts_with("SHA256:")));
    assert_eq!(ack.endpoint, Some(format!("{}:{}", cfg.host, cfg.port)));
    println!("C2-LOOPBACK-MONITOR ok");
}
