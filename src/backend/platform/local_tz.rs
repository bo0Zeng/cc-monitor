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
