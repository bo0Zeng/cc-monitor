//! **本人通道**：两头都是我们、又不是父子的那几条连接（本机壳 ↔ 本机常驻后端 · 远端那台的小中继 ↔ 那台常驻后端），
//! 只走只有本账号连得上的系统通道，不发钥匙。后端 · monitor 同一份（契约：两边对上同一种通道）。
//!
//! 门由内核给：
//! - **Unix**：套接字住只给本人的目录（`0700`，调用方建；本 crate 不写盘），收下的每条连接再核一次对端 uid 与本进程相同
//!   （tokio `peer_cred`：Linux `SO_PEERCRED` · macOS `getpeereid`）。独占：在听的那一个攥着那个目录的
//!   `flock(LOCK_EX | LOCK_NB)`（只读打开目录，不建锁文件）；抢不到 = 已有一个在听。进程没了内核替它放锁 ⇒ 不留陈旧锁；
//!   陈旧的套接字文件由拿到锁的那一个删掉重绑（删是调用方的事）。
//! - **Windows**：这一形今天没有调用方（Windows 本机后端由宿主 stdio 监护，远端 Windows 不支持常驻）⇒ 那一臂一律 `Unsupported`，
//!   类型照样在（调用方不写平台分支）。
//!
//! 被拒时说什么不在这里：本 crate 只回结构化的结局（[`Accepted`] · `io::ErrorKind`），各调用方按它挑文案表里的那一句。

use std::path::Path;

/// 一条连接（异步双向字节流）。
#[cfg(unix)]
pub type Stream = tokio::net::UnixStream;
/// 一条连接（异步双向字节流）。非 Unix 上没有调用方，造不出来（[`connect`] 回 `Unsupported`）。
#[cfg(not(unix))]
pub type Stream = tokio::io::DuplexStream;

/// 攥着的那把目录锁。落地即放（进程没了内核也替它放）。
pub struct Held {
    #[cfg(unix)]
    _dir: std::fs::File,
}

/// 抢那个目录的独占锁，不等：`Ok(None)` = 别人攥着（已有一个在听）。目录必须已经在。
#[cfg(unix)]
pub fn try_hold(dir: &Path) -> std::io::Result<Option<Held>> {
    use std::os::unix::io::AsRawFd;
    let f = std::fs::File::open(dir)?;
    loop {
        // SAFETY: `f` 活着、描述有效；`flock` 只读这个整数。
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc == 0 {
            return Ok(Some(Held { _dir: f }));
        }
        let e = std::io::Error::last_os_error();
        match e.raw_os_error() {
            Some(libc::EWOULDBLOCK) => return Ok(None),
            Some(libc::EINTR) => continue,
            _ => return Err(e),
        }
    }
}

/// 抢那个目录的独占锁（非 Unix：没有这一形）。
#[cfg(not(unix))]
pub fn try_hold(_dir: &Path) -> std::io::Result<Option<Held>> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// 在听的那个通道。
pub struct Listener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
}

/// 绑那个套接字（调用方已攥着锁、已删掉陈旧文件、目录只给本人）。要在 tokio 运行时里调。
#[cfg(unix)]
pub fn bind(path: &Path) -> std::io::Result<Listener> {
    Ok(Listener {
        inner: tokio::net::UnixListener::bind(path)?,
    })
}

/// 绑那个通道（非 Unix：没有这一形）。
#[cfg(not(unix))]
pub fn bind(_path: &Path) -> std::io::Result<Listener> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// 收下一条连接的结局。
pub enum Accepted {
    /// 对端与本进程同一个 uid。
    Ours(Stream),
    /// 对端是别的 uid（或问不出来，`None`）⇒ 调用方关掉它、出声。
    Foreign(Option<u32>),
}

impl Listener {
    /// 收下一条连接，并核对端是不是本人。
    #[cfg(unix)]
    pub async fn accept(&self) -> std::io::Result<Accepted> {
        let (s, _) = self.inner.accept().await?;
        Ok(match s.peer_cred().map(|c| c.uid()) {
            Ok(uid) if uid == my_uid() => Accepted::Ours(s),
            Ok(uid) => Accepted::Foreign(Some(uid)),
            Err(_) => Accepted::Foreign(None),
        })
    }

    /// 收下一条连接（非 Unix：造不出 [`Listener`]，走不到这里）。
    #[cfg(not(unix))]
    pub async fn accept(&self) -> std::io::Result<Accepted> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

#[cfg(unix)]
fn my_uid() -> u32 {
    // SAFETY: `getuid` 无副作用、不会失败。
    unsafe { libc::getuid() }
}

/// 连那个通道（异步）。要在 tokio 运行时里调。
#[cfg(unix)]
pub async fn connect(path: &Path) -> std::io::Result<Stream> {
    tokio::net::UnixStream::connect(path).await
}

/// 连那个通道（非 Unix：没有这一形）。
#[cfg(not(unix))]
pub async fn connect(_path: &Path) -> std::io::Result<Stream> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// 同步那一形的一条连接：握手那几行用阻塞读写 ＋ 期限读完，然后 [`BlockingStream::into_async`] 交给运行时。
#[cfg(unix)]
pub struct BlockingStream(std::os::unix::net::UnixStream);

/// 同步那一形的一条连接（非 Unix：造不出来）。
#[cfg(not(unix))]
pub struct BlockingStream(std::convert::Infallible);

/// 连那个通道（同步）。连不上的 `ErrorKind`（`NotFound` · `ConnectionRefused` = 没人在听；`PermissionDenied` = 不归本人）交调用方挑说法。
#[cfg(unix)]
pub fn connect_blocking(path: &Path) -> std::io::Result<BlockingStream> {
    std::os::unix::net::UnixStream::connect(path).map(BlockingStream)
}

/// 连那个通道（同步；非 Unix：没有这一形）。
#[cfg(not(unix))]
pub fn connect_blocking(_path: &Path) -> std::io::Result<BlockingStream> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(unix)]
impl BlockingStream {
    /// 这一次阻塞读最多等多久（不是定时器：有字节就返回）。
    pub fn set_read_timeout(&self, d: Option<std::time::Duration>) -> std::io::Result<()> {
        self.0.set_read_timeout(d)
    }
    /// 这一次阻塞写最多等多久。
    pub fn set_write_timeout(&self, d: Option<std::time::Duration>) -> std::io::Result<()> {
        self.0.set_write_timeout(d)
    }
    /// 握手完了：摘掉期限、换成非阻塞、交给运行时（要在 tokio 运行时里调）。
    pub fn into_async(self) -> std::io::Result<Stream> {
        self.0.set_read_timeout(None)?;
        self.0.set_write_timeout(None)?;
        self.0.set_nonblocking(true)?;
        tokio::net::UnixStream::from_std(self.0)
    }
}

#[cfg(unix)]
impl std::io::Read for &BlockingStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        (&self.0).read(buf)
    }
}

#[cfg(unix)]
impl std::io::Write for &BlockingStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        (&self.0).write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        (&self.0).flush()
    }
}

#[cfg(not(unix))]
impl BlockingStream {
    /// 非 Unix：造不出来。
    pub fn set_read_timeout(&self, _d: Option<std::time::Duration>) -> std::io::Result<()> {
        match self.0 {}
    }
    /// 非 Unix：造不出来。
    pub fn set_write_timeout(&self, _d: Option<std::time::Duration>) -> std::io::Result<()> {
        match self.0 {}
    }
    /// 非 Unix：造不出来。
    pub fn into_async(self) -> std::io::Result<Stream> {
        match self.0 {}
    }
}

#[cfg(not(unix))]
impl std::io::Read for &BlockingStream {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        match self.0 {}
    }
}

#[cfg(not(unix))]
impl std::io::Write for &BlockingStream {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        match self.0 {}
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self.0 {}
    }
}

/// 有没有人在听（连一下：没人 = 立刻 `NotFound` / `ConnectionRefused`，不等）。
pub fn someone_listening(path: &Path) -> bool {
    connect_blocking(path).is_ok()
}

#[cfg(test)]
#[path = "../../../../tests/common/own-chan/lib_tests.rs"]
mod tests;
