//! d（整份读按块）：块读者 [`Blocks`] 本身 —— 任意块大小下交出的块连起来 ＋ 残尾 ＝ 原字节、每块止于 `\n`、残尾里没有 `\n`。
//! 语料是结构性的假行，不含任何真会话正文。

use super::*;

/// 几种难切的：空 · 只有残尾 · 只有换行 · 跨块的多字节字 · CRLF · BOM · 比块长的一行 · 末尾正好是 `\n`。
fn corpora() -> Vec<Vec<u8>> {
    let long = format!("{{\"long\":\"{}\"}}\n", "中".repeat(3000));
    vec![
        Vec::new(),
        b"{\"half\":".to_vec(),
        b"\n\n\n".to_vec(),
        "{\"a\":\"中文\"}\r\n\u{feff}{\"b\":1}\n   \n{\"c\":\"残"
            .as_bytes()
            .to_vec(),
        format!("{long}{{\"n\":1}}\n{long}tail").into_bytes(),
        format!("{{\"n\":0}}\n{long}").into_bytes(),
    ]
}

#[test]
fn blocks_put_back_together_are_the_bytes_at_any_block_size() {
    for (i, bytes) in corpora().iter().enumerate() {
        for block in [1usize, 2, 7, 64, 4096, 1 << 20] {
            let mut b = Blocks::new(&bytes[..], block);
            let mut got = Vec::new();
            while let Some(whole) = b.next().expect("读得动") {
                assert!(
                    whole.ends_with(b"\n") && !whole.is_empty(),
                    "[{i} · 块 {block}] 交出的块没止于 \\n"
                );
                got.extend_from_slice(&whole);
            }
            let rest = b.rest();
            assert!(!rest.contains(&b'\n'), "[{i} · 块 {block}] 残尾里还有 \\n");
            assert_eq!(
                b.read(),
                bytes.len() as u64,
                "[{i} · 块 {block}] 读的字节数"
            );
            got.extend_from_slice(&rest);
            assert_eq!(&got, bytes, "[{i} · 块 {block}] 连起来不是原字节");
        }
    }
}

/// 一块止于最后一个 `\n`：同一块里几行不拆开（块数按读进来的块数，不按行数）。
#[test]
fn a_block_holds_every_whole_line_read_in_with_it() {
    let bytes = b"{\"a\":1}\n{\"b\":2}\n{\"c\":3}\n{\"d\"";
    let mut b = Blocks::new(&bytes[..], 1 << 20);
    assert_eq!(b.next().unwrap().as_deref(), Some(&bytes[..24]));
    assert_eq!(b.next().unwrap(), None);
    assert_eq!(b.rest(), b"{\"d\"".to_vec());
}
