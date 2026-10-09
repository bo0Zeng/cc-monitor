//! **本机能力**：界面要按「这台是什么系统」分的那几件事，判定只住这里（界面不再按 User-Agent 猜）。
//! 壳在每个 webview 起页时注入一份 `window.__CCM_HOST__`（[`init_script`]，`lib.rs` 装一个只带起页脚本的小插件），
//! 界面 `settings/host-os.ts` 只读它。每个平台那一行钉在跨语言金样 `tests/__fixtures__/host-facts.golden.json`。

use serde::Serialize;

/// 这台的那几件事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct HostFacts {
    /// 系统：`linux` · `macos` · `windows`（别的原样）。根元素 `data-host-os` 与「这台电脑」那一行的系统名用它。
    pub os: String,
    /// ↗「切到对应终端窗口」在这台上是不是真的：Windows 走 Win32（`EnumWindows` · `SetForegroundWindow`）；Linux 的 X11 会话走 EWMH，
    /// Wayland 会话照常显示、点了由壳照实说「这个桌面上切不了」并给［在 cc-monitor 里打开］（结局族 `desktop-wont-switch`）；
    /// macOS 上那几跳是桩 ⇒ 不显示。
    pub terminal_front: bool,
    /// 本机 shell 说哪种方言（别名块按它写）：`posix` · `powershell`。
    pub shell_dialect: String,
    /// 本机 PATH 上 `ccm` 那份短缓存作不作数（「重新对齐」时顺手作废它）。
    pub ccm_path_cache: bool,
}

/// 这台此刻的那一份。
pub fn host_facts() -> HostFacts {
    HostFacts {
        os: host_core::OS.to_string(),
        terminal_front: cfg!(any(windows, target_os = "linux")),
        shell_dialect: if cfg!(windows) { "powershell" } else { "posix" }.to_string(),
        ccm_path_cache: !cfg!(windows),
    }
}

/// 起页脚本：`window.__CCM_HOST__ = {…};`（每个 webview 一次，早于页面自己的脚本）。
pub fn init_script() -> String {
    format!(
        "window.__CCM_HOST__ = {};",
        serde_json::to_string(&host_facts()).unwrap_or_else(|_| "null".into())
    )
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/platform/host_facts_tests.rs"]
mod tests;
