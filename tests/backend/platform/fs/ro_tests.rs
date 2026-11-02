use super::*;

/// 超了即拒收的一道上限（前三条只看数，不看越界之后谁照办）。
fn reject(bytes: u64) -> Cap {
    Cap {
        bytes,
        over: Over::Reject,
    }
}

/// ★ **超限要在「读之前」就拒**。
///
/// 这条路存在的全部理由是「别让巨型文件吃爆内存」，而它此前要先把 `cap + 1` 字节
/// 读进内存才发现该拒 —— **安全拒绝本身成了 OOM 候选**。
///
/// ⚠ 判据用**小 cap**（`cap` 是入参）而不是造一个 256 MiB 的文件：
/// 要证的是「**先看长度**」这个顺序，不是「256 这个数」。
#[test]
fn an_oversized_file_is_rejected_before_it_is_read() {
    let dir = std::env::temp_dir().join(format!("ccm-f06-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let path = dir.join("big.jsonl");
    std::fs::write(&path, vec![b'x'; 100]).expect("写夹具");

    let err = Ro::read_capped(&path, reject(10)).expect_err("100 字节 > cap 10，必须拒");
    assert!(
        err.said.contains(&copy_core::copy_text(
            "beFs.readRegularCapped.tooBig",
            &[("cap", "10")]
        )),
        "拒绝信息要说清上限，实得：{err}"
    );
    assert!(
        err.said.contains("100"),
        "★ 要报出**实际大小** —— 只说「超限」，用户不知道差多少、也不知道该不该清理。实得：{err}"
    );

    // 不超限的照常读回来（防「早退写成恒拒」）。
    let ok = Ro::read_capped(&path, reject(1000)).expect("100 字节 < cap 1000，该放行");
    assert_eq!(ok.len(), 100);

    let _ = std::fs::remove_dir_all(&dir);
}

/// ★ **顺序**：早退必须在**读之前**，不只是「存在」〔audit-0805，08-06〕。
///
/// # 上面那条只钉住了「早退还在」，钉不住「它在哪一步」
///
/// 上面那条靠报错文案里的**实际大小**来区分（只有早退那条给得出）。
/// 变异实测：**删掉**早退 ⇒ 它红。但把早退**挪到 `read_to_end` 之后** ⇒
/// 报错文案一模一样、**它照样绿**，而这条路存在的全部理由（别把巨型文件读进内存）
/// 已经没了。
///
/// ⚠ 「**这个顺序没有判据**…只有内存/耗时能区分」。
/// **前半句已经旧了**（存在性有判据），**后半句是对的**——而「只有内存能区分」
/// 恰恰是 F22 建 `alloc_probe` 的理由。量具早就有了，只是没人回来用。
///
/// # 怎么量
///
/// `cap` 是入参 ⇒ 用**小 cap + 大文件**，不造 256 MiB 的夹具：
/// 8 MiB 的文件、cap 给 1 MiB。早退在前 ⇒ 一个字节都不读；早退在后 ⇒
/// `take(cap + 1)` 会先吃掉 1 MiB。两者差三个数量级，阈值取中间。
///
/// ⚠ 量具是**本线程**的（`alloc_probe` 用 thread-local）——本条是同步 `#[test]`，
/// 被测代码与断言同线程，成立。改成 `#[tokio::test(flavor = "multi_thread")]` 会让它
/// **静默变哑**（`alloc_probe` 头注逐字记过这个坑）。
#[test]
fn the_early_return_happens_before_any_read_not_just_somewhere() {
    let dir = std::env::temp_dir().join(format!("ccm-order-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let path = dir.join("huge.jsonl");
    const FILE: usize = 8 * 1024 * 1024;
    const CAP: u64 = 1024 * 1024;
    std::fs::write(&path, vec![b'x'; FILE]).expect("写夹具");

    let base = test_meters::alloc_probe::reset_peak();
    let err = Ro::read_capped(&path, reject(CAP)).expect_err("8 MiB > cap 1 MiB，必须拒");
    let grew = test_meters::alloc_probe::peak_since(base);

    // 抽取器自检：拒的是**这条路**（早退那条带实际大小），不是别的错。
    assert!(
        err.said.contains(&FILE.to_string()),
        "拒绝信息里没有实际大小 —— 走的不是早退那条，本条量的不是它：{err}"
    );
    assert!(
        grew < 256 * 1024,
        "拒绝一个 {FILE} 字节的文件时，本线程峰值涨了 {grew} 字节。\n\
             ★ 早退**没有发生在读之前** —— `take(cap + 1)` 已经把 {CAP} 字节吃进内存了。\n\
             这条路存在的全部理由就是「别让巨型文件吃爆内存」（生产 cap 是 256 MiB，\n\
             那时这一口就是 256 MiB）。**安全拒绝本身又成了 OOM 候选。**\n\
             ⚠ 报错文案分不出这两种顺序（两条给的话一样）—— 只有内存能分，所以用 `alloc_probe`。"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// 读不了那一句只是原因（不是普通文件 · 超出上限 · IO 原因词），系统原话进 `raw`。
#[test]
fn a_refused_read_says_the_reason_and_keeps_the_os_words_apart() {
    let dir = std::env::temp_dir().join(format!("ccm-fs-said-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let e = Ro::read_capped(&dir, reject(8)).expect_err("目录也读成了");
    assert_eq!(e.said, copy_text("reason.fs.notRegular", &[]));
    let gone = dir.join("gone");
    let e = Ro::read_capped(&gone, reject(8)).expect_err("不在的也读成了");
    assert_eq!(e.said, copy_core::io_reason(std::io::ErrorKind::NotFound));
    assert!(e.raw.is_some(), "系统原话没进详情：{e:?}");
    std::fs::remove_dir_all(&dir).ok();
}

/// 读动词只收 [`Cap`]：比上限多一个字节 ⇒ 回超出上限那一句（带上限与实际大小）；正好等于上限 ⇒ 照常读回来。
#[test]
fn a_capped_read_refuses_one_byte_over_and_keeps_the_exact_cap() {
    let dir = std::env::temp_dir().join(format!("ccm-capped-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let cap = Cap {
        bytes: 16,
        over: Over::Reject,
    };
    let over = dir.join("over.json");
    std::fs::write(&over, vec![b'x'; cap.bytes as usize + 1]).expect("写夹具");
    let e = Ro::read_capped(&over, cap).expect_err("比上限多一个字节也读成了");
    assert_eq!(
        e.said,
        copy_text(
            "beFs.readRegularCapped.tooBigSized",
            &[("cap", "16"), ("size", "17")]
        )
    );
    let e = Ro::read_string_capped(&over, cap).expect_err("读成字符串那一形没收上限");
    assert!(e.said.contains("17"), "实得：{e}");
    let e = Ro::read_json(&over, cap).expect_err("读成 JSON 那一形没收上限");
    assert!(e.said.contains("17"), "实得：{e}");

    let exact = dir.join("exact.json");
    std::fs::write(&exact, vec![b' '; cap.bytes as usize]).expect("写夹具");
    assert_eq!(
        Ro::read_capped(&exact, cap).expect("正好等于上限").len(),
        16
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 几形读出来的值：字节 · 字符串（不是 UTF-8 ⇒ 读不成，原话进详情）· JSON（剥 BOM 再解；解不开 ⇒「内容无法解析」）。
#[test]
fn the_capped_reads_hand_back_bytes_text_and_json() {
    let dir = std::env::temp_dir().join(format!("ccm-capped-forms-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let cap = Cap {
        bytes: 64,
        over: Over::HardError,
    };
    let p = dir.join("a.json");
    std::fs::write(&p, b"\xEF\xBB\xBF{\"a\":1}").expect("写夹具");
    assert_eq!(
        Ro::read_json(&p, cap).expect("带 BOM 的 JSON"),
        serde_json::json!({"a": 1})
    );
    std::fs::write(&p, b"{\"a\":1}").expect("写夹具");
    assert_eq!(
        Ro::read_string_capped(&p, cap).expect("读成字符串"),
        "{\"a\":1}"
    );
    std::fs::write(&p, b"{oops").expect("写夹具");
    let e = Ro::read_json(&p, cap).expect_err("坏 JSON 也解成了");
    assert_eq!(e.said, copy_text("reason.content.unparsable", &[]));
    std::fs::write(&p, b"\xFF\xFE").expect("写夹具");
    let e = Ro::read_string_capped(&p, cap).expect_err("不是 UTF-8 也读成了字符串");
    assert!(e.raw.is_some(), "解码原话没进详情：{e:?}");
    let e = Ro::read_capped(&dir, cap).expect_err("目录也读成了");
    assert_eq!(e.said, copy_text("reason.fs.notRegular", &[]));
    std::fs::remove_dir_all(&dir).ok();
}

/// 不跟链接那一形：最后一段是链接 ⇒ 不读（跟链接那一形照常读到目标）；普通文件照常读，上限照样收。
#[cfg(unix)]
#[test]
fn the_nofollow_capped_read_refuses_a_link_and_still_caps() {
    let dir = std::env::temp_dir().join(format!("ccm-capped-nofollow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let cap = Cap {
        bytes: 8,
        over: Over::Reject,
    };
    let target = dir.join("target");
    std::fs::write(&target, b"abc").expect("写夹具");
    let link = dir.join("link");
    std::os::unix::fs::symlink(&target, &link).expect("建链接");
    assert_eq!(
        Ro::read_capped_nofollow(&target, cap).expect("普通文件"),
        b"abc"
    );
    assert!(
        Ro::read_capped_nofollow(&link, cap).is_err(),
        "最后一段是链接也读成了"
    );
    assert_eq!(Ro::read_capped(&link, cap).expect("跟链接那一形"), b"abc");
    std::fs::write(&target, vec![b'x'; 9]).expect("写夹具");
    let e = Ro::read_capped_nofollow(&target, cap).expect_err("不跟链接那一形没收上限");
    assert!(e.said.contains('9'), "实得：{e}");
    std::fs::remove_dir_all(&dir).ok();
}

/// 拒一份超限的文件之前一个字节都不读（带上限的新动词同一道顺序）。
#[test]
fn a_capped_read_refuses_before_reading_anything() {
    let dir = std::env::temp_dir().join(format!("ccm-capped-order-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let path = dir.join("huge.jsonl");
    const FILE: usize = 8 * 1024 * 1024;
    let cap = Cap {
        bytes: 1024 * 1024,
        over: Over::Reject,
    };
    std::fs::write(&path, vec![b'x'; FILE]).expect("写夹具");
    for nofollow in [false, true] {
        let base = test_meters::alloc_probe::reset_peak();
        let err = if nofollow {
            Ro::read_capped_nofollow(&path, cap)
        } else {
            Ro::read_capped(&path, cap)
        }
        .expect_err("8 MiB > 上限 1 MiB");
        let grew = test_meters::alloc_probe::peak_since(base);
        assert!(
            err.said.contains(&FILE.to_string()),
            "走的不是先看长度那一条：{err}"
        );
        assert!(
            grew < 256 * 1024,
            "拒一份 {FILE} 字节的文件时本线程峰值涨了 {grew} 字节（不跟链接：{nofollow}）：先读进内存才拒"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// 盘上那一份的戳（改动时刻 · 长度 · inode）：同样长、同一刻原子替换过（新节点）⇒ 戳也不一样；不在 ⇒ `None`。
/// 额度账 · 轮换 · 血缘三份缓存都靠它判「别人写过没有」。
#[test]
fn the_stamp_tells_an_atomic_replace_apart_and_is_none_when_absent() {
    let dir = std::env::temp_dir().join(format!("ccm-stamp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let path = dir.join("book.json");
    assert_eq!(Ro::stamp(&path), None, "不在 ⇒ 没有戳");
    std::fs::write(&path, b"{}").expect("写夹具");
    let first = Ro::stamp(&path).expect("在 ⇒ 有戳");
    assert_eq!(first.1, 2, "长度那一格");
    let side = dir.join("book.json.tmp");
    std::fs::write(&side, b"{}").expect("写旁名");
    // 改动时刻也拨成同一刻：只剩 inode 那一格分得出两份。
    std::fs::File::options()
        .write(true)
        .open(&side)
        .and_then(|f| f.set_modified(first.0))
        .expect("拨改动时刻");
    std::fs::rename(&side, &path).expect("原子挪过去");
    #[cfg(unix)]
    assert_ne!(
        Ro::stamp(&path),
        Some(first),
        "换了节点的同长同内容一份，戳没变"
    );
    std::fs::remove_dir_all(&dir).ok();
}
