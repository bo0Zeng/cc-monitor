//! 安全的文件读取。住 `common/`：observe（几个生产调用点）与 control（`fork_write`）都用它，它是通用的安全读文件、不是 observe 的域逻辑；
//! 平台无关（纯 `std::fs`）、无域知识（不认识账号、会话、帧）。

use copy_core::copy_text;
use std::io::Read;
use std::path::Path;

/// 安全读取：先确认是常规文件（挡掉 FIFO / 字符设备 / socket —— 它们的 `metadata().len()` 报 0 会骗过大小检查，而无上限的读能把内存吃爆，
/// symlink→/dev/zero 几秒涨十几 GB），再 `take(cap)` 限量读。symlink 会被 `metadata()`（跟随）解析到目标类型：目标是常规文件才放行、是设备就拒。
pub(crate) fn read_regular_capped(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{e}"))?;
    if !meta.is_file() {
        return Err(copy_text("beFs.readRegularCapped.notRegular", &[]).into());
    }
    // 先看长度再决定读不读：拒绝一个巨型文件之前不先读 `cap + 1` 进内存。
    // 早退不取代下面那道 `take(cap + 1)`：`metadata` 与 `read_to_end` 之间文件还会长（TOCTOU），两道一起才完整。
    if meta.len() > cap {
        return Err(copy_text(
            "beFs.readRegularCapped.tooBigSized",
            &[
                ("cap", &cap.to_string()),
                ("size", &(meta.len()).to_string()),
            ],
        ));
    }
    let f = std::fs::File::open(path).map_err(|e| format!("{e}"))?;
    let mut buf = Vec::new();
    // take(cap+1)：读到 cap+1 就知道超限了，不必读满整个（可能无界的）文件
    f.take(cap + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{e}"))?;
    if buf.len() as u64 > cap {
        return Err(copy_text(
            "beFs.readRegularCapped.tooBig",
            &[("cap", &cap.to_string())],
        ));
    }
    Ok(buf)
}

#[cfg(test)]
#[path = "../../../tests/backend/common/fs_tests.rs"]
mod tests;
