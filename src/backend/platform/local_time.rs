//! 这台的本地钟：某一刻本地时间比 UTC 快多少秒（自动起算的时段按这台的本地钟算，含夏令时）。
//! 问不出来 ⇒ 0（当 UTC 算；只影响时段的边界落在哪一刻，不影响发不发）。

/// `unix`（秒）那一刻，本地时间 − UTC（秒）。
#[cfg(unix)]
pub(crate) fn utc_offset_at(unix: u64) -> i64 {
    let Ok(t) = libc::time_t::try_from(unix) else {
        return 0;
    };
    // SAFETY: `localtime_r` 只写交给它的那一格 `tm`，两个指针都指向本栈帧里活着的值。
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return 0;
        }
        i64::from(tm.tm_gmtoff)
    }
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FileTime {
    low: u32,
    high: u32,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct SysTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    millis: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn FileTimeToSystemTime(ft: *const FileTime, st: *mut SysTime) -> i32;
    fn SystemTimeToTzSpecificLocalTime(
        tz: *const core::ffi::c_void,
        utc: *const SysTime,
        local: *mut SysTime,
    ) -> i32;
    fn SystemTimeToFileTime(st: *const SysTime, ft: *mut FileTime) -> i32;
}

/// `unix`（秒）那一刻，本地时间 − UTC（秒）：UTC → 本地（按这台的时区规则，含夏令时）再比一次。
#[cfg(windows)]
pub(crate) fn utc_offset_at(unix: u64) -> i64 {
    /// 1601-01-01 到 1970-01-01 的 100 纳秒数。
    const EPOCH_GAP: u64 = 116_444_736_000_000_000;
    let ticks = unix.saturating_mul(10_000_000).saturating_add(EPOCH_GAP);
    let utc_ft = FileTime {
        low: ticks as u32,
        high: (ticks >> 32) as u32,
    };
    let (mut utc, mut local, mut local_ft) =
        (SysTime::default(), SysTime::default(), FileTime::default());
    // SAFETY: 三个调用都只读 / 写交给它们的本栈帧里的值；`tz` 为空 ⇒ 用这台此刻的时区。
    let ok = unsafe {
        FileTimeToSystemTime(&utc_ft, &mut utc) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) != 0
            && SystemTimeToFileTime(&local, &mut local_ft) != 0
    };
    if !ok {
        return 0;
    }
    let local_ticks = (u64::from(local_ft.high) << 32) | u64::from(local_ft.low);
    (local_ticks as i64 - ticks as i64) / 10_000_000
}
