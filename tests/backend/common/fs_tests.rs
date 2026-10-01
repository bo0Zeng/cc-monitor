use super::*;

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

    let err = read_regular_capped(&path, 10).expect_err("100 字节 > cap 10，必须拒");
    assert!(
        err.contains("超过 10 字节上限"),
        "拒绝信息要说清上限，实得：{err}"
    );
    assert!(
        err.contains("100"),
        "★ 要报出**实际大小** —— 只说「超限」，用户不知道差多少、也不知道该不该清理。实得：{err}"
    );

    // 不超限的照常读回来（防「早退写成恒拒」）。
    let ok = read_regular_capped(&path, 1000).expect("100 字节 < cap 1000，该放行");
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

    let base = crate::alloc_probe::reset_peak();
    let err = read_regular_capped(&path, CAP).expect_err("8 MiB > cap 1 MiB，必须拒");
    let grew = crate::alloc_probe::peak_since(base);

    // 抽取器自检：拒的是**这条路**（早退那条带实际大小），不是别的错。
    assert!(
        err.contains(&FILE.to_string()),
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
