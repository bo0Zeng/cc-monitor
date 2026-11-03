//! 骨架索引那一行的判据夹具：从一行原文起（生产里扫描图逐行已解析过、直接走 [`super::index_row_of`]）。
//! 扫描图 · 适配层经观测面那几条 · 格目录的判据（上层经 `test-support`）共用。

use super::{index_row_of, IndexRow};

/// 一行原文 → [`IndexRow`]（`kind` 是那一家，`offset` · `len` 是那一行在记录里的位置）。
pub fn index_row(kind: &str, line: &[u8], offset: u64, len: u64) -> IndexRow {
    let text = String::from_utf8_lossy(line);
    let at = crate::agents::Line::new(&text, offset);
    let rec = crate::agents::record_face(kind)
        .and_then(|f| (f.parse)(&at).ok().flatten())
        .and_then(|t| t.record);
    let v = crate::observe::lines::parse(line);
    let beat = v.as_ref().and_then(|v| crate::agents::beat_of(kind, v));
    index_row_of(kind, v.as_ref(), beat.as_ref(), rec.as_ref(), offset, len)
}
