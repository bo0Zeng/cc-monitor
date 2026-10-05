//! 借别的进程的控制台改一下标题（Win32 `AttachConsole` → `SetConsoleTitleW` → `FreeConsole`）。
//!
//! 只给 ↗ 那一档用：进程链断在一个控制台 shell 上（Windows 默认终端把它交给了 Windows Terminal），
//! 点击那一刻在它的控制台上挂一次记号标题、按标题找窗口、再还原。每次挂上只做一两个调用就摘下，
//! 找窗口时不挂着（挂着期间那个控制台被关会连累本进程）。同一时刻只许一份在借（挂控制台是整个进程的状态）。
//! 判定（哪个标题算命中、找不到说哪句）不在这里，在 `bind.rs`。别处每一问都答「借不到」。

/// 在 `pid` 的控制台上挂 `marker` 当标题，交 `look` 去找，之后把标题还原（标题中途被别人改了就不动它）。
/// 借不到那个控制台（进程没了 · 本进程自己有控制台 · 系统不给）⇒ `None`；借到了 ⇒ `Some(look 的结果)`。
#[cfg(windows)]
pub fn with_marker_title<T>(pid: u32, marker: &str, look: impl FnOnce() -> T) -> Option<T> {
    use std::sync::Mutex;
    static LENDING: Mutex<()> = Mutex::new(());
    let _one = LENDING.lock().unwrap_or_else(|e| e.into_inner());

    let wide: Vec<u16> = marker.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY：挂上之后只调读写标题那两个、立刻摘下；缓冲区长度照实传。
    let old = unsafe {
        if AttachConsole(pid) == 0 {
            return None;
        }
        let mut buf = vec![0u16; 4096];
        let n = GetConsoleTitleW(buf.as_mut_ptr(), buf.len() as u32) as usize;
        buf.truncate(n.min(buf.len()));
        let set = SetConsoleTitleW(wide.as_ptr()) != 0;
        FreeConsole();
        if !set {
            return None;
        }
        buf
    };
    let out = look();
    // SAFETY：同上；还在挂着记号才还原。
    unsafe {
        if AttachConsole(pid) != 0 {
            let mut buf = vec![0u16; wide.len() + 8];
            let n = GetConsoleTitleW(buf.as_mut_ptr(), buf.len() as u32) as usize;
            if buf[..n.min(buf.len())] == wide[..wide.len() - 1] {
                let mut back = old;
                back.push(0);
                SetConsoleTitleW(back.as_ptr());
            }
            FreeConsole();
        }
    }
    Some(out)
}

#[cfg(not(windows))]
pub fn with_marker_title<T>(_pid: u32, _marker: &str, _look: impl FnOnce() -> T) -> Option<T> {
    None
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(pid: u32) -> i32;
    fn FreeConsole() -> i32;
    fn GetConsoleTitleW(title: *mut u16, size: u32) -> u32;
    fn SetConsoleTitleW(title: *const u16) -> i32;
}
