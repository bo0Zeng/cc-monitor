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
/// 原子写落在原名旁边的旁名：造名与认名同一份形状。
pub(crate) mod side_name;
pub use face::{Face, FaceFile, NewFile, NewFileFail, Resolved};
pub(crate) use own::{
    inside_own_dir, AccountCreds, CredsLock, DiagFile, OwnPath, RenewLock, PRIVATE_DIR_MODE,
};
pub use own::{DiagLog, Own, OwnLock};
pub(crate) use ro::{Metadata, Perms, Ro, RoFile, Stamp};

/// Windows 的系统错号：拒绝访问。
const WIN_ACCESS_DENIED: i32 = 5;
/// Windows 的系统错号里「那份文件正被别的程序开着」那一类：共享冲突 · 锁冲突（标准库把它们归「未分类」）。
const WIN_HELD: [i32; 2] = [32, 33];

/// 改名顶掉目标没成，这回事归哪一类（Windows 的错号；纯函数，各平台都编、都测）：
/// 错号本身是占用那一类 ⇒ 被占用；拒绝访问而目标此刻按「删」打开回的是占用那一类 ⇒ 也是被占用
/// （换名到别的程序独占着的文件，系统回的是拒绝访问）；别的 ⇒ `None`（照原错，真没权限就是没权限）。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn replace_refusal(rename: i32, probe: Option<i32>) -> Option<std::io::ErrorKind> {
    let held = |c: i32| WIN_HELD.contains(&c);
    (held(rename) || (rename == WIN_ACCESS_DENIED && probe.is_some_and(held)))
        .then_some(std::io::ErrorKind::ResourceBusy)
}

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

/// 这台此刻建不建得了符号链接（只读地判，不试建）。unix 恒能；Windows 要开发者模式开着、或本进程的令牌里有建链接那一项特权
/// （管理员提权之后有）；别的平台没有这件原语。
pub(crate) fn links_creatable() -> bool {
    #[cfg(unix)]
    {
        true
    }
    #[cfg(windows)]
    {
        imp::links_creatable()
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
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

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateSymbolicLinkW(link: *const u16, target: *const u16, flags: u32) -> u8;
        fn GetCurrentProcess() -> isize;
        fn CloseHandle(h: isize) -> i32;
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            key: isize,
            sub: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut core::ffi::c_void,
            len: *mut u32,
        ) -> i32;
        fn OpenProcessToken(process: isize, access: u32, token: *mut isize) -> i32;
        fn LookupPrivilegeValueW(system: *const u16, name: *const u16, luid: *mut u64) -> i32;
        fn GetTokenInformation(
            token: isize,
            class: u32,
            info: *mut core::ffi::c_void,
            len: u32,
            ret: *mut u32,
        ) -> i32;
    }

    const SYMBOLIC_LINK_FLAG_DIRECTORY: u32 = 0x1;
    const SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE: u32 = 0x2;
    const ERROR_INVALID_PARAMETER: i32 = 87;
    /// `HKEY_LOCAL_MACHINE`（`0x80000002` 按有符号扩展成句柄宽）。
    const HKEY_LOCAL_MACHINE: isize = 0x8000_0002_u32 as i32 as isize;
    const RRF_RT_REG_DWORD: u32 = 0x10;
    const TOKEN_QUERY: u32 = 0x8;
    /// `TOKEN_INFORMATION_CLASS::TokenPrivileges`。
    const TOKEN_PRIVILEGES_CLASS: u32 = 3;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 建一条符号链接 `link → target`（目标文本不解，只把 `/` 换成 `\`：系统按原样存，存进去的 `/` 跟不过去）。`dir` ⇒ 目录那一种。
    /// 先带「开发者模式下不要特权」那一旗；老系统不认那一旗（`ERROR_INVALID_PARAMETER`）⇒ 不带再来一次。
    pub(super) fn symlink(target: &Path, link: &Path, dir: bool) -> std::io::Result<()> {
        let l = win_path_core::win32_path(link)?;
        // 目标是链接里存的那段文字，不是这里要开的路径 ⇒ 不加长路径前缀（加了就存成另一段文字）。
        // 不是合法 Unicode 的目标 ⇒ 不建（不替换成别的字再存进去）。
        let Some(text) = target.to_str() else {
            return Err(std::io::ErrorKind::InvalidInput.into());
        };
        let t: Vec<u16> = text
            .replace('/', "\\")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let kind = if dir { SYMBOLIC_LINK_FLAG_DIRECTORY } else { 0 };
        let mut last = std::io::Error::from(std::io::ErrorKind::Unsupported);
        for flags in [kind | SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE, kind] {
            // SAFETY：两个以 0 结尾的 UTF-16 串活过这一次调用。
            if unsafe { CreateSymbolicLinkW(l.as_ptr(), t.as_ptr(), flags) } != 0 {
                return Ok(());
            }
            last = std::io::Error::last_os_error();
            if last.raw_os_error() != Some(ERROR_INVALID_PARAMETER) {
                break;
            }
        }
        Err(last)
    }

    /// 开发者模式开着没有（`AppModelUnlock\AllowDevelopmentWithoutDevLicense` = 1）：开着 ⇒ 普通用户也建得了符号链接。
    fn developer_mode() -> bool {
        let sub = wide(r"SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock");
        let name = wide("AllowDevelopmentWithoutDevLicense");
        let mut v: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        // SAFETY：串以 0 结尾；`v` / `len` 是本栈上的、大小与 `RRF_RT_REG_DWORD` 要的一致。
        let rc = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                sub.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut v as *mut u32).cast(),
                &mut len,
            )
        };
        rc == 0 && v == 1
    }

    /// 本进程的令牌里有没有「建符号链接」那一项特权（管理员提权之后有；有就建得了，系统那一调用自己去启用它）。
    fn holds_symlink_privilege() -> bool {
        let name = wide("SeCreateSymbolicLinkPrivilege");
        let mut want: u64 = 0;
        // SAFETY：`name` 以 0 结尾；`want` 是 8 字节的 LUID 落点。
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut want) } == 0 {
            return false;
        }
        let mut tok: isize = 0;
        // SAFETY：当前进程的伪句柄不用关；`tok` 由系统填、下面关掉。
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) } == 0 {
            return false;
        }
        let mut len: u32 = 0;
        // SAFETY：先问要多大（这一次回失败是常态），再按那个大小读一次。
        unsafe {
            GetTokenInformation(
                tok,
                TOKEN_PRIVILEGES_CLASS,
                std::ptr::null_mut(),
                0,
                &mut len,
            )
        };
        let mut buf = vec![0u8; len as usize];
        // SAFETY：`buf` 恰好 `len` 字节、活过这一次调用。
        let ok = len >= 4
            && unsafe {
                GetTokenInformation(
                    tok,
                    TOKEN_PRIVILEGES_CLASS,
                    buf.as_mut_ptr().cast(),
                    len,
                    &mut len,
                )
            } != 0;
        // SAFETY：`tok` 是上面开出来的那一个。
        unsafe { CloseHandle(tok) };
        if !ok {
            return false;
        }
        // TOKEN_PRIVILEGES：u32 个数，接着每项 12 字节（LUID 低 u32 · 高 i32 · 属性 u32）。
        let count = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        buf[4..].chunks_exact(12).take(count).any(|e| {
            let low = u64::from(u32::from_le_bytes([e[0], e[1], e[2], e[3]]));
            let high = u64::from(u32::from_le_bytes([e[4], e[5], e[6], e[7]]));
            (high << 32 | low) == want
        })
    }

    pub(super) fn links_creatable() -> bool {
        developer_mode() || holds_symlink_privilege()
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
