//! 〔FIX5 · `设计/99 §2.2` · `设计/60 §7` 第 7 条〕文件管理写面要的两样平台原语：**不覆盖改名** · **开文件不跟链接的旗**。
//!
//! 只有原语，不判路径（路径解析住 `control/files_write.rs`）。谁调得到由 `readonly_guard` 第三层钉：
//! 改名这一件在第三层的改名闭集里（只许那五个模块调），别处一调当场红。
//!
//! | 平台 | 不覆盖改名 | 不跟链接 |
//! |---|---|---|
//! | Linux（gnu · musl） | `renameat2(RENAME_NOREPLACE)`，走 `SYS_renameat2` 裸调用（musl 没包装函数） | `O_NOFOLLOW` |
//! | macOS | `renamex_np(RENAME_EXCL)`（不承诺的平台，只编得过） | `O_NOFOLLOW` |
//! | Windows | `MoveFileExW(…, 0)`：不带 `MOVEFILE_REPLACE_EXISTING` 就是目标已在即失败 | **没有**：`FILE_FLAG_OPEN_REPARSE_POINT` 开的是链接本身，不是「遇链接就失败」⇒ 不给 |
//! | 别的 unix | 没有 ⇒ [`noreplace_unsupported`] 那一形 | `O_NOFOLLOW` |

use std::path::Path;

/// 把 `from` 改名成 `to`；**`to` 已在（含一条链接）⇒ `AlreadyExists`，一个字节不动**。原子：没有先看后改的窗。
/// 这个平台 / 这块盘没有这件原语 ⇒ 回一个 [`noreplace_unsupported`] 认得出的错，调用方自己决定退回什么。
pub(crate) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    imp::rename_noreplace(from, to)
}

/// 这一次失败是不是「没有这件原语」（不是目标已在、不是别的盘上错误）。
/// Linux 上文件系统不认这个旗回 `EINVAL`（NFS · 部分 FUSE），内核太老 / 被 seccomp 拦回 `ENOSYS`。
/// ⚠ `EINVAL` 也是「把目录挪进它自己底下」的码 —— 调用方退回普通改名之后那一下照样回 `EINVAL`，说法不变。
pub(crate) fn noreplace_unsupported(e: &std::io::Error) -> bool {
    imp::unsupported(e)
}

/// 开文件时加上的「最后一段是链接就失败」那个旗（开法的 `custom_flags` 收它）。
#[cfg(unix)]
pub(crate) const NO_FOLLOW: i32 = libc::O_NOFOLLOW;

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::Path;

    pub(super) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
        let f = CString::new(from.as_os_str().as_bytes())?;
        let t = CString::new(to.as_os_str().as_bytes())?;
        // SAFETY：两个以 NUL 结尾的串活过这一次调用；`renameat2` 只读它们。
        let rc = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                f.as_ptr(),
                libc::AT_FDCWD,
                t.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    pub(super) fn unsupported(e: &std::io::Error) -> bool {
        matches!(e.raw_os_error(), Some(libc::EINVAL) | Some(libc::ENOSYS))
    }
}

#[cfg(target_vendor = "apple")]
mod imp {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::Path;

    pub(super) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
        let f = CString::new(from.as_os_str().as_bytes())?;
        let t = CString::new(to.as_os_str().as_bytes())?;
        // SAFETY：同 Linux 那一臂。
        let rc = unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    pub(super) fn unsupported(e: &std::io::Error) -> bool {
        matches!(e.raw_os_error(), Some(libc::ENOTSUP) | Some(libc::EINVAL))
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub(super) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
        let (f, t) = (wide(from), wide(to));
        // SAFETY：两个以 0 结尾的 UTF-16 串活过这一次调用；flags = 0 ⇒ 目标已在即失败（`ERROR_ALREADY_EXISTS`）。
        if unsafe { MoveFileExW(f.as_ptr(), t.as_ptr(), 0) } != 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    pub(super) fn unsupported(_e: &std::io::Error) -> bool {
        false
    }
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple", windows)))]
mod imp {
    use std::path::Path;

    pub(super) fn rename_noreplace(_from: &Path, _to: &Path) -> std::io::Result<()> {
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }

    pub(super) fn unsupported(e: &std::io::Error) -> bool {
        e.kind() == std::io::ErrorKind::Unsupported
    }
}
