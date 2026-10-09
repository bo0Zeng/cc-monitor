//! **后端自有状态文件的跨进程锁** —— 平台原语，只许住 `platform/`。
//!
//! # 它治什么
//!
//! 第四层（`readonly_guard::OWN_STATE_MODULES`）每一份都是「读盘 → 改一格 → 临时文件 → 原子挪」。
//! 只靠进程内 `Mutex` 串不起它们：两个后端进程（本机常驻那一个 ＋ 一次性 CLI；
//! 远端上两台机器的 monitor 各自那条流）同时写同一份 ⇒ 后写的整份盖掉先写的。资产目录首建时还会生出两个 `self` id，
//! 输的那个成永不消失的幽灵机器；skill 装记录丢一条 ⇒ 那一趟装的文件从此卸不掉。
//!
//! # 锁住的是**那份状态文件所在的目录**，不是文件
//!
//! - 文件本身每写一次就被原子挪换成一个新 inode ⇒ 锁文件 = 锁一个随时会被换掉的东西。
//! - 另建一个 `.lock` 文件要一个写动词（第四层的动词闭集只有「建那一层目录 · 原子挪 · 删自己的临时文件」）；
//!   锁目录只要**只读打开**它 ⇒ 闭集一个动词都不加。
//!
//! # 两臂
//!
//! - unix：只读打开目录 ＋ `flock(LOCK_EX)`。`flock` 锁在**打开文件描述**上 ⇒ 同一进程里两个线程各开一次也互斥
//!   （所以调用方不必再叠一把进程内 `Mutex`）；守卫落地即关描述即放锁；持锁进程被杀，内核替它放。
//! - Windows：以目录路径（`path_key` 折叠大小写之后）的摘要命名的互斥量（`Global\` 先、`Local\` 兜底）。
//!   互斥量归**线程**所有、同一线程可重入 ⇒ 调用方不许在持锁时再拿同一把（本仓的调用方都是一趟读—改—写，没有嵌套）。
//!   🔴 真 Windows 上没跑过（本机只交叉编），如实登记。
//!
//! # 等锁不是定时器
//!
//! 阻塞等一个条件（别人放锁），不醒来、不驱动循环（`no_timer_guard` 管的是节拍）。持锁段只有一次读—改—写，
//! 别人放锁是毫秒级的事。⚠ 买不到：一个持锁的后端卡死在读—改—写中间 ⇒ 等它的那一个也跟着等（没有期限）。

use copy_core::copy_text;
use std::path::Path;

/// 持着的那把锁。落地即放。
pub(crate) struct DirLock {
    #[cfg(unix)]
    _dir: std::fs::File,
    #[cfg(windows)]
    mutex: std::os::windows::io::RawHandle,
}

/// 拿不到锁：那一句（带原因词）＋ 系统原话（可缺）。本层在 `common/` 之下、够不到 `common::said::Said`
/// ⇒ 交一对 std 类型，上面那一层经 `From` 换成 `Said`（`?` 直接过）。
pub(crate) type LockFail = (String, Option<String>);

fn lock_fail(said: String, raw: &std::io::Error) -> LockFail {
    (said, Some(raw.to_string()))
}

/// 拿 `dir` 那把锁（阻塞到拿到为止）。`dir` 必须已经在（调用方先建那一层 —— 建目录在第四层的动词闭集里，
/// 本层不建：`platform/` 在默认层，一个写动词都不许有）。
#[cfg(unix)]
pub(crate) fn hold(dir: &Path) -> Result<DirLock, LockFail> {
    use std::os::unix::io::AsRawFd;
    let f = std::fs::File::open(dir).map_err(|e| {
        lock_fail(
            copy_text(
                "bePlatformLock.hold.openFailed",
                &[
                    ("dir", &dir.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    loop {
        // SAFETY: `f` 活着、描述有效；`flock` 只读这个整数。
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
        if rc == 0 {
            return Ok(DirLock { _dir: f });
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != std::io::ErrorKind::Interrupted {
            return Err(lock_fail(
                copy_text(
                    "bePlatformLock.hold.lockFailed",
                    &[
                        ("dir", &dir.display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ));
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

/// 互斥量的名字：路径（大小写折叠后）的 FNV-1a 摘要。**纯函数**。
#[cfg(windows)]
fn mutex_name(namespace: &str, dir: &Path) -> Vec<u16> {
    let key = super::paths::path_key(dir);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.to_string_lossy().as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{namespace}\\ccm-own-state-{h:016x}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
pub(crate) fn hold(dir: &Path) -> Result<DirLock, LockFail> {
    /// 「一直等」—— 只等内核那一个事件（同 `win_proc.rs` 那一格）。
    const WAIT_FOREVER: u32 = 0xFFFF_FFFF;
    const WAIT_OBJECT_0: u32 = 0;
    /// 上一个持有者没放就没了 ⇒ 这把锁照样归我们（它那一趟读—改—写没做完，但盘上仍是原子挪之前或之后的某一份）。
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
        let e = std::io::Error::last_os_error();
        return Err(lock_fail(
            copy_text(
                "bePlatformLock.hold.lockFailed",
                &[
                    ("dir", &dir.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        ));
    }
    // SAFETY: `handle` 是刚拿到的互斥量句柄。
    match unsafe { WaitForSingleObject(handle, WAIT_FOREVER) } {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(DirLock { mutex: handle }),
        other => {
            // SAFETY: 同上；没拿到就只关句柄。
            unsafe { CloseHandle(handle) };
            Err((
                copy_text(
                    "bePlatformLock.hold.waitFailed",
                    &[
                        ("dir", &dir.display().to_string()),
                        ("status", &format!("{other:#x}")),
                    ],
                ),
                None,
            ))
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

#[cfg(test)]
#[path = "../../../tests/backend/platform/lock_tests.rs"]
mod tests;
