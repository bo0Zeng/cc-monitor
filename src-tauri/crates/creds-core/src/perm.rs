//! **`KS5` + `KS11` 的住址**：把这份文件收窄到只给本人（`KS5`），
//! 以及读之前查一次它是不是被放宽了（`KS11`）。
//!
//! # 两个平台**同一个签名**，各自的边界各自写清（`KS5`）
//!
//! 形状照 `src-tauri/src/platform_fs.rs::make_executable` 那个现成先例
//! （Unix 置 `0o700`、Windows 文档化的无操作、**签名两边一致**、由宿主注入给平台无关的一半）。
//!
//! ⚠⚠ **但本模块的 Windows 那半不许照抄那个「无操作」**：
//! `make_executable` 可以无操作（Windows 靠扩展名判可执行，本来就没有那个位）；
//! 而「**谁能读这个文件**」在 Windows 上**是有对应物的**（DACL）——
//! 写成无操作等于**两个平台的保证不一样而代码里看不出来**，
//! 那正是本工作区最贵的那族病（量具/口径的作用域对不上事实）。
//! ⇒ 由 [`tests::the_windows_half_is_not_a_no_op`] 逐条钉住。
//!
//! # 为什么 Windows 那半要**显式设 DACL 并断继承**，而不是靠 `%LOCALAPPDATA%` 的默认
//!
//! `%LOCALAPPDATA%` 的默认 ACL 已经只给「本人 + SYSTEM + Administrators」，
//! **但只靠继承是脆的**：目录被搬过、从一个宽松的父目录继承、或者用户自己改过，ACL 就变了，
//! 而**没有任何东西会告诉你它变了**。⇒ 建文件时显式设 DACL（`SetNamedSecurityInfoW`）
//! 并置 `PROTECTED_DACL_SECURITY_INFORMATION`（= 断继承）。
//!
//! ⭐ **设 DACL 与「能手编」不冲突**：ACL 限的是**谁**能读写，不是**用什么程序**读写。
//! 文件的属主照样能用记事本打开改。（被推翻的 DPAPI 冲突的是**格式**，不是权限，两者别混。）
//!
//! # 写与读的能力**是分开的**，而且是编译期分开的
//!
//! - [`make_private`] 只在 `harden` feature 打开时存在 ⇒ **daemon 那侧根本调不到它**
//!   （`K-H2a` 裁四：daemon 只许读）。这是编译器兜的，不是一条判据兜的 ——
//!   `scanning_guard_registry` 头注逐字写着本仓的偏好：「**让它写不出来，而不是再检测一遍**」。
//! - [`probe`] / [`judge`] 两侧都在。
//!
//! # ⚠ 诚实边界：没有判据钉「平台原语只许住这里」
//!
//! 这条是从 `platform_fs.rs` 的**诚实边界 `10g`** 原样继承来的：
//! `backend/mod.rs` 那条只扫 `backend/`，daemon 的 `fallback_guard` 只扫 `platform/`，
//! **两条都扫不到 `src-tauri/crates/`** ⇒ 有人在别处再写一个平台 cfg，**不会红**。
//! 今天靠约定。⚠ 而本 crate 是 `crates/` 这一层里**第一个**带平台 cfg 的
//! （现打 08-27：另外 6 个 crate 平台 cfg 全树 0 处），所以这条边界比在 `platform_fs.rs` 里更该说清。

/// 我们在盘上**量到**的保护状态。**平台中立** —— 判断住 [`judge`]，一处。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Protection {
    /// POSIX 的 mode 位（已 `& 0o777`）。
    Unix { mode: u32 },
    /// Windows：DACL 里出现的**宽泛主体**（`Everyone` / `Users` 一类）的 SDDL 简称。
    /// 空 = 一个宽泛主体都没有。
    Windows { wide_principals: Vec<String> },
    /// **量不出来。** ⚠ 这一档**不许**被当成「没问题」——
    /// daemon 那个 `platform/fallback_guard.rs` 整篇讲的就是这一条：
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

/// Windows 上算「宽泛」的主体（SDDL 简称）。**白名单的反面：这是一张点名表，射程写在下面。**
///
/// `WD` = Everyone · `BU` = BUILTIN\Users · `AU` = Authenticated Users · `IU` = Interactive ·
/// `WR` = Restricted Code。
///
/// ⚠ **射程**：它认的就是这 5 个简称。有人用**完整 SID**写同一个主体
/// （`S-1-1-0` 就是 Everyone），本表**看不见**。这一形没做，如实记在这里 ——
/// `KS4` 那条「黑名单必漏」的道理在这里同样成立，
/// 只是这一格今天**没有白名单可用**（DACL 里合法出现的主体集合不是一个能枚举的东西）。
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
                    how: format!(
                        "同机器上的别人也读得到它（mode 是 {:04o}，本人之外还开着 {:03o}）",
                        mode & 0o7777,
                        extra
                    ),
                    fix: "跑 `chmod 600 <这份文件>`，或者让程序重写一次它".to_string(),
                }
            }
        }
        Protection::Windows { wide_principals } => {
            if wide_principals.is_empty() {
                Verdict::OwnerOnly
            } else {
                Verdict::TooWide {
                    how: format!(
                        "这份文件的 DACL 里有宽泛主体：{}",
                        wide_principals.join(" / ")
                    ),
                    fix: "在资源管理器的「属性 → 安全」里删掉 Everyone / Users 这类条目，\
                          或者让程序重写一次它（会显式设 DACL 并断掉继承）"
                        .to_string(),
                }
            }
        }
        Protection::Undetermined { why } => Verdict::Undetermined {
            why: format!(
                "查不出这份文件的权限（{why}）。**这不等于它没问题** —— \
                 请自己确认一次只有你读得到它。"
            ),
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
                why: format!("读不到它的元数据：{e}"),
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
            why: "这一份构建没有开 `harden`，读不了 DACL".to_string(),
        };
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        return Protection::Undetermined {
            why: "这个平台上不知道怎么量文件权限".to_string(),
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
/// 与 `platform_fs.rs::make_executable` 同形：收一个路径，返回 `Result<(), String>`，
/// 调用方只知道「写完要让它只给本人」，不知道**这个平台上那句话怎么落**。
#[cfg(feature = "harden")]
pub fn make_private(p: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("把凭据文件收成只给本人失败: {e}"))?;
        return Ok(());
    }
    #[cfg(windows)]
    {
        return windows_set_owner_only_dacl(p);
    }
    #[cfg(not(any(unix, windows)))]
    {
        // ⚠ 这里**必须**是错，不许是 `Ok(())`。理由整段见 daemon 的 `platform/fallback_guard.rs`：
        // 一个答不上来的问题不该有一个看起来无害的答案。
        Err(format!(
            "这个平台上不知道怎么把 {} 收成只给本人 —— 没做到，不假装做到了",
            p.display()
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
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| format!("打不开本进程令牌: {}", e.message()))?;
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
        got.map_err(|e| format!("取不到当前用户 SID: {}", e.message()))?;
        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut s = PWSTR::null();
        ConvertSidToStringSidW(tu.User.Sid, &mut s)
            .map_err(|e| format!("SID 转不成串: {}", e.message()))?;
        let out = s.to_string().map_err(|e| format!("SID 串不是合法 UTF-16: {e}"))?;
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
        .map_err(|e| format!("SDDL 转不成安全描述符: {}", e.message()))?;
        let mut present = BOOL(0);
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut defaulted = BOOL(0);
        let got = GetSecurityDescriptorDacl(psd, &mut present, &mut dacl, &mut defaulted);
        if got.is_err() || !present.as_bool() || dacl.is_null() {
            let _ = LocalFree(HLOCAL(psd.0));
            return Err("从 SDDL 里取不出 DACL —— 没有设成只给本人".to_string());
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
            return Err(format!("设 DACL 失败（Win32 错误码 {}）", rc.0));
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
                why: format!("读不到它的 DACL（Win32 错误码 {}）", rc.0),
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
                why: "DACL 转不成 SDDL 串".to_string(),
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
mod tests {
    use super::*;

    fn production() -> String {
        guard_core::production_code(include_str!("perm.rs"))
    }

    /// ★★ **`KS5` 的机检那一半：Windows 那半不许是「无操作」。**
    ///
    /// # 窗口有界 + 次数钉死（`KP4` 那两个价钱，一个都不省）
    ///
    /// `KP4` 逐字记着：零命中守卫「读起来有牙」而审计一刀就绕过去了，
    /// 原因是窗口是 `[\s\S]*?`（无界）+ 登记表把次数登记成 `null`（不钉次数）。
    /// ⇒ 这里的窗口是**那个函数的函数体**（花括号配平切出来），并带**两条反空真自检**：
    /// 切不出函数体 ⇒ 红；切出来跨进下一个 item ⇒ 红。次数一律用**相等断言**。
    #[test]
    fn the_windows_half_is_not_a_no_op() {
        let prod = production();
        let body = fn_body(&prod, "fn windows_set_owner_only_dacl")
            .expect("切不出 `windows_set_owner_only_dacl` 的函数体 —— 本条按红处理，不是绿");

        // 反空真自检：窗口不许跨进下一个 item。
        assert!(
            !body.contains("\nfn ") && !body.contains("\npub fn "),
            "窗口跨进了下一个函数 —— 窗口无界，下面的断言不算数"
        );
        assert!(body.len() > 300, "窗口只有 {} 字节 —— 切法坏了", body.len());

        // ① 真的去改「谁能读这个文件」，**恰好一次**。
        let n_set = body.matches("SetNamedSecurityInfoW(").count();
        assert_eq!(
            n_set, 1,
            "Windows 那半调 `SetNamedSecurityInfoW` 的次数是 {n_set}，应当恰好 1 —— \
             0 次 = 它是个无操作（`KS5` 逐字禁止），多次 = 这条判据量错了对象"
        );
        // ② **断继承**，恰好一次。少了它就是「DACL 设上了但父目录的继承项还会回来」。
        //
        // ⚠ needle 带着那个 `| ` 是**改过一次的**：第一版数的是裸符号名，
        //   而它在 `use` 里还出现一次 ⇒ 实测「2 次，应当 1 次」当场红。
        //   红得对（次数钉死就是要这样），但它数的是**引入**不是**用上** ——
        //   靶子该是那个按位或表达式，因为「设 DACL」与「断继承」是同一次调用的两个位。
        let n_prot = body
            .matches("| PROTECTED_DACL_SECURITY_INFORMATION")
            .count();
        assert_eq!(
            n_prot, 1,
            "断继承那一位出现 {n_prot} 次，应当恰好 1 —— \
             只设 DACL 不断继承，是「看起来做了」的形状（`§0b` 第 3 条整条讲的就是这个）"
        );

        // ③ **派发点**：`make_private` 里必须真的调它，否则上面两条守的是一段死代码。
        let mp = fn_body(&prod, "pub fn make_private")
            .expect("切不出 `make_private` 的函数体 —— 本条按红处理");
        assert_eq!(
            mp.matches("windows_set_owner_only_dacl(").count(),
            1,
            "`make_private` 没有恰好一次派发到 Windows 那半 —— \
             上面两条就成了守着一段没人调的代码"
        );
        // ④ 第三类平台**不许凭空返回成功**（daemon `fallback_guard` 那条道理的同款）。
        assert!(
            mp.contains("不假装做到了"),
            "`make_private` 的非 unix/windows 分支没有诚实报错"
        );
        assert!(
            !mp.contains("let _ = p; // "),
            "`make_private` 里出现了 `make_executable` 那种「参数照收、什么都不做」的形状"
        );
    }

    /// 取一个函数的函数体（含花括号）。**只认签名的行首锚点**，且要求它恰好出现一次。
    fn fn_body<'a>(src: &'a str, sig: &str) -> Option<&'a str> {
        let hits = src.matches(sig).count();
        if hits != 1 {
            return None; // 0 = 找不着；>1 = 认不准，两种都按切不出处理
        }
        let at = src.find(sig)?;
        let open = src[at..].find('{')? + at;
        let b = src.as_bytes();
        let (mut depth, mut i) = (0i32, open);
        while i < src.len() {
            match b[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&src[open..=i]);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// `KS11`：Unix 上「过宽」的判断，以及它**说不说得清怎么修**。
    #[test]
    fn a_unix_mode_wider_than_owner_only_is_called_out_with_a_fix() {
        assert_eq!(judge(&Protection::Unix { mode: 0o600 }), Verdict::OwnerOnly);
        assert_eq!(judge(&Protection::Unix { mode: 0o400 }), Verdict::OwnerOnly);
        // 分母 = 我列出的这 4 形（组可读 / 其他可读 / 全开 / 只多一位）。
        for mode in [0o640, 0o604, 0o666, 0o601] {
            match judge(&Protection::Unix { mode }) {
                Verdict::TooWide { how, fix } => {
                    assert!(how.contains(&format!("{mode:04o}")), "说法里没有实际 mode：{how}");
                    assert!(fix.contains("chmod 600"), "没说清怎么修：{fix}");
                }
                other => panic!("mode {mode:04o} 应当判过宽，实得 {other:?}"),
            }
        }
    }

    /// `KS11`：Windows 那半按 DACL 里的宽泛主体判。
    #[test]
    fn a_windows_dacl_with_a_wide_principal_is_called_out() {
        let tight = wide_principals_in_sddl("D:P(A;;FA;;;S-1-5-21-1-2-3-1001)(A;;FA;;;SY)");
        assert!(tight.is_empty(), "只给本人 + SYSTEM 的 DACL 不该被判宽：{tight:?}");
        assert_eq!(judge(&Protection::Windows { wide_principals: tight }), Verdict::OwnerOnly);

        let loose = wide_principals_in_sddl("D:AI(A;;FA;;;WD)(A;;0x1200a9;;;BU)(A;;FA;;;SY)");
        assert_eq!(loose, vec!["WD".to_string(), "BU".to_string()]);
        match judge(&Protection::Windows { wide_principals: loose }) {
            Verdict::TooWide { how, fix } => {
                assert!(how.contains("WD") && how.contains("BU"), "说法里没点名主体：{how}");
                assert!(fix.contains("Everyone"), "没说清怎么修：{fix}");
            }
            other => panic!("带 Everyone 的 DACL 应当判过宽，实得 {other:?}"),
        }
    }

    /// ★ **「查不出来」不是绿灯。**
    ///
    /// 这一条守的是 daemon `fallback_guard.rs` 整篇讲的那个失败模式：
    /// 给一个答不上来的问题编一个看起来无害的答案。
    #[test]
    fn undetermined_is_not_a_green_light() {
        let v = judge(&Protection::Undetermined {
            why: "这一份构建没有开 `harden`".to_string(),
        });
        assert!(v.needs_attention(), "「查不出来」被当成了没问题");
        match &v {
            Verdict::Undetermined { why } => {
                assert!(why.contains("这不等于它没问题"), "说法太软：{why}");
                assert!(why.contains("harden"), "说法里没带上原因：{why}");
            }
            other => panic!("应当是 Undetermined，实得 {other:?}"),
        }
        // 非空对照：唯一不用出声的就是 OwnerOnly。
        assert!(!Verdict::OwnerOnly.needs_attention());
        assert!(Verdict::TooWide {
            how: String::new(),
            fix: String::new()
        }
        .needs_attention());
    }

    /// `probe` 在 Unix 上真的量到了盘上的位（不是编的）。
    #[cfg(unix)]
    #[test]
    fn probe_reads_the_real_mode_off_the_disk() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("kh2a-perm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let f = dir.join("probe-target");
        std::fs::write(&f, b"{}").expect("写夹具");

        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).expect("放宽");
        assert_eq!(probe(&f), Protection::Unix { mode: 0o644 });
        assert!(judge(&probe(&f)).needs_attention(), "0644 应当出声");

        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).expect("收紧");
        assert_eq!(probe(&f), Protection::Unix { mode: 0o600 });
        assert_eq!(judge(&probe(&f)), Verdict::OwnerOnly);

        // 不存在的文件 ⇒ **查不出来**，不是「没问题」。
        assert!(matches!(
            probe(&dir.join("nope")),
            Protection::Undetermined { .. }
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `KS5` 行为那一半（Unix）：`make_private` 真的把一份宽文件收成了 `0o600`。
    #[cfg(all(unix, feature = "harden"))]
    #[test]
    fn make_private_really_narrows_a_wide_file_to_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("kh2a-harden-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let f = dir.join("harden-target");
        std::fs::write(&f, b"{}").expect("写夹具");
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o666)).expect("先放宽");

        // 非空对照：动手之前它**确实是宽的**（否则下面那条断言可能恒真）。
        assert_eq!(probe(&f), Protection::Unix { mode: 0o666 });
        assert!(judge(&probe(&f)).needs_attention());

        make_private(&f).expect("收紧应当成功");
        assert_eq!(probe(&f), Protection::Unix { mode: 0o600 });
        assert_eq!(judge(&probe(&f)), Verdict::OwnerOnly);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
