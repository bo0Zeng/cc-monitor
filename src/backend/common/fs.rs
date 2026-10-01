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

use copy_core::copy_text;
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
        return Err(copy_text("beFs.readRegularCapped.notRegular", &[]).into());
    }
    // ★**先看长度再决定读不读**。
    //
    // 这里本来就已经 `metadata()` 了（上面判 `is_file`），却没用 `meta.len()` ——
    // 于是拒绝一个 5 GB 文件之前要先把 `cap + 1`（256 MiB）读进内存，
    // 而这条路存在的全部理由就是「别让巨型文件吃爆内存」。**安全拒绝本身成了 OOM 候选。**
    //
    // ⚠ 早退**不取代**下面那道 `take(cap + 1)`：`metadata` 与 `read_to_end` 之间文件还会长
    //   （TOCTOU），长过头时仍要靠 `take` 兜住。两道一起才完整。
    // ★ 顺带把一个**测不出来的问题整个绕开**了：报告怀疑「拒绝路径上 `Vec` 倍增会瞬时
    //   同时持有 1×+2×」，V1 在 glibc 上实测不成立，但后端是 **musl** 交叉编译的、
    //   musl 的 realloc 行为没测出来。走这条早退就根本不分配。
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
