//! 自家目录：建的那一下就只给本人 · 拿一个目录的跨进程锁（等到拿到 · 不等）。
//!
//! # 只给本人的目录
//!
//! unix：`DirBuilder` 的 mode 在创建时生效（没有「先按 umask 建出来、再收窄」的那一段）；**已在的不动**（那可能是用户自己设的）。
//! 别的平台照常建（那边不是权限位这一问，目录的 ACL 按父目录继承）。
//!
//! # 目录锁
//!
//! 目录的跨进程独占锁全仓只这一把，两种拿法：[`hold`] 等到拿到为止（自家状态的读—改—写）· [`try_hold`] 不等、别人攥着就回
//! `None`（本人通道在听的那一个抢门）。同一目录两种拿法互斥。
//! 自家状态每一份都是「读盘 → 改一格 → 旁名 → 原子换」；两个进程（后端两个 · monitor 两个 · 后端与 monitor）同写一份要靠它串起来。
//! 锁的是**那份文件所在的目录**：文件每写一次就被换成一个新 inode，锁文件等于锁一个随时会被换掉的东西；锁目录只要只读打开它。
//!
//! - unix：只读打开目录 ＋ `flock(LOCK_EX)`（不等 ⇒ 加 `LOCK_NB`）。锁在打开文件描述上 ⇒ 同一进程两个线程各开一次也互斥；
//!   守卫落地即关描述即放锁；持锁进程被杀，内核替它放。
//! - Windows：以目录路径（ASCII 折小写）的 FNV-1a 命名的互斥量（`Global\` 先、`Local\` 兜底；名字只由 `mutex_name` 起）；
//!   等 ⇒ 一直等它，不等 ⇒ 等 0 毫秒。互斥量归**线程**所有、同一线程可重入 ⇒ 进程里记着此刻攥着的那几把：同一进程再
//!   [`try_hold`] 同一把回 `None`（与 unix 两次打开互斥同一结局）；持锁时再 [`hold`] 同一把是调用方不许的嵌套（各调用方都是
//!   一趟读—改—写）。持锁进程没了，互斥量被弃，下一个拿到的照样算拿到。🔴 真 Windows 上没跑过（本机只交叉编）。
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
pub struct HoldError {
    pub stage: LockStage,
    pub err: io::Error,
}

/// 持着的那把锁。落地即放。
pub struct DirLock {
    #[cfg(unix)]
    _dir: std::fs::File,
    #[cfg(windows)]
    mutex: std::os::windows::io::RawHandle,
    /// 进程里记着的那一把（落地时划掉）。
    #[cfg(windows)]
    key: String,
}

/// 拿 `dir` 那把锁（阻塞到拿到为止）。`dir` 必须已经在（这里不建：建目录是调用方的事）。
pub fn hold(dir: &Path) -> Result<DirLock, HoldError> {
    lock(dir, true)?.ok_or_else(|| HoldError {
        stage: LockStage::Wait,
        err: io::Error::other("waited lock came back not held"),
    })
}

/// 抢 `dir` 那把锁，不等：`Ok(None)` = 别人攥着（别的进程，或本进程别处）。`dir` 必须已经在。
pub fn try_hold(dir: &Path) -> Result<Option<DirLock>, HoldError> {
    lock(dir, false)
}

#[cfg(unix)]
fn lock(dir: &Path, wait: bool) -> Result<Option<DirLock>, HoldError> {
    use std::os::fd::AsRawFd as _;
    let f = std::fs::File::open(dir).map_err(|err| HoldError {
        stage: LockStage::Open,
        err,
    })?;
    let op = if wait {
        libc::LOCK_EX
    } else {
        libc::LOCK_EX | libc::LOCK_NB
    };
    loop {
        // SAFETY: `f` 活着、描述有效；`flock` 只读这个整数。
        let rc = unsafe { libc::flock(f.as_raw_fd(), op) };
        if rc == 0 {
            return Ok(Some(DirLock { _dir: f }));
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EINTR) => continue,
            Some(libc::EWOULDBLOCK) if !wait => return Ok(None),
            _ => {
                return Err(HoldError {
                    stage: LockStage::Lock,
                    err,
                })
            }
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

/// 互斥量名里路径那一截：路径（ASCII 折小写）的 [`cmd_wire::fnv::fnv1a64`]。进程里记着的也是它。
#[cfg(windows)]
fn mutex_key(dir: &Path) -> String {
    let path = dir.to_string_lossy().to_ascii_lowercase();
    format!("{:016x}", cmd_wire::fnv::fnv1a64(path.as_bytes()))
}

/// 互斥量的名字（只这一处起名）。
#[cfg(windows)]
fn mutex_name(namespace: &str, key: &str) -> Vec<u16> {
    format!("{namespace}\\ccm-own-state-{key}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

/// 本进程此刻攥着的那几把（互斥量同一线程可重入：靠它让同一进程第二次 [`try_hold`] 拿不到）。
#[cfg(windows)]
static HELD: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[cfg(windows)]
fn held() -> std::sync::MutexGuard<'static, Vec<String>> {
    HELD.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(windows)]
fn lock(dir: &Path, wait: bool) -> Result<Option<DirLock>, HoldError> {
    /// 「一直等」—— 只等内核那一个事件。
    const WAIT_FOREVER: u32 = 0xFFFF_FFFF;
    const WAIT_OBJECT_0: u32 = 0;
    /// 上一个持有者没放就没了 ⇒ 这把锁照样归我们（它那一趟没做完，但盘上仍是原子换之前或之后的某一份）。
    const WAIT_ABANDONED: u32 = 0x80;
    /// 不等的那一形：别人攥着。
    const WAIT_TIMEOUT: u32 = 0x102;
    // 锁是按路径起名的互斥量、不打开目录本身 ⇒ 目录在不在先看一眼，与 unix 那一份同一结局（目录不在 ⇒ 打开那一步就失败）。
    match std::fs::metadata(dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(HoldError {
                stage: LockStage::Open,
                err: io::ErrorKind::NotADirectory.into(),
            })
        }
        Err(err) => {
            return Err(HoldError {
                stage: LockStage::Open,
                err,
            })
        }
    }
    let key = mutex_key(dir);
    if !wait && held().contains(&key) {
        return Ok(None);
    }
    let mut handle = std::ptr::null_mut();
    for ns in ["Global", "Local"] {
        let name = mutex_name(ns, &key);
        // SAFETY: `name` 以 0 结尾、活到调用结束；安全属性给空 = 默认。
        handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if !handle.is_null() {
            break;
        }
    }
    if handle.is_null() {
        return Err(HoldError {
            stage: LockStage::Lock,
            err: io::Error::last_os_error(),
        });
    }
    let ms = if wait { WAIT_FOREVER } else { 0 };
    // SAFETY: `handle` 是刚拿到的互斥量句柄。
    match unsafe { WaitForSingleObject(handle, ms) } {
        WAIT_OBJECT_0 | WAIT_ABANDONED => {
            held().push(key.clone());
            Ok(Some(DirLock { mutex: handle, key }))
        }
        WAIT_TIMEOUT if !wait => {
            // SAFETY: 同上；没拿到就只关句柄。
            unsafe { CloseHandle(handle) };
            Ok(None)
        }
        other => {
            // SAFETY: 同上；没拿到就只关句柄。
            unsafe { CloseHandle(handle) };
            Err(HoldError {
                stage: LockStage::Wait,
                err: io::Error::other(format!("WaitForSingleObject {other:#x}")),
            })
        }
    }
}

#[cfg(windows)]
impl Drop for DirLock {
    fn drop(&mut self) {
        {
            let mut held = held();
            if let Some(i) = held.iter().position(|k| *k == self.key) {
                held.swap_remove(i);
            }
        }
        // SAFETY: 句柄由 `lock` 拿到、只在这里放一次。
        unsafe {
            ReleaseMutex(self.mutex);
            CloseHandle(self.mutex);
        }
    }
}
