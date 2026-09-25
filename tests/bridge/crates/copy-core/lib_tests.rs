//! 〔CP2c〕`copy-core`（对外文案表的 Rust 取文口）的判据。
//!
//! 要求住址：`设计/01 §6.9` 逐字「**所有对外文案与报错都从一张表来**」；`设计/91 §5.1` 决定 2
//! 「一份文件，两侧各读，零转换」。「表 ↔ 引用」两向相等住 `tests/copy/copy-table.vitest.ts`。

use super::*;

/// 内嵌的就是盘上那一份（同一个文件，不是副本），而且解析得出条目（反空真：0 条时每一句都成了 `〔key〕`）。
#[test]
fn it_embeds_the_one_table_on_disk() {
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../shared/copy/table.json"),
    )
    .expect("读不到 src/shared/copy/table.json");
    assert_eq!(raw, TABLE_JSON, "内嵌的那一份与盘上那一份不是同一份");
    assert!(!entries().is_empty(), "表解析出 0 条");
    assert_eq!(
        copy_text(
            "panePreview.head.title",
            &[("origin", "aya"), ("target", "%1")]
        ),
        "预览画面 · [aya] tmux: %1"
    );
}

/// 表里没有 ⇒ `〔key〕`（不 panic）；没给的占位符原样留着。
#[test]
fn a_missing_key_says_its_name_instead_of_panicking() {
    assert_eq!(copy_text("no.such.key", &[]), "〔no.such.key〕");
    assert_eq!(
        copy_text("panePreview.head.title", &[("origin", "aya")]),
        "预览画面 · [aya] tmux: {target}"
    );
}

/// `copy_static!` 取的是同一条文案、给的是 `&'static str`，同一个调用点取两次是同一块内存（住一辈子，不是每次现造）。
#[test]
fn the_static_form_is_the_same_text_and_lives_forever() {
    fn once() -> &'static str {
        crate::copy_static!("panePreview.head.refresh")
    }
    assert_eq!(once(), copy_text("panePreview.head.refresh", &[]));
    assert!(
        std::ptr::eq(once(), once()),
        "同一个调用点取两次不是同一块内存"
    );
}
