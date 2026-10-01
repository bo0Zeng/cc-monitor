//!
//! 核原文：V23 逐字「今天每台机器只有一条连接可以看情况多开. 智能一点. 这是属于 ssh 优化的部分. 智能多开链接\压缩等等」；
//! 那张表「SSH 传输层 | 没显式开；russh `client::Config` 零处设 `preferred`」；
//! `§5.5`「默认已经压上 ⇒ 别重复投资；没有 ⇒ 一行 `preferred` 覆盖全部 SSH 跳」。
//! NT1 题面：「跨互联网那一跳按需开 SSH 压缩 …… 什么时候开（本机回环 / 局域网不开）写清判准，判准只有一处」。
//! V118 逐字〔选〕「打补丁版 russh，现在就开」：vendor 一份修好 zlib 解压的 russh（`[patch.crates-io]`），SSH 压缩按已写好的判准开。
//!
//! 闸开了（Z5 翻面 ＋ 多包一格）；补丁那一族两条：V1 副本只改了登记的那一份 · V2 补丁真接上了。
//!
//! `dial/connect.rs` 的压缩判准（[`compression_for`]）与它的接法。四件：
//! Z1 真值表 · Z2 两张偏好序 · Z3 判准只有一处、只有一个调用点 · Z4 回环上内核真量得到往返时间、判准答「不压」。
//! 真 sshd 上「强制压 ⇒ 协商出 zlib、线上字节变少」那一维是 [`ZR`] 那条 `#[ignore]` 读数
//! （由 `tests/evidence/NT1-net-loopback.py --compress` 带环境变量来跑）。

use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// ★ Z1：判准的真值表。**期望逐格取自 `NT1.md §1.1` 那三行字**（异源于实现）：
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
/// （远端关了压缩照样连得上）；不压 ⇒ 只有 `none`。`config()` 真把它们装进了 `preferred`。
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
            let c = config(probe, compress);
            assert_eq!(
                names(&c.preferred.compression),
                names(want),
                "config(probe={probe}, compress={compress}) 没把偏好序装进去"
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
    let root = crate::guard_support::src_root();
    let mut out = Vec::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let code = crate::guard_support::production_code(&src);
        let n = code.matches(needle).count();
        if n > 0 {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, n));
        }
    }
    out.sort();
    out
}

/// ★ Z3：**判准只有一处**。① 碰 SSH 压缩偏好的生产文件 == `{dial/connect.rs}`（两向）；
/// ② 判准函数在生产段恰好被调一次（`tcp_hop` —— 直连与跳板都经它）；③ 往返时间只从 `platform::tcp_rtt` 那一个口读。
/// 针运行时拼（本文件自己不被扫到的那一半由 `production_code` 剥测试段保证；拼接是第二道）。
#[test]
fn the_compression_judge_lives_in_exactly_one_place() {
    let pref = format!("{}::{}", "russh", "compression");
    // 6 = 两张表的类型名各 1 ＋ COMPRESS_ON 三项 ＋ COMPRESS_OFF 一项。
    assert_eq!(
        production_hits(&pref),
        vec![("dial/connect.rs".to_string(), 6)],
        "碰 SSH 压缩偏好的地方不止 connect.rs 那两张表"
    );
    let call = format!("{}(", "compression_for");
    // connect.rs 里：定义 1 ＋ 调用 1。
    assert_eq!(
        production_hits(&call),
        vec![("dial/connect.rs".to_string(), 2)],
        "判准函数的调用点不是恰好一处（tcp_hop）"
    );
    let rtt = format!("{}::{}(", "tcp_rtt", "rtt_us");
    assert_eq!(
        production_hits(&rtt),
        vec![("dial/connect.rs".to_string(), 1)],
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
    let (tcp, compress) = tcp_hop(&ep).await.expect("回环拨不通");
    let rtt = crate::platform::tcp_rtt::rtt_us(&tcp);
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
/// 今天链的是 `[patch.crates-io]` 那份补过的副本 ⇒ 对 ⇒ 闸开。多包那一格：三包连着走（1000 · 30000 · 200 字节，
/// 可压比都 ≫ 2），每包逐字节同 —— 补丁若只修了「这一包交全」而状态跨包错了，这一格看得见。
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

/// vendored russh 副本的住址（仓根相对）。与后端清单 `[patch.crates-io]` 那一行指的是同一处（V2 对拍）。
const VENDORED_RUSSH: &str = "src/vendor/russh";

/// 副本目录的绝对住址。住址写成字面量（`test_tiers` 扫描层按 `repo_root().join("…")` 字面量核它在盘上），
/// 与 [`VENDORED_RUSSH`] 同一处由这里的断言钉住（V2 拿常量去比 `[patch.crates-io]` 那一行）。
fn vendored_russh_dir() -> std::path::PathBuf {
    let dir = crate::guard_support::repo_root().join("src/vendor/russh");
    assert_eq!(dir, crate::guard_support::repo_root().join(VENDORED_RUSSH));
    dir
}

/// `VENDOR.md` 里 `<!-- {tag} 起 -->` 与 `<!-- {tag} 止 -->` 之间、代码围栏里的非空行，各按空白切成字段。
/// 标记缺一个 ⇒ `Err`（不许退化成空清单：空清单会让下面的对拍零命中地绿）。
fn vendor_block(doc: &str, tag: &str) -> Result<Vec<Vec<String>>, String> {
    let begin = format!("<!-- {tag} 起 -->");
    let end = format!("<!-- {tag} 止 -->");
    let rows: Vec<&str> = doc.lines().collect();
    let at = |needle: &str| {
        let hits: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, l)| l.trim() == needle)
            .map(|(i, _)| i)
            .collect();
        match hits.as_slice() {
            [i] => Ok(*i),
            _ => Err(format!(
                "`VENDOR.md` 里 `{needle}` 这一行出现 {} 次（要恰好 1 次）",
                hits.len()
            )),
        }
    };
    let (b, e) = (at(&begin)?, at(&end)?);
    if b >= e {
        return Err(format!("`VENDOR.md` 里 `{begin}` 不在 `{end}` 之前"));
    }
    Ok(rows[b + 1..e]
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("```"))
        .map(|l| l.split_whitespace().map(str::to_string).collect())
        .collect())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 目录下全部文件（相对路径 → 内容），走共享原语（空扩展名表 = 整棵树；副本里全是文本，读不成 UTF-8 就 panic）。
fn files_under(dir: &std::path::Path) -> BTreeMap<String, String> {
    guard_core::scan_tree_excluding(dir, &[], &[])
        .into_iter()
        .map(|(path, src)| {
            let rel = path.strip_prefix(dir).unwrap_or(&path);
            (rel.to_string_lossy().replace('\\', "/"), src)
        })
        .collect()
}

/// ★ V1：**副本只改了登记的那几份**（「副本是上游的镜子，不是分身」—— 改了哪几行要一眼可见、上游修好之后撤得干净）。
///
/// `VENDOR.md` 三张表：原样清单（`sha256  路径`，逐份取自 `russh-0.61.1.crate`，那份 `.crate` 的 sha256 == 后端 lock 原先锁的 checksum）·
/// 改过的文件（`路径  原样 sha256  补后 sha256`）· 副本特有的（`sha256  路径`）。判：
/// ① 盘上文件集合 == 原样清单 ∪ 副本特有 ∪ {`VENDOR.md`}（两向：多一份、少一份都红；`Cargo.lock` 不算 —— 仓根 `.gitignore` 挡着它、
///    path 依赖的 lock 不参加解析，有人在副本里跑过 cargo 就会长出来）；
/// ② 改过的文件：盘上 == 补后指纹、且补后 ≠ 原样、且原样指纹 == 原样清单那一行（改过的真是清单里那一份）；
/// ③ 其余每一份：盘上 == 登记的指纹。
/// 反空真：改过的文件恰好是 `{src/compression.rs}`（相等，不是地板）；原样清单里有 `Cargo.toml` 与 `src/lib.rs`（抽取器没抽空）。
#[test]
fn the_vendored_russh_differs_from_the_crate_only_where_registered() {
    let dir = vendored_russh_dir();
    let doc = std::fs::read_to_string(dir.join("VENDOR.md"))
        .unwrap_or_else(|e| panic!("{VENDORED_RUSSH}/VENDOR.md 读不到：{e}"));
    let manifest = vendor_block(&doc, "原样清单").unwrap();
    let patched = vendor_block(&doc, "改过的文件").unwrap();
    let extra = vendor_block(&doc, "副本特有").unwrap();
    let pairs = |rows: &[Vec<String>], what: &str| -> BTreeMap<String, String> {
        rows.iter()
            .map(|r| match r.as_slice() {
                [h, p] if h.len() == 64 => (p.clone(), h.clone()),
                _ => panic!("`VENDOR.md` {what} 这一行不是 `sha256  路径`：{r:?}"),
            })
            .collect()
    };
    let manifest = pairs(&manifest, "原样清单");
    let extra = pairs(&extra, "副本特有");
    let patched: BTreeMap<String, (String, String)> = patched
        .iter()
        .map(|r| match r.as_slice() {
            [p, a, b] if a.len() == 64 && b.len() == 64 => (p.clone(), (a.clone(), b.clone())),
            _ => panic!("`VENDOR.md` 改过的文件 这一行不是 `路径  原样  补后`：{r:?}"),
        })
        .collect();
    // 反空真。
    assert!(
        manifest.contains_key("Cargo.toml") && manifest.contains_key("src/lib.rs"),
        "原样清单抽空了 / 抽歪了：{} 行",
        manifest.len()
    );
    assert_eq!(
        patched.keys().cloned().collect::<Vec<_>>(),
        vec!["src/compression.rs".to_string()],
        "改过的文件变了 —— 补丁的射程只许是 `Decompress::decompress` 那一份（多改一份就先登记、写清为什么）"
    );

    // ① 集合两向相等。
    let mut contents = files_under(&dir);
    contents.remove("Cargo.lock");
    let on_disk: BTreeSet<String> = contents.keys().cloned().collect();
    let want: BTreeSet<String> = manifest
        .keys()
        .chain(extra.keys())
        .cloned()
        .chain(["VENDOR.md".to_string()])
        .collect();
    assert_eq!(
        on_disk.difference(&want).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "副本里多出了没登记的文件（往副本里加东西 ⇒ 先登记进 VENDOR.md 的「副本特有」或「改过的文件」）"
    );
    assert_eq!(
        want.difference(&on_disk).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "VENDOR.md 登记了、盘上却没有的文件（副本被删了几份）"
    );

    // ② ③ 逐份指纹。
    let mut wrong = Vec::new();
    for rel in &on_disk {
        if rel == "VENDOR.md" {
            continue;
        }
        let got = sha256_hex(contents[rel].as_bytes());
        let want = if let Some((orig, fixed)) = patched.get(rel) {
            assert_ne!(
                orig, fixed,
                "{rel}：登记的补后指纹 == 原样指纹 —— 那就不是改过的文件"
            );
            assert_eq!(
                manifest.get(rel),
                Some(orig),
                "{rel}：「改过的文件」里记的原样指纹与原样清单那一行不一致"
            );
            fixed
        } else if let Some(h) = manifest.get(rel) {
            h
        } else {
            &extra[rel]
        };
        if &got != want {
            wrong.push(format!("  {rel}: 盘上 {got} ≠ 登记 {want}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "副本里这几份与 VENDOR.md 登记的指纹不一致（改补丁就同拍改登记；别在副本里改出自己的版本）：\n{}",
        wrong.join("\n")
    );
}

/// ★ V2：**补丁真接上了**（三份文本对拍）。
///
/// ① 后端清单 `[patch.crates-io]` 段里恰好一条、就是 `russh`、指向 `../vendor/russh`（== [`VENDORED_RUSSH`]）；
/// ② 依赖声明那一行的版本 == 副本 `Cargo.toml` 的 `version` == lock 里 `russh` 那一块的 `version`；
/// ③ lock 里 `russh` 恰好一块、而且**没有** `source`（= path 来源；有 `source` 就是 crates.io 那份还坏着的）。
/// 没接上的样子（有人删了补丁那两行）：③ 当场红；Z5 也红（链回上游那份坏的）。
#[test]
fn the_russh_patch_is_really_wired() {
    let manifest = include_str!("../../src/backend/Cargo.toml");
    let lock = include_str!("../../src/backend/Cargo.lock");
    // ① `[patch.crates-io]` 段的条目（剥整行注释，按列 0 起的 `名 = …` 认）。
    let uncommented = guard_core::strip_hash_comment_lines(manifest);
    let mut section = String::new();
    let mut patch_rows = Vec::new();
    let mut dep_rows = Vec::new();
    for line in uncommented.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.to_string();
            continue;
        }
        if t.is_empty() || line.starts_with(char::is_whitespace) {
            continue;
        }
        if section == "[patch.crates-io]" {
            patch_rows.push(t.to_string());
        } else if section == "[dependencies]" && t.starts_with("russh =") {
            dep_rows.push(t.to_string());
        }
    }
    let want_path = format!(
        "path = \"../{}\"",
        VENDORED_RUSSH.trim_start_matches("src/")
    );
    assert_eq!(
        patch_rows,
        vec![format!("russh = {{ {want_path} }}")],
        "`[patch.crates-io]` 段不是恰好一条指向副本的 russh"
    );
    // ② 三处版本。
    let [dep] = dep_rows.as_slice() else {
        panic!("`[dependencies]` 里 `russh = …` 不是恰好一行：{dep_rows:?}");
    };
    let declared = dep
        .split("version = \"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .expect("依赖声明那一行抠不出 version");
    let vendored_dir = vendored_russh_dir();
    let vendored_toml =
        std::fs::read_to_string(vendored_dir.join("Cargo.toml")).expect("副本 Cargo.toml");
    // 只看 `[package]` 段（后面的依赖段里也有 `version = "…"`）。
    let vendored: Vec<&str> = vendored_toml
        .split("\n[")
        .find(|seg| seg.starts_with("package]"))
        .expect("副本 Cargo.toml 没有 [package] 段")
        .lines()
        .filter_map(|l| l.strip_prefix("version = \""))
        .filter_map(|r| r.strip_suffix('"'))
        .collect();
    // ③ lock 里那一块。
    let blocks: Vec<&str> = lock
        .split("[[package]]\n")
        .filter(|b| b.lines().next() == Some("name = \"russh\""))
        .collect();
    let [block] = blocks.as_slice() else {
        panic!("lock 里 `russh` 不是恰好一块：{} 块", blocks.len());
    };
    let locked = block
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|r| r.strip_suffix('"'))
        .expect("lock 那一块抠不出 version");
    assert_eq!(
        vendored,
        vec![declared],
        "副本 Cargo.toml 的 version 与依赖声明（{declared}）对不上 —— `[patch.crates-io]` 要求副本版本满足声明"
    );
    assert_eq!(locked, declared, "lock 里 russh 的版本与依赖声明对不上");
    assert!(
        !block.lines().any(|l| l.starts_with("source = ")),
        "lock 里 russh 那一块还带着 `source` —— 链的是 crates.io 那份（解压还坏着），不是副本：\n{block}"
    );
}

/// ★ ZR（读数，`#[ignore]`）：**真 sshd 上**同一段可压的字节（`seq 1 400000`），强制不压 vs 强制压。
///
/// 由 `tests/evidence/NT1-net-loopback.py --compress` 起回环 sshd、带 `NT1_COMPRESS={host,port,user,key_path}` 来跑；
/// 协商结果（`compression: none` / `zlib@openssh.com`）由那份脚本读 sshd 日志核（异源）。本条自己判：
/// - 不压那趟：载荷收全（> 2 MB）；
/// - 压的那趟：**闸关着（russh 解压坏着）⇒ 30 秒内收不全**（现打的样子是卡在第一条通道上）；
///   闸开着 ⇒ 载荷逐字节同、线上字节 < 不压那趟的一半。
/// - 上行那一半（客户端压、sshd 解 —— 与下行是两套代码）：同一条连接上把一段会话 jsonl 样子的载荷喂给远端 `sha256sum`，
///   两趟都要摘要 == 本侧算的；闸开着 ⇒ 压的那趟上行线上字节 < 不压那趟的一半。
#[ignore = "要真 sshd：由 tests/evidence/NT1-net-loopback.py --compress 带环境变量来跑"]
#[tokio::test(flavor = "multi_thread")]
async fn zr_real_sshd_negotiates_zlib_and_moves_fewer_bytes_when_forced() {
    let Ok(raw) = std::env::var("NT1_COMPRESS") else {
        panic!("没有 NT1_COMPRESS —— 这条读数由 NT1-net-loopback.py --compress 来跑");
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
                expected: None,
                observed: Default::default(),
                reported: Default::default(),
                stages: StageSink::new(false),
                endpoint: format!("{host}:{port}"),
            };
            let mut h = russh::client::connect_stream(config(false, compress), counted, checker)
                .await
                .unwrap();
            authenticate(&mut h, &user, Some(&key), None).await.unwrap();
            let body = tokio::time::timeout(std::time::Duration::from_secs(30), async {
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
            let up = tokio::time::timeout(std::time::Duration::from_secs(30), async {
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
/// （sshd「channel_input_data: … incomplete message」）。由 `tests/evidence/WF2-zlib-container.py` 起一次性容器 sshd、
/// 带 `NT1_COMPRESS={host,port,user,key_path}` 来跑；协商结果由那份脚本读 sshd 日志核（异源）。
#[ignore = "要真 sshd：由 tests/evidence/WF2-zlib-container.py 带环境变量来跑"]
#[tokio::test(flavor = "multi_thread")]
async fn zr_real_sshd_takes_incompressible_puts_when_forced() {
    let Ok(raw) = std::env::var("NT1_COMPRESS") else {
        panic!("没有 NT1_COMPRESS —— 这条读数由 WF2-zlib-container.py 来跑");
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
            expected: None,
            observed: Default::default(),
            reported: Default::default(),
            stages: StageSink::new(false),
            endpoint: format!("{host}:{port}"),
        };
        let mut h = russh::client::connect_stream(config(false, compress), tcp, checker)
            .await
            .unwrap();
        authenticate(&mut h, &user, Some(&key), None).await.unwrap();
        let ch = h.channel_open_session().await.unwrap();
        ch.request_subsystem(true, "sftp").await.unwrap();
        let s = crate::dial::sftp::Session::over(ch.into_stream(), Box::new(()))
            .await
            .unwrap();
        let dir = format!("{}/{}", s.home(), crate::dial::sftp::STAGING_ROOT);
        crate::dial::sftp::make_dirs(&s, &dir).await.unwrap();
        for (n, seed) in [(33_000usize, 7u64), (1 << 20, 11)] {
            let blob = incompressible(n, seed);
            let path = format!("{dir}/zr2-{compress}-{n}");
            let put = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                crate::dial::sftp::put_atomic(&s, &path, &blob, 0o600),
            )
            .await;
            let back = match &put {
                Ok(Ok(())) => crate::dial::sftp::read_all(&s, &path).await,
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

/// 〔「保住多地址那一格」〕一趟里每个地址的校验器都把「地址 → 实得指纹」记进同一格：
/// 接受支与拒绝支都记；两把钥匙 ⇒ 两格不同，同一把 ⇒ 两格相同（期望手写，指纹取钥匙本身）。
#[tokio::test]
async fn vis2_every_checker_of_one_run_writes_its_address_into_the_shared_book() {
    use russh::client::Handler as _;
    let key = |n: u8| {
        russh::keys::PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(
            &[n; 32],
        ))
        .public_key()
        .clone()
    };
    let fp = |k: &PublicKey| k.fingerprint(HashAlg::Sha256).to_string();
    async fn one_run(
        legs: Vec<(&'static str, PublicKey, Option<String>)>,
    ) -> (BTreeMap<String, String>, Vec<bool>) {
        let book: Arc<Mutex<BTreeMap<String, String>>> = Arc::default();
        let mut verdicts = Vec::new();
        for (ep, k, expected) in legs {
            let mut c = Checker {
                expected,
                observed: Arc::default(),
                reported: Arc::clone(&book),
                stages: StageSink::new(false),
                endpoint: ep.to_string(),
            };
            verdicts.push(c.check_server_key(&k).await.unwrap());
        }
        let got = book.lock().unwrap().clone();
        (got, verdicts)
    }
    let (a, b) = (key(1), key(2));
    assert_ne!(fp(&a), fp(&b), "夹具坏了：两把钥匙同一个指纹");
    let (got, verdicts) = one_run(vec![
        ("a:22", a.clone(), None),
        ("b:22", b.clone(), Some(fp(&a))),
    ])
    .await;
    assert_eq!(
        verdicts,
        vec![true, false],
        "b 那一支配了对不上的期望，该拒"
    );
    assert_eq!(
        got,
        BTreeMap::from([("a:22".to_string(), fp(&a)), ("b:22".to_string(), fp(&b))]),
        "拒掉的那一支也要记下它报的指纹"
    );
    let (got, _) = one_run(vec![("a:22", a.clone(), None), ("b:2222", a.clone(), None)]).await;
    assert_eq!(
        got,
        BTreeMap::from([("a:22".to_string(), fp(&a)), ("b:2222".to_string(), fp(&a))])
    );
}

/// 接线：目标那一趟的竞速与经跳板那一次握手都记进同一格、那一格进 `Linked`；跳板自己那一趟另开一格。带正控。
/// 〔第二问「只当跳板用的机器一直 TOFU（设计没写）」〕跳板那一格也进 `Linked`（`jump_fingerprints`），界面按那一台自己固化。
#[test]
fn vis2_the_target_run_and_only_it_feeds_the_linked_book() {
    let prod =
        crate::guard_support::production_code(include_str!("../../src/backend/dial/connect.rs"));
    let at = prod
        .find("pub(crate) async fn establish(")
        .expect("establish 不在了");
    let body = &prod[at..];
    let n = |t: &str, needle: &str| t.matches(needle).count();
    assert_eq!(
        n(body, "                &reported,\n"),
        1,
        "目标那一趟的竞速没拿到那一格"
    );
    assert_eq!(
        n(body, "                &jump_reported,\n"),
        1,
        "跳板那一趟该另开一格"
    );
    assert_eq!(
        n(body, "&Arc::default(),"),
        0,
        "跳板那一趟的指纹又被丢进一格没人读的临时格"
    );
    assert_eq!(
        n(body, "let jump_fingerprints = jump_reported.lock()"),
        1,
        "跳板那一格没交进 `Linked`"
    );
    assert_eq!(
        n(body, "reported: Arc::clone(&reported),"),
        1,
        "经跳板那一次握手没记进那一格"
    );
    assert_eq!(
        n(body, "let fingerprints = reported.lock()"),
        1,
        "那一格没交进 `Linked`"
    );
    let race_at = prod.find("async fn race(").expect("race 不在了");
    assert_eq!(
        n(&prod[race_at..at], "let reported = Arc::clone(reported);"),
        1,
        "竞速里每个地址的校验器没拿同一格"
    );
    assert_eq!(
        n(&format!("{body}\n&Arc::default(),"), "&Arc::default(),"),
        1,
        "量具正控"
    );
}
