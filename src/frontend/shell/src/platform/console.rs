//! 别的进程的控制台这一族的读法。
//!
//! Windows：借 `pid` 的控制台问一句「显示它的是哪个窗口」（Win32 `AttachConsole` → `GetConsoleWindow` → `GetWindow(GW_OWNER)`
//! → `FreeConsole`）。Windows Terminal 里每个标签的控制台是一个伪控制台窗口，它的属主就是承载那个标签的终端窗口（Win11 虚拟机
//! WT 1.24 现打：两个窗口三个标签各自对得上；默认终端交接到 Windows Terminal 的同样对得上）；经典控制台没有属主，控制台窗口
//! 自己就是那个窗口。不要登记、不看是谁起的它。
//!
//! **借控制台只在本二进制的一次性小助手里**（隐藏子命令 `--ccm-console`，[`helper_main`]）：挂控制台是整个进程的状态，
//! 调用方自己挂着控制台时恒失败（`ERROR_ACCESS_DENIED`：开发构建是控制台程序，就是这一形）。小助手起来先摘掉自己的控制台，
//! 逐个借、每个进程印一行就退；monitor 一次 ↗ 起它一趟问整条链（[`console_looks`]），自己从不挂别人的控制台。
//! `AttachConsole` 全仓生产段只这一个调用点（`borrow` 里），判据 `bind_tests.rs` 钉着。
//! 判定（属主 / 自己 / 不可见 / 错误码怎么算）在 [`window_of_console`] · [`refused_by`]，平台无关、判据喂替身。
//!
//! Linux 那一份是 [`tty_title`]：往本机 bash / zsh 接入块记下的那个终端设备写 xterm 的改标题序列，
//! 终端程序据此改它窗口的标题（认窗口用，见 `bind.rs::claim_tty`）。

use std::collections::HashMap;

/// 借一个进程的控制台、问它显示在哪个窗口，读到的四种样子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLook {
    /// 显示在这个可见窗口里。
    Shown(isize),
    /// 借到了，那个控制台不显示在任何可见窗口里（隐藏的控制台 · 不把伪控制台挂到自己窗口下的终端程序）。
    Hidden,
    /// 系统不给借（提权的进程 · 本进程自己挂着控制台）。
    Denied,
    /// 它没有控制台 / 进程没了。
    None,
}

/// `AttachConsole` 失败时的错误码 ⇒ 读到的样子：`ERROR_ACCESS_DENIED`（5）⇒ 不给借；别的（没有控制台 6 · 没有这个进程 87）⇒ 没有。
#[cfg(any(windows, test))]
pub fn refused_by(err: u32) -> ConsoleLook {
    if err == 5 {
        ConsoleLook::Denied
    } else {
        ConsoleLook::None
    }
}

/// 借到的控制台窗口 `console` ⇒ 显示它的那个窗口：有属主 ⇒ 属主（Windows Terminal 的伪控制台窗口 ⇒ 终端窗口）；
/// 没有 ⇒ 它自己（经典控制台）。那个窗口不可见 ⇒ 隐藏（不在任何人眼前）；句柄为 0 ⇒ 没有控制台。读法是参数。
/// 调用方只在 Windows 上（下面那一份）；判据在各平台喂替身判它。
#[cfg(any(windows, test))]
pub fn window_of_console(
    console: isize,
    owner_of: impl Fn(isize) -> isize,
    visible: impl Fn(isize) -> bool,
) -> ConsoleLook {
    if console == 0 {
        return ConsoleLook::None;
    }
    let w = match owner_of(console) {
        0 => console,
        o => o,
    };
    if visible(w) {
        ConsoleLook::Shown(w)
    } else {
        ConsoleLook::Hidden
    }
}

/// 小助手的命令行开头（本二进制的隐藏子命令）。
pub const HELPER_FLAG: &str = "--ccm-console";

/// 起小助手要它做的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperRequest {
    /// 逐个借这几个进程的控制台，各印一行显示在哪个窗口（[`look_line`]）。
    Windows(Vec<u32>),
}

/// 起小助手的那一串参数（不含程序名）。
pub fn helper_argv(req: &HelperRequest) -> Vec<String> {
    let mut v = vec![HELPER_FLAG.to_string()];
    match req {
        HelperRequest::Windows(pids) => {
            v.push("windows".into());
            v.extend(pids.iter().map(u32::to_string));
        }
    }
    v
}

/// 本进程的命令行（含程序名）是不是小助手那一形：紧跟程序名的就是 [`HELPER_FLAG`]、子命令认得、参数都是数。
/// 不是 ⇒ `None`（照常起 monitor）。
pub fn helper_request(args: &[String]) -> Option<HelperRequest> {
    let (flag, rest) = args.get(1..)?.split_first()?;
    if flag != HELPER_FLAG {
        return None;
    }
    let (verb, rest) = rest.split_first()?;
    match verb.as_str() {
        "windows" if !rest.is_empty() => rest
            .iter()
            .map(|p| p.parse::<u32>().ok())
            .collect::<Option<Vec<_>>>()
            .map(HelperRequest::Windows),
        _ => None,
    }
}

/// 小助手为一个进程印的那一行：`<pid> shown 0x<窗口>` · `<pid> hidden` · `<pid> denied` · `<pid> none`。
pub fn look_line(pid: u32, look: ConsoleLook) -> String {
    match look {
        ConsoleLook::Shown(w) => format!("{pid} shown {w:#x}"),
        ConsoleLook::Hidden => format!("{pid} hidden"),
        ConsoleLook::Denied => format!("{pid} denied"),
        ConsoleLook::None => format!("{pid} none"),
    }
}

/// 小助手的输出 ⇒ 每个进程读到的样子（认不出的行不算；小助手没答的进程不在表里）。
pub fn parse_looks(out: &str) -> HashMap<u32, ConsoleLook> {
    out.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let pid = w.next()?.parse::<u32>().ok()?;
            let look = match (w.next()?, w.next(), w.next()) {
                ("shown", Some(h), None) => {
                    ConsoleLook::Shown(isize::from_str_radix(h.strip_prefix("0x")?, 16).ok()?)
                }
                ("hidden", None, None) => ConsoleLook::Hidden,
                ("denied", None, None) => ConsoleLook::Denied,
                ("none", None, None) => ConsoleLook::None,
                _ => return None,
            };
            Some((pid, look))
        })
        .collect()
}

/// 本进程是不是被当成小助手起的：是 ⇒ 做完那件事、回退出码（调用方立刻退出，不起 monitor）；不是 ⇒ `None`。
/// 进程入口（`lib.rs::run`）第一件事就问它。
#[cfg(windows)]
pub fn helper_main() -> Option<i32> {
    let req = helper_request(&std::env::args().collect::<Vec<_>>())?;
    // 摘掉自己的控制台（开发构建是控制台程序，起它时带的 `CREATE_NO_WINDOW` 给了它一个隐藏的）：
    // 自己挂着控制台时借别人的恒被拒。
    // SAFETY：本进程此后不再用控制台；标准输出是 monitor 给的管子，不受影响。
    unsafe {
        FreeConsole();
    }
    match req {
        HelperRequest::Windows(pids) => helper_windows(&pids),
    }
    Some(0)
}

/// 别处没有小助手这回事（照常起 monitor）。
#[cfg(not(windows))]
pub fn helper_main() -> Option<i32> {
    None
}

/// 小助手的 windows 那一件：逐个借、各印一行。
#[cfg(windows)]
fn helper_windows(pids: &[u32]) {
    use std::io::Write;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, IsWindowVisible, GW_OWNER};
    let mut out = String::new();
    for &pid in pids {
        // SAFETY：挂着时只取一个句柄。
        let look = match borrow(pid, || unsafe { GetConsoleWindow() }) {
            Err(e) => refused_by(e),
            Ok(c) => window_of_console(
                c,
                // SAFETY：只读窗口属性；句柄失效时两个调用各回 0 / false。
                |h| unsafe { GetWindow(HWND(h), GW_OWNER) }.0,
                |h| unsafe { IsWindowVisible(HWND(h)) }.as_bool(),
            ),
        };
        out.push_str(&look_line(pid, look));
        out.push('\n');
    }
    let _ = std::io::stdout().write_all(out.as_bytes());
}

/// 借 `pid` 的控制台做 `f`，做完摘下；借不到 ⇒ `AttachConsole` 的错误码。挂着期间不响应 Ctrl+C（那个控制台里有人按
/// Ctrl+C 时别连累本进程）；挂控制台会把本进程的标准句柄换成那个控制台的 ⇒ 挂之前记下、摘下之后放回（小助手的答话
/// 还要从标准输出那根管子出去）。只在小助手里调。
#[cfg(windows)]
fn borrow<T>(pid: u32, f: impl FnOnce() -> T) -> Result<T, u32> {
    // 三个标准句柄的编号（Win32 `STD_INPUT_HANDLE` · `STD_OUTPUT_HANDLE` · `STD_ERROR_HANDLE`：-10 · -11 · -12）。
    let std_ids: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32];
    // SAFETY：挂上 → 做 f → 摘下；Ctrl+C 忽略只在挂着的这一段；标准句柄原样放回。
    unsafe {
        let saved = std_ids.map(|n| GetStdHandle(n));
        if AttachConsole(pid) == 0 {
            return Err(GetLastError());
        }
        SetConsoleCtrlHandler(None, 1);
        let r = f();
        FreeConsole();
        SetConsoleCtrlHandler(None, 0);
        for (n, old) in std_ids.into_iter().zip(saved) {
            SetStdHandle(n, old);
        }
        Ok(r)
    }
}

/// 一次 ↗ 问整条链：每个进程的控制台显示在哪个窗口（读法见 [`ConsoleLooks::of`]）。
pub struct ConsoleLooks(HashMap<u32, ConsoleLook>);

impl ConsoleLooks {
    /// 小助手没答这个进程（没起来 · 超时 · 漏了一行）⇒ 当借不成（被拒）：说「读不到控制台」，不说「没有控制台」。
    pub fn of(&self, pid: u32) -> ConsoleLook {
        self.0.get(&pid).copied().unwrap_or(ConsoleLook::Denied)
    }
}

/// 小助手最多等多久（起进程 ＋ 逐个借，真机几十毫秒）。
#[cfg(windows)]
const HELPER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// 起一趟小助手（本二进制 `--ccm-console windows <pid>…`）问这几个进程的控制台显示在哪个窗口。
#[cfg(windows)]
pub fn console_looks(pids: &[u32]) -> ConsoleLooks {
    ConsoleLooks(
        run_helper(&HelperRequest::Windows(pids.to_vec()))
            .map(|out| parse_looks(&out))
            .unwrap_or_default(),
    )
}

/// 别处没有「别的进程的控制台」这回事。
#[cfg(not(windows))]
pub fn console_looks(_pids: &[u32]) -> ConsoleLooks {
    ConsoleLooks(HashMap::new())
}

/// 起一趟小助手（本二进制，带 [`helper_argv`]），等它答完，回它的标准输出；起不来 / 超时 ⇒ `None`。
#[cfg(windows)]
fn run_helper(req: &HelperRequest) -> Option<String> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    let me = std::env::current_exe().ok()?;
    let argv = helper_argv(req);
    let got = crate::ccm_probe::capture_full(
        HELPER_TIMEOUT,
        &|| {
            let mut c = std::process::Command::new(&me);
            c.args(&argv)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped());
            spawn_managed_cmd(
                &mut c,
                ConsolePolicy::Hidden,
                Lifetime::JobKillOnClose,
                StderrSink::Null,
            )
        },
        None,
    );
    match got {
        Ok(c) => Some(String::from_utf8_lossy(&c.stdout).into_owned()),
        Err(_) => {
            tracing::info!("bind: 借控制台的小助手没答话（{argv:?}）");
            None
        }
    }
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
    fn GetLastError() -> u32;
}
