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
/// 环境里的 `TZ` 是 IANA 名 ⇒ 按它（同 C 库与浏览器，[`local_offset_at`] 也认它）；否则问系统那一份。
/// 问不到 · 长过 [`TZ_ROOM`] ⇒ `None`（不带那一格，后端按 UTC 写）。
pub fn viewer_tz() -> Option<String> {
    viewer_tz_from(std::env::var("TZ").ok().as_deref(), || {
        iana_time_zone::get_timezone().ok()
    })
}

/// [`viewer_tz`] 的可喂那一半：`tz_var` ＝ 环境里的 `TZ`（POSIX 的 `:` 前缀照去）；像 IANA 名（`区/地` 或 `UTC`，只含字母数字与 `_+-/`）才认，
/// 偏移串（`CST-8`）· 文件路径 · 空 ⇒ 当没给、问 `system`。
pub(crate) fn viewer_tz_from(
    tz_var: Option<&str>,
    system: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let fits = |n: &str| !n.is_empty() && n.len() <= TZ_ROOM;
    let iana = |n: &str| {
        fits(n)
            && (n == "UTC" || (n.contains('/') && !n.starts_with('/') && !n.ends_with('/')))
            && n.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_+-/".contains(&b))
    };
    match tz_var.map(|v| v.strip_prefix(':').unwrap_or(v)) {
        Some(n) if iana(n) => Some(n.to_string()),
        _ => system().filter(|n| fits(n)),
    }
}

/// 信封里 `tz` 那一格最长几个字节（IANA 名今天最长三十出头）：量一行请求有多长的那几处按它留位子。
pub const TZ_ROOM: usize = 64;
