//! v1.7.1：`auto-launch.json` 配置 + monitor 自记录路径机制。
//!
//! 让 cc function（PowerShell 端）能在 monitor 没在跑时主动启动它，
//! 同时保持 cc-monitor.exe 是 portable（不硬编码安装路径）。
//!
//! ## 数据交换
//!
//! 文件：`<monitor_data_dir>/auto-launch.json`
//!
//! ```json
//! { "auto_launch_enabled": false, "monitor_exe_path": "C:\\Users\\...\\cc-monitor.exe" }
//! ```
//!
//! - `monitor_exe_path`：monitor 每次启动时调 `std::env::current_exe()` 写入；用户移动 exe
//!   后下次启动会自动更新
//! - `auto_launch_enabled`：UI 上的 toggle 控制；cc function 检查此 flag 决定是否
//!   `Start-Process` 启动 monitor

use crate::platform::own_fs::OwnState;
use copy_core::copy_text;
use copy_core::said::Said;
use host_core::Capped;
use relay_route_core::{Cap, Over};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct AutoLaunchConfig {
    /// UI toggle 控制：cc function 在 monitor 没跑时是否主动启动它
    #[serde(default)]
    pub auto_launch_enabled: bool,
    /// cc-monitor.exe 的当前路径。monitor 每次启动自动更新。
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

/// 数据目录里那份文件的名字（PowerShell 别名块 `cc.ps1.tpl` 照同一个名字读）。
const FILE: &str = "auto-launch.json";

/// 那份文件的读上限：里面只有一个开关和一条 exe 路径（几百字节）。超了当坏了的那一份：照缺省起、日志说是哪一份；
/// 路径那一格 monitor 每次启动都重记，下一次写就换成好的。
const MAX: Cap = Cap {
    bytes: 64 * 1024,
    over: Over::SkipAndSay,
};

/// 读 auto-launch.json。文件不存在、读不出、超了上限或损坏时返回 default。
pub fn load(own: &OwnState) -> AutoLaunchConfig {
    let Ok(file) = own.path(FILE) else {
        return AutoLaunchConfig::default();
    };
    match host_core::read_capped(&file, MAX) {
        Ok(Capped::Whole(s)) => serde_json::from_str(&s).unwrap_or_default(),
        Ok(Capped::TooBig) => {
            tracing::warn!("{} 超过读上限 {} 字节，照缺省起", file.display(), MAX.bytes);
            AutoLaunchConfig::default()
        }
        Ok(Capped::Absent) | Err(_) => AutoLaunchConfig::default(),
    }
}

/// 写 auto-launch.json：monitor 自己的状态，经 `own_fs` 原子写（临时件先落盘再换名上位），crash 不丢 monitor exe 路径记录。
pub fn save(own: &OwnState, cfg: &AutoLaunchConfig) -> std::io::Result<()> {
    let body = serde_json::to_vec_pretty(cfg).map_err(std::io::Error::other)?;
    own.write(FILE, &body)
}

/// monitor 启动时调：把当前 exe 路径更新到 auto-launch.json（保留 auto_launch_enabled）。
///
/// 用户移动 cc-monitor.exe 后下次启动会自动更新。
pub fn update_monitor_path_on_startup(own: &OwnState) {
    let mut cfg = load(own);
    let Ok(current) = std::env::current_exe() else {
        tracing::warn!("auto_launch: can't resolve current_exe");
        return;
    };
    let current_str = current.to_string_lossy().to_string();
    if cfg.monitor_exe_path.as_deref() == Some(current_str.as_str()) {
        return; // 没变化，不写
    }
    cfg.monitor_exe_path = Some(current_str.clone());
    if let Err(e) = save(own, &cfg) {
        tracing::warn!("auto_launch: save failed: {e}");
    } else {
        tracing::info!("auto_launch: recorded monitor path: {}", current_str);
    }
}

/// 暴露给前端 UI（读 toggle 状态 + 当前记录的 path）
pub fn get_config(own: &OwnState) -> AutoLaunchConfig {
    load(own)
}

/// UI toggle 改变时调
pub fn set_enabled(own: &OwnState, enabled: bool) -> Result<(), Said> {
    let mut cfg = load(own);
    cfg.auto_launch_enabled = enabled;
    save(own, &cfg).map_err(|e| {
        Said::with_raw(
            copy_text(
                "rsShellCmd.autoLaunch.saveFailed",
                &[("why", &copy_core::reason::io_reason(e.kind()))],
            ),
            format!("{}\n{e}", own.root().join(FILE).display()),
        )
    })
}

/// 写 auto-launch.json 的句柄（IPC 命令那一口造一次）：数据目录说不出 ⇒ `None`。
pub fn own_state() -> Option<OwnState> {
    OwnState::issue()
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/auto_launch_tests.rs"]
mod tests;
