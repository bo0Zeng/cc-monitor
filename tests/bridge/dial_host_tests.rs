//! 〔C2 · `设计/05 §13`〕拨号宿主（`dial_host`）的判据。〔SR1a〕起它不再起任何进程：链路开在本机常驻后端里。
//!
//! 买到：请求按蛇形键写（与后端 `dial::DialRequest` 读的那一侧逐键对拍，异源：后端源码）·
//! 只放私钥**路径**不放本体 · 竞速顺序 last-good 排首 · 跳板环当场拒 · **没有进程内回落、也不起代理进程**
//! （本文件生产段零 `russh`、零起进程；monitor 生产段 `--dial` 那一套零命中）。
//! **买不到**：真后端 × 真 sshd（读数脚本 `tests/evidence/SR1a-link-loopback.py`）。

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
        // 〔SR1a〕常驻后端活得比界面长 ⇒ agent 套接字由界面交过去（缺席时是 null，键照样在）。
        "agent_sock",
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

/// 🔴 **没有退路**（`D11`）：宿主生产段里零 `russh`、**零起进程**（〔SR1a〕C2 那一版恰好一处，起的是
/// `--dial` 代理），拿链路的四个入口都经同一个 `open(`，而 `open` 恰好一次经本机那条流开链路。
/// 进程内拨号只许住 `inproc_dial.rs`（只剩 SFTP，SR1b 的事）。
#[test]
fn the_host_never_dials_in_process_and_spawns_nothing() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/dial_host.rs"));
    assert!(
        !prod.contains("russh"),
        "宿主里出现了 russh —— 进程内拨号长回来了"
    );
    assert_eq!(
        prod.matches("Command::new(").count(),
        0,
        "宿主里又起进程了 —— 每链路一个代理进程的形态回来了（SR1a 裁的是单一常驻后端）"
    );
    guard_core::find_pinned(&prod, "LinkStream::open(client,").expect("开链路那一处不是恰好一处");
    guard_core::find_pinned(&prod, "let client = local_channel().await")
        .expect("拿本机那条流的那一处不是恰好一处");
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

/// `--dial` 那一套的三根针（运行时拼：直接写字面量的话，本文件自己就会被别的扫描器命中）。
fn dial_needles() -> [String; 3] {
    [
        format!("{}{}", "\"--", "dial\""),
        format!("{}{}", "CCM_DIAL_", "REQUEST"),
        format!("{}{}", "CCM_DIAL_", "PROXY"),
    ]
}

/// 一段生产代码里命中了哪几根针。
fn dial_traces(prod: &str) -> Vec<String> {
    dial_needles()
        .into_iter()
        .filter(|n| prod.contains(n.as_str()))
        .collect()
}

/// ★ M2：`--dial` 那一套（子命令 · 请求环境变量 · 代理住址环境变量）在 monitor **生产段零命中**。
/// 正控：同一个判定函数喂一段含三根针的合成代码，三根都要量到（不是「这把尺子什么都量不到」）。
#[test]
fn the_dial_proxy_leaves_no_trace_in_monitor_production() {
    let root = crate::guard_support::repo_root().join("src/bridge/src");
    let mut hits: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for (path, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        for n in dial_traces(&guard_core::production_code(&src)) {
            hits.push(format!("{} 里有 `{n}`", path.display()));
        }
    }
    assert!(
        scanned > 100,
        "只扫到 {scanned} 份 —— 扫描面坏了，本条在空转"
    );
    assert!(hits.is_empty(), "拨号代理那一套还有残留：{hits:?}");
    let [a, b, c] = dial_needles();
    let synthetic = format!("fn f() {{ c.arg({a}).env(\"{b}\", x); std::env::var(\"{c}\"); }}\n");
    assert_eq!(
        dial_traces(&guard_core::production_code(&synthetic)).len(),
        3,
        "正控：三根针在合成代码里没全量到 —— 判定函数坏了，上面的零命中是空真"
    );
}

/// 〔C2 → SR1a · 读数，不进门禁〕**界面这一侧对着真回环 sshd 走一遍**：起一个**真的**本机后端（stdio 载体，
/// 隔离过：私有 HOME / TMUX_TMPDIR、摘掉 TMUX —— `C7i` 红线），用**真的**本机吸收点接上它，
/// 之后宿主经它开链路、成员读真应答。
///
/// 由 `tests/evidence/SR1a-link-loopback.py --monitor` 起 sshd 之后带着环境变量 `SR1A_LOOPBACK`
/// （一份 JSON：`{host,port,user,key_path,backend,home}`）来跑；没有那个变量就**明说跳过**（`#[ignore]`，默认不跑）。
/// 买到：`connect_and_exec_cmd` 的字节流 · `connect_and_exec_capture` 的退出码 · 测试连接的阶段 ＋ 指纹 ·
/// 两条链路复用同一条 SSH（第二条的握手时间只剩开 channel）—— 全部经本机常驻后端，界面进程零 russh、零代理进程。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "要真 sshd ＋ 真后端二进制：由 tests/evidence/SR1a-link-loopback.py --monitor 带环境变量来跑"]
async fn loopback_roundtrip_through_the_resident_backend() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let _local = crate::backend::control::inbound_client::local_origin_test_lock();
    let raw = std::env::var("SR1A_LOOPBACK").expect("没有 SR1A_LOOPBACK —— 这条只该由读数脚本来跑");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    // 起真后端（stdio 载体），用**生产那一个**消费者接上它 ⇒ `<local>` 那条流登记上。
    let home = v["home"].as_str().unwrap();
    let mut child = std::process::Command::new(v["backend"].as_str().unwrap())
        .env("HOME", home)
        .env("TMUX_TMPDIR", home)
        .env_remove("TMUX")
        .env_remove("CCM_LISTEN_PORT")
        .env_remove("CCM_LISTEN_TOKEN")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("起不了后端");
    let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    std::thread::spawn(move || {
        crate::backend::control::local_backend::local_stdio_consumer(stdin, stdout)
    });
    let cfg = RemoteConfig {
        host: v["host"].as_str().unwrap().into(),
        label: "sr1a-loopback".into(),
        port: v["port"].as_u64().unwrap() as u16,
        user: v["user"].as_str().unwrap().into(),
        key_path: v["key_path"].as_str().map(String::from),
        backend_path: "cat".into(),
        host_key_fingerprint: None,
        addresses: vec![],
        jump: None,
    };
    // ① 字节流：远端 `head -n1`，写进去什么回来什么；它读完一行自己退 ⇒ 下行 EOF ⇒ 链路收工
    //   （第一条链路：池里没有 ⇒ 真拨一次）
    let mut s = crate::ssh_source::connect_and_exec_cmd(&cfg, "head -n1")
        .await
        .expect("开不了流");
    s.write_all(b"hello\n").await.unwrap();
    s.flush().await.unwrap();
    let mut got = String::new();
    s.read_to_string(&mut got).await.unwrap();
    assert_eq!(got, "hello\n");
    // ①b 界面关了写半边 ⇒ 链路收工（`D3③`：界面走了它跟着走）—— 远端 `cat` 永不自己退
    let mut s = crate::ssh_source::connect_and_exec_cmd(&cfg, "cat")
        .await
        .expect("开不了流");
    s.shutdown().await.unwrap();
    let mut rest = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), s.read_to_end(&mut rest))
        .await
        .expect("关了写半边 10 秒链路还没收工 —— 它挂在一条没人收的下行上了")
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
    drop(_link);
    let _ = child.kill();
    let _ = child.wait();
    println!("SR1A-LOOPBACK-MONITOR ok");
}
