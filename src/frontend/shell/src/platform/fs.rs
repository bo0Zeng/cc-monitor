//! 平台相关的文件系统原语。**存在的理由是 `backend-split` 的 C10**〔用 08-01〕：
//! 「`platform/` 是**唯一**允许平台原语与平台 cfg 的地方」——`backend/` 那一半必须平台无关，
//! 所以凡是带 `#[cfg(unix)]` / `std::os::*` 的文件操作都住在这里，由宿主注入给 backend。
//!
//! # 诚实边界 10g：**没有判据钉「平台原语只许住这里」**
//!
//! `backend/mod.rs` 的 `the_backend_half_stays_platform_agnostic` 只扫 `backend/`，
//! 它保证的是「那半没有平台原语」，**不保证「平台原语都在本文件」**。
//! 有人在 `backend/` 之外的别处再写一个 `#[cfg(unix)]`，**没有任何东西会红**。
//! ⇒ 今天靠约定。解锁条件：backend 侧出现第二处平台原语时建 `backend/platform/`，
//! 届时把这条一并收进那层的判据。
//!
//! ⚠ **不是「工具函数堆」** —— 只放「同一件事在两个平台上做法不同」的那种原语。
//! 纯逻辑（路径拼接、命名规则）不许进来：那些在 backend 里就能测，搬进来反而丢了可测性。

use crate::copy_table::copy_text;
use crate::detail::Said;
use std::path::Path;

/// 置可执行位。
///
/// Unix：`0o700`（**只给本人**——释放出来的是后端二进制，没有理由让同机别的用户能跑它）。
/// Windows：**无操作** —— 可执行性由扩展名决定，没有对应的位可置。
///
/// 这是 `local_backend.rs::extract_embedded_to` 的注入参数：
/// backend 那边只知道「写完要让它可执行」，不知道**这个平台上那句话怎么落**。
// 这里原来有 `make_private`〔散文墓碑〕（转发 `creds_core::perm::make_private`，把凭据文件收成只给本人）。
// 唯一的调用方是 monitor 那侧的凭据写口，那个写口随「本机那一份也交本机常驻后端写」删了 ⇒ 它零调用、删掉。
// 收窄那条原语照旧只住 `creds_core::perm`，今天只有后端账号域那一份写口在用。

pub fn make_executable(p: &Path) -> Result<(), Said> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "rsPlatformFs.chmod.failed",
                    &[("why", &copy_core::io_reason(e.kind()))],
                ),
                &e,
            )
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = p; // Windows 上没有可执行位；参数照收，签名两边一致。
    }
    Ok(())
}

/// **monitor 建后端自家目录（`~/.cc-monitor` 与它底下几层）的那一个函数**：
/// 建的那一下就是只给本人（unix：`DirBuilder` 的 mode 在创建时生效，没有「先按 umask 建出来、再收窄」的那一段），
/// 缺的中间几层一并这样建；**已在的不动**（那可能是用户自己设的）。别的平台照常建（那边不是 unix 权限位这一问）。
///
/// 与后端那一份（`src/backend/common/own_dir.rs::ensure_private_dir`）是两个 crate 各一份：两个 crate 没有能放平台原语的共享落点
/// （`creds-core` 的平台那半是 `harden` feature，monitor 不开）—— 权限位同一个值（0700），各自的判据各钉一半。
/// 这是 `local_backend.rs` 那几处释放的注入参数（`C10`：那一半不认识平台），宿主自己也直接调。
pub fn ensure_private_dir(dir: &Path) -> Result<(), Said> {
    if dir.is_dir() {
        return Ok(());
    }
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        b.mode(0o700);
    }
    b.create(dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "rsPlatformFs.mkdir.failed",
                &[
                    ("dir", &(dir.display()).to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })
}

/// 这一趟 `open` 若是**新建**，新文件只给本人（unix：`mode(0o600)`，创建那一刻生效）；别的平台照常建。
/// 调用方是 `local_backend_host.rs` 写监听口钥匙 / pid 记录的那两处（钥匙是那条回环口上唯一的门）。
pub fn only_me_on_create(opts: &mut std::fs::OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    #[cfg(not(unix))]
    {
        let _ = opts; // 不是 unix 权限位这一问；参数照收，签名两边一致。
    }
}

/// **monitor 自有状态文件的跨进程锁**：锁那份文件所在的**目录**（不是文件：文件每写一次就被原子挪换成新 inode）。
///
/// 守的要求：「CFG1 把 `config.json` 收成单一写口 …… 两个 monitor 进程同写没有跨进程锁 —— 用你那一族同一套 `flock`
/// （Windows 对应）把它也包上」。与后端那一份（`src/backend/platform/lock.rs::hold`）是**同一种锁**、两个 crate 各一份
/// （理由同 [`ensure_private_dir`]：没有能放平台原语的共享落点）：
/// - unix：只读打开目录 ＋ std `File::lock`（Linux 上就是 `flock(LOCK_EX)`，与后端 `libc::flock` 进内核同一张表 ⇒ 两边互斥）；
///   锁在打开文件描述上 ⇒ 同一进程两个线程各开一次也互斥；守卫落地即关描述即放锁，进程被杀内核替它放。
/// - Windows：`Global\ccm-own-state-<FNV-1a(小写路径)>` 命名互斥量（名字拼法与后端逐字相同，判据对拍）。🔴 真 Windows 上没跑过。
///
/// 阻塞等到拿到为止（等的是别人放锁，不是节拍）。`dir` 必须已经在（这里不建：建目录是调用方的事）。
pub struct DirLock {
    #[cfg(unix)]
    _dir: std::fs::File,
    #[cfg(windows)]
    mutex: std::os::windows::io::RawHandle,
}

#[cfg(unix)]
pub fn hold_dir_lock(dir: &Path) -> Result<DirLock, Said> {
    let f = std::fs::File::open(dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "rsPlatformFs.lock.openFailed",
                &[
                    ("dir", &dir.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    f.lock().map_err(|e| {
        Said::with_raw(
            copy_text(
                "rsPlatformFs.lock.failed",
                &[("dir", &dir.display().to_string())],
            ),
            &e,
        )
    })?;
    Ok(DirLock { _dir: f })
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

/// 互斥量的名字（与后端 `platform/lock.rs::mutex_name` 逐字同一种拼法：小写路径的 FNV-1a）。
#[cfg(windows)]
fn dir_lock_name(namespace: &str, dir: &Path) -> Vec<u16> {
    let key = dir.to_string_lossy().to_ascii_lowercase();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{namespace}\\ccm-own-state-{h:016x}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
pub fn hold_dir_lock(dir: &Path) -> Result<DirLock, Said> {
    const WAIT_FOREVER: u32 = 0xFFFF_FFFF;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_ABANDONED: u32 = 0x80;
    let mut handle = std::ptr::null_mut();
    for ns in ["Global", "Local"] {
        let name = dir_lock_name(ns, dir);
        // SAFETY: `name` 以 0 结尾、活到调用结束；安全属性给空 = 默认。
        handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if !handle.is_null() {
            break;
        }
    }
    if handle.is_null() {
        return Err(Said::with_raw(
            copy_text(
                "rsPlatformFs.lock.failed",
                &[("dir", &dir.display().to_string())],
            ),
            std::io::Error::last_os_error(),
        ));
    }
    // SAFETY: `handle` 是刚拿到的互斥量句柄。
    match unsafe { WaitForSingleObject(handle, WAIT_FOREVER) } {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(DirLock { mutex: handle }),
        other => {
            // SAFETY: 同上；没拿到就只关句柄。
            unsafe { CloseHandle(handle) };
            Err(Said::with_raw(
                copy_text(
                    "rsPlatformFs.lock.failed",
                    &[("dir", &dir.display().to_string())],
                ),
                format!("{other:#x}"),
            ))
        }
    }
}

#[cfg(windows)]
impl Drop for DirLock {
    fn drop(&mut self) {
        // SAFETY: 句柄由 `hold_dir_lock` 拿到、只在这里放一次。
        unsafe {
            ReleaseMutex(self.mutex);
            CloseHandle(self.mutex);
        }
    }
}

// 原住 `config.rs`（逐字）：monitor 自己的文件（config.json）的原子替换，两个平台臂。
/// 把 src 原子替换到 dst。
///
/// ⚠ **从私有改成 `pub(crate)`，理由不是「顺手」**：
/// 那个调用方（`creds_store::write_key`〔散文墓碑〕）随本机凭据文件的写者换成本机常驻后端一起删了；
/// 下面是它当年的理由，留作来历：它要一次原子替换，而它**不许自己写一个 `fs::rename`** ——
/// `atomic_replace_registry` 按「`rename` / `MoveFileExW` 的**出现次数**」逐文件登记，
/// 那张表不在 `K-H2a` 的写区。复用这一份 ⇒ 新文件里那两个字面量出现 **0** 次，
/// 既不动那张表，也不给它挖洞。
/// 选它（`MoveFileExW` 那套语义）而不是 `ReplaceFileW` 是**有理由的**：
/// `INVARIANTS §4` 那条 ACL 保留只限定在**用户的**文件，而凭据文件与 `config.json` 同类
/// ——**都是 monitor 自己的文件**（登记表里那两行逐字这么写的；住址今天是 `platform/fs.rs`）。
/// std::fs::rename 在 Windows 上目标文件已存在时会失败（不像 POSIX 原子覆盖），
/// 所以这里走 MoveFileExW(MOVEFILE_REPLACE_EXISTING)；非 Windows 走 std::fs::rename。
#[cfg(windows)]
pub(crate) fn atomic_replace(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING};

    // 带长路径前缀：过 260 字符的路径照样换得上。
    let (src_w, dst_w) = (
        win_path_core::win32_path(src)?,
        win_path_core::win32_path(dst)?,
    );
    unsafe {
        MoveFileExW(
            PCWSTR(src_w.as_ptr()),
            PCWSTR(dst_w.as_ptr()),
            MOVEFILE_REPLACE_EXISTING,
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.message().to_string()))
    }
}

#[cfg(not(windows))]
pub(crate) fn atomic_replace(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}
