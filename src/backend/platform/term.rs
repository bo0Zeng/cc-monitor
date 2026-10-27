//! 瘦客户端（`host/managed/attach.rs`）跑在用户的真终端里：那个外层终端的原始模式 · 窗口尺寸 · 尺寸变了的信号。
//!
//! Unix 一臂做实（termios · `TIOCGWINSZ` · SIGWINCH）；Windows 一臂一律 `Unsupported`（控制台那一套见 A3-3-9）。

/// 原始模式的守卫：落地即还原进来之前的那一份设置。
pub(crate) struct Raw {
    #[cfg(unix)]
    saved: libc::termios,
}

/// stdin 是不是一个终端。
pub(crate) fn stdin_is_terminal() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal()
}

#[cfg(unix)]
mod os {
    use super::Raw;

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

    pub(super) fn resizes() -> std::io::Result<tokio::sync::mpsc::UnboundedReceiver<()>> {
        let mut sig =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())?;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while sig.recv().await.is_some() {
                if tx.send(()).is_err() {
                    return;
                }
            }
        });
        Ok(rx)
    }
}

#[cfg(not(unix))]
mod os {
    use super::Raw;

    fn unsupported() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "managed terminal client: not on this platform yet",
        )
    }

    pub(super) fn enter_raw() -> std::io::Result<Raw> {
        Err(unsupported())
    }

    pub(super) fn leave_raw(_r: &Raw) {}

    pub(super) fn size() -> Option<(u16, u16)> {
        None
    }

    pub(super) fn resizes() -> std::io::Result<tokio::sync::mpsc::UnboundedReceiver<()>> {
        Err(unsupported())
    }
}

/// 外层终端进原始模式（不回显、不按行、Ctrl+C 不变信号）；交回落地即还原的守卫。
pub(crate) fn enter_raw() -> std::io::Result<Raw> {
    os::enter_raw()
}

impl Drop for Raw {
    fn drop(&mut self) {
        os::leave_raw(self);
    }
}

/// 外层终端此刻的尺寸（行 · 列）；问不出 ⇒ `None`。
pub(crate) fn size() -> Option<(u16, u16)> {
    os::size()
}

/// 外层终端尺寸变了的信号，一次一条（要在 tokio 运行时里调）。
pub(crate) fn resizes() -> std::io::Result<tokio::sync::mpsc::UnboundedReceiver<()>> {
    os::resizes()
}
