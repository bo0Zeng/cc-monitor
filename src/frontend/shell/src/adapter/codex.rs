//! Phase 2（Codex 泛化）：OpenAI Codex CLI 适配器（AgentKind 的「第二个样本」）。
//!
//! **F1 slice：只落定位/发现相关字段**（data_root 等）——`active()` 仍默认 Claude，
//! 本适配器经 `for_kind(AgentKind::Codex)` 取用 + 单测覆盖，尚未接进 discovery 派发（后续 slice）。
//! F4（判活）/F6（resume）相关方法先给**占位实现 + 标注**，到那两个 feature 再落实。
//!
//! Codex 事实源：`code-picture/codex-vs-claude-事实对照_2026-07-18.md`（本机实测 codex-cli 0.144.6 +
//! openai/codex 源码/web 交叉核）。要点：会话 `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`
//! （按日期分区、sid=文件名末 UUID）；无 pidfile（判活走 F4 的 logs_2/proc 策略、不用 liveness 目录）。

use super::AgentAdapter;
use std::path::PathBuf;

/// Codex 数据根：`$CODEX_HOME`（若设）→ `~/.codex`。（Claude 侧还有「设置面板手选」一级；Codex 的
/// 用户手选路径留到 settings feature 再加，F1 先 env + 默认。）
fn resolve_codex_dir() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("CODEX_HOME") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return Some(p);
        }
    }
    Some(creds_core::store::home_dir()?.join(".codex"))
}

/// OpenAI Codex CLI 适配器（ZST）。
pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn data_root(&self) -> Option<PathBuf> {
        resolve_codex_dir()
    }
    fn nested_env_to_scrub(&self) -> &'static [&'static str] {
        // F6 占位：Codex 的嵌套会话 env（若有）待 resume feature 实测/查证再填；F1 不用（空安全）。
        &[]
    }
    fn resume_flag(&self) -> &'static str {
        // F6 占位：Codex resume 是**子命令** `codex resume <uuid>`（非 `--flag`）——F6 让命令构建支持
        // subcommand 形；此处先给子命令名，resolve/launch 接线时按 kind 区分 flag vs subcommand。
        "resume"
    }
    fn default_launcher(&self) -> &'static str {
        "codex"
    }
    fn launcher_alias(&self) -> Option<&'static str> {
        None
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/adapter/codex_tests.rs"]
mod tests;
