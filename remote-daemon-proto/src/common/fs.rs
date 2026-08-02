//! U3（2026-08-01）：**安全的文件读取**。
//!
//! # 为什么它在 `common/` 而不是留在 `accounts_query`
//!
//! U3 摸底发现一条**反向依赖**：`fork_write`（control）在 import
//! `accounts_query::read_regular_capped`（observe）。而 §1.1-2 明写
//! 「允许 observe → control 的一条窄接口，**反向不许**」。
//!
//! 但这条边不该用「加个例外」解决 —— `read_regular_capped` 根本不是 observe 的域逻辑，
//! 它是通用的安全读文件。搬进 `common/` 之后**反向边自然消失**，不需要任何豁免。
//! 这是铁律 6 的正例：**改结构让问题不存在，而不是给它开口子。**
//!
//! 三条门槛（见 [`super`]）逐条对：**≥2 层用**（observe 3 个生产调用点 + control 1 个）·
//! **平台无关**（纯 `std::fs`）· **无域知识**（不认识账号、会话、帧）。

use std::io::Read;
use std::path::Path;

/// **安全读取**：先确认是常规文件（挡掉 FIFO / 字符设备 / socket——它们的
/// `metadata().len()` 报 0 会骗过大小检查，而 `read_to_string` 无上限 → 远端 OOM，
/// 审计实测 symlink→/dev/zero 6 秒涨 11GB），再 `take(cap)` 限量读，
/// 一步消掉 metadata↔read 之间的 TOCTOU。symlink 会被 `metadata()`（跟随）解析到
/// 目标类型：目标是常规文件才放行、是设备就拒。
pub(crate) fn read_regular_capped(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{e}"))?;
    if !meta.is_file() {
        return Err("不是常规文件（可能是 FIFO/设备/目录）".into());
    }
    let f = std::fs::File::open(path).map_err(|e| format!("{e}"))?;
    let mut buf = Vec::new();
    // take(cap+1)：读到 cap+1 就知道超限了，不必读满整个（可能无界的）文件
    f.take(cap + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{e}"))?;
    if buf.len() as u64 > cap {
        return Err(format!("超过 {cap} 字节上限"));
    }
    Ok(buf)
}
