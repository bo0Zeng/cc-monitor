//! 安全的文件读取。住 `common/`：observe（几个生产调用点）与 control（`fork_write`）都用它，它是通用的安全读文件、不是 observe 的域逻辑；
//! 平台无关（纯 `std::fs`）、无域知识（不认识账号、会话、帧）。

use crate::common::said::Said;
use copy_core::copy_text;
use std::io::Read;
use std::path::Path;

/// 安全读取：先确认是常规文件（挡掉 FIFO / 字符设备 / socket —— 它们的 `metadata().len()` 报 0 会骗过大小检查，而无上限的读能把内存吃爆，
/// symlink→/dev/zero 几秒涨十几 GB），再 `take(cap)` 限量读。symlink 会被 `metadata()`（跟随）解析到目标类型：目标是常规文件才放行、是设备就拒。
pub(crate) fn read_regular_capped(path: &Path, cap: u64) -> Result<Vec<u8>, Said> {
    // 读不了的那一句只是原因（原因词 · 不是普通文件 · 超出上限）；系统原话进 `raw`（复制详情）。
    let io = |e: std::io::Error| Said::with_raw(copy_core::io_reason(e.kind()), e);
    let meta = std::fs::metadata(path).map_err(io)?;
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
        )
        .into());
    }
    let f = std::fs::File::open(path).map_err(io)?;
    let mut buf = Vec::new();
    // take(cap+1)：读到 cap+1 就知道超限了，不必读满整个（可能无界的）文件
    f.take(cap + 1).read_to_end(&mut buf).map_err(io)?;
    if buf.len() as u64 > cap {
        return Err(copy_text(
            "beFs.readRegularCapped.tooBig",
            &[("cap", &cap.to_string())],
        )
        .into());
    }
    Ok(buf)
}

/// 安全读一份 JSON（同 [`read_regular_capped`]，剥掉 UTF-8 BOM 再解）：读不了 ⇒ 那一句是原因；解不开 ⇒「内容无法解析」，解析器原话进 `raw`。
pub(crate) fn read_json_capped(path: &Path, cap: u64) -> Result<serde_json::Value, Said> {
    let b = read_regular_capped(path, cap)?;
    serde_json::from_slice(b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&b))
        .map_err(|e| Said::with_raw(copy_text("reason.content.unparsable", &[]), e))
}

#[cfg(test)]
#[path = "../../../tests/backend/common/fs_tests.rs"]
mod tests;
