//! 进程这一族的平台读法〔阶段 H：；原住 `bind.rs`〕：一个 pid 的起始时刻 · 往上的祖先 · 还活没活。
//!
//! Windows 那一臂走 `OpenProcess` 一族 · ToolHelp；Linux 那一臂读 `/proc/<pid>/stat`；别处答「读不到」（`None` / `false`）。
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

/// 一个进程的起始时刻戳（只在同一台上比「是不是同一个进程」：Windows 是 FILETIME，Linux 是开机后的时钟滴答
/// —— 与那台接入块记下的同一个数）。读不到 ⇒ `None`。
pub fn start_stamp(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    {
        creation_filetime(pid).map(|ft| ft.0)
    }
    #[cfg(target_os = "linux")]
    {
        linux_stat(pid).map(|s| s.start)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = pid;
        None
    }
}

/// `/proc/<pid>/stat` 要的三格（名字夹在第一个左括号与最后一个右括号之间：名字里可以有括号与空格）。
#[cfg(target_os = "linux")]
struct LinuxStat {
    ppid: u32,
    name: String,
    start: u64,
}

#[cfg(target_os = "linux")]
fn linux_stat(pid: u32) -> Option<LinuxStat> {
    if pid == 0 {
        return None;
    }
    let line = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (open, close) = (line.find('(')?, line.rfind(')')?);
    let rest: Vec<&str> = line.get(close + 1..)?.split_whitespace().collect();
    // 右括号之后：state(3) ppid(4) … starttime(22)
    Some(LinuxStat {
        ppid: rest.get(1)?.parse().ok()?,
        name: line.get(open + 1..close)?.to_string(),
        start: rest.get(19)?.parse().ok()?,
    })
}

/// 一个 pid 往上的祖先（父进程在前，不含它自己）：`(pid, 程序名)`。一趟 ToolHelp 快照；最多 32 级（父进程表成环 / 读坏了也停得下来）。
/// 读不到 ⇒ 空。别处恒空。
#[cfg(windows)]
pub fn ancestors(pid: u32) -> Vec<(u32, String)> {
    use std::collections::HashMap;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut table: HashMap<u32, (u32, String)> = HashMap::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return Vec::new();
        };
        if snap.is_invalid() {
            return Vec::new();
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let n = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                table.insert(
                    entry.th32ProcessID,
                    (
                        entry.th32ParentProcessID,
                        String::from_utf16_lossy(&entry.szExeFile[..n]),
                    ),
                );
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    let mut out = Vec::new();
    let mut cur = pid;
    while out.len() < 32 {
        let Some((ppid, _)) = table.get(&cur) else {
            break;
        };
        let ppid = *ppid;
        if ppid == 0 || ppid == cur || out.iter().any(|(p, _)| *p == ppid) {
            break;
        }
        let Some((_, name)) = table.get(&ppid) else {
            break;
        };
        out.push((ppid, name.clone()));
        cur = ppid;
    }
    out
}

/// Linux：逐级读 `/proc/<pid>/stat` 往上数（同 Windows 那一臂：父进程在前、不含自己、最多 32 级、成环就停）。
#[cfg(target_os = "linux")]
pub fn ancestors(pid: u32) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = Vec::new();
    let mut cur = pid;
    while out.len() < 32 {
        let Some(me) = linux_stat(cur) else {
            break;
        };
        let ppid = me.ppid;
        if ppid == 0 || ppid == cur || out.iter().any(|(p, _)| *p == ppid) {
            break;
        }
        let Some(parent) = linux_stat(ppid) else {
            break;
        };
        out.push((ppid, parent.name));
        cur = ppid;
    }
    out
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn ancestors(_pid: u32) -> Vec<(u32, String)> {
    Vec::new()
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

/// Linux：`/proc/<pid>` 还在、不是僵尸。
#[cfg(target_os = "linux")]
pub fn is_alive(pid: u32) -> bool {
    pid != 0
        && std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|l| {
                l.rfind(')')
                    .map(|i| l[i + 1..].trim_start().starts_with('Z'))
            })
            .is_some_and(|zombie| !zombie)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn is_alive(_pid: u32) -> bool {
    false
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../../../tests/frontend/shell/platform/pid_linux_tests.rs"]
mod tests;
