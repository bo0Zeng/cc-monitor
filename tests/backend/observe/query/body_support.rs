//! 读正文核的判据夹具：照记录树那一家（Claude）的记录读一页，成品按线上的样子摊成 `Value`。
//! 本包的判据与主 crate 挂的那几条（拿帧面那一层的投影去剥一页成品）经 `test-support` 共用。

use serde_json::Value;
use std::path::Path;

/// 一页原文（`offset` 起、`seq` 起数）读成记录行，回（每行成品，下一行的 seq）。
pub fn record_lines_of_page(path: &Path, seq: u64, offset: u64, bytes: &[u8]) -> (Vec<Value>, u64) {
    let face = crate::agents::claudecode::RECORDS;
    let mut reader = super::Reader::new(&face, Default::default());
    let (got, next) = super::record_lines_of_page(&mut reader, path, seq, offset, bytes);
    (
        got.iter()
            .map(|l| serde_json::to_value(l).unwrap())
            .collect(),
        next,
    )
}

/// 一页原文（`offset` 起）的行摘要（`ReadRow` 经 serde）。
pub fn rows_of_page(offset: u64, bytes: &[u8]) -> Vec<Value> {
    let face = crate::agents::claudecode::RECORDS;
    let mut reader = super::Reader::new(&face, Default::default());
    super::rows_of(&mut reader, offset, bytes)
        .into_iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect()
}
