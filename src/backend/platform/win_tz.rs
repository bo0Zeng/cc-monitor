//! 本地时区偏移的 Windows 读法（[`super::local_tz`] 的那一臂）：`GetTimeZoneInformation` 答此刻是标准时还是夏令时。
//! 本 crate 不引 `windows-sys`，照 `win_proc.rs` 手写 `extern "system"`；结构体逐格对着 Win32 `TIME_ZONE_INFORMATION`。

#![cfg(windows)]

#[repr(C)]
#[allow(dead_code)] // 只经 FFI 填；读的只有几格偏移
struct SystemTime {
    parts: [u16; 8],
}

#[repr(C)]
#[allow(dead_code)] // 同上
struct TimeZoneInformation {
    bias: i32,
    standard_name: [u16; 32],
    standard_date: SystemTime,
    standard_bias: i32,
    daylight_name: [u16; 32],
    daylight_date: SystemTime,
    daylight_bias: i32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
}

/// 此刻的本地偏移（Windows 只答「此刻」，`t` 不用）。
pub(crate) fn offset_secs(_t: u64) -> Option<i64> {
    // SAFETY: 结构体逐格对着 Win32 `TIME_ZONE_INFORMATION` 写；全零是合法初值，函数只写进这一份。
    let mut info: TimeZoneInformation = unsafe { std::mem::zeroed() };
    let id = unsafe { GetTimeZoneInformation(&mut info) };
    let extra = match id {
        1 => info.standard_bias,
        2 => info.daylight_bias,
        0 => 0,
        _ => return None,
    };
    Some(-i64::from(info.bias + extra) * 60)
}
