//! 把自家秘密文件（API key 凭据文件 · 中转钥匙文件）收窄到只给本人，以及读之前查一次它是不是被放宽了。
//!
//! 两个平台同一个签名，Windows 那半不是「无操作」：「谁能读这个文件」在 Windows 上有对应物（属主 ＋ DACL），写成无操作等于两个平台的保证不一样而代码里看不出来
//! （[`tests::the_windows_half_is_not_a_no_op`] 钉住）。建文件时带着 DACL 建（`CreateFileW` ＋ `SECURITY_ATTRIBUTES`，SDDL 里 `D:P` = 断继承）：
//! `%LOCALAPPDATA%` 的默认 ACL 只靠继承，目录被搬过、从宽松的父目录继承、或者用户改过，ACL 就变了而没人知道。
//! 设 DACL 与「能手编」不冲突：ACL 限的是谁能读写，不是用什么程序读写。
//!
//! - [`judge`] 两侧都在（纯函数，两个平台的量法都在 Linux 上测得到）。
//! - [`open_checked`] / [`verdict_at`] / [`create_private`] 只在 `harden` feature 打开时存在（只有后端开：后端读、写这几份文件；monitor 都不碰）。
//!   量与读同一个句柄：先按路径量、再按路径读，中间被换掉时量的就不是读的那一份。
//!
//! 没有判据钉「平台原语只许住这里」：后端那几条只扫 `backend/` / `platform/`，扫不到 `src/common/` ⇒ 别处再写一个平台 cfg 不会红，今天靠约定。

use copy_core::copy_text;

/// 我们在盘上**量到**的保护状态。**平台中立** —— 判断住 [`judge`]，一处。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Protection {
    /// POSIX：mode 位（已 `& 0o7777`）· 文件属主（uid ＋ 翻得出的用户名）· 本进程的有效用户。
    Unix {
        mode: u32,
        owner: u32,
        owner_name: Option<String>,
        me: u32,
    },
    /// Windows：属主（SID 串 ＋ 翻得出的账号名）与本进程用户（SID 串）· DACL 逐条（`None` ＝ 没有 DACL，人人全权）。
    Windows {
        owner: String,
        owner_name: Option<String>,
        me: String,
        dacl: Option<Vec<Ace>>,
    },
    /// **量不出来。** ⚠ 这一档**不许**被当成「没问题」——
    /// backend 那个 `platform/fallback_guard.rs` 整篇讲的就是这一条：
    /// 「为了让代码在别的平台上也能跑一下，给一个答不上来的问题编一个看起来无害的答案」，
    /// 而 `true` / `Some(..)` / `Ok(..)` 恰恰是最危险的那几个。
    /// `why` 上屏（原因词来自闭集）；`raw` 是下层原话（系统报错 · Windows 错误码），只进复制详情 / 日志，可缺。
    Undetermined { why: String, raw: Option<String> },
}

/// DACL 里作用在文件本身的一条（只继承给子项的那种不算）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ace {
    /// 允许类（`false` ＝ 拒绝类：拒绝不会放宽，判宽时不看）。
    pub allow: bool,
    /// 主体的 SID 串。
    pub who: String,
    /// 那个 SID 翻成的账号名（`域\名`，本机账号的域是机器名）；翻不出 ⇒ `None`，说法里给 SID。
    pub name: Option<String>,
    /// 访问掩码。
    pub mask: u32,
}

/// 除本人之外许它碰这份文件的主体：SYSTEM 与 Administrators（管理员本来就能夺属主，算不上放宽）。
pub const TRUSTED_SIDS: &[&str] = &["S-1-5-18", "S-1-5-32-544"];

/// 掩码里读得到内容的位：读数据 0x1 · GENERIC_READ 0x8000_0000 · GENERIC_ALL 0x1000_0000。
const MASK_READ: u32 = 0x9000_0001;
/// 写得进内容的位：写数据 0x2 · 追加 0x4 · GENERIC_WRITE 0x4000_0000 · GENERIC_ALL。
const MASK_WRITE: u32 = 0x5000_0006;
/// 改得了权限的位：WRITE_DAC 0x4_0000 · WRITE_OWNER 0x8_0000 · GENERIC_ALL。
const MASK_PERMS: u32 = 0x100c_0000;

/// 一条访问掩码够不够碰到内容或改权限：读 · 写 · 改权限三类位（[`MASK_READ`] · [`MASK_WRITE`] · [`MASK_PERMS`]）
/// （写得进去的能改上游地址把 key 引走，改得了 DACL / 属主的能给自己补读权）。
pub fn mask_reaches(mask: u32) -> bool {
    mask & (MASK_READ | MASK_WRITE | MASK_PERMS) != 0
}

/// 一个主体能做什么，照实际给的位说：可读 · 可写 · 可读写，改得了权限另说一句；一样都没有 ⇒ `None`。
fn can_words(read: bool, write: bool, perms: bool) -> Option<String> {
    let rw = match (read, write) {
        (true, true) => Some(copy_text("credsPerm.judge.canReadWrite", &[])),
        (true, false) => Some(copy_text("credsPerm.judge.canRead", &[])),
        (false, true) => Some(copy_text("credsPerm.judge.canWrite", &[])),
        (false, false) => None,
    };
    let perms = perms.then(|| copy_text("credsPerm.judge.canChangePerms", &[]));
    let words: Vec<String> = rw.into_iter().chain(perms).collect();
    (!words.is_empty()).then(|| words.join(&copy_text("kit.text.sep", &[])))
}

/// 「谁（能做什么）」那几段连成一句。
fn who_can(parts: &[(String, String)]) -> String {
    parts
        .iter()
        .map(|(who, can)| copy_text("credsPerm.judge.whoCan", &[("who", who), ("can", can)]))
        .collect::<Vec<_>>()
        .join(&copy_text("credsPerm.judge.listSep", &[]))
}

/// 判完的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 只给本人（外加 Windows 上的 SYSTEM · Administrators）。
    OwnerOnly,
    /// 过宽。`how` 说清宽在哪，`fix` 说清怎么修 —— **两样都要有**。
    TooWide { how: String, fix: String },
    /// 查不出来。**不是绿灯**：调用方要照 `TooWide` 一样把它显出来。`raw` 同 [`Protection::Undetermined`]。
    Undetermined { why: String, raw: Option<String> },
}

impl Verdict {
    /// 要不要在界面上出声。`OwnerOnly` 之外**全都要**。
    ///
    /// ⚠ 写成「`matches!(self, TooWide{..})`」是错的 —— 那会让
    /// `Undetermined` 静默通过，正是上面那条 fallback 病。
    #[cfg(test)]
    pub fn needs_attention(&self) -> bool {
        !matches!(self, Verdict::OwnerOnly)
    }
}

/// **判断只有这一处。** 两个平台、三种量到的东西，都在这里变成同一种结论。
/// 白名单：属主得是本人（Windows 上另认 [`TRUSTED_SIDS`]），本人之外许碰内容的只有 [`TRUSTED_SIDS`]。
pub fn judge(p: &Protection) -> Verdict {
    let wide = |how: String, fix: String| Verdict::TooWide { how, fix };
    match p {
        Protection::Unix {
            owner,
            owner_name,
            me,
            ..
        } if owner != me => wide(
            copy_text(
                "credsPerm.judge.notOwner",
                &[(
                    "owner",
                    &owner_name.clone().unwrap_or_else(|| owner.to_string()),
                )],
            ),
            copy_text("credsPerm.judge.fixOwner", &[]),
        ),
        Protection::Unix { mode, .. } => {
            // 只看读 · 写两位：执行位碰不到数据文件的内容（同 Windows 那半不看执行位）。
            let parts: Vec<(String, String)> = [
                (copy_text("credsPerm.judge.group", &[]), mode >> 3),
                (copy_text("credsPerm.judge.others", &[]), *mode),
            ]
            .into_iter()
            .filter_map(|(who, bits)| {
                can_words(bits & 0o4 != 0, bits & 0o2 != 0, false).map(|can| (who, can))
            })
            .collect();
            if parts.is_empty() {
                Verdict::OwnerOnly
            } else {
                wide(
                    copy_text(
                        "credsPerm.judge.tooWideUnix",
                        &[
                            ("mode", &format!("{:04o}", mode & 0o7777)),
                            ("who", &who_can(&parts)),
                        ],
                    ),
                    copy_text("credsPerm.judge.fixUnix", &[]),
                )
            }
        }
        Protection::Windows {
            owner,
            owner_name,
            me,
            dacl,
        } => {
            let trusted = |sid: &str| sid == me || TRUSTED_SIDS.contains(&sid);
            if !trusted(owner) {
                return wide(
                    copy_text(
                        "credsPerm.judge.notOwner",
                        &[("owner", owner_name.as_deref().unwrap_or(owner))],
                    ),
                    copy_text("credsPerm.judge.fixOwner", &[]),
                );
            }
            let Some(aces) = dacl else {
                return wide(
                    copy_text("credsPerm.judge.noDacl", &[]),
                    copy_text("credsPerm.judge.fixWindows", &[]),
                );
            };
            // 同一个主体的几条允许项并起来说（照先出现的先后）。
            let mut who: Vec<(&str, Option<&str>, u32)> = Vec::new();
            for a in aces {
                if !a.allow || !mask_reaches(a.mask) || trusted(&a.who) {
                    continue;
                }
                match who.iter_mut().find(|(sid, ..)| *sid == a.who) {
                    Some((_, name, mask)) => {
                        *mask |= a.mask;
                        *name = name.or(a.name.as_deref());
                    }
                    None => who.push((&a.who, a.name.as_deref(), a.mask)),
                }
            }
            let parts: Vec<(String, String)> = who
                .into_iter()
                .filter_map(|(sid, name, mask)| {
                    can_words(
                        mask & MASK_READ != 0,
                        mask & MASK_WRITE != 0,
                        mask & MASK_PERMS != 0,
                    )
                    .map(|can| (name.unwrap_or(sid).to_string(), can))
                })
                .collect();
            if parts.is_empty() {
                Verdict::OwnerOnly
            } else {
                wide(
                    copy_text(
                        "credsPerm.judge.tooWideWindows",
                        &[("who", &who_can(&parts))],
                    ),
                    copy_text("credsPerm.judge.fixWindows", &[]),
                )
            }
        }
        Protection::Undetermined { why, raw } => Verdict::Undetermined {
            why: copy_text("credsPerm.judge.undetermined", &[("why", &why.to_string())]),
            raw: raw.clone(),
        },
    }
}

// ─────────────────────── 读那一半（只有 `harden` 才有） ───────────────────────

/// 开一次、按这个句柄量、交回句柄：调用方从同一个句柄读内容（量的就是读的那一份）。**只读。**
/// 打不开 ⇒ 系统那个错原样（不在 ＝ `NotFound`，调用方各自决定那算什么）。
#[cfg(feature = "harden")]
pub fn open_checked(path: &std::path::Path) -> std::io::Result<(std::fs::File, Verdict)> {
    let f = std::fs::File::open(path)?;
    let v = judge(&measure(&f));
    Ok((f, v))
}

/// 打得开却量不了 / 打不开（不是「不在」）时的结论：查不出来，不是绿灯。
pub fn open_failed(e: &std::io::Error) -> Verdict {
    judge(&Protection::Undetermined {
        why: copy_text(
            "credsPerm.measure.noMetadata",
            &[("why", &copy_core::io_reason(e.kind()))],
        ),
        raw: Some(e.to_string()),
    })
}

/// 一份自家秘密文件此刻的结论（只量不读内容）：不在 ⇒ `None`；打不开 ⇒ 查不出（[`open_failed`]）。
#[cfg(feature = "harden")]
pub fn verdict_at(path: &std::path::Path) -> Option<Verdict> {
    match open_checked(path) {
        Ok((_, v)) => Some(v),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => Some(open_failed(&e)),
    }
}

/// 按句柄量一次。
#[cfg(all(unix, feature = "harden"))]
fn measure(f: &std::fs::File) -> Protection {
    use std::os::unix::fs::MetadataExt;
    match f.metadata() {
        Ok(m) => Protection::Unix {
            mode: m.mode() & 0o7777,
            owner: m.uid(),
            owner_name: user_name(m.uid()),
            // SAFETY: geteuid 不读写任何内存、恒成功。
            me: unsafe { libc::geteuid() },
        },
        Err(e) => Protection::Undetermined {
            why: copy_text(
                "credsPerm.measure.noMetadata",
                &[("why", &copy_core::io_reason(e.kind()))],
            ),
            raw: Some(e.to_string()),
        },
    }
}

/// uid ⇒ 用户名（`getpwuid_r`）；查不到 ⇒ `None`（说法里给 uid）。
#[cfg(all(unix, feature = "harden"))]
fn user_name(uid: u32) -> Option<String> {
    let mut buf = vec![0 as libc::c_char; 4096];
    // SAFETY: 全零的 passwd 只当出参；getpwuid_r 只往 pwd · buf 里写、写不超过给的长度。
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut out: *mut libc::passwd = std::ptr::null_mut();
    let rc = unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut out) };
    if rc != 0 || out.is_null() || pwd.pw_name.is_null() {
        return None;
    }
    // SAFETY: 成功时 pw_name 指向 buf 里一个以 0 结尾的串。
    let name = unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) };
    Some(name.to_string_lossy().into_owned()).filter(|n| !n.is_empty())
}

#[cfg(all(not(any(unix, windows)), feature = "harden"))]
fn measure(_: &std::fs::File) -> Protection {
    Protection::Undetermined {
        why: copy_text("credsPerm.measure.unsupported", &[]),
        raw: None,
    }
}

// ─────────────────────── 写那一半（只有 `harden` 才有） ───────────────────────

/// 建一个只给本人的新文件，并把写句柄交出来。不做「先按 umask 建出来、写进明文、再收窄」：
/// 那条路上，文件出生到收窄之间有一个宽窗口（常见 umask `0022` 下是 `0644`，全机可读），里面已经有明文 ⇒ 本函数管出生那一刻。
///
/// - Unix：`OpenOptions::create_new(true).mode(0o600)` —— `mode` 在创建时生效。用 `create_new`（`O_EXCL`）：目标若已存在（崩溃残骸、或别人预置的符号链接），
///   `create` 会跟随并截断它，那时权限是它的不是我们的；`O_EXCL` 让这种情况直接失败，调用方先删再建。
/// - Windows：`CreateFileW` + `SECURITY_ATTRIBUTES`，DACL 是 `owner_only_sddl` 那一句（`D:P` 断继承）。`CREATE_NEW` 是 `O_EXCL` 的对应物。
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

    let sid = current_user_sid()?;
    let sddl: Vec<u16> = owner_only_sddl(&sid)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let path_w = win_path_core::win32_path(p)?;
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
/// 当前进程那个用户的 SID 串。失败 ⇒ 系统那个错原样（带 OS 码，调用方按种类出原因词；不在这里拼句子）。
fn current_user_sid() -> std::io::Result<String> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(std::io::Error::from)?;
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
        got.map_err(std::io::Error::from)?;
        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        sid_text(tu.User.Sid)
    }
}

/// 按句柄取属主与 DACL，逐条读 ACE（不转 SDDL 文本：文本按位置切会取错带条件的那几形）。
#[cfg(all(windows, feature = "harden"))]
fn measure(f: &std::fs::File) -> Protection {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{LocalFree, HANDLE, HLOCAL, PSID};
    use windows::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
    use windows::Win32::Security::{
        GetAce, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, INHERIT_ONLY_ACE,
        OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    };
    let unreadable = |e: std::io::Error, raw: String| Protection::Undetermined {
        why: copy_text(
            "credsPerm.measure.noSecurity",
            &[("why", &copy_core::io_reason(e.kind()))],
        ),
        raw: Some(raw),
    };
    let me = match current_user_sid() {
        Ok(s) => s,
        Err(e) => {
            let raw = e.to_string();
            return unreadable(e, raw);
        }
    };
    unsafe {
        let mut owner = PSID::default();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut psd = PSECURITY_DESCRIPTOR::default();
        let rc = GetSecurityInfo(
            HANDLE(f.as_raw_handle() as isize),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            Some(&mut dacl),
            None,
            Some(&mut psd),
        );
        if rc.is_err() {
            let e = std::io::Error::from_raw_os_error(rc.0 as i32);
            return unreadable(e, format!("GetSecurityInfo {}", rc.0));
        }
        let owner_sid = owner;
        let read = (|| -> Result<Protection, Protection> {
            let owner = sid_text(owner).map_err(|e| {
                let raw = e.to_string();
                unreadable(e, raw)
            })?;
            let owner_name = account_name(owner_sid);
            if dacl.is_null() {
                return Ok(Protection::Windows {
                    owner,
                    owner_name,
                    me: me.clone(),
                    dacl: None,
                });
            }
            let mut aces = Vec::new();
            for i in 0..u32::from((*dacl).AceCount) {
                let mut p: *mut core::ffi::c_void = std::ptr::null_mut();
                if let Err(e) = GetAce(dacl, i, &mut p) {
                    let e = std::io::Error::from(e);
                    let raw = e.to_string();
                    return Err(unreadable(e, raw));
                }
                let h = &*(p as *const ACE_HEADER);
                if u32::from(h.AceFlags) & INHERIT_ONLY_ACE.0 != 0 {
                    continue;
                }
                // 0 允许 · 9 允许（带回调）· 1 拒绝 · 10 拒绝（带回调）：四种头之后都是 掩码 ＋ SID。别的（对象类）文件上不该有，认不出就说认不出。
                let allow = match h.AceType {
                    0 | 9 => true,
                    1 | 10 => false,
                    other => {
                        return Err(Protection::Undetermined {
                            why: copy_text(
                                "credsPerm.measure.unknownAce",
                                &[("kind", &other.to_string())],
                            ),
                            raw: None,
                        })
                    }
                };
                let a = &*(p as *const ACCESS_ALLOWED_ACE);
                let sid = PSID(&a.SidStart as *const u32 as *mut core::ffi::c_void);
                let who = sid_text(sid).map_err(|e| {
                    let raw = e.to_string();
                    unreadable(e, raw)
                })?;
                aces.push(Ace {
                    allow,
                    who,
                    name: account_name(sid),
                    mask: a.Mask,
                });
            }
            Ok(Protection::Windows {
                owner,
                owner_name,
                me: me.clone(),
                dacl: Some(aces),
            })
        })();
        let _ = LocalFree(HLOCAL(psd.0));
        match read {
            Ok(p) | Err(p) => p,
        }
    }
}

/// 一个 SID ⇒ 账号名（`LookupAccountSidW`：有域 ⇒ `域\名`，内置组那种没有域的 ⇒ 名）；翻不出 ⇒ `None`（说法里给 SID 串）。
#[cfg(all(windows, feature = "harden"))]
unsafe fn account_name(sid: windows::Win32::Foundation::PSID) -> Option<String> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Security::{LookupAccountSidW, SID_NAME_USE};
    let (mut n, mut d) = (0u32, 0u32);
    let mut kind = SID_NAME_USE::default();
    let _ = LookupAccountSidW(
        PCWSTR::null(),
        sid,
        PWSTR::null(),
        &mut n,
        PWSTR::null(),
        &mut d,
        &mut kind,
    );
    if n == 0 {
        return None;
    }
    let mut name = vec![0u16; n as usize];
    let mut dom = vec![0u16; d.max(1) as usize];
    LookupAccountSidW(
        PCWSTR::null(),
        sid,
        PWSTR(name.as_mut_ptr()),
        &mut n,
        PWSTR(dom.as_mut_ptr()),
        &mut d,
        &mut kind,
    )
    .ok()?;
    let text = |v: &[u16], len: u32| String::from_utf16_lossy(&v[..len as usize]);
    let name = text(&name, n);
    let dom = text(&dom, d);
    if name.is_empty() {
        None
    } else if dom.is_empty() {
        Some(name)
    } else {
        Some(format!("{dom}\\{name}"))
    }
}

/// 一个 SID ⇒ 它的串（`S-1-5-…`）。
#[cfg(all(windows, feature = "harden"))]
unsafe fn sid_text(sid: windows::Win32::Foundation::PSID) -> std::io::Result<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    let mut s = PWSTR::null();
    ConvertSidToStringSidW(sid, &mut s).map_err(std::io::Error::from)?;
    let out = s
        .to_string()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e));
    let _ = LocalFree(HLOCAL(s.0 as *mut core::ffi::c_void));
    out
}

#[cfg(test)]
#[path = "../../../../tests/common/creds-core/perm_tests.rs"]
mod tests;
