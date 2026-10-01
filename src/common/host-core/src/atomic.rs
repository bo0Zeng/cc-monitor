//! 前端自有状态文件（不是用户文件）的原子覆盖〔原 `src/frontend/shell/src/utils.rs`，逐字搬来〕。
//!
//! 早期 `bind.rs` / `history.rs` / `auto_launch.rs` 三处各自手写 `write(tmp) + remove + rename` 三步非原子，crash 即丢；
//! 统一走本 helper 后 Windows 上走 `ReplaceFileW`，非 Windows 走 `std::fs::rename`，全程原子。
//! **作用范围限于** monitor 数据目录（`~/.cc-monitor/`）下前端自己的产物（详 IPC-PROTOCOL.md § 通用约束）；
//! 用户文件一律经后端文件管理面写（INVARIANTS § 4）。今天的调用方：monitor 的 `bind.rs` · `auto_launch.rs`、文件窗口的书签。

/// 原子 JSON 写入。Windows 走 `ReplaceFileW`（dst 不存在时 fallback 到 rename），
/// 非 Windows 走 `std::fs::rename`。失败时清理 tmp。
///
/// 设计权衡：本 helper 不做 backup / 写后校验（那是 profile_installer 的职责）；
/// 这里只解决"原子覆盖"问题，避免 `remove + rename` 中间 crash 丢文件。
pub fn atomic_write_json<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let fname = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "out.json".to_string());
    let tmp = path.with_file_name(format!("{fname}.ccm-tmp-{ms}-{}", std::process::id()));
    std::fs::write(&tmp, body)?;
    let r = atomic_replace_path(&tmp, path);
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

#[cfg(windows)]
fn atomic_replace_path(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_WRITE_THROUGH};

    let to_wide = |p: &std::path::Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let src_w = to_wide(src);
    let dst_w = to_wide(dst);

    if !dst.exists() {
        // dst 不存在 ReplaceFileW 会失败；首次写直接 rename
        return std::fs::rename(src, dst);
    }

    unsafe {
        ReplaceFileW(
            PCWSTR(dst_w.as_ptr()),
            PCWSTR(src_w.as_ptr()),
            PCWSTR::null(),
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.message().to_string()))
    }
}

#[cfg(not(windows))]
fn atomic_replace_path(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}
