//! 本进程里几条帧命令对后端其余部分开的门：资产域够用户文件那一扇 · 两台之间枢纽问这台自己那一跳 · 各号 MCP 同步的监听器。

use super::{Run, REGISTRY};
use crate::stream::wire::Request;

/// 资产域（`assets/`）够用户文件的那一扇门：**本进程里那几条 `files-*` 帧命令本身**（阻塞档，原样调它们的 `run`）。
/// 住入方向是因为 `readonly_guard` 第三层只许命令表文件管理那一族（与分派的传输硬臂）够得着写面；资产模块只拿这个句柄，不直呼 `files_write`。
#[derive(Clone, Copy)]
pub(crate) struct LocalFiles;

impl crate::assets::door::Door for LocalFiles {
    fn ask(
        &self,
        cmd: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, (String, String)> {
        let spec = REGISTRY
            .iter()
            .find(|s| s.name == cmd && s.name.starts_with("files-"))
            .ok_or_else(|| ("unknown_command".to_string(), cmd.to_string()))?;
        let Run::Blocking(run) = spec.run else {
            return Err(("unknown_command".to_string(), cmd.to_string()));
        };
        let req = Request {
            id: "in-process".to_string(),
            cmd: cmd.to_string(),
            args,
            within_ms: None,
            until: None,
        };
        run(req).map(|v| v.unwrap_or(serde_json::Value::Null))
    }
}

/// 两台之间那几件的枢纽问**这台自己**的那一跳：本进程那几条内层命令本身（阻塞档，原样调它们的 `run`）。
/// 限 [`HUB_INNER`] 那几条 —— 枢纽不是一扇通到任意命令的门。
pub(crate) struct LocalFrames;

/// 枢纽问得到的内层命令（来源那台读 · 被写那台判 / 写）。
pub(crate) const HUB_INNER: &[&str] = &[
    "cc-bus-install-state",
    "cc-bus-install",
    "mcp-sync-source",
    "mcp-sync-preview",
    "mcp-sync-apply",
    "skill-read",
    "skill-install-plan",
    "skill-install-apply",
];

impl crate::assets::hub::Here for LocalFrames {
    fn ask(
        &self,
        cmd: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, (String, String)> {
        let spec = REGISTRY
            .iter()
            .find(|s| s.name == cmd && HUB_INNER.contains(&s.name))
            .ok_or_else(|| ("unknown_command".to_string(), cmd.to_string()))?;
        let Run::Blocking(run) = spec.run else {
            return Err(("unknown_command".to_string(), cmd.to_string()));
        };
        let req = Request {
            id: "in-process".to_string(),
            cmd: cmd.to_string(),
            args,
            within_ms: None,
            until: None,
        };
        run(req).map(|v| v.unwrap_or(serde_json::Value::Null))
    }
}

pub(super) fn hub_here() -> std::sync::Arc<dyn crate::assets::hub::Here> {
    std::sync::Arc::new(LocalFrames)
}

/// 常驻后端里起「各号的配置文件一变就同步一趟用户级 MCP」那个监听器（写经 [`LocalFiles`]，与帧命令同一扇门）。
pub fn watch_account_mcp() {
    crate::accounts::manage::mcp_share_watch::start(&LocalFiles);
}
