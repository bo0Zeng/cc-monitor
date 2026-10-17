//! 后端碰盘的唯一住址：读盘那一半住 [`ro`]（[`Ro`] · [`RoFile`]，全后端都用）；文件管理面的写住 [`face`]（[`Face`] · [`Resolved`]，
//! 只有文件管理那一族拿得到）；后端自家状态的写住 [`own`]（[`Own`]，启动时造）。本文件是两样平台原语：
//! **不覆盖改名在这块盘上有没有**（[`noreplace_unsupported`]）· **开文件不跟链接的旗**（读那一侧用）。
//!
//! | 平台 | 不覆盖改名（`Face::move_noreplace`） | 不跟链接 |
//! |---|---|---|
//! | Linux（gnu · musl） | `renameat2(RENAME_NOREPLACE)`，走 `SYS_renameat2` 裸调用（musl 没包装函数） | `O_NOFOLLOW` |
//! | macOS | `renameatx_np(RENAME_EXCL)`（不承诺的平台，只编得过） | `O_NOFOLLOW` |
//! | Windows | `MoveFileExW(…, 0)`：不带 `MOVEFILE_REPLACE_EXISTING` 就是目标已在即失败 | **没有**：`FILE_FLAG_OPEN_REPARSE_POINT` 开的是链接本身，不是「遇链接就失败」⇒ 不给 |
//! | 别的 unix | 没有 ⇒ [`noreplace_unsupported`] 那一形 | `O_NOFOLLOW` |
//!
//! 那块盘不认不覆盖改名时：普通文件走 `Face::move_by_link`（`link` ＋ `unlink`），目录拒（调用方判）；不退回先看后改。

use std::path::Path;

mod face;
mod own;
mod ro;
pub use face::{Face, FaceFile, NewFile, NewFileFail, Resolved};
pub(crate) use own::{
    inside_own_dir, AccountCreds, CredsLock, DiagFile, OwnPath, RenewLock, PRIVATE_DIR_MODE,
};
pub use own::{DiagLog, Own, OwnLock};
pub(crate) use ro::{Metadata, Perms, Ro, RoFile, Stamp};

/// 非 unix 的不覆盖改名（按路径；unix 那一臂在 `face.rs` 里相对目录句柄做）。
#[cfg(not(unix))]
fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    imp::rename_noreplace(from, to)
}

/// 这个路径今天是不是一个能跑的文件。判文件再判执行位，两个都要：只判执行位会被同名的目录劫持。
///
/// 非 unix：Windows 那一臂是 [`windows_launchable_name`]（只认 `.exe`）；其余平台保守 `false`（确证的空实现：一个插件都找不到）——
/// 不返回「什么都算可执行」：那会让 `plugin::discover::find` 把 `PATH` 上第一个同名文件当成插件交出去（[`crate::platform::cfgless_guard`] 钉着这一族）。
/// 找不到的那条路上有 `plugin::discover::not_installed_message`，它会说出查过哪些地方。
pub(crate) fn is_executable(p: &Path) -> bool {
    let Ok(md) = Ro::metadata(p) else {
        return false;
    };
    if !md.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        Ro::mode(&md).is_some_and(|mode| mode & 0o111 != 0)
    }
    #[cfg(windows)]
    {
        windows_launchable_name(p)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Windows 上「这个普通文件能不能被当程序起」—— 只认 `.exe`（大小写不敏感）。
///
/// 不读 `PATHEXT`：那一整套（`.bat` / `.cmd` / `.com` …）是「在 `PATH` 上按名字找命令」的规矩，而经这一口起的东西名字里就带着 `.exe`；
/// `.bat` / `.cmd` 要经 `cmd.exe` 解释才起得来，插件口是 argv 直传、不过 shell 的。这不是完整的 Windows 语义。
/// 纯函数（不碰盘、不带平台门），在哪个平台上都能测；只有 Windows 那一臂用它。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_launchable_name(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

/// 这一次失败是不是「没有这件原语」（不是目标已在、不是别的盘上错误）。
/// Linux 上文件系统不认这个旗回 `EINVAL`（NFS · 部分 FUSE），内核太老 / 被 seccomp 拦回 `ENOSYS`。
/// ⚠ `EINVAL` 也是「把目录挪进它自己底下」的码 —— 调用方退回普通改名之后那一下照样回 `EINVAL`，说法不变。
pub(crate) fn noreplace_unsupported(e: &std::io::Error) -> bool {
    imp::unsupported(e)
}

/// 开文件时加上的「最后一段是链接就失败」那个旗（开法的 `custom_flags` 收它）。
#[cfg(unix)]
const NO_FOLLOW: i32 = libc::O_NOFOLLOW;

#[cfg(target_os = "linux")]
mod imp {
    pub(super) fn unsupported(e: &std::io::Error) -> bool {
        matches!(e.raw_os_error(), Some(libc::EINVAL) | Some(libc::ENOSYS))
    }
}

#[cfg(target_vendor = "apple")]
mod imp {
    pub(super) fn unsupported(e: &std::io::Error) -> bool {
        matches!(e.raw_os_error(), Some(libc::ENOTSUP) | Some(libc::EINVAL))
    }
}

#[cfg(windows)]
mod imp {
    use std::path::Path;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }

    pub(super) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
        // 带长路径前缀：用户目录下过 260 字符的路径照样改得了名。
        let (f, t) = (
            win_path_core::win32_path(from)?,
            win_path_core::win32_path(to)?,
        );
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
    #[cfg(not(unix))]
    pub(super) fn rename_noreplace(
        _from: &std::path::Path,
        _to: &std::path::Path,
    ) -> std::io::Result<()> {
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }

    pub(super) fn unsupported(e: &std::io::Error) -> bool {
        e.kind() == std::io::ErrorKind::Unsupported
    }
}
