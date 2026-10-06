//! 桌面窗口这一族的平台读法〔阶段 H：；原住 `bind.rs`〕：可见窗口枚举（按标题 / 按属主进程）· 句柄还在不在 · 属主 pid · 拉到前台。
//!
//! 只有 Windows 有这一族（Win32 `HWND`）；别处每一问都答「没有」。
//! 判定不在这里（「翻译官只翻译事实的读法」）：哪个标题算我们的窗口 · 一条绑定还作不作数 · 拉不动说哪句，都在 `bind.rs`。

/// 这台系统有没有「按句柄找 / 验 / 拉前一个桌面窗口」这一族（今天只有 Windows）。
pub const SUPPORTED: bool = cfg!(windows);

/// 枚举命中的那个窗口的快照（`find_window_by_marker_substr` 的输出）。
pub struct MarkerHit {
    pub hwnd: isize,
    pub owner_pid: u32,
    pub title: String,
}

/// EnumWindows 扫一遍所有可见窗口，返回 `matches(标题, needle)` 为真的**第一个**（规则由调用方给）。
///
/// 从 `bind.rs::find_window_by_marker_substr` 原样搬来：同样的 thread_local FOUND/MARKER、同样的 512-u16 buffer、
/// 同样**不过滤 owner=0**（见下方注释）；只把「标题含 marker」那一句换成调用方给的 `matches`。
#[cfg(windows)]
pub fn first_visible_window(needle: &str, matches: fn(&str, &str) -> bool) -> Option<MarkerHit> {
    use std::cell::{Cell, RefCell};
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    thread_local! {
        static FOUND: RefCell<Option<MarkerHit>> = const { RefCell::new(None) };
        static MARKER: RefCell<String> = const { RefCell::new(String::new()) };
        static MATCHES: Cell<Option<fn(&str, &str) -> bool>> = const { Cell::new(None) };
    }

    MARKER.with(|m| *m.borrow_mut() = needle.to_string());
    MATCHES.with(|f| f.set(Some(matches)));
    FOUND.with(|f| *f.borrow_mut() = None);

    // v1.7.5 修：不再过滤 `GetWindow(hwnd, GW_OWNER) != 0` 的窗口。
    //
    // 原本继承自 v1.6.x 4-tier 算法的"只看 top-level 无 owner 窗口"过滤，
    // 在 v1.7 cc 注入式绑定下导致 bug：WindowsTerminal 的 XAML 子窗口（Microsoft.UI.Xaml.*）
    // owner != 0（owner = WT 主窗口），会被过滤掉。PowerShell 的
    // `$Host.UI.RawUI.WindowTitle` 可能同步到这些 XAML 子窗口而非 WT 主窗口
    // （取决于 WT/conhost 版本）。
    //
    // marker 字符串 = "ccm-bind-{PID}-{8 char UUID}" 极独特，不会撞别的窗口
    // title，不需要 owner=0 这个保险。
    unsafe extern "system" fn cb(hwnd: HWND, _lp: LPARAM) -> BOOL {
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }
        // v1.7.7：不再用 GetWindowTextLengthW 预查询长度。
        // 对 Microsoft.UI.Xaml.Controls / WinUI 控件（Windows Terminal 用的），
        // GetWindowTextLengthW 经常返回 0（WinRT 控件兼容 Win32 API 的 quirk），
        // 但 GetWindowTextW 直接给 buffer 调用能拿到实际 title。
        // 固定 512 buffer 跟用户端诊断脚本一致；marker 长 ≤ 50 字符肯定够。
        let title = unsafe {
            let mut buf = vec![0u16; 512];
            let n = GetWindowTextW(hwnd, &mut buf);
            if n > 0 {
                String::from_utf16_lossy(&buf[..n as usize])
            } else {
                String::new()
            }
        };
        let marker_match = MARKER.with(|m| {
            let m = m.borrow();
            MATCHES.with(|f| f.get().is_some_and(|f| f(&title, m.as_str())))
        });
        if !marker_match {
            return BOOL(1);
        }
        let mut owner_pid: u32 = 0;
        let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_pid)) };
        FOUND.with(|f| {
            *f.borrow_mut() = Some(MarkerHit {
                hwnd: hwnd.0,
                owner_pid,
                title,
            });
        });
        BOOL(0) // 找到了，停止枚举
    }

    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(0));
    }

    FOUND.with(|f| f.borrow_mut().take())
}

#[cfg(not(windows))]
pub fn first_visible_window(_needle: &str, _matches: fn(&str, &str) -> bool) -> Option<MarkerHit> {
    None
}

/// 同一个 EnumWindows，按**属主进程号**筛：属主是 `pid` 的可见、无主（`GW_OWNER` 为空）顶层窗口 —— 任务栏上那种应用窗口；
/// 对话框 / 工具窗有主人，不算。别处恒空。
#[cfg(windows)]
pub fn visible_top_windows_of(pid: u32) -> Vec<isize> {
    use std::cell::RefCell;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER,
    };

    thread_local! {
        static HITS: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
    }

    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }
        if unsafe { GetWindow(hwnd, GW_OWNER) }.0 != 0 {
            return BOOL(1);
        }
        let mut owner: u32 = 0;
        let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if owner as isize == lp.0 {
            HITS.with(|h| h.borrow_mut().push(hwnd.0));
        }
        BOOL(1)
    }

    HITS.with(|h| h.borrow_mut().clear());
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(pid as isize));
    }
    HITS.with(|h| std::mem::take(&mut *h.borrow_mut()))
}

#[cfg(not(windows))]
pub fn visible_top_windows_of(_pid: u32) -> Vec<isize> {
    Vec::new()
}

/// 这个句柄此刻还是不是一个窗口（`IsWindow`）。
#[cfg(windows)]
pub fn exists(hwnd: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;
    unsafe { IsWindow(HWND(hwnd)).as_bool() }
}

#[cfg(not(windows))]
pub fn exists(_hwnd: isize) -> bool {
    false
}

/// 这个窗口此刻的属主进程 pid（`GetWindowThreadProcessId` 的出参；读不到 = 0）。
#[cfg(windows)]
pub fn owner_pid(hwnd: isize) -> u32 {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let hwnd = HWND(hwnd);
    let mut cur_owner: u32 = 0;
    unsafe {
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut cur_owner));
    }
    cur_owner
}

#[cfg(not(windows))]
pub fn owner_pid(_hwnd: isize) -> u32 {
    0
}

/// 把窗口拉到前台：最小化着就先还原，再 `SetForegroundWindow`。回的是系统答没答应（不答应时 OS 会让它在任务栏闪）。
#[cfg(windows)]
pub fn bring_to_front(hwnd: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    unsafe {
        let h = HWND(hwnd);
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        SetForegroundWindow(h).as_bool()
    }
}

#[cfg(not(windows))]
pub fn bring_to_front(_hwnd: isize) -> bool {
    false
}

/// 让窗口在任务栏闪（`FlashWindowEx`：标题栏与任务栏按钮一起闪，直到它到前台）；不切、不抢焦点。回系统收下了没有。
#[cfg(windows)]
pub fn flash(hwnd: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        FlashWindowEx, FLASHWINFO, FLASHW_ALL, FLASHW_TIMERNOFG,
    };
    let info = FLASHWINFO {
        cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
        hwnd: HWND(hwnd),
        dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
        uCount: 0,
        dwTimeout: 0,
    };
    unsafe {
        let _ = FlashWindowEx(&info);
    }
    true
}

#[cfg(not(windows))]
pub fn flash(_hwnd: isize) -> bool {
    false
}
