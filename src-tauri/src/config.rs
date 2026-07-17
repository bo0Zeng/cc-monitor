//! 用户配置 R/W —— `~/.claude/claudecode-frontend/config.json`。
//!
//! Rust 端不解释配置内容（schema 在前端定义），只负责原子读写 + 文件缺失时
//! 给出最小骨架。前端 theme.ts / settings UI 通过 invoke 调到这里。
//!
//! 配置文件位置走 `paths::resolve_config_path` —— monitor 自己的设置永远在
//! 默认 `~/.claude/claudecode-frontend/` 下，不跟随 `claudeDir` 字段变化。

use crate::paths;
use serde_json::Value;

#[tauri::command]
pub fn load_config() -> Result<Value, String> {
    let path = paths::resolve_config_path().ok_or_else(|| "no home dir".to_string())?;
    if !path.exists() {
        return Ok(default_config());
    }
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))
}

#[tauri::command]
pub fn save_config(value: Value) -> Result<(), String> {
    let path = paths::resolve_config_path().ok_or_else(|| "no home dir".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let pretty = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, pretty).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    atomic_replace(&tmp, &path).map_err(|e| format!("replace → {}: {e}", path.display()))?;
    Ok(())
}

/// 把 src 原子替换到 dst。
/// std::fs::rename 在 Windows 上目标文件已存在时会失败（不像 POSIX 原子覆盖），
/// 所以这里走 MoveFileExW(MOVEFILE_REPLACE_EXISTING)；非 Windows 走 std::fs::rename。
#[cfg(windows)]
fn atomic_replace(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING};

    let to_wide = |p: &std::path::Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let src_w = to_wide(src);
    let dst_w = to_wide(dst);
    unsafe {
        MoveFileExW(
            PCWSTR(src_w.as_ptr()),
            PCWSTR(dst_w.as_ptr()),
            MOVEFILE_REPLACE_EXISTING,
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.message().to_string()))
    }
}

#[cfg(not(windows))]
fn atomic_replace(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

/// 文件缺失时返回的最小骨架。仅做占位，前端读到没有 `theme` 字段会用 :root 默认值。
fn default_config() -> Value {
    serde_json::json!({})
}
