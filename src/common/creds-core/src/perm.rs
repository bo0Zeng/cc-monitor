//! 把凭据文件收窄到只给本人，以及读之前查一次它是不是被放宽了。
//!
//! 两个平台同一个签名，Windows 那半不是「无操作」：「谁能读这个文件」在 Windows 上有对应物（DACL），写成无操作等于两个平台的保证不一样而代码里看不出来
//! （[`tests::the_windows_half_is_not_a_no_op`] 钉住）。建文件时显式设 DACL（`SetNamedSecurityInfoW`）并置 `PROTECTED_DACL_SECURITY_INFORMATION`（= 断继承）：
//! `%LOCALAPPDATA%` 的默认 ACL 只靠继承，目录被搬过、从宽松的父目录继承、或者用户改过，ACL 就变了而没人知道。
//! 设 DACL 与「能手编」不冲突：ACL 限的是谁能读写，不是用什么程序读写。
//!
//! - [`make_private`] / [`create_private`] 只在 `harden` feature 打开时存在（后端也开了它：远端那台的 key 只能由那台的后端写；
//!   「后端里只有账号域那一份碰得到写半边」由 `readonly_guard::g6_dependency_signoff` 那条判据兜）。
//! - [`probe`] / [`judge`] 两侧都在。
//!
//! 没有判据钉「平台原语只许住这里」：后端那几条只扫 `backend/` / `platform/`，扫不到 `src/common/` ⇒ 别处再写一个平台 cfg 不会红，今天靠约定。

use copy_core::copy_text;

/// 我们在盘上**量到**的保护状态。**平台中立** —— 判断住 [`judge`]，一处。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Protection {
    /// POSIX 的 mode 位（已 `& 0o777`）。
    Unix { mode: u32 },
    /// Windows：DACL 里出现的**宽泛主体**（`Everyone` / `Users` 一类）的 SDDL 简称。
    /// 空 = 一个宽泛主体都没有。
    Windows { wide_principals: Vec<String> },
    /// **量不出来。** ⚠ 这一档**不许**被当成「没问题」——
    /// backend 那个 `platform/fallback_guard.rs` 整篇讲的就是这一条：
    /// 「为了让代码在别的平台上也能跑一下，给一个答不上来的问题编一个看起来无害的答案」，
    /// 而 `true` / `Some(..)` / `Ok(..)` 恰恰是最危险的那几个。
    Undetermined { why: String },
}

/// 判完的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 只给本人（外加 Windows 上的 SYSTEM）。
    OwnerOnly,
    /// 过宽。`how` 说清宽在哪，`fix` 说清怎么修 —— **两样都要有**。
    TooWide { how: String, fix: String },
    /// 查不出来。**不是绿灯**：调用方要照 `TooWide` 一样把它显出来。
    Undetermined { why: String },
}

impl Verdict {
    /// 要不要在界面上出声。`OwnerOnly` 之外**全都要**。
    ///
    /// ⚠ 写成「`matches!(self, TooWide{..})`」是错的 —— 那会让
    /// `Undetermined` 静默通过，正是上面那条 fallback 病。
    pub fn needs_attention(&self) -> bool {
        !matches!(self, Verdict::OwnerOnly)
    }
}

/// Windows 上算「宽泛」的主体（SDDL 简称）：`WD` = Everyone · `BU` = BUILTIN\Users · `AU` = Authenticated Users · `IU` = Interactive · `WR` = Restricted Code。
/// 它认的就是这 5 个简称；有人用完整 SID 写同一个主体（`S-1-1-0` 就是 Everyone），本表看不见（DACL 里合法出现的主体集合不是能枚举的东西，没有白名单可用）。
pub const WIDE_PRINCIPALS: &[&str] = &["WD", "BU", "AU", "IU", "WR"];

/// **判断只有这一处。** 两个平台、三种量到的东西，都在这里变成同一种结论。
pub fn judge(p: &Protection) -> Verdict {
    match p {
        Protection::Unix { mode } => {
            let extra = mode & 0o077;
            if extra == 0 {
                Verdict::OwnerOnly
            } else {
                Verdict::TooWide {
                    how: copy_text(
                        "credsPerm.judge.tooWideUnix",
                        &[
                            ("mode", &format!("{:04o}", mode & 0o7777)),
                            ("extra", &format!("{:03o}", extra)),
                        ],
                    ),
                    fix: copy_text("credsPerm.judge.fixUnix", &[]),
                }
            }
        }
        Protection::Windows { wide_principals } => {
            if wide_principals.is_empty() {
                Verdict::OwnerOnly
            } else {
                Verdict::TooWide {
                    how: copy_text(
                        "credsPerm.judge.tooWideWindows",
                        &[("who", &(wide_principals.join(" / ")).to_string())],
                    ),
                    fix: copy_text("credsPerm.judge.fixWindows", &[]),
                }
            }
        }
        Protection::Undetermined { why } => Verdict::Undetermined {
            why: copy_text("credsPerm.judge.undetermined", &[("why", &why.to_string())]),
        },
    }
}

// ─────────────────────────── 读那一半（两侧都有） ───────────────────────────

/// 量一次盘上的保护状态。**只读。**
pub fn probe(path: &std::path::Path) -> Protection {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return match std::fs::metadata(path) {
            Ok(m) => Protection::Unix {
                mode: m.permissions().mode() & 0o7777,
            },
            Err(e) => Protection::Undetermined {
                why: copy_text("credsPerm.probe.noMetadata", &[("e", &e.to_string())]),
            },
        };
    }
    #[cfg(all(windows, feature = "harden"))]
    {
        return windows_probe(path);
    }
    #[cfg(all(windows, not(feature = "harden")))]
    {
        let _ = path;
        return Protection::Undetermined {
            why: copy_text("credsPerm.probe.notHardened", &[]),
        };
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        return Protection::Undetermined {
            why: copy_text("credsPerm.probe.unsupported", &[]),
        };
    }
}

// ─────────────────────── 写那一半（只有 `harden` 才有） ───────────────────────

/// 把这份文件收窄到**只给本人**。
///
/// - Unix：`0o600`。
/// - Windows：显式设 DACL（只留**当前用户** + `SYSTEM`）并**断掉继承**。
/// - 其余平台：**报错**，不假装做到了。
///
/// # 签名两边一致
///
/// 与 `platform/fs.rs::make_executable` 同形：收一个路径，返回 `Result<(), String>`，
/// 调用方只知道「写完要让它只给本人」，不知道**这个平台上那句话怎么落**。
#[cfg(feature = "harden")]
pub fn make_private(p: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| copy_text("credsPerm.makePrivate.failed", &[("e", &e.to_string())]))?;
        return Ok(());
    }
    #[cfg(windows)]
    {
        return windows_set_owner_only_dacl(p);
    }
    #[cfg(not(any(unix, windows)))]
    {
        // ⚠ 这里**必须**是错，不许是 `Ok(())`。理由整段见后端的 `platform/fallback_guard.rs`：
        // 一个答不上来的问题不该有一个看起来无害的答案。
        Err(copy_text(
            "credsPerm.makePrivate.unsupported",
            &[("path", &(p.display()).to_string())],
        ))
    }
}

/// 建一个只给本人的新文件，并把写句柄交出来。与 [`make_private`]（把已经在盘上的文件收窄，管终态）是两件事：「先按 umask 建出来、写进明文、再收窄」
/// 那条路上，文件出生到收窄之间有一个宽窗口（常见 umask `0022` 下是 `0644`，全机可读），里面已经有明文 ⇒ 本函数管出生那一刻。
///
/// - Unix：`OpenOptions::create_new(true).mode(0o600)` —— `mode` 在创建时生效。用 `create_new`（`O_EXCL`）：目标若已存在（崩溃残骸、或别人预置的符号链接），
///   `create` 会跟随并截断它，那时权限是它的不是我们的；`O_EXCL` 让这种情况直接失败，调用方先删再建。
/// - Windows：`CreateFileW` + `SECURITY_ATTRIBUTES`，DACL 与 [`make_private`] 用同一句 SDDL（`owner_only_sddl`）。`CREATE_NEW` 是 `O_EXCL` 的对应物。
/// - 其余平台：报错，不假装做到了。
#[cfg(feature = "harden")]
pub fn create_private(p: &std::path::Path) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        return std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(p);
    }
    #[cfg(windows)]
    {
        return windows_create_owner_only(p);
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            copy_text(
                "credsPerm.createPrivate.unsupported",
                &[("path", &(p.display()).to_string())],
            ),
        ))
    }
}

#[cfg(all(windows, feature = "harden"))]
fn windows_create_owner_only(p: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, BOOL, HLOCAL};
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, FILE_SHARE_MODE,
    };

    let sid = current_user_sid().map_err(std::io::Error::other)?;
    let sddl: Vec<u16> = owner_only_sddl(&sid)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let path_w: Vec<u16> = p
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut psd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut psd,
            None,
        )
        .map_err(|e| std::io::Error::other(e.message().to_string()))?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: psd.0,
            bInheritHandle: BOOL(0),
        };
        let h = CreateFileW(
            PCWSTR(path_w.as_ptr()),
            FILE_GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            Some(&sa as *const SECURITY_ATTRIBUTES),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            None,
        );
        let _ = LocalFree(HLOCAL(psd.0));
        let h = h.map_err(|e| std::io::Error::other(e.message().to_string()))?;
        Ok(std::fs::File::from_raw_handle(
            h.0 as *mut core::ffi::c_void,
        ))
    }
}

/// 当前用户 + SYSTEM，全权；`D:P` 里的 `P` = **断继承**。
#[cfg(all(windows, feature = "harden"))]
fn owner_only_sddl(user_sid: &str) -> String {
    format!("D:P(A;;FA;;;{user_sid})(A;;FA;;;SY)")
}

#[cfg(all(windows, feature = "harden"))]
fn current_user_sid() -> Result<String, String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(|e| {
            copy_text(
                "credsPerm.currentUserSid.noToken",
                &[("e", &(e.message()).to_string())],
            )
        })?;
        let mut need = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut need);
        let mut buf = vec![0u8; need.max(1) as usize];
        let got = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            need,
            &mut need,
        );
        let _ = CloseHandle(token);
        got.map_err(|e| {
            copy_text(
                "credsPerm.currentUserSid.noUser",
                &[("e", &(e.message()).to_string())],
            )
        })?;
        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut s = PWSTR::null();
        ConvertSidToStringSidW(tu.User.Sid, &mut s).map_err(|e| {
            copy_text(
                "credsPerm.currentUserSid.toText",
                &[("e", &(e.message()).to_string())],
            )
        })?;
        let out = s.to_string().map_err(|e| {
            copy_text(
                "credsPerm.currentUserSid.notUtf16",
                &[("e", &e.to_string())],
            )
        })?;
        let _ = LocalFree(HLOCAL(s.0 as *mut core::ffi::c_void));
        Ok(out)
    }
}

#[cfg(all(windows, feature = "harden"))]
fn windows_set_owner_only_dacl(p: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, BOOL, HLOCAL, PSID};
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows::Win32::Security::{
        GetSecurityDescriptorDacl, ACL, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    };

    let sid = current_user_sid()?;
    let sddl: Vec<u16> = owner_only_sddl(&sid)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut path_w: Vec<u16> = p
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut psd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut psd,
            None,
        )
        .map_err(|e| {
            copy_text(
                "credsPerm.windowsSetOwnerOnlyDacl.parse",
                &[("e", &(e.message()).to_string())],
            )
        })?;
        let mut present = BOOL(0);
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut defaulted = BOOL(0);
        let got = GetSecurityDescriptorDacl(psd, &mut present, &mut dacl, &mut defaulted);
        if got.is_err() || !present.as_bool() || dacl.is_null() {
            let _ = LocalFree(HLOCAL(psd.0));
            return Err(copy_text("credsPerm.windowsSetOwnerOnlyDacl.noList", &[]));
        }
        // `DACL_SECURITY_INFORMATION` = 换 DACL；`PROTECTED_DACL_SECURITY_INFORMATION` = **断继承**。
        // 少了后一个，DACL 设上了但父目录的继承项还会回来 —— 那是「看起来做了」的形状。
        let rc = SetNamedSecurityInfoW(
            PCWSTR(path_w.as_mut_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            PSID::default(),
            PSID::default(),
            Some(dacl as *const ACL),
            None,
        );
        let _ = LocalFree(HLOCAL(psd.0));
        if rc.is_err() {
            return Err(copy_text(
                "credsPerm.windowsSetOwnerOnlyDacl.failed",
                &[("rc", &(rc.0).to_string())],
            ));
        }
    }
    Ok(())
}

#[cfg(all(windows, feature = "harden"))]
fn windows_probe(p: &std::path::Path) -> Protection {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};

    let path_w: Vec<u16> = p
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut psd = PSECURITY_DESCRIPTOR::default();
        let rc = GetNamedSecurityInfoW(
            PCWSTR(path_w.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            &mut psd,
        );
        if rc.is_err() {
            return Protection::Undetermined {
                why: copy_text(
                    "credsPerm.windowsProbe.unreadable",
                    &[("rc", &(rc.0).to_string())],
                ),
            };
        }
        let mut s = PWSTR::null();
        let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(
            psd,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut s,
            None,
        );
        if ok.is_err() {
            let _ = LocalFree(HLOCAL(psd.0));
            return Protection::Undetermined {
                why: copy_text("credsPerm.windowsProbe.toText", &[]),
            };
        }
        let sddl = s.to_string().unwrap_or_default();
        let _ = LocalFree(HLOCAL(s.0 as *mut core::ffi::c_void));
        let _ = LocalFree(HLOCAL(psd.0));
        Protection::Windows {
            wide_principals: wide_principals_in_sddl(&sddl),
        }
    }
}

/// 从一串 SDDL 里挑出宽泛主体。**纯函数，两个平台都编得到** ——
/// 抽出来是为了让它在 Linux 上也测得到（Windows 那条真调用测不了，但这条判断测得到）。
pub fn wide_principals_in_sddl(sddl: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // ACE 的形状是 `(A;;FA;;;WD)` —— 主体是最后一段，以 `)` 收尾。
    for ace in sddl.split('(').skip(1) {
        let Some(end) = ace.find(')') else { continue };
        let Some(who) = ace[..end].rsplit(';').next() else {
            continue;
        };
        let who = who.trim();
        if WIDE_PRINCIPALS.contains(&who) && !out.iter().any(|x| x == who) {
            out.push(who.to_string());
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/common/creds-core/perm_tests.rs"]
mod tests;
