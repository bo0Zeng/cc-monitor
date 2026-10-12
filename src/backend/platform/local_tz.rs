//! 这台机器此刻的本地时区偏移（东正、秒）—— 给「几点」那几处排版用（终端 `--text` 写 `HH:MM` 要按本地钟）。
//!
//! 读法：POSIX 走 `localtime_r` 的 `tm_gmtoff`（读 `TZ` 与 `/etc/localtime`，同 `date`）；Windows 走 `GetTimeZoneInformation`
//! （此刻是标准时还是夏令时由它答）。读不出 ⇒ `None`，调用方按 UTC 排、不猜。

/// 时刻 `t`（unix 秒）那一刻的本地偏移。
#[cfg(unix)]
pub(crate) fn offset_secs(t: u64) -> Option<i64> {
    let tt = libc::time_t::try_from(t).ok()?;
    // SAFETY: `tm` 是纯数据（全零是合法值）；`localtime_r` 只写进我们给的那一份、不留指针。
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let got = unsafe { libc::localtime_r(&tt, &mut tm) };
    (!got.is_null()).then_some(i64::from(tm.tm_gmtoff))
}

/// Windows 那一臂住 [`super::win_tz`]（整份文件按平台选进来）。
#[cfg(windows)]
pub(crate) use super::win_tz::offset_secs;

/// 这台机器的时区叫什么（IANA 名，例：`America/Los_Angeles`）：`TZ` 是 IANA 名 ⇒ 它；不然看 `/etc/localtime` 指向 zoneinfo 里的哪一份。
/// 说不出（`TZ` 是 POSIX 串 · 没有那条链 · Windows）⇒ `None`：调用方说「时区不明」，不猜成 UTC。全后端只这一处取。
pub(crate) fn iana_name() -> Option<String> {
    let tz = std::env::var("TZ").ok();
    let link = crate::platform::fs::Ro::read_link("/etc/localtime").ok();
    iana_of(tz.as_deref(), link.as_deref())
}

/// [`iana_name`] 的判法（量具直接喂）：名字都过一遍时区库，库里没有的不算。
pub(crate) fn iana_of(
    tz: Option<&str>,
    localtime_link: Option<&std::path::Path>,
) -> Option<String> {
    let known = |n: &str| {
        (!n.is_empty() && crate::common::time::Tz::iana(n).is_some()).then(|| n.to_string())
    };
    if let Some(n) = tz.map(|t| t.strip_prefix(':').unwrap_or(t)).and_then(known) {
        return Some(n);
    }
    let link = localtime_link?.to_str()?;
    let at = link.rfind("zoneinfo/")? + "zoneinfo/".len();
    known(&link[at..])
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/local_tz_tests.rs"]
mod tests;
