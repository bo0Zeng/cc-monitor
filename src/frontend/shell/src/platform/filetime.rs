//! Win32 `FILETIME` 那一件〔余下〕：[`crate::utils::FileTime`] 的平台那一半，
//! 逐字从 `utils.rs` 搬来（同一个类型的第二个 impl 块 —— 调用方照旧写 `FileTime::from_win32`）。

#[cfg(windows)]
impl crate::utils::FileTime {
    /// 从 Win32 FILETIME struct 提取 u64。
    pub fn from_win32(ft: &windows::Win32::Foundation::FILETIME) -> Self {
        Self(((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64))
    }
}
