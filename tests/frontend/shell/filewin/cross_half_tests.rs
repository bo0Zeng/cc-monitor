//! 文件窗口独立成包之后**跨两半**的几条判据 —— 一侧是窗口包（`cc_monitor_filewin`），异源那一侧是 monitor 的东西，
//! 只有 monitor 的测试档两边都够得着（monitor 链窗口包只为那个 `[[bin]]`，生产段零引用；测试档引它不进产物）。
//! 原住窗口那几份判据文件（`editor_tests` · `source_tests` · `transfer_tests`），期望一个字没改。
//!
//! 要求：「装不进一行的走分块」（量的那一行 == 真发出去的那一行）· `§2.3`（窗口只说 `call`，开单与订阅的线上名字两侧对上）。

use cc_monitor_filewin::editor::{
    chunk_budget, commit_args, plan_chunks, request_line_len, save_args, stage_args,
    CMD_COMMIT_TEXT, CMD_STAGE_CHUNK, CMD_WRITE_TEXT, REQUEST_ID_ROOM, SAVE_LINE_CAP,
};

const SAVE_PATH: &str = "/srv/data/app.conf";

/// 发起方期限那一格最长的数位（窗口量的时候按它留位子）。
const LONGEST_WITHIN: std::time::Duration = std::time::Duration::from_millis(u64::MAX);

/// 只用来占位的摘要。
const SHA0: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// 🔴 **窗口量的那一行 == monitor 真发出去的那一行**（按最长的 id、最长的发起方期限算）。
///
/// 异源：另一侧是 monitor 那一侧真正编请求行的纯函数 `inbound_client::encode_request`，
/// 不是本模块的 [`request_line_len`]。语料覆盖会被转义变长的每一类
/// （引号 · 反斜杠 · 控制字符 · 换行 · 中文不转义）；三条命令（整份一行 · 一块 · 提交）各量一遍。
/// 另一格：`id` 的最长形状（u128 毫秒十六进制 ＋ 两个 u64）装得进 [`REQUEST_ID_ROOM`]。
#[test]
fn the_measured_save_line_is_byte_for_byte_the_line_that_is_sent() {
    let key = "0123456789abcdef0123456789abcdef";
    for content in [
        String::new(),
        "a=1\n".into(),
        "引号\"反斜杠\\制表\t换行\n回车\r".into(),
        "\u{1}\u{2}\u{1f}".repeat(100),
        "中文不转义".repeat(1000),
        "x".repeat(70_000),
    ] {
        for (cmd, args) in [
            (CMD_WRITE_TEXT, save_args(SAVE_PATH, &content, SHA0)),
            (CMD_STAGE_CHUNK, stage_args(key, u64::MAX, &content)),
            (
                CMD_COMMIT_TEXT,
                commit_args(SAVE_PATH, key, 17, content.len(), SHA0),
            ),
        ] {
            let sent = crate::inbound_client::encode_request(
                &"0".repeat(REQUEST_ID_ROOM),
                cmd,
                &args,
                Some(LONGEST_WITHIN),
                Some(&"Z".repeat(host_core::TZ_ROOM)),
            );
            assert!(sent.ends_with('\n'));
            assert_eq!(
                request_line_len(cmd, &args),
                sent.len() - 1,
                "`{cmd}`：窗口量的那一行与真编出来的那一行不一样长（内容 {} 字节）",
                content.len()
            );
        }
    }
    let longest_id = format!("m{:x}.{}-{}", u128::MAX, u64::MAX, u64::MAX);
    assert_eq!(
        longest_id.len(),
        REQUEST_ID_ROOM,
        "id 的最长形状与留的位子对不上"
    );
}

/// 🔴 **切块三条性质一起钉，合起来恰好刻画「贪心取满」那一种切法**（不是切成一字一块的空真）：
/// ① 拼回来逐字节 == 原文；② 每块真编出来的请求行（`encode_request`，最长 id、块号按最大）≤ 一行上限；
/// ③ 除最后一块，每块再添下一块的第一个字，那一行就越线。语料覆盖每一种转义长度（1 · 2 · 3 · 4 · 6 字节）。
#[test]
fn chunks_reassemble_exactly_each_fits_one_line_and_each_is_filled() {
    let key = "0123456789abcdef0123456789abcdef";
    let line_of = |chunk: &str| {
        crate::inbound_client::encode_request(
            &"0".repeat(REQUEST_ID_ROOM),
            CMD_STAGE_CHUNK,
            &stage_args(key, u64::MAX, chunk),
            Some(LONGEST_WITHIN),
            Some(&"Z".repeat(host_core::TZ_ROOM)),
        )
        .len()
            - 1
    };
    let mixed: String = (0..400_000u32)
        .map(|i| {
            [
                'a', '"', '\\', '\n', '\u{1}', '中', '🦀', 'é', '\u{7f}', '\u{2028}',
            ][(i * 7 % 10) as usize]
        })
        .collect();
    for (label, text) in [
        ("纯 ASCII 3 MiB", "abcdefgh".repeat(3 * 1024 * 1024 / 8)),
        ("满控制字符 1 MiB（×6）", "\u{1}".repeat(1024 * 1024)),
        ("中文 2 MiB", "汉".repeat(2 * 1024 * 1024 / 3)),
        ("混合", mixed),
        ("刚好一块", "x".repeat(chunk_budget())),
    ] {
        let chunks = plan_chunks(&text, chunk_budget());
        assert_eq!(chunks.concat(), text, "{label}：拼回来不是原文");
        assert!(
            chunks.iter().all(|c| !c.is_empty()),
            "{label}：切出了空块（后端拒空块）"
        );
        for (i, c) in chunks.iter().enumerate() {
            assert!(
                line_of(c) <= SAVE_LINE_CAP,
                "{label}：第 {i} 块那一行 {} 字节，越过一行上限 {SAVE_LINE_CAP}",
                line_of(c)
            );
            if let Some(next) = chunks.get(i + 1) {
                let first = next.chars().next().unwrap();
                assert!(
                    line_of(&format!("{c}{first}")) > SAVE_LINE_CAP,
                    "{label}：第 {i} 块没取满（再添一个字还装得下）"
                );
            }
        }
    }
    assert_eq!(
        plan_chunks(&"x".repeat(chunk_budget()), chunk_budget()).len(),
        1
    );
    assert_eq!(
        plan_chunks(&"x".repeat(chunk_budget() + 1), chunk_budget()).len(),
        2
    );
}

/// 🔴 窗口这一侧写死的那几个**线上名字**与对面（monitor 的传输中继）那一份逐字相等（原住 `transfer_tests` 的前半；
/// 后端提交命令表那一半留在窗口包里）。漂开的症状是具体的：开单答「不认」、订阅答「没有这条流」。
#[test]
fn the_transfer_names_the_window_says_are_the_ones_the_monitor_relays() {
    assert_eq!(
        cc_monitor_filewin::transfer::OP_UPLOAD,
        crate::sftp_pool::TRANSFER_UPLOAD
    );
    assert_eq!(
        cc_monitor_filewin::download::OP_DOWNLOAD,
        crate::sftp_pool::TRANSFER_DOWNLOAD
    );
    assert_eq!(
        cc_monitor_filewin::transfer::KIND_PREFIX,
        crate::sftp_pool::TRANSFER_KIND_PREFIX
    );
}
