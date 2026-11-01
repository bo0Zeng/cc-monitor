//! 瘦客户端（`host/managed/attach.rs`）跑在用户的真终端里：那个外层终端的原始模式 · 窗口尺寸 · 送进来的键与尺寸变了。
//!
//! - Unix：termios · `TIOCGWINSZ`；键按字节读 stdin，尺寸变了是 SIGWINCH。
//! - Windows：控制台模式（`SetConsoleMode`：输入关行缓冲 · 回显 · Ctrl+C 变信号，开虚拟终端输入与窗口事件；输出开虚拟终端处理）·
//!   `GetConsoleScreenBufferInfo` 的可见窗口；键与尺寸变了都从控制台的输入事件队列来（`ReadConsoleInputW`：键事件带着虚拟终端那一串字符，
//!   尺寸事件另是一种）—— 按字节读 stdin 会把尺寸事件丢掉，所以 Windows 上一条线读事件、两样各交各的。

/// 原始模式的守卫：落地即还原进来之前的那一份设置。
pub struct Raw {
    #[cfg(unix)]
    saved: libc::termios,
    /// 输入 · 输出两个句柄进来之前的控制台模式。
    #[cfg(windows)]
    saved: (u32, u32),
}

/// 外层终端送进来的：键（原样字节）· 尺寸变了（一次一条）。
pub struct Input {
    pub keys: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    pub resized: tokio::sync::mpsc::UnboundedReceiver<()>,
}

/// stdin 是不是一个终端。
pub fn stdin_is_terminal() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal()
}

#[cfg(unix)]
mod os {
    use super::{Input, Raw};

    pub(super) fn enter_raw() -> std::io::Result<Raw> {
        // SAFETY: fd 0 在本进程活着；`termios` 是本栈上的、大小与系统要的一致。
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut t) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let saved = t;
            libc::cfmakeraw(&mut t);
            if libc::tcsetattr(0, libc::TCSANOW, &t) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Raw { saved })
        }
    }

    pub(super) fn leave_raw(r: &Raw) {
        // SAFETY: 同上；还原的是进来之前读到的那一份。
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &r.saved);
        }
    }

    pub(super) fn size() -> Option<(u16, u16)> {
        for fd in [1, 0, 2] {
            // SAFETY: `winsize` 在本栈上；问的是本进程的标准描述符。
            let mut w: libc::winsize = unsafe { std::mem::zeroed() };
            if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut w) } == 0
                && w.ws_row > 0
                && w.ws_col > 0
            {
                return Some((w.ws_row, w.ws_col));
            }
        }
        None
    }

    pub(super) fn input() -> std::io::Result<Input> {
        let mut sig =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())?;
        let (rtx, resized) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while sig.recv().await.is_some() {
                if rtx.send(()).is_err() {
                    return;
                }
            }
        });
        let (ktx, keys) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        std::thread::spawn(move || {
            use std::io::Read as _;
            let mut stdin = std::io::stdin().lock();
            let mut buf = [0u8; 4096];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        if ktx.send(buf[..n].to_vec()).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Ok(Input { keys, resized })
    }
}

/// 控制台输入事件 ⇒ 键的字节（Windows 那一臂用；判据在哪个平台上都测）。
#[cfg(any(windows, test))]
pub(crate) mod console_keys {
    /// 一条控制台输入事件里我们认的那几样。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Event {
        /// 键事件：按下 / 抬起、重复几次、带的那个 UTF-16 码元（0 ＝ 不带字，如只按了 Shift）。
        Key { down: bool, repeat: u16, unit: u16 },
        /// 窗口尺寸变了。
        Resized,
        /// 别的（鼠标 · 焦点 · 菜单）：不管。
        Other,
    }

    /// 一批解出来的：键的字节（UTF-8）· 这一批里尺寸变没变。
    #[derive(Debug, Default, PartialEq, Eq)]
    pub(crate) struct Decoded {
        pub(crate) bytes: Vec<u8>,
        pub(crate) resized: bool,
    }

    /// 解码器：只认按下的键；代理对的前半个留到下一个码元来再拼（可能跨两批）。
    #[derive(Default)]
    pub(crate) struct Decoder {
        high: Option<u16>,
    }

    impl Decoder {
        pub(crate) fn feed(&mut self, events: &[Event]) -> Decoded {
            let mut out = Decoded::default();
            for e in events {
                match *e {
                    Event::Key {
                        down: true,
                        repeat,
                        unit,
                    } if unit != 0 => {
                        let Some(c) = self.char_of(unit) else {
                            continue;
                        };
                        let mut b = [0u8; 4];
                        let s = c.encode_utf8(&mut b).as_bytes();
                        for _ in 0..repeat.max(1) {
                            out.bytes.extend_from_slice(s);
                        }
                    }
                    Event::Resized => out.resized = true,
                    Event::Key { .. } | Event::Other => {}
                }
            }
            out
        }

        fn char_of(&mut self, unit: u16) -> Option<char> {
            match unit {
                0xD800..=0xDBFF => {
                    self.high = Some(unit);
                    None
                }
                0xDC00..=0xDFFF => {
                    let high = self.high.take()?;
                    char::decode_utf16([high, unit]).next()?.ok()
                }
                _ => {
                    self.high = None;
                    char::from_u32(u32::from(unit))
                }
            }
        }
    }
}

#[cfg(windows)]
#[path = "win_term.rs"]
mod os;

/// 外层终端进原始模式（不回显、不按行、Ctrl+C 不变信号）；交回落地即还原的守卫。
pub fn enter_raw() -> std::io::Result<Raw> {
    os::enter_raw()
}

impl Drop for Raw {
    fn drop(&mut self) {
        os::leave_raw(self);
    }
}

/// 外层终端此刻的尺寸（行 · 列）；问不出 ⇒ `None`。
pub fn size() -> Option<(u16, u16)> {
    os::size()
}

/// 外层终端送进来的键与尺寸变了（要在 tokio 运行时里调；读的那条线一直等到有东西，不是定时器）。
pub fn input() -> std::io::Result<Input> {
    os::input()
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/term_tests.rs"]
mod tests;
