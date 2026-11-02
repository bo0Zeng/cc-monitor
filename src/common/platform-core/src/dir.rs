//! 自家目录：建的那一下就只给本人 · 拿一个目录的跨进程锁。
//!
//! # 只给本人的目录
//!
//! unix：`DirBuilder` 的 mode 在创建时生效（没有「先按 umask 建出来、再收窄」的那一段）；**已在的不动**（那可能是用户自己设的）。
//! 别的平台照常建（那边不是权限位这一问，目录的 ACL 按父目录继承）。
//!
//! # 目录锁
//!
//! 自家状态每一份都是「读盘 → 改一格 → 旁名 → 原子换」；两个进程（后端两个 · monitor 两个 · 后端与 monitor）同写一份要靠它串起来。
//! 锁的是**那份文件所在的目录**：文件每写一次就被换成一个新 inode，锁文件等于锁一个随时会被换掉的东西；锁目录只要只读打开它。
//!
//! - unix：只读打开目录 ＋ `flock(LOCK_EX)`。锁在打开文件描述上 ⇒ 同一进程两个线程各开一次也互斥；守卫落地即关描述即放锁；
//!   持锁进程被杀，内核替它放。
//! - Windows：以目录路径（ASCII 折小写）的 FNV-1a 命名的互斥量（`Global\` 先、`Local\` 兜底）。互斥量归**线程**所有、同一线程可重入
//!   ⇒ 调用方不许在持锁时再拿同一把（各调用方都是一趟读—改—写，没有嵌套）。🔴 真 Windows 上没跑过（本机只交叉编）。
//!
//! 等锁不是定时器：阻塞等一个条件（别人放锁），不醒来、不驱动循环。持锁段只有一次读—改—写。
//! ⚠ 买不到：一个持锁的进程卡死在读—改—写中间 ⇒ 等它的那一个也跟着等（没有期限）。

use std::io;
use std::path::Path;

/// 自家目录建的那一下给的权限位：只给本人（远端 SFTP 部署建目录用同一个数）。
pub const PRIVATE_DIR_MODE: u32 = 0o700;

/// 建一层（`parents`：缺的上层一并建）；已在不算错。
fn build(dir: &Path, parents: bool) -> io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(parents);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        b.mode(PRIVATE_DIR_MODE);
    }
    match b.create(dir) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        other => other,
    }
}

/// 建**一层**目录，建的那一下只给本人；已在的不动。上层不在 ⇒ 错（调用方逐层核的那一种用它）。
pub fn private_dir(dir: &Path) -> io::Result<()> {
    build(dir, false)
}

/// 建目录，缺的上层一并建，建出来的每一层只给本人；已在的不动。
pub fn private_dir_all(dir: &Path) -> io::Result<()> {
    build(dir, true)
}

/// 拿锁卡在哪一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockStage {
    /// 打开目录（unix）。
    Open,
    /// 上锁（unix `flock` · Windows 建互斥量）。
    Lock,
    /// 等互斥量（Windows）。
    Wait,
}

/// 拿不到锁：卡在哪一步 ＋ 系统原话。
#[derive(Debug)]
pub struct LockFail {
    pub stage: LockStage,
    pub err: io::Error,
}

/// 持着的那把锁。落地即放。
pub struct DirLock {
    #[cfg(unix)]
    _dir: std::fs::File,
    #[cfg(windows)]
    mutex: std::os::windows::io::RawHandle,
}

/// 拿 `dir` 那把锁（阻塞到拿到为止）。`dir` 必须已经在（这里不建：建目录是调用方的事）。
#[cfg(unix)]
pub fn hold(dir: &Path) -> Result<DirLock, LockFail> {
    use std::os::fd::AsRawFd as _;
    let f = std::fs::File::open(dir).map_err(|err| LockFail {
        stage: LockStage::Open,
        err,
    })?;
    loop {
        // SAFETY: `f` 活着、描述有效；`flock` 只读这个整数。
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
        if rc == 0 {
            return Ok(DirLock { _dir: f });
        }
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted {
            return Err(LockFail {
                stage: LockStage::Lock,
                err,
            });
        }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        attrs: *const core::ffi::c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> std::os::windows::io::RawHandle;
    fn WaitForSingleObject(handle: std::os::windows::io::RawHandle, milliseconds: u32) -> u32;
    fn ReleaseMutex(handle: std::os::windows::io::RawHandle) -> i32;
    fn CloseHandle(handle: std::os::windows::io::RawHandle) -> i32;
}

/// 互斥量的名字：路径（ASCII 折小写）的 [`cmd_wire::fnv::fnv1a64`]。
#[cfg(windows)]
fn mutex_name(namespace: &str, dir: &Path) -> Vec<u16> {
    let key = dir.to_string_lossy().to_ascii_lowercase();
    let h = cmd_wire::fnv::fnv1a64(key.as_bytes());
    format!("{namespace}\\ccm-own-state-{h:016x}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
pub fn hold(dir: &Path) -> Result<DirLock, LockFail> {
    /// 「一直等」—— 只等内核那一个事件。
    const WAIT_FOREVER: u32 = 0xFFFF_FFFF;
    const WAIT_OBJECT_0: u32 = 0;
    /// 上一个持有者没放就没了 ⇒ 这把锁照样归我们（它那一趟没做完，但盘上仍是原子换之前或之后的某一份）。
    const WAIT_ABANDONED: u32 = 0x80;
    let mut handle = std::ptr::null_mut();
    for ns in ["Global", "Local"] {
        let name = mutex_name(ns, dir);
        // SAFETY: `name` 以 0 结尾、活到调用结束；安全属性给空 = 默认。
        handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if !handle.is_null() {
            break;
        }
    }
    if handle.is_null() {
        return Err(LockFail {
            stage: LockStage::Lock,
            err: io::Error::last_os_error(),
        });
    }
    // SAFETY: `handle` 是刚拿到的互斥量句柄。
    match unsafe { WaitForSingleObject(handle, WAIT_FOREVER) } {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(DirLock { mutex: handle }),
        other => {
            // SAFETY: 同上；没拿到就只关句柄。
            unsafe { CloseHandle(handle) };
            Err(LockFail {
                stage: LockStage::Wait,
                err: io::Error::other(format!("WaitForSingleObject {other:#x}")),
            })
        }
    }
}

#[cfg(windows)]
impl Drop for DirLock {
    fn drop(&mut self) {
        // SAFETY: 句柄由 `hold` 拿到、只在这里放一次。
        unsafe {
            ReleaseMutex(self.mutex);
            CloseHandle(self.mutex);
        }
    }
}
