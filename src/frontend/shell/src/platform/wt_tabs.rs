//! Windows Terminal 窗口里的标签页（UI Automation）：列出一个窗口里各标签页的名字 · 选中其中一个。
//!
//! 只给小助手用（`platform/console.rs::helper_main` 的 tab 那一支，借着那个会话的控制台挂记号标题时现读）；
//! 「名字列表 ＋ 记号 ⇒ 选第几个 / 为什么没选成」那条规则在 `bind.rs::pick_tab`，这里只读、只点。
//! 窗口是不是 Windows Terminal 按窗口类名认（[`is_terminal_window`]）；别的终端程序不试。
//! 切不了标签页时（窗口是终端那一级认出的：提权 · 读不到控制台）标签个数照进程树数（[`tab_count`]，monitor 这边现数）。

/// Windows Terminal 顶层窗口的类名。
#[cfg(windows)]
pub const WT_WINDOW_CLASS: &str = "CASCADIA_HOSTING_WINDOW_CLASS";

/// Windows Terminal 每个标签（分屏时每一格）底下挂的伪控制台宿主：终端进程的直接子进程。
const PSEUDOCONSOLE_HOST: &str = "OpenConsole.exe";

/// 进程表（`(进程号, 父进程号, 程序名)`）里 `owner` 名下的伪控制台宿主有几个 ⇒ 那个终端进程开着几个标签（分屏的多算，只会往多里数）。
pub fn hosted_tabs(procs: &[(u32, u32, String)], owner: u32) -> usize {
    procs
        .iter()
        .filter(|(_, ppid, name)| *ppid == owner && name.eq_ignore_ascii_case(PSEUDOCONSOLE_HOST))
        .count()
}

/// 终端进程 `owner` 此刻开着几个标签：照进程树数（不用 UI Automation，提权的终端也数得出：进程快照不用打开那个进程）。
/// 读不到 ⇒ 0。
pub fn tab_count(owner: u32) -> usize {
    hosted_tabs(&crate::platform::pid::processes(), owner)
}

/// 这个窗口是不是 Windows Terminal 的（按类名；读不到 ⇒ 不是）。
#[cfg(windows)]
pub fn is_terminal_window(hwnd: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut b = [0u16; 128];
    // SAFETY：只读窗口类名到本地缓冲；句柄失效时回 0。
    let n = unsafe { GetClassNameW(HWND(hwnd), &mut b) };
    n > 0 && String::from_utf16_lossy(&b[..n as usize]) == WT_WINDOW_CLASS
}

#[cfg(not(windows))]
pub fn is_terminal_window(_hwnd: isize) -> bool {
    false
}

/// 一个 Windows Terminal 窗口的标签页（UI Automation 现读）。
#[cfg(windows)]
pub struct WtTabs {
    window: windows::Win32::UI::Accessibility::IUIAutomationElement,
    tab_items: windows::Win32::UI::Accessibility::IUIAutomationCondition,
    last: Option<windows::Win32::UI::Accessibility::IUIAutomationElementArray>,
}

#[cfg(windows)]
impl WtTabs {
    /// 打开窗口 `hwnd` 的 UI Automation 树；拿不到 ⇒ `None`。
    pub fn open(hwnd: isize) -> Option<Self> {
        use windows::core::VARIANT;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
        };
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, UIA_ControlTypePropertyId, UIA_TabItemControlTypeId,
        };
        // SAFETY：本进程（小助手）只这一处用 COM，用完即退。
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().ok()?;
            let uia: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let window = uia.ElementFromHandle(HWND(hwnd)).ok()?;
            let tab_items = uia
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_TabItemControlTypeId.0),
                )
                .ok()?;
            Some(WtTabs {
                window,
                tab_items,
                last: None,
            })
        }
    }

    /// 此刻各标签页的名字（标签栏上从左到右）；读不到 ⇒ `None`。
    pub fn names(&mut self) -> Option<Vec<String>> {
        use windows::Win32::UI::Accessibility::TreeScope_Descendants;
        // SAFETY：只读 UI Automation 树。
        unsafe {
            let all = self
                .window
                .FindAll(TreeScope_Descendants, &self.tab_items)
                .ok()?;
            let n = all.Length().ok()?;
            let mut names = Vec::with_capacity(n.max(0) as usize);
            for i in 0..n {
                names.push(all.GetElement(i).ok()?.CurrentName().ok()?.to_string());
            }
            self.last = Some(all);
            Some(names)
        }
    }

    /// 选中上一次 [`Self::names`] 读到的第 `i` 个标签页；选不了 ⇒ `false`。
    pub fn select(&mut self, i: usize) -> bool {
        use windows::Win32::UI::Accessibility::{
            IUIAutomationSelectionItemPattern, UIA_SelectionItemPatternId,
        };
        let Some(all) = self.last.as_ref() else {
            return false;
        };
        // SAFETY：点一下那个标签页的「选中」。
        unsafe {
            all.GetElement(i as i32)
                .and_then(|e| {
                    e.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                        UIA_SelectionItemPatternId,
                    )
                })
                .and_then(|p| p.Select())
                .is_ok()
        }
    }
}
