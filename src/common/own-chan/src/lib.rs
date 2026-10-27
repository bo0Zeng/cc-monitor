//! **本人通道**：两头都是我们、又不是父子的那几条连接（本机壳 ↔ 本机常驻后端 · 远端那台的小中继 ↔ 那台常驻后端），
//! 只走只有本账号连得上的系统通道，不发钥匙。后端 · monitor 同一份（契约：两边对上同一种通道）。
//!
//! 门由内核给：
//! - **Unix**：套接字住只给本人的目录（`0700`，调用方建；本 crate 不写盘），收下的每条连接再核一次对端 uid 与本进程相同
//!   （tokio `peer_cred`：Linux `SO_PEERCRED` · macOS `getpeereid`）。独占：在听的那一个攥着那个目录的
//!   `flock(LOCK_EX | LOCK_NB)`（只读打开目录，不建锁文件）；抢不到 = 已有一个在听。进程没了内核替它放锁 ⇒ 不留陈旧锁；
//!   陈旧的套接字文件由拿到锁的那一个删掉重绑（删是调用方的事）。
//! - **Windows**：命名管道。管道名由那个套接字路径算出（同一个家 ⇒ 同一个名，`\\.\pipe\ccm-own-<散列>`；盘上不落文件）；
//!   绑的那一下要「第一个实例」（名字已被别人占着 ⇒ 绑不上）、只收本机客户端；收下的每条连接按对端进程的用户 SID 核是不是本人。
//!   客户端连时只许对端认身份、不许冒用（`SECURITY_IDENTIFICATION`）。独占：在听的那一个攥着一个由目录路径算出名字的互斥体，
//!   建的时候已有 = 已有一个在听；进程没了系统替它关句柄 ⇒ 不留陈旧锁。
//!
//! 被拒时说什么不在这里：本 crate 只回结构化的结局（[`Accepted`] · `io::ErrorKind`），各调用方按它挑文案表里的那一句。

use std::path::Path;

/// 这个平台的通道是不是盘上的一个文件（Unix 套接字是；Windows 命名管道不是 ⇒ 没有陈旧文件要删、也没有路径长度上限）。
pub const BOUND_AS_FILE: bool = cfg!(unix);

/// 一条连接（异步双向字节流）。
#[cfg(unix)]
pub type Stream = tokio::net::UnixStream;
/// 一条连接（异步双向字节流）：命名管道的服务端那一头或客户端那一头。
#[cfg(windows)]
pub use win::Stream;

/// 攥着的那把目录锁。落地即放（进程没了内核也替它放）。
pub struct Held {
    #[cfg(unix)]
    _dir: std::fs::File,
    #[cfg(windows)]
    _mutex: win::Handle,
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

/// 抢那个目录的独占锁（Windows：由目录路径算出名字的互斥体，建的时候已有 ⇒ 别人攥着）。
#[cfg(windows)]
pub fn try_hold(dir: &Path) -> std::io::Result<Option<Held>> {
    Ok(win::try_hold(dir)?.map(|h| Held { _mutex: h }))
}

/// 在听的那个通道。
pub struct Listener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    inner: win::Listener,
}

/// 绑那个套接字（调用方已攥着锁、已删掉陈旧文件、目录只给本人）。要在 tokio 运行时里调。
#[cfg(unix)]
pub fn bind(path: &Path) -> std::io::Result<Listener> {
    Ok(Listener {
        inner: tokio::net::UnixListener::bind(path)?,
    })
}

/// 绑那个通道（Windows：那个路径算出的管道名，要「第一个实例」；名字被别人占着 ⇒ 绑不上）。要在 tokio 运行时里调。
#[cfg(windows)]
pub fn bind(path: &Path) -> std::io::Result<Listener> {
    Ok(Listener {
        inner: win::Listener::bind(path)?,
    })
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

    /// 收下一条连接，并按对端进程的用户 SID 核是不是本人（不是 / 问不出来 ⇒ `Foreign(None)`：Windows 上没有 uid 这个数）。
    #[cfg(windows)]
    pub async fn accept(&self) -> std::io::Result<Accepted> {
        let (s, ours) = self.inner.accept().await?;
        Ok(if ours {
            Accepted::Ours(s)
        } else {
            Accepted::Foreign(None)
        })
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

/// 连那个通道（Windows：管道全忙时稍等再连，至多 [`win::BUSY_WAIT_MS`] 毫秒）。要在 tokio 运行时里调。
#[cfg(windows)]
pub async fn connect(path: &Path) -> std::io::Result<Stream> {
    win::connect(path).await
}

/// 同步那一形的一条连接：握手那几行用阻塞读写 ＋ 期限读完，然后 [`BlockingStream::into_async`] 交给运行时。
#[cfg(unix)]
pub struct BlockingStream(std::os::unix::net::UnixStream);

/// 同步那一形的一条连接（Windows：重叠方式开的管道句柄，读写各带一个期限）。
#[cfg(windows)]
pub struct BlockingStream(win::Blocking);

/// 连那个通道（同步）。断开的 `ErrorKind`（`NotFound` · `ConnectionRefused` = 没人在听；`PermissionDenied` = 不归本人）交调用方挑说法。
#[cfg(unix)]
pub fn connect_blocking(path: &Path) -> std::io::Result<BlockingStream> {
    std::os::unix::net::UnixStream::connect(path).map(BlockingStream)
}

/// 连那个通道（同步；Windows：没人在听 ⇒ `NotFound`；管道全忙 ⇒ 等至多 [`win::BUSY_WAIT_MS`] 毫秒再连）。
#[cfg(windows)]
pub fn connect_blocking(path: &Path) -> std::io::Result<BlockingStream> {
    win::Blocking::connect(path).map(BlockingStream)
}

#[cfg(unix)]
impl BlockingStream {
    /// 对端是谁（内核给，不靠盘上的记录）：`(uid, pid)`。Linux `SO_PEERCRED`；macOS `getpeereid` ＋ `LOCAL_PEERPID`；
    /// 别的 Unix 拿不到 pid ⇒ `Unsupported`。
    pub fn peer(&self) -> std::io::Result<(u32, u32)> {
        use std::os::unix::io::AsRawFd;
        peer_of(self.0.as_raw_fd())
    }

    /// 在听的那一位的 pid（内核给，不靠盘上的记录）。两个平台都有这一问。
    pub fn peer_pid(&self) -> std::io::Result<u32> {
        self.peer().map(|(_, pid)| pid)
    }

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

#[cfg(target_os = "linux")]
fn peer_of(fd: std::os::unix::io::RawFd) -> std::io::Result<(u32, u32)> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `fd` 是活着的套接字；`cred` / `len` 是本栈上的、大小与 `SO_PEERCRED` 要的一致。
    let rc = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    u32::try_from(cred.pid)
        .ok()
        .filter(|p| *p != 0)
        .map(|pid| (cred.uid, pid))
        .ok_or_else(|| std::io::ErrorKind::NotFound.into())
}

#[cfg(target_os = "macos")]
fn peer_of(fd: std::os::unix::io::RawFd) -> std::io::Result<(u32, u32)> {
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: `fd` 是活着的套接字；两个出参在本栈上。
    if unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: 同上；`LOCAL_PEERPID` 回一个 `pid_t`。
    let rc = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&mut pid as *mut libc::pid_t).cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    u32::try_from(pid)
        .ok()
        .filter(|p| *p != 0)
        .map(|pid| (uid, pid))
        .ok_or_else(|| std::io::ErrorKind::NotFound.into())
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn peer_of(_fd: std::os::unix::io::RawFd) -> std::io::Result<(u32, u32)> {
    Err(std::io::ErrorKind::Unsupported.into())
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

#[cfg(windows)]
impl BlockingStream {
    /// 在听的那一位的 pid（`GetNamedPipeServerProcessId`：系统给，不靠盘上的记录）。
    pub fn peer_pid(&self) -> std::io::Result<u32> {
        self.0.server_pid()
    }
    /// 这一次阻塞读最多等多久（到点没字节 ⇒ `TimedOut`，那一次读被撤掉）。
    pub fn set_read_timeout(&self, d: Option<std::time::Duration>) -> std::io::Result<()> {
        self.0.set_read_timeout(d);
        Ok(())
    }
    /// 这一次阻塞写最多等多久。
    pub fn set_write_timeout(&self, d: Option<std::time::Duration>) -> std::io::Result<()> {
        self.0.set_write_timeout(d);
        Ok(())
    }
    /// 握手完了：交给运行时（要在 tokio 运行时里调；句柄本来就是重叠方式开的）。
    pub fn into_async(self) -> std::io::Result<Stream> {
        self.0.into_async()
    }
}

#[cfg(windows)]
impl std::io::Read for &BlockingStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

#[cfg(windows)]
impl std::io::Write for &BlockingStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Windows 那一臂：命名管道 ＋ 几个 `kernel32` / `advapi32` 函数（手写声明，不为它们加依赖）。
#[cfg(windows)]
mod win {
    use std::io;
    use std::path::Path;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    type Raw = isize;

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: Raw,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(attrs: *const core::ffi::c_void, owner: i32, name: *const u16) -> Raw;
        fn CloseHandle(h: Raw) -> i32;
        fn GetLastError() -> u32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            attrs: *const core::ffi::c_void,
            disposition: u32,
            flags: u32,
            template: Raw,
        ) -> Raw;
        fn WaitNamedPipeW(name: *const u16, timeout_ms: u32) -> i32;
        fn CreateEventW(
            attrs: *const core::ffi::c_void,
            manual: i32,
            initial: i32,
            name: *const u16,
        ) -> Raw;
        fn ReadFile(h: Raw, buf: *mut u8, len: u32, read: *mut u32, ov: *mut Overlapped) -> i32;
        fn WriteFile(h: Raw, buf: *const u8, len: u32, wrote: *mut u32, ov: *mut Overlapped)
            -> i32;
        fn GetOverlappedResult(h: Raw, ov: *mut Overlapped, n: *mut u32, wait: i32) -> i32;
        fn WaitForSingleObject(h: Raw, ms: u32) -> u32;
        fn CancelIoEx(h: Raw, ov: *mut Overlapped) -> i32;
        fn GetNamedPipeServerProcessId(h: Raw, pid: *mut u32) -> i32;
        fn GetNamedPipeClientProcessId(h: Raw, pid: *mut u32) -> i32;
        fn GetCurrentProcess() -> Raw;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Raw;
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: Raw, access: u32, token: *mut Raw) -> i32;
        fn GetTokenInformation(
            token: Raw,
            class: u32,
            info: *mut core::ffi::c_void,
            len: u32,
            ret: *mut u32,
        ) -> i32;
        fn EqualSid(a: *const core::ffi::c_void, b: *const core::ffi::c_void) -> i32;
    }

    const ERROR_ALREADY_EXISTS: u32 = 183;
    const ERROR_PIPE_BUSY: i32 = 231;
    const ERROR_IO_PENDING: i32 = 997;
    const ERROR_BROKEN_PIPE: i32 = 109;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
    const SECURITY_SQOS_PRESENT: u32 = 0x0010_0000;
    /// `SECURITY_IDENTIFICATION`（`SecurityIdentification << 16`）：对端能认出我是谁，不能冒用我。
    const SECURITY_IDENTIFICATION: u32 = 0x0001_0000;
    const WAIT_TIMEOUT: u32 = 258;
    const INFINITE: u32 = u32::MAX;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const TOKEN_QUERY: u32 = 0x8;
    /// `TOKEN_INFORMATION_CLASS::TokenUser`。
    const TOKEN_USER_CLASS: u32 = 1;

    /// 管道全忙（每个实例都连着人）时最多等多久再连：服务端收下一条连接就补一个实例，这一刻很短。
    pub const BUSY_WAIT_MS: u32 = 2_000;

    /// 一个内核句柄，落地即关。
    pub struct Handle(Raw);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY：句柄是本类型独占的那一个，只关这一次。
            unsafe { CloseHandle(self.0) };
        }
    }
    // SAFETY：内核句柄可以跨线程用、跨线程关。
    unsafe impl Send for Handle {}
    // SAFETY：本类型只读句柄值，不经它改内核对象的状态（读写各自带重叠结构）。
    unsafe impl Sync for Handle {}

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 路径写进名字的那一截（不散列，原样可读）：不分大小写（Windows 的路径不分）、两种分隔符都换成 `|`
    /// （管道名与内核对象名里唯一不许的字符是 `\`）。
    fn path_key(p: &Path) -> String {
        p.to_string_lossy()
            .chars()
            .flat_map(char::to_lowercase)
            .map(|c| if c == '/' || c == '\\' { '|' } else { c })
            .collect()
    }

    /// 名字里路径那一截的长度上限（管道名 256、内核对象名 260 个字符，留出前缀）。
    /// 路径长到写不进 ⇒ 不绑 / 不锁 / 不连，回 `InvalidInput`（不截断：截了就是另一个名字）。
    const KEY_MAX: usize = 230;

    fn key_of(p: &Path) -> io::Result<String> {
        let k = path_key(p);
        if k.chars().count() > KEY_MAX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("path longer than {KEY_MAX} characters"),
            ));
        }
        Ok(k)
    }

    /// 那个套接字路径对应的管道名。
    fn pipe_name(p: &Path) -> io::Result<String> {
        Ok(format!(r"\\.\pipe\ccm-own-{}", key_of(p)?))
    }

    pub fn try_hold(dir: &Path) -> io::Result<Option<Handle>> {
        let name = wide(&format!("ccm-own-lock-{}", key_of(dir)?));
        // SAFETY：名字以 0 结尾；不要初始持有（持有与否不要紧：在不在才是那把锁）。
        let h = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        // SAFETY：紧跟着 `CreateMutexW` 读它留下的错号。
        let last = unsafe { GetLastError() };
        if h == 0 {
            return Err(io::Error::last_os_error());
        }
        let h = Handle(h);
        if last == ERROR_ALREADY_EXISTS {
            return Ok(None);
        }
        Ok(Some(h))
    }

    /// 本进程（或 `pid` 那个进程）令牌里的用户 SID，原样那几个字节（`TOKEN_USER` 整份，SID 指针指在它里面）。
    fn token_user(process: Raw) -> io::Result<Vec<u8>> {
        let mut tok: Raw = 0;
        // SAFETY：`process` 是活着的进程句柄（或当前进程的伪句柄）；`tok` 由系统填、下面关掉。
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut tok) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let tok = Handle(tok);
        let mut len: u32 = 0;
        // SAFETY：先问要多大（这一次回失败是常态）。
        unsafe { GetTokenInformation(tok.0, TOKEN_USER_CLASS, std::ptr::null_mut(), 0, &mut len) };
        let mut buf = vec![0u8; len as usize];
        // SAFETY：`buf` 恰好 `len` 字节、活过这一次调用。
        if len == 0
            || unsafe {
                GetTokenInformation(
                    tok.0,
                    TOKEN_USER_CLASS,
                    buf.as_mut_ptr().cast(),
                    len,
                    &mut len,
                )
            } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(buf)
    }

    /// `TOKEN_USER` 开头那个 SID 指针。
    fn sid_of(token_user: &[u8]) -> *const core::ffi::c_void {
        let mut p = [0u8; std::mem::size_of::<usize>()];
        p.copy_from_slice(&token_user[..std::mem::size_of::<usize>()]);
        usize::from_ne_bytes(p) as *const core::ffi::c_void
    }

    /// `pid` 那个进程是不是与本进程同一个用户（问不出来 ⇒ 不是）。
    fn same_user(pid: u32) -> bool {
        // SAFETY：只问查询那一项最小权限；句柄下面关掉。
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if h == 0 {
            return false;
        }
        let h = Handle(h);
        // SAFETY：当前进程的伪句柄不用关。
        let (Ok(theirs), Ok(mine)) = (token_user(h.0), token_user(unsafe { GetCurrentProcess() }))
        else {
            return false;
        };
        // SAFETY：两个 SID 指针都指在各自活着的缓冲区里。
        unsafe { EqualSid(sid_of(&theirs), sid_of(&mine)) != 0 }
    }

    /// 一条连接：服务端那一头（常驻后端收下的）或客户端那一头（连过去的）。
    pub enum Stream {
        Server(NamedPipeServer),
        Client(NamedPipeClient),
    }

    impl AsyncRead for Stream {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Stream::Server(s) => Pin::new(s).poll_read(cx, buf),
                Stream::Client(c) => Pin::new(c).poll_read(cx, buf),
            }
        }
    }

    impl AsyncWrite for Stream {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            match self.get_mut() {
                Stream::Server(s) => Pin::new(s).poll_write(cx, buf),
                Stream::Client(c) => Pin::new(c).poll_write(cx, buf),
            }
        }
        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Stream::Server(s) => Pin::new(s).poll_flush(cx),
                Stream::Client(c) => Pin::new(c).poll_flush(cx),
            }
        }
        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Stream::Server(s) => Pin::new(s).poll_shutdown(cx),
                Stream::Client(c) => Pin::new(c).poll_shutdown(cx),
            }
        }
    }

    /// 在听的那个管道：手里永远攥着一个还没人连的实例（收下一条就补一个）。
    pub struct Listener {
        name: String,
        next: std::sync::Mutex<Option<NamedPipeServer>>,
    }

    fn instance(name: &str, first: bool) -> io::Result<NamedPipeServer> {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create(name)
    }

    impl Listener {
        pub fn bind(path: &Path) -> io::Result<Listener> {
            let name = pipe_name(path)?;
            let first = instance(&name, true)?;
            Ok(Listener {
                name,
                next: std::sync::Mutex::new(Some(first)),
            })
        }

        /// 收下一条：`(那条连接, 对端是不是本人)`。
        pub async fn accept(&self) -> io::Result<(Stream, bool)> {
            let taken = self.next.lock().unwrap_or_else(|e| e.into_inner()).take();
            let server = match taken {
                Some(s) => s,
                None => instance(&self.name, false)?,
            };
            server.connect().await?;
            // 连上了 ⇒ 先补一个空着的实例，后来的人才连得上。
            let fresh = instance(&self.name, false)?;
            *self.next.lock().unwrap_or_else(|e| e.into_inner()) = Some(fresh);
            use std::os::windows::io::AsRawHandle as _;
            let mut pid: u32 = 0;
            // SAFETY：句柄属于活着的 `server`；`pid` 在本栈上。
            let ok =
                unsafe { GetNamedPipeClientProcessId(server.as_raw_handle() as Raw, &mut pid) }
                    != 0;
            let ours = ok && same_user(pid);
            Ok((Stream::Server(server), ours))
        }
    }

    pub async fn connect(path: &Path) -> io::Result<Stream> {
        let name = pipe_name(path)?;
        let mut waited = 0u32;
        loop {
            match ClientOptions::new()
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(&name)
            {
                Ok(c) => {
                    use std::os::windows::io::AsRawHandle as _;
                    let mut pid: u32 = 0;
                    // SAFETY：句柄属于活着的 `c`；`pid` 在本栈上。
                    let ok =
                        unsafe { GetNamedPipeServerProcessId(c.as_raw_handle() as Raw, &mut pid) }
                            != 0;
                    // 在听的那一位不是本人 ⇒ 不归本人（见同步那一形）。
                    return if ok && same_user(pid) {
                        Ok(Stream::Client(c))
                    } else {
                        Err(io::ErrorKind::PermissionDenied.into())
                    };
                }
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && waited < BUSY_WAIT_MS => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    waited += 20;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// 同步那一形：重叠方式开的客户端句柄 ＋ 读写各一个期限（毫秒；`INFINITE` = 不限）。
    pub struct Blocking {
        h: Option<Handle>,
        read_ms: AtomicU32,
        write_ms: AtomicU32,
    }

    fn ms_of(d: Option<std::time::Duration>) -> u32 {
        d.map_or(INFINITE, |d| {
            u32::try_from(d.as_millis()).unwrap_or(INFINITE - 1).max(1)
        })
    }

    impl Blocking {
        pub fn connect(path: &Path) -> io::Result<Blocking> {
            let name = wide(&pipe_name(path)?);
            let mut waited = false;
            loop {
                // SAFETY：名字以 0 结尾；其余入参是常量。
                let h = unsafe {
                    CreateFileW(
                        name.as_ptr(),
                        GENERIC_READ | GENERIC_WRITE,
                        0,
                        std::ptr::null(),
                        OPEN_EXISTING,
                        FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                        0,
                    )
                };
                if h != -1 {
                    let b = Blocking {
                        h: Some(Handle(h)),
                        read_ms: AtomicU32::new(INFINITE),
                        write_ms: AtomicU32::new(INFINITE),
                    };
                    // 管道名谁都能抢先建：在听的那一位不是本人 ⇒ 不归本人（同 Unix 那一臂目录不归本人时的 `PermissionDenied`）。
                    return if b.server_pid().is_ok_and(same_user) {
                        Ok(b)
                    } else {
                        Err(io::ErrorKind::PermissionDenied.into())
                    };
                }
                let e = io::Error::last_os_error();
                if e.raw_os_error() != Some(ERROR_PIPE_BUSY) || waited {
                    return Err(e);
                }
                waited = true;
                // SAFETY：名字以 0 结尾。
                unsafe { WaitNamedPipeW(name.as_ptr(), BUSY_WAIT_MS) };
            }
        }

        fn raw(&self) -> Raw {
            self.h.as_ref().map_or(0, |h| h.0)
        }

        pub fn server_pid(&self) -> io::Result<u32> {
            let mut pid: u32 = 0;
            // SAFETY：句柄活着；`pid` 在本栈上。
            if unsafe { GetNamedPipeServerProcessId(self.raw(), &mut pid) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(pid)
        }

        pub fn set_read_timeout(&self, d: Option<std::time::Duration>) {
            self.read_ms.store(ms_of(d), Ordering::Relaxed);
        }
        pub fn set_write_timeout(&self, d: Option<std::time::Duration>) {
            self.write_ms.store(ms_of(d), Ordering::Relaxed);
        }

        /// 一次重叠读 / 写，至多等 `ms`；到点 ⇒ 撤掉那一次、回 `TimedOut`。对端关了 ⇒ 读回 0（同 EOF）。
        fn io_once(
            &self,
            ms: u32,
            f: impl FnOnce(Raw, *mut Overlapped) -> i32,
        ) -> io::Result<usize> {
            // SAFETY：手动复位、初始未发；下面关掉。
            let ev = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
            if ev == 0 {
                return Err(io::Error::last_os_error());
            }
            let ev = Handle(ev);
            let mut ov = Overlapped {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                event: ev.0,
            };
            let h = self.raw();
            if f(h, &mut ov) == 0 {
                let e = io::Error::last_os_error();
                match e.raw_os_error() {
                    Some(ERROR_IO_PENDING) => {}
                    Some(ERROR_BROKEN_PIPE) => return Ok(0),
                    _ => return Err(e),
                }
                // SAFETY：事件句柄活着。
                if unsafe { WaitForSingleObject(ev.0, ms) } == WAIT_TIMEOUT {
                    let mut n = 0u32;
                    // SAFETY：撤掉这一次，再等它真结束（结构与缓冲区活到它结束为止）。
                    unsafe {
                        CancelIoEx(h, &mut ov);
                        GetOverlappedResult(h, &mut ov, &mut n, 1);
                    }
                    return Err(io::ErrorKind::TimedOut.into());
                }
            }
            let mut n = 0u32;
            // SAFETY：这一次已经结束（或同步完成），只取结果。
            if unsafe { GetOverlappedResult(h, &mut ov, &mut n, 1) } == 0 {
                let e = io::Error::last_os_error();
                return if e.raw_os_error() == Some(ERROR_BROKEN_PIPE) {
                    Ok(0)
                } else {
                    Err(e)
                };
            }
            Ok(n as usize)
        }

        pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
            let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
            let p = buf.as_mut_ptr();
            // SAFETY：`buf` 活过 `io_once` 整段（它等到那一次结束才回）。
            self.io_once(self.read_ms.load(Ordering::Relaxed), |h, ov| unsafe {
                ReadFile(h, p, len, std::ptr::null_mut(), ov)
            })
        }

        pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
            let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
            let p = buf.as_ptr();
            // SAFETY：同上。
            self.io_once(self.write_ms.load(Ordering::Relaxed), |h, ov| unsafe {
                WriteFile(h, p, len, std::ptr::null_mut(), ov)
            })
        }

        pub fn into_async(mut self) -> io::Result<Stream> {
            let Some(h) = self.h.take() else {
                return Err(io::ErrorKind::NotConnected.into());
            };
            let raw = h.0;
            std::mem::forget(h);
            // SAFETY：句柄是重叠方式开的命名管道客户端句柄，所有权就此交给它。
            let c = unsafe {
                NamedPipeClient::from_raw_handle(raw as std::os::windows::io::RawHandle)
            }?;
            Ok(Stream::Client(c))
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/own-chan/lib_tests.rs"]
mod tests;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
