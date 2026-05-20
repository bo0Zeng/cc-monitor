//! Claude 数据目录与 monitor 自己配置文件的路径解析。
//!
//! ## 三级回退（resolve_claude_dir）
//!
//! 1. 用户在设置面板里手动选的路径 —— 写在 monitor config.json 的 `claudeDir` 字段
//! 2. 环境变量 `CLAUDE_CONFIG_DIR`（Anthropic 官方约定）
//! 3. `~/.claude`（默认）
//!
//! ## 关于循环依赖
//!
//! monitor 自己的 config.json 始终保存在**默认位置** `~/.claude/claudecode-frontend/config.json`，
//! **不**跟随 claudeDir 变化。这样：
//!   - 读 monitor 配置不需要先解析 claudeDir
//!   - 用户切换 Claude 数据目录后，monitor 的 theme/字体设置不会丢
//!   - `claudeDir` 字段只决定 monitor 去哪里找 `projects/` 和 `sessions/`
//!
//! 文档化为"monitor 设置永远在默认位置，'Claude 数据目录'只影响数据源指向"。

use std::path::PathBuf;

/// Claude 数据目录根（即 `.claude` 的实际位置）。所有 projects / sessions 读取都从这里派生。
pub fn resolve_claude_dir() -> Option<PathBuf> {
    if let Some(p) = read_user_override() {
        if p.exists() {
            tracing::info!("claude_dir from user config: {}", p.display());
            return Some(p);
        }
        tracing::warn!(
            "claude_dir from user config does not exist: {} — falling back",
            p.display()
        );
    }
    if let Ok(env_path) = std::env::var("CLAUDE_CONFIG_DIR") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            tracing::info!("claude_dir from CLAUDE_CONFIG_DIR: {}", p.display());
            return Some(p);
        }
        tracing::warn!(
            "CLAUDE_CONFIG_DIR points to non-existent path: {} — falling back",
            p.display()
        );
    }
    let home = dirs::home_dir()?;
    let default_path = home.join(".claude");
    tracing::info!("claude_dir default: {}", default_path.display());
    Some(default_path)
}

/// Monitor 自己的 user-data 目录：始终在 `~/.claude/claudecode-frontend/`，
/// 不跟随 `claudeDir` 变化（避免循环依赖、且保留用户设置在切换数据目录后仍存在）。
pub fn resolve_monitor_data_dir() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join(".claude")
            .join("claudecode-frontend"),
    )
}

/// Monitor 配置文件：`<monitor_data_dir>/config.json`
pub fn resolve_config_path() -> Option<PathBuf> {
    Some(resolve_monitor_data_dir()?.join("config.json"))
}

/// 读取 monitor config.json 的 `claudeDir` 字段（用户在设置面板里写入）。
fn read_user_override() -> Option<PathBuf> {
    let cfg = resolve_config_path()?;
    if !cfg.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(&cfg).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let dir_str = value.get("claudeDir")?.as_str()?;
    if dir_str.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(dir_str))
}
