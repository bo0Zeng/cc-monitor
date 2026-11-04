//!
//! 用户要求：「今天每台机器只有一条连接可以看情况多开. 智能一点. 这是属于 ssh 优化的部分. 智能多开链接\压缩等等」。
//! 跨互联网那一跳按需开 SSH 压缩（本机回环 / 局域网不开），判准只有一处；一行 `preferred` 覆盖全部 SSH 跳。
//! russh 链 crates.io 上游那一份（0.62.7 起修好 zlib 解压，0.65 的压缩那一半也修了）。
//!
//! 闸开着（Z5 两向钉「闸 == russh 一来一回对不对」）；VR 钉 russh 从哪来（crates.io · 版本 ≥ 0.62.7 · 仓内不留副本）。
//!
//! `ssh-dial/connect.rs` 的压缩判准（[`compression_for`]）与它的接法。四件：
//! Z1 真值表 · Z2 两张偏好序 · Z3 判准只有一处、只有一个调用点 · Z4 回环上内核真量得到往返时间、判准答「不压」。
//! 真 sshd 上「强制压 ⇒ 协商出 zlib、线上字节变少」那一维是 [`ZR`] 那条 `#[ignore]` 读数
//! （由 `tests/evidence/net-loopback.py --compress` 带环境变量来跑）。

use super::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// ★ Z1：判准的真值表。**期望逐格手写**（异源于实现）：
/// 回环 ⇒ 不压；读得到 ⇒ 到门槛才压；读不到 ⇒ 压。
#[test]
fn the_compression_judge_answers_exactly_this_table() {
    let v4 = |a, b, c, d| IpAddr::V4(Ipv4Addr::new(a, b, c, d));
    let floor = COMPRESS_RTT_FLOOR_US;
    let rows: &[(IpAddr, Option<u32>, bool, &str)] = &[
        (v4(127, 0, 0, 1), None, false, "回环、读不到"),
        (
            v4(127, 0, 0, 1),
            Some(u32::MAX),
            false,
            "回环、往返极大（回环一律不压）",
        ),
        (IpAddr::V6(Ipv6Addr::LOCALHOST), Some(floor), false, "::1"),
        (
            IpAddr::V6(Ipv4Addr::new(127, 0, 0, 1).to_ipv6_mapped()),
            None,
            false,
            "IPv4 映射的回环",
        ),
        (
            v4(10, 144, 144, 72),
            Some(12_810),
            true,
            "私网段、跨互联网的覆盖网（现打 12.8 ms）",
        ),
        (v4(10, 144, 144, 72), Some(floor), true, "恰在门槛上 ⇒ 压"),
        (
            v4(10, 144, 144, 72),
            Some(floor - 1),
            false,
            "门槛下一微秒 ⇒ 不压",
        ),
        (v4(192, 168, 1, 226), Some(300), false, "局域网 0.3 ms"),
        (v4(47, 245, 114, 38), Some(186_190), true, "公网 186 ms"),
        (v4(47, 245, 114, 38), None, true, "公网、读不到 ⇒ 压"),
        (
            v4(192, 168, 1, 226),
            None,
            true,
            "局域网、读不到 ⇒ 也压（宁可多花 CPU）",
        ),
    ];
    for (peer, rtt, want, why) in rows {
        assert_eq!(
            compression_for(*peer, *rtt),
            *want,
            "{why}：peer={peer} rtt={rtt:?}"
        );
    }
}

/// ★ Z2：两张偏好序。压 ⇒ `zlib@openssh.com` 在首（OpenSSH 服务端 `Compression yes` 今天只给它）、`none` 垫底
/// （远端关了压缩照样连得上）；不压 ⇒ 只有 `none`。`config_with()` 真把它们装进了 `preferred`。
#[test]
fn the_two_preference_lists_are_exactly_these_and_reach_the_config() {
    let names = |xs: &[russh::compression::Name]| -> Vec<String> {
        xs.iter().map(|n| n.as_ref().to_string()).collect()
    };
    assert_eq!(
        names(COMPRESS_ON),
        ["zlib@openssh.com", "zlib", "none"],
        "压的那张偏好序变了"
    );
    assert_eq!(names(COMPRESS_OFF), ["none"], "不压的那张偏好序变了");
    for (compress, want) in [(true, COMPRESS_ON), (false, COMPRESS_OFF)] {
        for probe in [true, false] {
            let c = config_with(probe, compress, KEEPALIVE);
            assert_eq!(
                names(&c.preferred.compression),
                names(want),
                "config_with(probe={probe}, compress={compress}) 没把偏好序装进去"
            );
            // 其余偏好一个不动（只覆盖压缩那一栏）。
            assert_eq!(
                format!("{:?}", c.preferred.cipher),
                format!("{:?}", russh::Preferred::DEFAULT.cipher)
            );
        }
    }
}

/// 生产段里「命中 `needle` 的文件 → 处数」。
fn production_hits(needle: &str) -> Vec<(String, usize)> {
    let root = guard_core::addr::backend_src_root();
    let mut out = Vec::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let code = guard_core::production_code(&src);
        let n = code.matches(needle).count();
        if n > 0 {
            out.push((guard_core::module_address(&root, &path), n));
        }
    }
    out.sort();
    out
}

/// ★ Z3：**判准只有一处**。① 碰 SSH 压缩偏好的生产文件 == `{ssh-dial/connect.rs}`（两向）；
/// ② 判准函数在生产段恰好被调一次（`tcp_hop` —— 直连与跳板都经它）；③ 往返时间只从 `tcp_rtt` 那一个口读。
/// 针运行时拼（本文件自己不被扫到的那一半由 `production_code` 剥测试段保证；拼接是第二道）。
#[test]
fn the_compression_judge_lives_in_exactly_one_place() {
    let pref = format!("{}::{}", "russh", "compression");
    // 6 = 两张表的类型名各 1 ＋ COMPRESS_ON 三项 ＋ COMPRESS_OFF 一项。
    assert_eq!(
        production_hits(&pref),
        vec![("ssh-dial/connect.rs".to_string(), 6)],
        "碰 SSH 压缩偏好的地方不止 connect.rs 那两张表"
    );
    let call = format!("{}(", "compression_for");
    // connect.rs 里：定义 1 ＋ 调用 1。
    assert_eq!(
        production_hits(&call),
        vec![("ssh-dial/connect.rs".to_string(), 2)],
        "判准函数的调用点不是恰好一处（tcp_hop）"
    );
    let rtt = format!("{}::{}(", "tcp_rtt", "rtt_us");
    assert_eq!(
        production_hits(&rtt),
        vec![("ssh-dial/connect.rs".to_string(), 1)],
        "往返时间不止从 tcp_hop 那一处读"
    );
    // 正控：同一个计数器喂一段合成代码，三根针各中一次（针没写坏）。
    let synthetic = format!("{pref} {call} {rtt}");
    assert_eq!(
        (
            synthetic.matches(&pref).count(),
            synthetic.matches(&call).count(),
            synthetic.matches(&rtt).count()
        ),
        (1, 1, 1)
    );
}

/// ★ Z4：回环上**内核真量得到**这一跳的往返时间（读不到就是平台那一半坏了），而且它在门槛之下、判准答「不压」。
/// 异源 = 内核。Linux 与 Windows（`SIO_TCP_INFO`）两臂读得到；其余平台那一支回 `None`，本条不在那里编。
/// ⚠ Windows 那一格本机只交叉编译、没在真 Windows 上跑过。
#[cfg(any(target_os = "linux", windows))]
#[tokio::test]
async fn the_kernel_measures_the_loopback_hop_and_the_judge_says_no() {
    let ls = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = ls.local_addr().unwrap();
    let accept = tokio::spawn(async move { ls.accept().await.map(|(s, _)| s) });
    let ep = Endpoint {
        host: "127.0.0.1".to_string(),
        port: addr.port(),
    };
    let (tcp, compress, _) = tcp_hop(&ep, egress::here()).await.expect("回环拨不通");
    let rtt = crate::tcp_rtt::rtt_us(&tcp);
    let us = rtt.expect("回环 socket 上 TCP_INFO 读不到往返时间 —— 平台那一半坏了");
    assert!(
        us < COMPRESS_RTT_FLOOR_US,
        "回环往返 {us} µs 竟然不在门槛之下"
    );
    assert!(!compress, "回环上判准答了「压」");
    drop(accept);
}

/// russh 自己的 zlib 一来一回：用它的 `Compress`（`Z_PARTIAL_FLUSH`，与 OpenSSH 每包的冲刷同形）压一包、再用它的 `Decompress` 解，
/// 字节逐字相等才算对。`payload` 取可压的（压缩比远大于 2）。
fn russh_zlib_round_trips(payload: &[u8]) -> bool {
    use russh::compression::{Compress, Compression, Decompress};
    let mut c = Compress::None;
    Compression::Zlib.init_compress(&mut c);
    let mut d = Decompress::None;
    Compression::Zlib.init_decompress(&mut d);
    let mut buf = Vec::new();
    let Ok(packet) = c.compress(payload, &mut buf) else {
        return false;
    };
    let packet = packet.to_vec();
    let mut out = Vec::new();
    d.decompress(&packet, &mut out)
        .is_ok_and(|got| got == payload)
}

/// 同一对压 / 解器**连着**走几包（SSH 一条连接上的压缩状态跨包延续，与线上同形）：每一包解出来都逐字节同才算对。
/// 上游缺陷的真形是「一包只交出前 ≈ 2 × 包长、余下的拼进**下一包**」—— 单包只看得见前一半，多包看得见错位。
fn russh_zlib_stream_round_trips(packets: &[Vec<u8>]) -> bool {
    use russh::compression::{Compress, Compression, Decompress};
    let mut c = Compress::None;
    Compression::ZlibOpenSSH.init_compress(&mut c);
    let mut d = Decompress::None;
    Compression::ZlibOpenSSH.init_decompress(&mut d);
    packets.iter().all(|payload| {
        let mut buf = Vec::new();
        let Ok(packet) = c.compress(payload, &mut buf) else {
            return false;
        };
        let packet = packet.to_vec();
        let mut out = Vec::new();
        d.decompress(&packet, &mut out)
            .is_ok_and(|got| got == payload.as_slice())
    })
}

/// **压**那一半：用 russh 生产路上那个 `compress_into`（`PacketWriter` 每包调它、输出接在 5 字节包头之后）
/// 连着压几包，再用**另一只**解压器（flate2 原生 `Decompress`，每包 `Sync` 冲刷，与对端 sshd 的 inflate 同形）逐包解 ——
/// 每包逐字节同、包头原样才算对。上游缺陷的真形：一包的压缩输出超出「输入 + 10」那一刻就收工，没吐完的拼进下一包。
fn russh_zlib_compresses_whole_packets(packets: &[Vec<u8>]) -> bool {
    use russh::compression::{Compress, Compression};
    const HEAD: usize = 5;
    let mut c = Compress::None;
    Compression::ZlibOpenSSH.init_compress(&mut c);
    let mut d = flate2::Decompress::new(true);
    packets.iter().all(|payload| {
        let mut buf = vec![0xA5u8; HEAD];
        let Ok(n) = c.compress_into(payload, &mut buf, HEAD) else {
            return false;
        };
        if buf.len() != HEAD + n || buf[..HEAD] != [0xA5u8; HEAD] {
            return false;
        }
        let input = &buf[HEAD..];
        let start = d.total_in();
        let mut out: Vec<u8> = Vec::new();
        loop {
            out.reserve(payload.len() + 64);
            let used = (d.total_in() - start) as usize;
            if d.decompress_vec(&input[used..], &mut out, flate2::FlushDecompress::Sync)
                .is_err()
            {
                return false;
            }
            let used = (d.total_in() - start) as usize;
            if (used == input.len() && out.len() < out.capacity()) || out.len() > payload.len() {
                break;
            }
        }
        out == *payload
    })
}

/// ★ Z5：**闸 == russh 的解压今天对不对**（两向相等）。异源 = russh 自己的编解码。
///
/// NT1 现打（上游 0.61.1）：一包 1000 字节的可压载荷（压成 39 字节）解回来只有 78 字节 —— 解压器最多交出 ≈ 2 × 包长。
/// 今天链的是 crates.io 的 0.65（0.62.7 起修好）⇒ 对 ⇒ 闸开。多包那一格：三包连着走（1000 · 30000 · 200 字节，
/// 可压比都 ≫ 2），每包逐字节同 —— 修法若只修了「这一包交全」而状态跨包错了，这一格看得见。
/// 正控：压缩比 < 2 的一包（短、近乎不可压）照样一来一回全对 —— 量具本身没用错，坏的只是「解出来比输入多一倍以上」那一形。
#[test]
fn the_gate_matches_what_russh_really_does() {
    let compressible: Vec<u8> = (0..1000).map(|i| b"abcabcabd"[i % 9]).collect();
    let line: &[u8] = b"{\"type\":\"assistant\"}\n";
    let stream: Vec<Vec<u8>> = [1000usize, 30_000, 200]
        .iter()
        .enumerate()
        .map(|(k, &n)| (0..n).map(|i| line[(i + k) % line.len()]).collect())
        .collect();
    // 压那一半：满长的一包通道数据（32 768 字节 ＋ 9 字节消息头）不可压 · 一包可压的 · 再一包更长的不可压 · 收尾一包可压的。
    let pressed: Vec<Vec<u8>> = vec![
        incompressible(32_777, 7),
        stream[0].clone(),
        incompressible(70_000, 11),
        stream[2].clone(),
    ];
    let (single, multi, whole) = (
        russh_zlib_round_trips(&compressible),
        russh_zlib_stream_round_trips(&stream),
        russh_zlib_compresses_whole_packets(&pressed),
    );
    let sound = single && multi && whole;
    assert_eq!(
        RUSSH_ZLIB_SOUND, sound,
        "闸（RUSSH_ZLIB_SOUND = {RUSSH_ZLIB_SOUND}）与 russh 的 zlib 实况（一来一回对不对 = {sound}：单包 {single} · 多包 {multi} · 不可压整包 {whole}）不一致 —— \
         russh 修好了就开闸（压缩判准的答案才落到连接上）；还坏着就别开（开了每条远端连接都会在第一条通道上卡死 / 大块写断链）"
    );
    assert!(
        russh_zlib_round_trips(b"q7#Kx"),
        "正控：一包近乎不可压的短载荷都一来一回不对 —— 量具用错了，不是 russh 的那一形"
    );
    assert!(
        russh_zlib_compresses_whole_packets(&stream),
        "正控：可压的几包经另一只解压器都解不回 —— 压那一半的量具用错了，不是「不可压大包」那一形"
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// lock 里 russh 那一块的版本号拆成三段数字（`0.65.0` ⇒ `(0, 65, 0)`）；带预发布尾巴（`-beta.1`）的不认 ⇒ `None`。
fn semver_triple(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.split('.').map(|p| p.parse::<u64>().ok());
    match (it.next(), it.next(), it.next(), it.next()) {
        (Some(Some(a)), Some(Some(b)), Some(Some(c)), None) => Some((a, b, c)),
        _ => None,
    }
}

/// ★ VR：**链的是 crates.io 上游那一份、而且是修好 zlib 的版本**（仓内不留补过的副本）。
///
/// 上游 russh issue #749「zlib decompression truncates packets」修在 `0.62.7`（解压收尾判断改用调用之后的进度）；
/// 压那一半（输出缓冲撑满回 `Ok` 就收工）`0.65` 的 `compression.rs` 同样修了（它自带一条不可压整包一来一回的单测）。
/// 两半对不对由上面 Z5 的闸判据现打；本条只钉「从哪来」：
/// ① 后端清单（workspace 根）里没有 `[patch.…]` 段 —— 谁再把 russh 换回一份仓内副本就红；
/// ② lock 里 `russh` 恰好一块、`source` 是 crates.io、版本 ≥ `0.62.7`（几个 crate 的声明对不齐 ⇒ 多块 ⇒ 红）。
/// 副本目录删干净这一格不在这里判：仓内不存在的路径写成字面量会被扫描层判成断路（`test_tiers`），没有 `[patch]` 就没人链得到它。
#[test]
fn russh_comes_from_crates_io_at_a_version_with_the_zlib_fix() {
    let manifest = include_str!("../../src/backend/Cargo.toml");
    let lock = include_str!("../../src/backend/Cargo.lock");
    // ① 剥整行注释，按段头认。
    let uncommented = guard_core::strip_hash_comment_lines(manifest);
    let patch_sections: Vec<&str> = uncommented
        .lines()
        .map(str::trim)
        .filter(|t| t.starts_with("[patch"))
        .collect();
    assert_eq!(
        patch_sections,
        Vec::<&str>::new(),
        "后端清单里还有 `[patch.…]` 段 —— russh 要链 crates.io 上游那一份"
    );
    // ② lock 里那一块。
    let blocks: Vec<&str> = lock
        .split("[[package]]\n")
        .filter(|b| b.lines().next() == Some("name = \"russh\""))
        .collect();
    let [block] = blocks.as_slice() else {
        panic!(
            "lock 里 `russh` 不是恰好一块：{} 块（几个 crate 的版本声明对不齐）",
            blocks.len()
        );
    };
    let field = |key: &str| {
        let head = format!("{key} = \"");
        block
            .lines()
            .find_map(|l| l.strip_prefix(head.as_str()).map(str::to_string))
            .and_then(|r| r.strip_suffix('"').map(str::to_string))
    };
    assert_eq!(
        field("source").as_deref(),
        Some("registry+https://github.com/rust-lang/crates.io-index"),
        "lock 里 russh 那一块不是 crates.io 来的：\n{block}"
    );
    let version = field("version").expect("lock 那一块抠不出 version");
    let triple = semver_triple(&version)
        .unwrap_or_else(|| panic!("lock 里 russh 的版本 `{version}` 不是三段数字"));
    assert!(
        triple >= (0, 62, 7),
        "lock 里 russh 是 {version}：zlib 解压截包（上游 issue #749）修在 0.62.7，低于它就还坏着"
    );
}

/// ★ ZR（读数，`#[ignore]`）：**真 sshd 上**同一段可压的字节（`seq 1 400000`），强制不压 vs 强制压。
///
/// 由 `tests/evidence/net-loopback.py --compress` 起回环 sshd、带 `NT1_COMPRESS={host,port,user,key_path}` 来跑；
/// 协商结果（`compression: none` / `zlib@openssh.com`）由那份脚本读 sshd 日志核（异源）。本条自己判：
/// - 不压那趟：载荷收全（> 2 MB）；
/// - 压的那趟：**闸关着（russh 解压坏着）⇒ 30 秒内收不全**（现打的样子是卡在第一条通道上）；
///   闸开着 ⇒ 载荷逐字节同、线上字节 < 不压那趟的一半。
/// - 上行那一半（客户端压、sshd 解 —— 与下行是两套代码）：同一条连接上把一段会话 jsonl 样子的载荷喂给远端 `sha256sum`，
///   两趟都要摘要 == 本侧算的；闸开着 ⇒ 压的那趟上行线上字节 < 不压那趟的一半。
#[ignore = "要真 sshd：由 tests/evidence/net-loopback.py --compress 带环境变量来跑"]
#[tokio::test(flavor = "multi_thread")]
async fn zr_real_sshd_negotiates_zlib_and_moves_fewer_bytes_when_forced() {
    let Ok(raw) = std::env::var("NT1_COMPRESS") else {
        panic!("没有 NT1_COMPRESS —— 这条读数由 net-loopback.py --compress 来跑");
    };
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let host = v["host"].as_str().unwrap().to_string();
    let port = v["port"].as_u64().unwrap() as u16;
    let user = v["user"].as_str().unwrap().to_string();
    let key = v["key_path"].as_str().unwrap().to_string();
    // 上行载荷：会话 jsonl 的样子（可压比 ≫ 2），约 2.7 MB。
    let line: &[u8] =
        b"{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"";
    let upload: Vec<u8> = (0..40_000u32)
        .flat_map(|i| {
            let mut row = line.to_vec();
            row.extend_from_slice(format!("{i}\"}}]}}}}\n").as_bytes());
            row
        })
        .collect();
    let upload_hash = sha256_hex(&upload);
    let leg = |compress: bool| {
        let (host, user, key) = (host.clone(), user.clone(), key.clone());
        let upload = upload.clone();
        async move {
            let tcp = tokio::net::TcpStream::connect((host.as_str(), port))
                .await
                .unwrap();
            let wire = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
            let sent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
            let counted = Counting {
                inner: tcp,
                read: std::sync::Arc::clone(&wire),
                written: std::sync::Arc::clone(&sent),
            };
            let checker = Checker {
                trust: Default::default(),
                observed: Default::default(),
                stages: StageSink::new(false),
                endpoint: Endpoint {
                    host: host.to_string(),
                    port,
                },
                label: format!("{host}:{port}"),
                heard: Default::default(),
            };
            let mut h = russh::client::connect_stream(
                config_with(false, compress, KEEPALIVE),
                counted,
                checker,
            )
            .await
            .unwrap();
            authenticate(&mut h, &user, Some(&key), None).await.unwrap();
            let body = tokio::time::timeout(guard_core::wall::CEILING, async {
                let mut ch = h.channel_open_session().await.ok()?;
                ch.exec(true, "seq 1 400000").await.ok()?;
                let mut body = Vec::new();
                while let Some(m) = ch.wait().await {
                    match m {
                        russh::ChannelMsg::Data { data } => body.extend_from_slice(&data),
                        russh::ChannelMsg::Close => break,
                        _ => {}
                    }
                }
                Some(body)
            })
            .await
            .ok()
            .flatten();
            // 上行那一半：同一条连接上把 `upload` 喂给远端 `sha256sum`，回来的摘要 == 本侧算的才算收全
            // （客户端压、sshd 解 —— 与下行走的是两套代码）。上行字节另记（`sent`）。
            let sent_before = sent.load(std::sync::atomic::Ordering::SeqCst);
            let up = tokio::time::timeout(guard_core::wall::CEILING, async {
                let mut ch = h.channel_open_session().await.ok()?;
                ch.exec(true, "sha256sum").await.ok()?;
                ch.data(upload.as_slice()).await.ok()?;
                ch.eof().await.ok()?;
                let mut out = Vec::new();
                while let Some(m) = ch.wait().await {
                    match m {
                        russh::ChannelMsg::Data { data } => out.extend_from_slice(&data),
                        russh::ChannelMsg::Close => break,
                        _ => {}
                    }
                }
                String::from_utf8(out)
                    .ok()?
                    .split_whitespace()
                    .next()
                    .map(str::to_string)
            })
            .await
            .ok()
            .flatten();
            let up_wire = sent.load(std::sync::atomic::Ordering::SeqCst) - sent_before;
            let _ = h.disconnect(russh::Disconnect::ByApplication, "", "").await;
            (
                wire.load(std::sync::atomic::Ordering::SeqCst),
                body,
                up_wire,
                up,
            )
        }
    };
    let (wire_off, off, up_off, up_hash_off) = leg(false).await;
    let (wire_on, on, up_on, up_hash_on) = leg(true).await;
    let off = off.expect("不压那趟都没收全");
    println!(
        "NT1-COMPRESS payload={} wire_off={wire_off} wire_on={wire_on} on_complete={} \
         upload={} up_off={up_off} up_on={up_on} up_off_ok={} up_on_ok={}",
        off.len(),
        on.as_ref().is_some_and(|b| *b == off),
        upload.len(),
        up_hash_off.as_deref() == Some(upload_hash.as_str()),
        up_hash_on.as_deref() == Some(upload_hash.as_str()),
    );
    assert!(off.len() > 2_000_000, "载荷太短：{}", off.len());
    assert_eq!(
        up_hash_off.as_deref(),
        Some(upload_hash.as_str()),
        "不压那趟上行都没收全"
    );
    if RUSSH_ZLIB_SOUND {
        assert_eq!(on.as_deref(), Some(off.as_slice()), "压的那趟载荷不同");
        assert!(
            wire_on * 2 < wire_off,
            "压的那趟线上字节 {wire_on} 不到不压那趟 {wire_off} 的一半"
        );
        assert_eq!(
            up_hash_on.as_deref(),
            Some(upload_hash.as_str()),
            "压的那趟上行：远端算出的摘要与本侧不同（客户端压 / sshd 解那一半坏了）"
        );
        assert!(
            up_on * 2 < up_off,
            "压的那趟上行线上字节 {up_on} 不到不压那趟 {up_off} 的一半"
        );
    } else {
        assert!(
            on.as_ref() != Some(&off),
            "闸关着（russh 解压坏着）而真 sshd 上压的那趟收全了 —— russh 那一形不在了？先看 the_gate_matches_what_russh_really_does"
        );
    }
}

/// 不可压的字节（xorshift64*，定种子 ⇒ 每趟同一份）：zlib 压不动、输出比输入略长 —— WIN3 读数 A 那一形。
fn incompressible(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..n)
        .map(|_| {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 56) as u8
        })
        .collect()
}

/// ★ ZR2（读数，`#[ignore]`）：**真 sshd 上**强制压 / 强制不压各一趟，经部署那条路（`sftp::put_atomic`）
/// 往暂存区放 33 000 字节与 1 MiB **不可压**的字节，读回逐字节同。WIN3 现打（p5p）：压的那趟 > 32 000 字节就断
/// （sshd「channel_input_data: … incomplete message」）。由 `tests/evidence/zlib-container.py` 起一次性容器 sshd、
/// 带 `NT1_COMPRESS={host,port,user,key_path}` 来跑；协商结果由那份脚本读 sshd 日志核（异源）。
#[ignore = "要真 sshd：由 tests/evidence/zlib-container.py 带环境变量来跑"]
#[tokio::test(flavor = "multi_thread")]
async fn zr_real_sshd_takes_incompressible_puts_when_forced() {
    let Ok(raw) = std::env::var("NT1_COMPRESS") else {
        panic!("没有 NT1_COMPRESS —— 这条读数由 zlib-container.py 来跑");
    };
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let host = v["host"].as_str().unwrap().to_string();
    let port = v["port"].as_u64().unwrap() as u16;
    let user = v["user"].as_str().unwrap().to_string();
    let key = v["key_path"].as_str().unwrap().to_string();
    let mut bad = Vec::new();
    for compress in [false, true] {
        let tcp = tokio::net::TcpStream::connect((host.as_str(), port))
            .await
            .unwrap();
        let checker = Checker {
            trust: Default::default(),
            observed: Default::default(),
            stages: StageSink::new(false),
            endpoint: Endpoint {
                host: host.to_string(),
                port,
            },
            label: format!("{host}:{port}"),
            heard: Default::default(),
        };
        let mut h =
            russh::client::connect_stream(config_with(false, compress, KEEPALIVE), tcp, checker)
                .await
                .unwrap();
        authenticate(&mut h, &user, Some(&key), None).await.unwrap();
        let ch = h.channel_open_session().await.unwrap();
        ch.request_subsystem(true, "sftp").await.unwrap();
        let s = crate::sftp::Session::over(ch.into_stream(), Box::new(()))
            .await
            .unwrap();
        let dir = format!("{}/{}", s.home(), crate::sftp::STAGING_ROOT);
        crate::sftp::make_dirs(&s, &dir).await.unwrap();
        for (n, seed) in [(33_000usize, 7u64), (1 << 20, 11)] {
            let blob = incompressible(n, seed);
            let path = format!("{dir}/zr2-{compress}-{n}");
            let put = tokio::time::timeout(
                guard_core::wall::CEILING,
                crate::sftp::put_atomic(&s, &path, &blob, 0o600),
            )
            .await;
            let back = match &put {
                Ok(Ok(())) => crate::sftp::read_all(&s, &path).await,
                _ => None,
            };
            let ok = back.as_deref() == Some(blob.as_slice());
            println!(
                "WF2-ZR2 compress={compress} n={n} put={} readback_ok={ok}",
                match &put {
                    Ok(Ok(())) => "ok".to_string(),
                    Ok(Err(e)) => format!("err({e})"),
                    Err(_) => "timeout".to_string(),
                }
            );
            if !ok {
                bad.push(format!("compress={compress} n={n}"));
            }
        }
        drop(s);
        let _ = h.disconnect(russh::Disconnect::ByApplication, "", "").await;
    }
    assert!(bad.is_empty(), "这几趟没放上 / 读回不同：{bad:?}");
}

/// 数读到 / 写出的字节（线上字节 = TCP 上读写的，压缩在它之上）。
struct Counting {
    inner: tokio::net::TcpStream,
    read: std::sync::Arc<std::sync::atomic::AtomicU64>,
    written: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl tokio::io::AsyncRead for Counting {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        let n = (buf.filled().len() - before) as u64;
        self.read.fetch_add(n, std::sync::atomic::Ordering::SeqCst);
        r
    }
}

impl tokio::io::AsyncWrite for Counting {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let r = std::pin::Pin::new(&mut self.inner).poll_write(cx, buf);
        if let std::task::Poll::Ready(Ok(n)) = &r {
            self.written
                .fetch_add(*n as u64, std::sync::atomic::Ordering::SeqCst);
        }
        r
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// 校验器怎么判一把钥匙（期望手写，指纹取钥匙本身）：库里没记过、也没有待核的 ⇒ 首见收下；记过同类型的 ⇒ 只认那一把；
/// 记过别的类型 ⇒ 拒；只有待核的指纹 ⇒ 按它比。拒的那一下把库里同类型那一把（没有 ⇒ 待核的，再没有 ⇒ 空串）记进这一趟。
#[tokio::test]
async fn the_checker_trusts_only_what_the_library_holds() {
    use russh::client::Handler as _;
    let key = |n: u8| {
        russh::keys::PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(
            &[n; 32],
        ))
        .public_key()
        .clone()
    };
    let ec = russh::keys::PrivateKey::random(
        &mut rand::rng(),
        russh::keys::Algorithm::Ecdsa {
            curve: russh::keys::EcdsaCurve::NistP256,
        },
    )
    .unwrap()
    .public_key()
    .clone();
    let fp = |k: &PublicKey| k.fingerprint(HashAlg::Sha256).to_string();
    async fn judge_offer(trust: Trust, offered: &PublicKeyOrCertificate) -> (bool, Option<String>) {
        let stages = StageSink::new(false);
        let mut c = Checker {
            trust,
            observed: Arc::default(),
            stages: stages.clone(),
            endpoint: Endpoint {
                host: "a".into(),
                port: 22,
            },
            label: "a:22".to_string(),
            heard: Default::default(),
        };
        (
            c.check_server_key(offered).await.unwrap(),
            stages.expected(),
        )
    }
    async fn judge(trust: Trust, offered: &PublicKey) -> (bool, Option<String>) {
        judge_offer(trust, &PublicKeyOrCertificate::from(offered.clone())).await
    }
    let (a, b) = (key(1), key(2));
    let known = |ks: Vec<PublicKey>| Trust { known: ks };
    assert_eq!(judge(Trust::default(), &a).await, (true, None), "首见收下");
    assert_eq!(judge(known(vec![a.clone()]), &a).await, (true, None));
    assert_eq!(
        judge(known(vec![a.clone()]), &b).await,
        (false, Some(fp(&a)))
    );
    assert_eq!(
        judge(known(vec![ec.clone()]), &a).await,
        (false, Some(String::new()))
    );

    // 交来的是 host 证书（本端不播报证书类算法，协商之外）：证书里那把钥匙就是库里记过的那一把，也不认；首见也不收。
    let cert = russh::keys::Certificate::from_openssh(HOST_CERT).expect("夹具证书解得开");
    let inner = PublicKey::from_openssh(HOST_KEY).expect("夹具钥匙解得开");
    assert_eq!(
        cert.public_key(),
        inner.key_data(),
        "夹具：证书里那把就是这把钥匙"
    );
    let offered = PublicKeyOrCertificate::from(cert);
    assert!(
        !judge_offer(known(vec![inner.clone()]), &offered).await.0,
        "证书里的钥匙记过 ⇒ 仍不认"
    );
    assert!(
        !judge_offer(Trust::default(), &offered).await.0,
        "首见的证书 ⇒ 不收"
    );
    assert!(
        judge(known(vec![inner.clone()]), &inner).await.0,
        "正控：同一把裸钥匙照认"
    );
}

/// 一次性生成的夹具（`ssh-keygen -s <一次性 CA> -h`，CA 私钥没留）：host 证书与它签的那把 ed25519 钥匙。
const HOST_CERT: &str = "ssh-ed25519-cert-v01@openssh.com AAAAIHNzaC1lZDI1NTE5LWNlcnQtdjAxQG9wZW5zc2guY29tAAAAIJuOCJgC3QI3sS1MIb69b5knvtw9u6t+1CZg1QydDfhxAAAAIN5PoygyZ6/rhEULYYG2b/9PWPcyzV+rkl88snLcUkaDAAAAAAAAAAAAAAACAAAADGZpeHR1cmUtaG9zdAAAAA0AAAAJYS5leGFtcGxlAAAAAAAAAAD//////////wAAAAAAAAAAAAAAAAAAADMAAAALc3NoLWVkMjU1MTkAAAAgIUjnwniAhMh6DezKphpGheSNVSN+bkyUXa+s6Aw86jgAAABTAAAAC3NzaC1lZDI1NTE5AAAAQGVkyH9mpS84cj+Z8j7O03hQ8yd/EJYGH09psJTnO35l1YpvrN6MbpjTWlOWnqwj6ILPswFTk6KseKBO1vDE4gw= fixture-host";
const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIN5PoygyZ6/rhEULYYG2b/9PWPcyzV+rkl88snLcUkaD fixture-host";

/// 竞速全输了：那一句只带原因词（拨不通 ⇒「端口不通」）与试过的地址，各地址的系统原话进详情、不上句子。
#[tokio::test]
async fn a_lost_race_says_the_reason_word_and_keeps_the_os_words_for_the_detail() {
    let port = crate::refusing_port::refusing_port();
    let ep = Endpoint {
        host: "127.0.0.1".to_string(),
        port,
    };
    let stages = StageSink::new(false);
    let Err((said, _)) = race(
        true,
        true,
        Trust::default(),
        vec![ep.clone()],
        &stages,
        egress::here(),
    )
    .await
    else {
        panic!("关着的口居然拨通了");
    };
    let addr = format!("127.0.0.1:{port}");
    assert_eq!(
        said.said,
        copy_text(
            "beConnect.race.allFailed",
            &[
                ("why", &copy_text("reason.dial.refused", &[])),
                ("addrs", &addr)
            ]
        ),
        "那一句换了事实或带了原话：{said:?}"
    );
    let raw = said.raw.expect("各地址的原话没进详情");
    assert!(
        raw.starts_with(&addr) && raw.len() > addr.len() + 1,
        "详情里没有那条地址的系统原话：{raw:?}"
    );
}

/// 竞速原因词：解析不出 · 拨不通 · 超时 · 主机指纹不符各一个词；别的（握手）说不清 ⇒ 原因不明。
#[test]
fn the_race_reason_word_follows_the_worst_stage() {
    let w = |k: &str| copy_text(k, &[]);
    assert_eq!(race_reason(Some("resolve")), w("reason.dial.resolve"));
    assert_eq!(race_reason(Some("tcp")), w("reason.dial.refused"));
    assert_eq!(race_reason(Some("timeout")), w("reason.link.noAnswer"));
    assert_eq!(race_reason(Some("hostkey")), w("reason.dial.hostKey"));
    assert_eq!(race_reason(Some("other")), w("reason.io.unknown"));
    assert_eq!(race_reason(None), w("reason.io.unknown"));
}

/// 经跳板那一条隧道的样子：外面那条连接没了之后，读到头、写和关都回「断了」。
struct CarrierGone {
    inner: tokio::io::DuplexStream,
    gone: Arc<std::sync::atomic::AtomicBool>,
}

impl CarrierGone {
    fn broken(&self) -> bool {
        self.gone.load(std::sync::atomic::Ordering::SeqCst)
    }
}

fn pipe_closed() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel closed")
}

impl tokio::io::AsyncRead for CarrierGone {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.broken() {
            return std::task::Poll::Ready(Ok(()));
        }
        let r = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        if r.is_pending() {
            // 断的那一刻要叫醒在等的读（真隧道是外面那条连接收场时把收端关掉）。
            let w = cx.waker().clone();
            let gone = Arc::clone(&self.gone);
            tokio::spawn(async move {
                while !gone.load(std::sync::atomic::Ordering::SeqCst) {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                w.wake();
            });
        }
        r
    }
}

impl tokio::io::AsyncWrite for CarrierGone {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        if self.broken() {
            return std::task::Poll::Ready(Err(pipe_closed()));
        }
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.broken() {
            return std::task::Poll::Ready(Err(pipe_closed()));
        }
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[derive(Clone)]
struct LetIn;

impl russh::server::Handler for LetIn {
    type Error = russh::Error;
    async fn auth_none(&mut self, _user: &str) -> Result<russh::server::Auth, Self::Error> {
        Ok(russh::server::Auth::Accept)
    }
}

/// ★ 经跳板的那条连接：外面那条没了（隧道读到头、关也关不上）⇒ 这条照样记成「没了」，握着它的链路（`use: hold`）跟着收工。
/// 关不上不许吞掉「没了」那一下：否则握着的那条一直算活着，SSH 那层永远等不到它断、终端停在出错页回不来。
/// 200 ms 的保活走虚拟钟：墙钟上忙机里回话晚到一轮，正控那半就把好着的隧道判成没了。
#[tokio::test(start_paused = true)]
async fn a_tunnelled_connection_is_marked_gone_when_its_carrier_dies() {
    let (near, far) = tokio::io::duplex(64 * 1024);
    let gone = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let key = russh::keys::PrivateKey::from(
        russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[9u8; 32]),
    );
    let server_cfg = Arc::new(russh::server::Config {
        keys: vec![key],
        inactivity_timeout: None,
        keepalive_interval: None,
        auth_rejection_time: std::time::Duration::ZERO,
        ..Default::default()
    });
    tokio::spawn(async move {
        if let Ok(running) = russh::server::run_stream(server_cfg, far, LetIn).await {
            let _ = running.await;
        }
    });
    let heard = Arc::new(crate::pool::Heard::new());
    let checker = Checker {
        trust: Trust::default(),
        observed: Arc::default(),
        stages: StageSink::new(false),
        endpoint: Endpoint {
            host: "t".into(),
            port: 22,
        },
        label: "t:22".to_string(),
        heard: Arc::clone(&heard),
    };
    let road = Hearing::new(
        CarrierGone {
            inner: near,
            gone: Arc::clone(&gone),
        },
        Arc::clone(&heard),
    );
    let mut h = russh::client::connect_stream(
        config_with(false, false, std::time::Duration::from_millis(200)),
        road,
        checker,
    )
    .await
    .expect("进程内服务端握手没过");
    assert!(h
        .authenticate_none("u")
        .await
        .expect("鉴权没回话")
        .success());
    let alive = tokio::time::timeout(std::time::Duration::from_millis(500), heard.gone()).await;
    assert!(alive.is_err(), "正控：隧道好着的时候不许记成没了");
    gone.store(true, std::sync::atomic::Ordering::SeqCst);
    let got = tokio::time::timeout(guard_core::wall::CEILING, heard.gone()).await;
    assert!(
        got.is_ok(),
        "隧道断了，这条连接到兜底上限还没记成「没了」（握着它的链路不会收工）"
    );
}
