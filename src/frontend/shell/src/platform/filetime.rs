//! Win32 `FILETIME` 那两件〔余下〕：[`crate::utils::FileTime`] 的平台那一半，
//! 逐字从 `utils.rs` 搬来（同一个类型的第二个 impl 块 —— 调用方照旧写 `FileTime::from_win32`）。

#[cfg(windows)]
impl crate::utils::FileTime {
    /// 从 Win32 FILETIME struct 提取 u64。
    pub fn from_win32(ft: &windows::Win32::Foundation::FILETIME) -> Self {
        Self(((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64))
    }

    /// FILETIME UTC → .NET Local Ticks。
    /// Claude Code 写的 procStart 是 .NET 形式；要跟它比较时必须先转。
    /// Windows-only —— 用 `FileTimeToLocalFileTime` 修当地时区偏移。
    pub fn to_net_local_ticks(self) -> crate::utils::NetTicks {
        use windows::Win32::Foundation::FILETIME;
        /// 从 .NET 0001-01-01 起到 Win32 1601-01-01 之间的 100ns 数。
        const NET_EPOCH_TO_WIN32_FILETIME_TICKS: u64 = 504_911_232_000_000_000;

        #[link(name = "kernel32")]
        extern "system" {
            fn FileTimeToLocalFileTime(
                lpFileTime: *const FILETIME,
                lpLocalFileTime: *mut FILETIME,
            ) -> i32;
        }

        let utc = FILETIME {
            dwLowDateTime: (self.0 & 0xFFFF_FFFF) as u32,
            dwHighDateTime: (self.0 >> 32) as u32,
        };
        let mut local = FILETIME::default();
        let ok = unsafe { FileTimeToLocalFileTime(&utc, &mut local) };
        if ok == 0 {
            // 极罕见 — 退化为 UTC + 偏移（仍然单调但跟 Claude 比有时区差）
            return crate::utils::NetTicks(self.0 + NET_EPOCH_TO_WIN32_FILETIME_TICKS);
        }
        let local_u64 = ((local.dwHighDateTime as u64) << 32) | (local.dwLowDateTime as u64);
        crate::utils::NetTicks(local_u64 + NET_EPOCH_TO_WIN32_FILETIME_TICKS)
    }
}
