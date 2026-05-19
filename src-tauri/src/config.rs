//! 用户配置 R/W —— `~/.claude/claudecode-frontend/config.json`。
//!
//! Rust 端不解释配置内容（schema 在前端定义），只负责原子读写 + 文件缺失时
//! 给出最小骨架。前端 theme.ts / 后续 settings UI 通过 invoke 调到这里。

use serde_json::Value;
use std::path::PathBuf;

#[tauri::command]
pub fn load_config() -> Result<Value, String> {
    let path = config_path().ok_or_else(|| "no home dir".to_string())?;
    if !path.exists() {
        return Ok(default_config());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))
}

#[tauri::command]
pub fn save_config(value: Value) -> Result<(), String> {
    let path = config_path().ok_or_else(|| "no home dir".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let pretty = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, pretty).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename → {}: {e}", path.display()))?;
    Ok(())
}

fn config_path() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join(".claude")
            .join("claudecode-frontend")
            .join("config.json"),
    )
}

/// 文件缺失时返回的最小骨架。仅做占位，前端读到没有 `theme` 字段会用 :root 默认值。
fn default_config() -> Value {
    serde_json::json!({})
}
