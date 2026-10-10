//! 别的进程的控制台这一族的读法。
//!
//! Windows：[`console_window`] —— 借 `pid` 的控制台问一句「显示它的是哪个窗口」（Win32 `AttachConsole` → `GetConsoleWindow`
//! → `GetWindow(GW_OWNER)` → `FreeConsole`）。Windows Terminal 里每个标签的控制台是一个伪控制台窗口，它的属主就是承载那个标签的
//! 终端窗口（Win11 虚拟机 WT 1.24 现打：两个窗口三个标签各自对得上；默认终端交接到 Windows Terminal 的同样对得上）；
//! 经典控制台没有属主，控制台窗口自己就是那个窗口。不要登记、不要改标题、不看是谁起的它。
//! **壳里借别的进程的控制台只此一处**（`AttachConsole` 全仓生产段唯一一个调用点，判据 `bind_tests.rs` 钉着）；
//! 同一时刻只许一份在借（挂控制台是整个进程的状态），挂上只读一个句柄就摘下。判定（属主 / 自己 / 不可见怎么算）在
//! [`window_of_console`]，平台无关、判据喂替身。
//!
//! Linux 那一份是 [`tty_title`]：往本机 bash / zsh 接入块记下的那个终端设备写 xterm 的改标题序列，
//! 终端程序据此改它窗口的标题（认窗口用，见 `bind.rs::claim_tty`）。

/// 控制台窗口 `console`（借不到 ⇒ `None`）⇒ 显示它的那个窗口：有属主 ⇒ 属主（Windows Terminal 的伪控制台窗口 ⇒ 终端窗口）；
/// 没有 ⇒ 它自己（经典控制台）。那个窗口不可见 ⇒ 不算（隐藏的控制台不在任何人眼前）。读法是参数。
/// 调用方只在 Windows 上（下面那一份）；判据在各平台喂替身判它。
#[cfg(any(windows, test))]
pub fn window_of_console(
    console: Option<isize>,
    owner_of: impl Fn(isize) -> isize,
    visible: impl Fn(isize) -> bool,
) -> Option<isize> {
    let c = console.filter(|&c| c != 0)?;
    let w = match owner_of(c) {
        0 => c,
        o => o,
    };
    visible(w).then_some(w)
}

/// `pid` 的控制台此刻显示在哪个窗口（规则见 [`window_of_console`]）。借不到那个控制台（进程没了 · 没有控制台 · 本进程自己挂着
/// 控制台 · 系统不给）⇒ `None`。挂着期间不响应 Ctrl+C（那个控制台里有人按 Ctrl+C 时别连累本进程）。
#[cfg(windows)]
pub fn console_window(pid: u32) -> Option<isize> {
    use std::sync::Mutex;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, IsWindowVisible, GW_OWNER};
    static LENDING: Mutex<()> = Mutex::new(());
    let _one = LENDING.lock().unwrap_or_else(|e| e.into_inner());

    // 挂控制台会把本进程空着的三个标准句柄换成那个控制台的，摘下后它们就失效了 ⇒ 挂之前记下、摘下之后放回。
    // 三个标准句柄的编号（Win32 `STD_INPUT_HANDLE` · `STD_OUTPUT_HANDLE` · `STD_ERROR_HANDLE`：-10 · -11 · -12）。
    let std_ids: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32];
    // SAFETY：挂上之后只调一个取句柄的、立刻摘下；Ctrl+C 忽略只在挂着的这一小段；标准句柄原样放回。
    let console = unsafe {
        let saved = std_ids.map(|n| GetStdHandle(n));
        if AttachConsole(pid) == 0 {
            None
        } else {
            SetConsoleCtrlHandler(None, 1);
            let h = GetConsoleWindow();
            FreeConsole();
            SetConsoleCtrlHandler(None, 0);
            for (n, old) in std_ids.into_iter().zip(saved) {
                SetStdHandle(n, old);
            }
            Some(h)
        }
    };
    window_of_console(
        console,
        // SAFETY：只读窗口属性；句柄失效时两个调用各回 0 / false。
        |h| unsafe { GetWindow(HWND(h), GW_OWNER) }.0,
        |h| unsafe { IsWindowVisible(HWND(h)) }.as_bool(),
    )
}

/// 别处没有「别的进程的控制台」这回事。
#[cfg(not(windows))]
pub fn console_window(_pid: u32) -> Option<isize> {
    None
}

/// 这台有没有「本机 bash / zsh 接入块留终端记录、monitor 往那个终端写改标题序列认窗口」那一套（`bind.rs::BindRegistry`）：只有 Linux。
pub const TTY_RECORDS: bool = cfg!(target_os = "linux");

/// 改终端标题的那三下（xterm 控制序列；终端程序不认标题栈时入栈 / 出栈那两下它不理，标题就留到下一个提示符自己改回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtyTitle<'a> {
    /// 当前标题入栈（`CSI 22 ; 2 t`）。
    Push,
    /// 设标题（`OSC 2 ; … BEL`）。
    Set(&'a str),
    /// 出栈还原（`CSI 23 ; 2 t`）。
    Pop,
}

/// 那一下写进终端的字节（控制字符从标题里摘掉，免得把序列提前截断）。
pub fn tty_title_bytes(op: TtyTitle) -> Vec<u8> {
    match op {
        TtyTitle::Push => b"\x1b[22;2t".to_vec(),
        TtyTitle::Pop => b"\x1b[23;2t".to_vec(),
        TtyTitle::Set(t) => {
            let clean: String = t.chars().filter(|c| !c.is_control()).collect();
            format!("\x1b]2;{clean}\x07").into_bytes()
        }
    }
}

/// 往终端设备 `tty`（`/dev/pts/N` 一类）写那一下；打不开 / 写不进 ⇒ `false`。不把它认作本进程的控制终端（`O_NOCTTY`）。
#[cfg(target_os = "linux")]
pub fn tty_title(tty: &str, op: TtyTitle) -> bool {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    const O_NOCTTY: i32 = 0o400;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(O_NOCTTY)
        .open(tty)
        .and_then(|mut f| f.write_all(&tty_title_bytes(op)))
        .is_ok()
}

#[cfg(not(target_os = "linux"))]
pub fn tty_title(_tty: &str, _op: TtyTitle) -> bool {
    false
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(pid: u32) -> i32;
    fn FreeConsole() -> i32;
    fn GetConsoleWindow() -> isize;
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
    fn GetStdHandle(which: u32) -> isize;
    fn SetStdHandle(which: u32, handle: isize) -> i32;
}
