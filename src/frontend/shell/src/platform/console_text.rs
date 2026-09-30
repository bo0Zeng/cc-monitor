//! 〔P2 · 主会话 09-29 裁〕**控制台子进程写进管道的字节 → 文本。**
//!
//! Windows：控制台程序（PowerShell 5.1 的报错就是这一形）按那台控制台的 OEM 代码页写（`GetOEMCP`，中文系统 936），
//! 按同一个代码页解（`MultiByteToWideChar`）；解不动 ⇒ 退回 UTF-8（有损，话不丢）。其余平台：UTF-8（有损）。
//! 照后端 `platform/win_proc.rs` 先例手写 `extern "system"`，不为两个函数加依赖。
//! ⚠ 只给「按控制台编码写出来的」那一路用：自己把 UTF-8 字节直写标准输出的（PATH 探针）不走这里。

/// 控制台子进程吐出来的一段字节 ⇒ 文本（见模块头注）。
/// 生产调用方只在 Windows 臂（`profile_installer::run_user_path_powershell`）；非 Windows 只有测试用它。
#[cfg_attr(not(windows), allow(dead_code))]
pub fn console_text(bytes: &[u8]) -> String {
    #[cfg(windows)]
    {
        // SAFETY：无参、只读系统设置。
        let cp = unsafe { GetOEMCP() };
        from_code_page(bytes, cp).unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned())
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// 按代码页 `cp` 解一段多字节文本。代码页不认 / 参数不对 ⇒ `None`。
#[cfg(windows)]
fn from_code_page(bytes: &[u8], cp: u32) -> Option<String> {
    if bytes.is_empty() {
        return Some(String::new());
    }
    let len = i32::try_from(bytes.len()).ok()?;
    // SAFETY：第一趟只问要多少个 UTF-16 单元（输出指针空、长度 0）；第二趟往一块恰好那么长的缓冲里写。
    let need = unsafe { MultiByteToWideChar(cp, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0) };
    if need <= 0 {
        return None;
    }
    let mut wide = vec![0u16; need as usize];
    let got = unsafe { MultiByteToWideChar(cp, 0, bytes.as_ptr(), len, wide.as_mut_ptr(), need) };
    if got <= 0 {
        return None;
    }
    wide.truncate(got as usize);
    Some(String::from_utf16_lossy(&wide))
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetOEMCP() -> u32;
    fn MultiByteToWideChar(
        code_page: u32,
        flags: u32,
        multi_byte: *const u8,
        multi_byte_len: i32,
        wide: *mut u16,
        wide_len: i32,
    ) -> i32;
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/platform/console_text_tests.rs"]
mod tests;
