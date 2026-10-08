//! `copy_table.rs`（对外文案表的 Rust 读口）的判据。
//!
//! # 要求住址
//!
//! 要求：「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
//! 决定 2：一份文件两侧各读，不是两份表加一条对拍。
//! 「表 ↔ 引用」两向相等那一条住 `tests/copy/copy-table.vitest.ts`（它把 `.rs` 的 `copy_text` 调用点也收进引用一侧）。

use super::*;

/// 读的就是前端那一份（同一个文件，不是副本）：表里一条已有的前端文案取得出来、占位符填得上。
#[test]
fn it_reads_the_same_table_the_frontend_reads() {
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/copy/table.json"),
    )
    .expect("读不到 src/shared/copy/table.json");
    assert_eq!(raw, TABLE_JSON, "内嵌的那一份与盘上那一份不是同一份");
    assert_eq!(
        copy_text(
            "panePreview.head.title",
            &[("origin", "devbox"), ("target", "%1")]
        ),
        copy_core::copy_text(
            "panePreview.head.title",
            &[("origin", "devbox"), ("target", "%1")]
        )
    );
}

/// 表里没有 ⇒ ``（不 panic）；没给的占位符原样留着（调用方写错会在界面上看得见，由 vitest 那条对拍在上游拦）。
#[test]
fn a_missing_key_says_its_name_instead_of_panicking() {
    assert_eq!(copy_text("no.such.key", &[]), "〔no.such.key〕");
    assert_eq!(
        copy_text("panePreview.head.title", &[("origin", "devbox")]),
        "预览 · devbox · tmux 会话 {target}"
    );
}
