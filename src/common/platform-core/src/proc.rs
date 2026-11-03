//! 进程这一族的平台事实：`/proc/<pid>/stat` 一行怎么切 · Windows 的进程表 · 起子进程不要窗口那一位。
//! 读哪份文件 · 读上限多大是调用方的事（各家有自己的有上限读口）；拿这些事实判什么也是调用方的事。

/// `CreateProcess` 的 `CREATE_NO_WINDOW`。是位掩码，不是尺寸。
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `/proc/<pid>/stat` 一行里要的几格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    /// 父进程号（第 4 栏）。
    pub ppid: u32,
    /// 程序名（第 2 栏，去掉括号）。
    pub name: String,
    /// 控制终端（第 7 栏；0 ＝ 没有）。
    pub tty_nr: i64,
    /// 起始时刻，开机后的时钟滴答（第 22 栏）。
    pub start: u64,
}

/// 切 `/proc/<pid>/stat` 一行。名字夹在第一个左括号与**最后一个**右括号之间（名字里可以有括号与空格），
/// 其余各栏从最后一个右括号之后数：第一格是第 3 栏 `state`。缺栏 · 不是数 ⇒ `None`。
pub fn parse_stat(line: &str) -> Option<Stat> {
    /// 右括号之后第 0 格是第 3 栏。
    const fn after_comm(field: usize) -> usize {
        field - 3
    }
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close <= open {
        return None;
    }
    let name = line.get(open + 1..close)?.to_string();
    let rest: Vec<&str> = line.get(close + 1..)?.split_whitespace().collect();
    Some(Stat {
        ppid: rest.get(after_comm(4))?.parse().ok()?,
        name,
        tty_nr: rest.get(after_comm(7))?.parse().ok()?,
        start: rest.get(after_comm(22))?.parse().ok()?,
    })
}

/// 进程表里的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcEntry {
    pub pid: u32,
    pub ppid: u32,
    /// 程序名（`foo.exe`）。
    pub name: String,
}

#[cfg(windows)]
mod win {
    use std::os::windows::io::{FromRawHandle as _, OwnedHandle, RawHandle};

    pub(super) const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    pub(super) const MAX_PATH: usize = 260;

    /// `PROCESSENTRY32W`（字段照 tlhelp32.h 的次序与宽度）。
    #[repr(C)]
    pub(super) struct ProcessEntry32W {
        pub(super) size: u32,
        pub(super) usage: u32,
        pub(super) pid: u32,
        pub(super) default_heap_id: usize,
        pub(super) module_id: u32,
        pub(super) threads: u32,
        pub(super) parent_pid: u32,
        pub(super) pri_class_base: i32,
        pub(super) flags: u32,
        pub(super) exe_file: [u16; MAX_PATH],
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub(super) fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> RawHandle;
        pub(super) fn Process32FirstW(snapshot: RawHandle, entry: *mut ProcessEntry32W) -> i32;
        pub(super) fn Process32NextW(snapshot: RawHandle, entry: *mut ProcessEntry32W) -> i32;
    }

    /// 拍一张快照；拍不成 ⇒ 系统原话。成功的句柄归 `OwnedHandle`（落地即关）。
    pub(super) fn snapshot() -> std::io::Result<OwnedHandle> {
        // SAFETY：只按值收两个整数；失败回 `INVALID_HANDLE_VALUE`（-1）。
        let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if raw.is_null() || raw as isize == -1 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY：刚分配、无人持有的合法句柄。
        Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
    }
}

/// 此刻的进程表：一趟 ToolHelp 快照（不用打开各个进程：提权的也列得出）。拍不成 ⇒ 系统原话。
#[cfg(windows)]
pub fn processes() -> std::io::Result<Vec<ProcEntry>> {
    use std::os::windows::io::AsRawHandle as _;
    let snap = win::snapshot()?;
    let h = snap.as_raw_handle();
    let mut e = win::ProcessEntry32W {
        size: std::mem::size_of::<win::ProcessEntry32W>() as u32,
        usage: 0,
        pid: 0,
        default_heap_id: 0,
        module_id: 0,
        threads: 0,
        parent_pid: 0,
        pri_class_base: 0,
        flags: 0,
        exe_file: [0; win::MAX_PATH],
    };
    let mut out = Vec::new();
    // SAFETY：`e` 是栈上一份 `dwSize` 填好的结构，句柄在 `snap` 活着的这段里有效。
    let mut ok = unsafe { win::Process32FirstW(h, &mut e) } != 0;
    while ok {
        let len = e
            .exe_file
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(win::MAX_PATH);
        out.push(ProcEntry {
            pid: e.pid,
            ppid: e.parent_pid,
            name: String::from_utf16_lossy(&e.exe_file[..len]),
        });
        // SAFETY：同上。
        ok = unsafe { win::Process32NextW(h, &mut e) } != 0;
    }
    Ok(out)
}
