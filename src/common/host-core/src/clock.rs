//! 那一刻本机时区与 UTC 的差。两个前端都按它把时刻写成本地时间（后端那一侧另有它自己的 `platform/local_tz.rs`：后端不链本 crate）。

/// 那一刻本机时区与 UTC 的差（秒；夏令时按那一刻算）。问不到 ⇒ 0。
pub fn local_offset_at(secs: i64) -> i64 {
    use chrono::{Offset, TimeZone};
    chrono::Local
        .timestamp_opt(secs, 0)
        .single()
        .map_or(0, |t| i64::from(t.offset().fix().local_minus_utc()))
}
