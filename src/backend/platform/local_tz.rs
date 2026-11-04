//! 这台机器此刻的本地时区偏移（东正、秒）—— 给「几点」那几处排版用（终端 `--text` 写 `HH:MM` 要按本地钟）。
//!
//! 读法：POSIX 走 `localtime_r` 的 `tm_gmtoff`（读 `TZ` 与 `/etc/localtime`，同 `date`）；Windows 走 `GetTimeZoneInformation`
//! （此刻是标准时还是夏令时由它答）。读不出 ⇒ `None`，调用方按 UTC 排、不猜。

/// 时刻 `t`（unix 秒）那一刻的本地偏移。
#[cfg(unix)]
#[cfg_attr(
    target_pointer_width = "64",
    expect(
        clippy::useless_conversion,
        reason = "`tm_gmtoff` 是 `c_long`：64 位上就是 `i64`，32 位上不是；转换照写、两边都对"
    )
)]
pub fn offset_secs(t: u64) -> Option<i64> {
    let tt = libc::time_t::try_from(t).ok()?;
    // SAFETY: `tm` 是纯数据（全零是合法值）；`localtime_r` 只写进我们给的那一份、不留指针。
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let got = unsafe { libc::localtime_r(&tt, &mut tm) };
    (!got.is_null()).then_some(i64::from(tm.tm_gmtoff))
}

/// Windows 那一臂住 [`super::win_tz`]（整份文件按平台选进来）。
#[cfg(windows)]
pub use super::win_tz::offset_secs;

/// 这台机器的时区叫什么（IANA 名，例：`America/Los_Angeles`）：`TZ` 是 IANA 名 ⇒ 它；不然看 `/etc/localtime` 指向 zoneinfo 里的哪一份；
/// 再不然问系统（Windows：系统时区名经 `iana-time-zone` 映射成 IANA 名 —— Windows 名 → IANA 名只走这一处）。
/// 都说不出 ⇒ `None`：调用方说「时区不明」，不猜成 UTC。全后端只这一处取。
pub fn iana_name() -> Option<String> {
    let tz = std::env::var("TZ").ok();
    #[cfg(unix)]
    let (link, system) = (
        crate::platform::fs::Ro::read_link("/etc/localtime").ok(),
        None::<String>,
    );
    #[cfg(not(unix))]
    let (link, system) = (
        None::<std::path::PathBuf>,
        iana_time_zone::get_timezone().ok(),
    );
    iana_of(tz.as_deref(), link.as_deref(), system.as_deref())
}

/// [`iana_name`] 的判法（量具直接喂）：`TZ` · 那条链 · 系统答的名依次试；名字都过一遍时区库，库里没有的不算。
pub(crate) fn iana_of(
    tz: Option<&str>,
    localtime_link: Option<&std::path::Path>,
    system: Option<&str>,
) -> Option<String> {
    let known =
        |n: &str| (!n.is_empty() && jiff::tz::TimeZone::get(n).is_ok()).then(|| n.to_string());
    if let Some(n) = tz.map(|t| t.strip_prefix(':').unwrap_or(t)).and_then(known) {
        return Some(n);
    }
    let from_link = localtime_link.and_then(|p| p.to_str()).and_then(|link| {
        let at = link.rfind("zoneinfo/")? + "zoneinfo/".len();
        known(&link[at..])
    });
    from_link.or_else(|| system.and_then(known))
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/local_tz_tests.rs"]
mod tests;
