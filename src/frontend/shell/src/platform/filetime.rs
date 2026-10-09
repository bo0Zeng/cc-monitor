//! Win32 `FILETIME`（自 1601-01-01 UTC，100ns 单位）：`GetProcessTimes` 给的起始时刻。只有 Windows 那一臂用它
//! （Linux 的起始时刻戳读 `/proc/<pid>/stat`，见 `platform/pid.rs`）。

/// Win32 FILETIME (自 1601-01-01 UTC, 100ns 单位)。
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileTime(pub u64);

#[cfg(windows)]
impl FileTime {
    /// 从 Win32 FILETIME struct 提取 u64。
    pub fn from_win32(ft: &windows::Win32::Foundation::FILETIME) -> Self {
        Self(((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64))
    }
}
