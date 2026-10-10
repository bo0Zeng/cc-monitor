//! 那一刻本机时区与 UTC 的差。两个前端都按它把时刻写成本地时间（后端那一侧另有它自己的 `platform/local_tz.rs`：后端不链本 crate）。

/// 那一刻本机时区与 UTC 的差（秒；夏令时按那一刻算）。问不到 ⇒ 0。
pub fn local_offset_at(secs: i64) -> i64 {
    use chrono::{Offset, TimeZone};
    chrono::Local
        .timestamp_opt(secs, 0)
        .single()
        .map_or(0, |t| i64::from(t.offset().fix().local_minus_utc()))
}

/// 看的这一台的时区（IANA 名，如 `Asia/Shanghai`）：请求信封与起流旗标里的 `tz` 那一格（后端按它写「几点」「今天 / 昨天」）。
/// 问不到 · 长过 [`TZ_ROOM`] ⇒ `None`（不带那一格，后端按 UTC 写）。
pub fn viewer_tz() -> Option<String> {
    iana_time_zone::get_timezone()
        .ok()
        .filter(|n| !n.is_empty() && n.len() <= TZ_ROOM)
}

/// 信封里 `tz` 那一格最长几个字节（IANA 名今天最长三十出头）：量一行请求有多长的那几处按它留位子。
pub const TZ_ROOM: usize = 64;
