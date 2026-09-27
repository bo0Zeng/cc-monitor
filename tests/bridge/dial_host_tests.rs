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

// ═══ 〔NT2 · A4〕一次性那一趟的总时限 ═══════════════════════════════════════════════════════════
//
// 守的要求（住址，纪律 19）：`设计/15 §3.2` 第 4 条红线（逐字）「**必须先给无期限路径装期限，再复用**，次序不能换 ——
// 不复用时一条卡住只坏它自己那条连接；复用后它占掉共享连接一个槽永不释放，局部卡死升级成全局卡死」·
// `设计/05 §3.3.2`（逐字）「🔴 **一次调用一个绝对时刻**，不是每跳一个 `Duration`」。
// 设计与现打：`调研/第四波记录/NT2.md §0.1 · §1`。

/// A1 ★ 期限真生效：一条永不回字节的链路，到点读报 `TimedOut`；到点之前写进来的字节照常交出、到点之前读还在等（正控）；
/// 摘掉期限的那条过点仍在等（另一向）。异源：一对内存管子当链路 —— 不经开链路那套逻辑，量的是生产那一层 `Bounded` 本体。
/// （真时钟：bridge 的 tokio 没开 `test-util`，不为一条判据动依赖表；期限取 1 s，判的是先后，不是毫秒。）
#[tokio::test]
async fn the_one_shot_deadline_really_cuts_a_silent_link_and_only_after_its_time() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let total = std::time::Duration::from_secs(1);
    let until = tokio::time::Instant::now() + total;

    // ① 到点之前写进来的那几个字节照常读到（正控：期限层不是「什么都读不到」）。
    let (mut far, near) = tokio::io::duplex(64);
    let mut s = Bounded::new(near, Some((until, total)));
    far.write_all(b"ok").await.unwrap();
    let mut two = [0u8; 2];
    s.read_exact(&mut two).await.unwrap();
    assert_eq!(
        &two, b"ok",
        "到点之前的字节没交出来 —— 期限层把正常的读也挡了"
    );

    // ② 对面再不说话：刚开始读时还在等（没提前报）⇒ 到点 ⇒ 报 `TimedOut`。
    let mut buf = [0u8; 8];
    {
        let read = s.read(&mut buf);
        tokio::pin!(read);
        assert!(
            futures::poll!(read.as_mut()).is_pending(),
            "还没到点就报了 —— 期限提前生效"
        );
        let e = tokio::time::timeout(std::time::Duration::from_secs(30), read)
            .await
            .expect("过了总时限 29 秒读还在等 —— 期限没生效，这一格会永远占着")
            .expect_err("对面一个字节都没写，读却成功了");
        assert_eq!(e.kind(), std::io::ErrorKind::TimedOut);
    }
    // 写也一样被挡（调用方还往链路里写 ⇒ 当场报错、丢链路）。
    let w = s.write_all(b"x").await.expect_err("过点之后写还成功");
    assert_eq!(w.kind(), std::io::ErrorKind::TimedOut);
    drop(far);

    // ③ 另一向：不设期限（长活那三形）⇒ 过了两倍的时间仍在等。
    let (_far2, near2) = tokio::io::duplex(64);
    let mut long = Bounded::new(near2, None);
    assert!(
        tokio::time::timeout(total * 2, long.read(&mut buf))
            .await
            .is_err(),
        "没设期限的那一条也被掐了（或者读到了东西）—— 长连接流 / 端口转发会被误杀"
    );
}

/// 长活那三形：`(文件, 所在函数, 处数, 为什么它不要总时限)`。**这就是「逐处豁免」那张表**（`NT2.md §1.2`）。
const LIVES_LONG: &[(&str, &str, usize, &str)] = &[
    (
        "ssh_source.rs",
        "connect_and_exec",
        1,
        "后端长连接流是订阅：`05 §3.3.2`「`Budget` 只盖建流；流建起来之后没有总期限」；死链靠 keepalive ＋ EOF",
    ),
    (
        "dial_host.rs",
        "forward",
        1,
        "端口转发：用户开着就一直在，关了（丢 `ForwardLink`）就收",
    ),
    (
        "remote_resident.rs",
        "attach",
        1,
        "〔HOST〕接上远端常驻后端之后那条流是订阅（同 `connect_and_exec`）；握手那几行仍在一次性总时限里",
    ),
    (
        "dial_host.rs",
        "open",
        1,
        "`RemoteFs::open`（部署文件面）：一次部署问好几次，每一问自带期限（`FILES_ASK_DEADLINE` / `FILES_PUT_DEADLINE`）",
    ),
];

/// 一份生产段里 `.lives_long` / `::lives_long` 的每一处，按「所在的最近一个 `fn` 名」记账（定义那一行不算）。
/// 注释先经共用原语剥掉（`guard_core::strip_comment_lines`），这里不另写一份剥法。
fn lives_long_sites(file: &str, prod: &str) -> Vec<(String, String)> {
    let code = guard_core::strip_comment_lines(prod);
    let mut out = Vec::new();
    let mut current_fn = String::new();
    for line in code.lines() {
        let t = line.trim_start();
        if let Some((head, rest)) = t.split_once("fn ") {
            if head
                .split_whitespace()
                .all(|w| matches!(w, "pub" | "pub(crate)" | "async" | "const" | "unsafe"))
            {
                current_fn = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
            }
        }
        if t.contains("fn lives_long(") {
            continue;
        }
        for _ in 0..(t.matches(".lives_long").count() + t.matches("::lives_long").count()) {
            out.push((file.to_string(), current_fn.clone()));
        }
    }
    out
}

/// A2 ★ **只有那三形摘掉总时限**（两向，按所在函数与处数）。多一处 = 又有一条一次性的路没有总时限；
/// 少一处 = 长连接流 / 端口转发 / 部署会在 120 s 被掐断。正控：合成语料里多种一处必被认出、且认对所在函数。
#[test]
fn only_the_three_long_lived_links_drop_the_deadline() {
    let root = crate::guard_support::crate_src_root();
    let mut got: Vec<(String, String)> = Vec::new();
    let mut scanned = 0usize;
    for (path, file_text) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        got.extend(lives_long_sites(
            &rel,
            &guard_core::production_code(&file_text),
        ));
    }
    assert!(
        scanned > 100,
        "只扫到 {scanned} 份 monitor 源码 —— 遍历坏了"
    );
    let mut want: Vec<(String, String)> = LIVES_LONG
        .iter()
        .flat_map(|(f, func, n, why)| {
            assert!(
                why.chars().count() >= 20,
                "`{f}::{func}` 没写清为什么不要总时限"
            );
            std::iter::repeat_n((f.to_string(), func.to_string()), *n)
        })
        .collect();
    got.sort();
    want.sort();
    assert_eq!(
        got, want,
        "摘掉一次性总时限（`lives_long`）的地方与登记的那三形对不上。\n\
         多出来的 ⇒ 那条路没有总时限了（`15 §3.2` 第 4 条红线）；该长活就进 `LIVES_LONG` 并写理由。"
    );
    // 正控：识别器认得出、认得对所在函数。
    let synthetic =
        "pub(crate) async fn probe_x() {\n    let s = open_it().await?.lives_long();\n}\n";
    assert_eq!(
        lives_long_sites("x.rs", synthetic),
        vec![("x.rs".to_string(), "probe_x".to_string())],
        "识别器瞎了 —— 上面那条相等不可信"
    );
}

/// A3 接线（文本 —— 如实登记：按行为量要真后端 ＋ 真 sshd，那一格在读数脚本里）：
/// `open` 里起算期限恰好一处、交给链路的恰好是它；`DialStream` 读的那一层就是 `Bounded`。
/// 绕过形态：另造一条不经 `open` 的 `DialStream` —— 构造点只许一处（`DialStream {` 恰好一处且在 `open` 里）。
#[test]
fn every_link_is_born_with_the_one_shot_deadline() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/dial_host.rs"));
    let count = |n: &str| prod.matches(n).count();
    assert_eq!(
        count("tokio::time::Instant::now() + ONE_SHOT_DEADLINE"),
        1,
        "起算总时限的地方不是恰好一处"
    );
    assert_eq!(
        count("Bounded::new(link, Some((due, ONE_SHOT_DEADLINE)))"),
        1,
        "链路不是带着那个期限出生的"
    );
    assert_eq!(
        count("r: BufReader<Bounded<LinkStream>>,"),
        1,
        "`DialStream` 读的那一层不是 `Bounded` 了"
    );
    let at = guard_core::find_pinned(&prod, "\nasync fn open(")
        .unwrap_or_else(|e| panic!("`open` 不是恰好一处：{e}"));
    let (open_fn, _) = prod[at..]
        .split_once("\n}\n")
        .expect("切不出 `open` 的函数体");
    assert!(
        open_fn.contains("ONE_SHOT_DEADLINE") && open_fn.contains("DialStream { r }"),
        "`open` 里没有起算期限、或 `DialStream` 不在这里造"
    );
    assert_eq!(
        count("DialStream { r"),
        1,
        "`DialStream` 的构造应当恰好一处（`open` 里那一次）—— 多一处 = 一条不经 `open`、不带期限的链路"
    );
}

// ═══ 〔W5-VIS〕一个预算、按段归因（`设计/15 §3.6` 小病 · `设计/05 §3.3.2`） ═══════════════════════════
//
// 守的要求（住址，纪律 19）：`设计/05 §3.3.2`（逐字）「**一个预算、多个归因点**：超时回的是 `Hop { at, reach, why: Overrun }`
// —— 期限只有一个，但卡在哪一跳说得出来（治的是「同一个数盖了握手与远端跑查询两段、而两种成因处置完全不同」那一格）」·
// `设计/15 §3.6` 小病（逐字）「⇒ 一个预算、多个归因点（`05 §3.3.2`；W5-VIS）」。

/// B1 ★ 到点那句话按段说：握完手之后到点 ⇒ 说出握手用了多久、之后远端跑了多久（两个数 == 喂进去的两段）；
/// 还在握手就到点 ⇒ 另一句（两句不同）。纯函数，直接喂时长（不睡墙钟）。
#[test]
fn w5vis_the_expiry_note_says_which_leg_ate_the_budget() {
    let total = std::time::Duration::from_secs(120);
    let n = expiry_note(
        total,
        Some(std::time::Duration::from_millis(44_200)),
        std::time::Duration::from_millis(120_000),
    );
    for must in ["120", "44.2", "75.8"] {
        assert!(n.contains(must), "按段归因那句话里缺 `{must}`：{n}");
    }
    let in_shake = expiry_note(total, None, total);
    assert!(in_shake.contains("120"), "{in_shake}");
    assert!(
        !in_shake.contains("44.2") && in_shake != n,
        "还在握手就到点那一形与握完手之后到点那一形说成了同一句：{in_shake}"
    );
    // 两段加起来不超过等了多久（不许凭空多出一段）。
    let n0 = expiry_note(total, Some(total), total);
    assert!(
        n0.contains("0.0"),
        "握手吃光了整个预算时「之后远端跑了」应是 0.0：{n0}"
    );
}

/// B2 ★ 真链路层：`mark_shaken` 之后到点 ⇒ 报出来的错**就是**按段归因那一句（握手那一段 ≈ 0）；
/// 没 `mark_shaken` ⇒ 说「还在握手」。真时钟（bridge 的 tokio 没开 `test-util`，同 A1），期限 1 s。
#[tokio::test]
async fn w5vis_a_timed_out_link_names_the_leg_it_timed_out_in() {
    use tokio::io::AsyncReadExt;
    let total = std::time::Duration::from_secs(1);
    let mut buf = [0u8; 4];
    let (_far, near) = tokio::io::duplex(64);
    let mut s = Bounded::new(near, Some((tokio::time::Instant::now() + total, total)));
    s.mark_shaken();
    let e = tokio::time::timeout(std::time::Duration::from_secs(30), s.read(&mut buf))
        .await
        .expect("期限没生效")
        .expect_err("对面没写却读到了");
    let shaken = expiry_note(total, Some(std::time::Duration::ZERO), total);
    let head = &shaken[..shaken.find("0.0").expect("握手那一段该是 0.0")];
    assert!(
        e.to_string().starts_with(head) && e.to_string().contains("0.0"),
        "握完手之后到点，报的不是按段归因那一句：{e}"
    );
    let (_far2, near2) = tokio::io::duplex(64);
    let mut s2 = Bounded::new(near2, Some((tokio::time::Instant::now() + total, total)));
    let e2 = tokio::time::timeout(std::time::Duration::from_secs(30), s2.read(&mut buf))
        .await
        .expect("期限没生效")
        .expect_err("对面没写却读到了");
    assert_eq!(e2.to_string(), expiry_note(total, None, total));
}

/// B3 接线：`mark_shaken()` 生产段恰好一处、在 `open` 里、排在读完 ack（`let ack = match`）之后、交出 `DialStream` 之前；
/// 到点那一句经 `expiry_note(` 出。正控：合成的 `open` 缺那一行必须被认出。
#[test]
fn w5vis_open_marks_the_handshake_done_right_after_the_ack() {
    fn wired(prod: &str) -> Result<(), String> {
        let n = prod.matches(".mark_shaken()").count();
        if n != 1 {
            return Err(format!("`.mark_shaken()` 调用 {n} 处（要恰好 1）"));
        }
        let at = prod.find("\nasync fn open(").ok_or("找不到 `open`")?;
        let (open_fn, _) = prod[at..].split_once("\n}\n").ok_or("切不出 `open`")?;
        let ack = open_fn
            .find("let ack = match")
            .ok_or("`open` 里没有读 ack 那一段")?;
        let mark = open_fn
            .find(".mark_shaken()")
            .ok_or("`.mark_shaken()` 不在 `open` 里")?;
        let out = open_fn
            .find("Ok((DialStream { r }, ack))")
            .ok_or("`open` 里没有交出 `DialStream`")?;
        if !(ack < mark && mark < out) {
            return Err("`.mark_shaken()` 不在「读完 ack」与「交出链路」之间".into());
        }
        Ok(())
    }
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/dial_host.rs"));
    wired(&prod).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        prod.matches("expiry_note(").count(),
        2,
        "`expiry_note(` 应当恰好两处（定义 ＋ `expired` 里那一次调用）"
    );
    let synthetic =
        "\nasync fn open() {\n    let ack = match x {};\n    Ok((DialStream { r }, ack))\n}\n";
    assert!(
        wired(synthetic).is_err(),
        "缺 `mark_shaken` 的 `open` 没被认出 —— 量具瞎了"
    );
}

// ═══ 〔VIS2 · `设计/15 §3.4 ①`「自动固化 ＋ 默认转严格 ＋ 保住多地址那一格」〕═══════════════════════════════

fn vis2_book(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// 判定逐格（期望手写）：probe 不问 · 已配严格 · 没报不固化 · 全同固化 · 不同说出来。
#[test]
fn vis2_the_pin_verdict_is_exactly_this_table() {
    let same = vis2_book(&[("a:22", "SHA256:x"), ("b:22", "SHA256:x")]);
    let differ = vis2_book(&[("a:22", "SHA256:x"), ("b:22", "SHA256:y")]);
    let none = vis2_book(&[]);
    let got = [
        pin_verdict(true, None, &same),
        pin_verdict(false, Some("SHA256:x"), &same),
        pin_verdict(false, Some("  "), &same),
        pin_verdict(false, None, &none),
        pin_verdict(false, None, &same),
        pin_verdict(false, None, &differ),
    ];
    assert_eq!(
        got,
        [
            PinVerdict::NotAsked,
            PinVerdict::AlreadyStrict,
            PinVerdict::Pin("SHA256:x".into()),
            PinVerdict::NoneReported,
            PinVerdict::Pin("SHA256:x".into()),
            PinVerdict::Differs(differ.clone()),
        ]
    );
}

/// 写：只改那一台的 `hostKeyFingerprint`、别的逐值不动；已有 / 找不到 / 两台同名 / 读不懂 ⇒ 一个字节不写。
#[test]
fn vis2_pinning_writes_only_that_hosts_fingerprint_through_the_patch_door() {
    let dir = std::env::temp_dir().join(format!("ccm-vis2-pin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    let base = serde_json::json!({
        "theme": {"x": 1},
        "remote": {"enabled": true, "hosts": [
            {"label": "aya", "host": "h1", "user": "u", "hostKeyFingerprint": ""},
            {"label": "", "host": "h2", "user": "u", "hostKeyFingerprint": "SHA256:old"},
            {"label": "dup", "host": "h3", "user": "u"},
            {"label": "dup", "host": "h3", "user": "v"}
        ]}
    });
    let write = |v: &serde_json::Value| {
        std::fs::write(&path, serde_json::to_string_pretty(v).unwrap()).unwrap()
    };
    let read = || {
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path).unwrap()).unwrap()
    };

    write(&base);
    assert_eq!(
        pin_host_key_at(&path, "aya", "h1", "SHA256:new"),
        Ok(PinWrite::Written)
    );
    let mut want = base.clone();
    want["remote"]["hosts"][0]["hostKeyFingerprint"] = "SHA256:new".into();
    assert_eq!(read(), want, "只该改 aya 那一台的指纹");

    for (origin, host, why) in [
        ("h2", "h2", PinWrite::AlreadySet),
        ("nope", "h9", PinWrite::NotFound),
        ("dup", "h3", PinWrite::Ambiguous),
        ("aya", "h-other", PinWrite::NotFound),
    ] {
        write(&base);
        let before = std::fs::read(&path).unwrap();
        assert_eq!(
            pin_host_key_at(&path, origin, host, "SHA256:new"),
            Ok(why),
            "{origin}"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "{origin}：不该写却写了"
        );
    }
    std::fs::write(&path, "{ not json").unwrap();
    assert!(pin_host_key_at(&path, "aya", "h1", "SHA256:new").is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    std::fs::remove_dir_all(&dir).ok();
}

/// 默认转严格：`cfg` 里没有指纹 ⇒ 用盘上同一台（host 也相同）的；`cfg` 里有 ⇒ 用它；盘上那台 host 不同 ⇒ 不借。
#[test]
fn vis2_a_pinned_key_on_disk_makes_the_next_dial_strict() {
    let mut tofu = cfg("vis2-strict");
    tofu.host_key_fingerprint = None;
    let mut disk = tofu.clone();
    disk.host_key_fingerprint = Some("SHA256:disk".into());
    let mut elsewhere = disk.clone();
    elsewhere.host = "other.example".into();
    let got = [
        effective_fingerprint_in(&tofu, || Some(disk.clone())),
        effective_fingerprint_in(&cfg("vis2-strict"), || Some(disk.clone())),
        effective_fingerprint_in(&tofu, || Some(elsewhere.clone())),
        effective_fingerprint_in(&tofu, || None),
    ];
    assert_eq!(
        got,
        [
            Some("SHA256:disk".to_string()),
            Some("SHA256:abc".to_string()),
            None,
            None
        ]
    );
}

/// 接线（剥注释）：`open` 在 `mark_shaken` 之后恰好一处 `settle_host_key(`；请求里的指纹来自 `effective_fingerprint(cfg)`。带正控。
#[test]
fn vis2_open_settles_the_host_key_after_the_ack_and_the_request_uses_the_effective_key() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/dial_host.rs"));
    let at = prod.find("async fn open(").expect("open 不在了");
    let end = at + prod[at..].find("\n}\n").expect("open 的尾巴");
    let body = &prod[at..end];
    let n = |t: &str, k: &str| t.matches(k).count();
    assert_eq!(
        n(body, "settle_host_key(cfg, req, &ack);"),
        1,
        "open 里没有（恰好一处）判固化"
    );
    let shaken = body.find("mark_shaken()").expect("mark_shaken 不在了");
    assert!(
        body.find("settle_host_key(").unwrap() > shaken,
        "判固化要在读完 ack 之后"
    );
    assert_eq!(
        n(
            &prod,
            "\"host_key_fingerprint\": effective_fingerprint(cfg),"
        ),
        1,
        "请求没用有效指纹"
    );
    assert_eq!(
        n(
            &format!("{body}\nsettle_host_key(cfg, req, &ack);"),
            "settle_host_key(cfg, req, &ack);"
        ),
        2,
        "量具正控"
    );
}

/// ★ 〔FIX · `设计/99 §2 ㊶` 第二问「只当跳板用的机器一直 TOFU（设计没写）」〕经跳板那一趟要判两台：目标按它自己那一格、
/// 跳板按请求里 `jump` 那一台（origin = 它的 label、否则 host）与 ack 的 `jump_fingerprints`；直连只判目标一台。
/// 固化之后跳板那一台交进下一趟请求的就是盘上那份（`request` 现查跳板配置 ⇒ 默认转严格对跳板同样成立）。
#[test]
fn a_jump_host_is_pinned_under_its_own_entry_and_a_direct_dial_judges_only_the_target() {
    let fp = |pairs: &[(&str, &str)]| -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    let ack = Ack {
        ok: true,
        error: None,
        fingerprint: None,
        fingerprints: fp(&[("h.example:2222", "SHA256:T")]),
        jump_fingerprints: fp(&[("j.lan:22", "SHA256:J")]),
        endpoint: None,
        v: 2,
        uses: vec![],
    };
    let target = cfg("tgt");
    let direct = serde_json::json!({"host": "h.example", "host_key_fingerprint": null});
    let got = pin_targets(&target, &direct, &ack);
    assert_eq!(got.len(), 1, "直连只该判目标一台");
    let via = serde_json::json!({"host": "h.example", "host_key_fingerprint": null,
        "jump": {"host": "j.lan", "port": 22, "label": "bastion", "host_key_fingerprint": null}});
    let got: Vec<_> = pin_targets(&target, &via, &ack)
        .into_iter()
        .map(|(o, h, c, r)| (o, h, c, pin_verdict(false, None, r)))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "tgt".into(),
                "h.example".into(),
                None,
                PinVerdict::Pin("SHA256:T".into())
            ),
            (
                "bastion".into(),
                "j.lan".into(),
                None,
                PinVerdict::Pin("SHA256:J".into())
            ),
        ]
    );
    // 跳板没有 label ⇒ origin 是它的 host（同 `origin_label`）；配过指纹 ⇒ 那一格已严格。
    let via2 = serde_json::json!({"jump": {"host": "j.lan", "label": "", "host_key_fingerprint": "SHA256:J"}});
    let (o, h, c, r) = pin_targets(&target, &via2, &ack).pop().expect("跳板那一台");
    assert_eq!((o.as_str(), h.as_str()), ("j.lan", "j.lan"));
    assert_eq!(
        pin_verdict(false, c.as_deref(), r),
        PinVerdict::AlreadyStrict
    );
}
