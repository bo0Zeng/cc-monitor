//! **常驻后端听的那个 Unix 套接字**（平台原语）：独占锁 · 绑 · 收连接时核对端 uid · 一次性那几条子命令连它。
//!
//! 门由内核给，没有钥匙：套接字住只给本人的目录（`0700`，`relay_route_core::LISTEN_DIR_REL`），
//! 收下的每条连接再核一次对端 uid 与本进程相同（`SO_PEERCRED` / `getpeereid`，tokio 的 `peer_cred`）。
//! 独占：常驻后端活着就攥着那个目录的 `flock(LOCK_EX | LOCK_NB)`（只读打开目录，不建锁文件）；抢不到 = 已有一个在听。
//! 进程没了内核替它放锁 ⇒ 不留陈旧锁；留下的陈旧套接字文件由拿到锁的那一个删掉重绑（删在 `control/resident.rs`，本层不写盘）。
//!
//! 只有 Unix 有常驻这一形（Windows 本机后端由宿主 stdio 监护，远端 Windows 不支持常驻）⇒ 非 Unix 那一臂一律 `Unsupported`。

use std::path::Path;

/// 收下来的一条连接（双向字节流）。
#[cfg(unix)]
pub(crate) type Stream = tokio::net::UnixStream;
#[cfg(not(unix))]
pub(crate) type Stream = tokio::io::DuplexStream;

/// 攥着的那把目录锁。落地即放（进程没了内核也替它放）。
pub(crate) struct Held {
    #[cfg(unix)]
    _dir: std::fs::File,
}

/// 抢那个目录的独占锁，不等：`Ok(None)` = 别人攥着（已有一个常驻后端在听）。目录必须已经在。
#[cfg(unix)]
pub(crate) fn try_hold(dir: &Path) -> std::io::Result<Option<Held>> {
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

#[cfg(not(unix))]
pub(crate) fn try_hold(_dir: &Path) -> std::io::Result<Option<Held>> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// 在听的那个套接字。
pub(crate) struct Listener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
}

/// 绑那个套接字（调用方已攥着锁、已删掉陈旧文件）。
#[cfg(unix)]
pub(crate) fn bind(path: &Path) -> std::io::Result<Listener> {
    Ok(Listener {
        inner: tokio::net::UnixListener::bind(path)?,
    })
}

#[cfg(not(unix))]
pub(crate) fn bind(_path: &Path) -> std::io::Result<Listener> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// 收下一条连接的结局。
pub(crate) enum Accepted {
    /// 对端与本进程同一个 uid。
    Ours(Stream),
    /// 对端是别的 uid（或问不出来）⇒ 调用方关掉它、出声。带对端 uid（问不出 = `None`）。
    Foreign(Option<u32>),
}

impl Listener {
    #[cfg(unix)]
    pub(crate) async fn accept(&self) -> std::io::Result<Accepted> {
        let (s, _) = self.inner.accept().await?;
        let me = super::paths::current_uid();
        Ok(match s.peer_cred().map(|c| c.uid()) {
            Ok(uid) if uid == me => Accepted::Ours(s),
            Ok(uid) => Accepted::Foreign(Some(uid)),
            Err(_) => Accepted::Foreign(None),
        })
    }

    #[cfg(not(unix))]
    pub(crate) async fn accept(&self) -> std::io::Result<Accepted> {
        std::future::pending().await
    }
}

/// 一次性那几条子命令连它（`--resident-ensure` 问「有没有人在听」· `--resident-attach` 中继）。
#[cfg(unix)]
pub(crate) async fn connect(path: &Path) -> std::io::Result<Stream> {
    tokio::net::UnixStream::connect(path).await
}

#[cfg(not(unix))]
pub(crate) async fn connect(_path: &Path) -> std::io::Result<Stream> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// 同步那一形（`--resident-ensure` 在同步路径里问一下有没有人在听）：连得上 = 有人在听。
#[cfg(unix)]
pub(crate) fn someone_listening(path: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(path).is_ok()
}

#[cfg(not(unix))]
pub(crate) fn someone_listening(_path: &Path) -> bool {
    false
}
