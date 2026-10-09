//! 交给 Win32 文件 API 的路径只从这里来。
//!
//! 标准库的文件函数会自己给超过 260 字符的路径补 `\\?\` 前缀，直接调 Win32 的（`ReplaceFileW` · `MoveFileExW` ·
//! `CreateFileW` · `Get/SetNamedSecurityInfoW` …）不会：传原样路径，过 260 字符就失败。
//! [`win32_long_path`] 是那一步的纯函数（各平台都编、都测）；[`win32_path`] 是 Windows 上调用点用的那一形。

/// 绝对路径的 UTF-16（不带结尾 0）⇒ 带长路径前缀的那一形：
/// `C:\…` → `\\?\C:\…`；`\\服务器\共享\…` → `\\?\UNC\服务器\共享\…`；已带 `\\?\` / `\\.\` 的原样；
/// 别的（相对路径）原样。带了前缀的路径系统不再替你换分隔符 ⇒ `/` 一并换成 `\`。
pub fn win32_long_path(path: impl IntoIterator<Item = u16>) -> Vec<u16> {
    const SEP: u16 = b'\\' as u16;
    let p: Vec<u16> = path.into_iter().collect();
    let is_sep = |c: u16| c == SEP || c == b'/' as u16;
    let at = |i: usize| p.get(i).copied().unwrap_or(0);
    let verbatim = |c: u16| c == b'?' as u16 || c == b'.' as u16;
    if is_sep(at(0)) && is_sep(at(1)) && verbatim(at(2)) && is_sep(at(3)) {
        return p;
    }
    let slashed = |s: &[u16]| -> Vec<u16> {
        s.iter()
            .map(|&c| if c == b'/' as u16 { SEP } else { c })
            .collect()
    };
    let letter = |c: u16| c < 128 && (c as u8).is_ascii_alphabetic();
    let drive = letter(at(0)) && at(1) == b':' as u16 && is_sep(at(2));
    let mut out: Vec<u16> = r"\\?\".encode_utf16().collect();
    if drive {
        out.extend(slashed(&p));
    } else if is_sep(at(0)) && is_sep(at(1)) {
        out.extend("UNC\\".encode_utf16());
        out.extend(slashed(&p[2..]));
    } else {
        return p;
    }
    out
}

/// `p` 交给 Win32 的那一串：先取绝对路径，再补长路径前缀，末尾带 0。
#[cfg(windows)]
pub fn win32_path(p: &std::path::Path) -> std::io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt as _;
    let mut w = win32_long_path(std::path::absolute(p)?.as_os_str().encode_wide());
    w.push(0);
    Ok(w)
}

#[cfg(test)]
#[path = "../../../../tests/common/win-path-core/lib_tests.rs"]
mod tests;
