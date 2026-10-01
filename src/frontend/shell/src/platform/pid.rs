//! 进程这一族的平台读法〔阶段 H：；原住 `bind.rs`〕：一个 pid 的起始时刻 · 父 pid · 还活没活。
//!
//! 只有 Windows 那一臂有实现（`OpenProcess` 一族 · ToolHelp）；别处答「读不到」（`None` / `false`），与搬家前 `bind.rs` 那几个桩逐字同义。
//! 拿这几个事实去判「绑定还作不作数 / 这条登记该不该撤」的规则在 `bind.rs`。

/// 指定 pid 的 `GetProcessTimes` 起始 FILETIME。读不到 ⇒ `None`。
#[cfg(windows)]
pub fn creation_filetime(pid: u32) -> Option<crate::utils::FileTime> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) if !h.is_invalid() => h,
            _ => return None,
        };
        let mut creation = FILETIME::default();
        let mut exit_t = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let ok =
            GetProcessTimes(handle, &mut creation, &mut exit_t, &mut kernel, &mut user).is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return None;
        }
        Some(crate::utils::FileTime::from_win32(&creation))
    }
}

#[cfg(not(windows))]
pub fn creation_filetime(_pid: u32) -> Option<crate::utils::FileTime> {
    None
}

/// 用 ToolHelp 拿指定 pid 的父 pid。读不到 ⇒ `None`。
#[cfg(windows)]
pub fn parent_pid(pid: u32) -> Option<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        if snap.is_invalid() {
            return None;
        }
        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };
        let mut result = None;
        if Process32First(snap, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    result = Some(entry.th32ParentProcessID);
                    break;
                }
                if Process32Next(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        result
    }
}

#[cfg(not(windows))]
pub fn parent_pid(_pid: u32) -> Option<u32> {
    None
}

/// 这个 pid 还在跑没有（`GetExitCodeProcess == STILL_ACTIVE`）。别处恒 `false`（`bind.rs` 心跳那一格的诚实边界照旧）。
#[cfg(windows)]
pub fn is_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    const STILL_ACTIVE: u32 = 259;
    if pid == 0 {
        return false;
    }
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) if !h.is_invalid() => h,
            _ => return false,
        };
        let mut code: u32 = 0;
        let alive = GetExitCodeProcess(handle, &mut code).is_ok() && code == STILL_ACTIVE;
        let _ = CloseHandle(handle);
        alive
    }
}

#[cfg(not(windows))]
pub fn is_alive(_pid: u32) -> bool {
    false
}
