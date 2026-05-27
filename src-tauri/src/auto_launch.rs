//! v1.7.1：`auto-launch.json` 配置 + monitor 自记录路径机制。
//!
//! 让 cc function（PowerShell 端）能在 monitor 没在跑时主动启动它，
//! 同时保持 monitor.exe 是 portable（不硬编码安装路径）。
//!
//! ## 数据交换
//!
//! 文件：`<monitor_data_dir>/auto-launch.json`
//!
//! ```json
//! { "auto_launch_enabled": false, "monitor_exe_path": "C:\\Users\\...\\monitor.exe" }
//! ```
//!
//! - `monitor_exe_path`：monitor 每次启动时调 `std::env::current_exe()` 写入；用户移动 exe
//!   后下次启动会自动更新
//! - `auto_launch_enabled`：UI 上的 toggle 控制；cc function 检查此 flag 决定是否
//!   `Start-Process` 启动 monitor

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoLaunchConfig {
    /// UI toggle 控制：cc function 在 monitor 没跑时是否主动启动它
    #[serde(default)]
    pub auto_launch_enabled: bool,
    /// monitor.exe 的当前路径。monitor 每次启动自动更新。
    #[serde(default)]
    pub monitor_exe_path: Option<String>,
}

impl Default for AutoLaunchConfig {
    fn default() -> Self {
        Self {
            auto_launch_enabled: false,
            monitor_exe_path: None,
        }
    }
}

/// 读 auto-launch.json。文件不存在或损坏时返回 default。
pub fn load(file: &Path) -> AutoLaunchConfig {
    let Ok(s) = std::fs::read_to_string(file) else {
        return AutoLaunchConfig::default();
    };
    serde_json::from_str(&s).unwrap_or_default()
}

/// 原子写 auto-launch.json — 走 utils::atomic_write_json（Windows ReplaceFileW；
/// 非 Windows rename），确保 crash 不丢 monitor exe 路径记录。
pub fn save(file: &Path, cfg: &AutoLaunchConfig) -> std::io::Result<()> {
    crate::utils::atomic_write_json(file, cfg)
}

/// monitor 启动时调：把当前 exe 路径更新到 auto-launch.json（保留 auto_launch_enabled）。
///
/// 用户移动 monitor.exe 后下次启动会自动更新。
pub fn update_monitor_path_on_startup(monitor_data_dir: &Path) {
    let file = monitor_data_dir.join("auto-launch.json");
    let mut cfg = load(&file);
    let Ok(current) = std::env::current_exe() else {
        tracing::warn!("auto_launch: can't resolve current_exe");
        return;
    };
    let current_str = current.to_string_lossy().to_string();
    if cfg.monitor_exe_path.as_deref() == Some(current_str.as_str()) {
        return; // 没变化，不写
    }
    cfg.monitor_exe_path = Some(current_str.clone());
    if let Err(e) = save(&file, &cfg) {
        tracing::warn!("auto_launch: save failed: {e}");
    } else {
        tracing::info!("auto_launch: recorded monitor path: {}", current_str);
    }
}

/// 暴露给前端 UI（读 toggle 状态 + 当前记录的 path）
pub fn get_config(monitor_data_dir: &Path) -> AutoLaunchConfig {
    load(&monitor_data_dir.join("auto-launch.json"))
}

/// UI toggle 改变时调
pub fn set_enabled(monitor_data_dir: &Path, enabled: bool) -> Result<(), String> {
    let file = monitor_data_dir.join("auto-launch.json");
    let mut cfg = load(&file);
    cfg.auto_launch_enabled = enabled;
    save(&file, &cfg).map_err(|e| format!("save auto-launch.json failed: {e}"))?;
    Ok(())
}

/// 返回 monitor_data_dir 的 PathBuf 便利方法（IPC 命令用）
pub fn data_dir() -> Option<PathBuf> {
    crate::paths::resolve_monitor_data_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn default_when_missing() {
        let tmp = std::env::temp_dir().join(format!("ccm-auto-launch-{}.json", std::process::id()));
        let _ = fs::remove_file(&tmp);
        let cfg = load(&tmp);
        assert!(!cfg.auto_launch_enabled);
        assert!(cfg.monitor_exe_path.is_none());
    }

    #[test]
    fn roundtrip_serialize() {
        let cfg = AutoLaunchConfig {
            auto_launch_enabled: true,
            monitor_exe_path: Some(r"C:\foo\monitor.exe".to_string()),
        };
        let s = serde_json::to_string(&cfg).unwrap();
        let parsed: AutoLaunchConfig = serde_json::from_str(&s).unwrap();
        assert!(parsed.auto_launch_enabled);
        assert_eq!(parsed.monitor_exe_path.unwrap(), r"C:\foo\monitor.exe");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp =
            std::env::temp_dir().join(format!("ccm-auto-launch-rt-{}.json", std::process::id()));
        let _ = fs::remove_file(&tmp);
        let cfg = AutoLaunchConfig {
            auto_launch_enabled: true,
            monitor_exe_path: Some(r"C:\bar\monitor.exe".to_string()),
        };
        save(&tmp, &cfg).unwrap();
        let loaded = load(&tmp);
        assert!(loaded.auto_launch_enabled);
        assert_eq!(loaded.monitor_exe_path.unwrap(), r"C:\bar\monitor.exe");
        let _ = fs::remove_file(&tmp);
    }
}
