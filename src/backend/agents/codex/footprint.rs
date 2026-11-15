//! Codex 的**足迹面**：cc-bus 的 Stop 钩子住 `hooks.json` 的哪一格 ·
//! 那一条在 `config.toml` 里的信任对没对上。只算不写：要加的两段（`hooks.json` 那一条 · `config.toml` 那一节）交给待办，
//! 由用户自己贴（用户的文件用户自己改）。
//!
//! 依据（codex-rs，核到 tag `rust-v0.167.0`；名字都是那边的全路径）：`hooks.json` 是 `{"hooks": {"Stop": [{"matcher"?, "hooks": [{"type": "command",
//! "command", "timeout"?}]}]}}`（`codex_config::hook_config::HooksFile` · `codex_protocol::hooks::HookEventsToml`），住每一层配置目录
//! （用户层就是 `$CODEX_HOME/hooks.json`，`codex_hooks::engine::discovery::load_hooks_json`）；不是托管来源的钩子要
//! `hooks.state."<键>".trusted_hash` 等于那一条的哈希才跑；键 ＝ `<那份 hooks.json 的路径>:stop:<组序>:<条序>`（`codex_hooks::hook_key`），
//! 哈希照 [`super::hook_trust::trusted_hash`]，没写超时按 600 秒算（那边找钩子时的规范化）。

use super::accounts::{config_doc, set_string_at, string_at, toml_quote, CONFIG_FILE};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 用户层钩子文件的名字（住 `CODEX_HOME`）。
pub const HOOKS_FILE: &str = "hooks.json";
/// 钩子事件的小写下划线名（键与哈希里那一段）。
const STOP_LABEL: &str = "stop";
/// 没写超时的那一条按这个秒数规范化（Stop 不是 `SessionEnd`）。
const DEFAULT_TIMEOUT_SECS: u64 = 600;

/// 用户层那两份：钩子（`hooks.json`，cc-bus 钩子诊断读第一份）· 配置（钩子信任 · 目录信任住这里）。
fn user_settings(codex_home: &Path) -> [PathBuf; 2] {
    [codex_home.join(HOOKS_FILE), codex_home.join(CONFIG_FILE)]
}

/// `hooks.json` 里 Stop 底下每一组每一条的（组序, 条序, 那一条）——只认 `type = "command"` 的。
fn stop_handlers(v: &Value) -> Vec<(usize, usize, &serde_json::Map<String, Value>)> {
    let Some(groups) = v
        .get("hooks")
        .and_then(|h| h.get("Stop"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        let Some(list) = g.get("hooks").and_then(Value::as_array) else {
            continue;
        };
        for (hi, h) in list.iter().enumerate() {
            if let Some(h) = h.as_object() {
                if h.get("type").and_then(Value::as_str) == Some("command")
                    && h.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|c| !c.trim().is_empty())
                {
                    out.push((gi, hi, h));
                }
            }
        }
    }
    out
}

/// 解析出来的 `hooks.json` ⇒ Stop 底下登记的每条命令（照先后）。
pub fn stop_hook_commands(v: &Value) -> Vec<String> {
    stop_handlers(v)
        .into_iter()
        .filter_map(|(_, _, h)| h.get("command")?.as_str().map(str::to_string))
        .collect()
}

/// 一条命令 ⇒ 要并进 `hooks.json` 的那一项：`hooks.Stop` 数组末尾加一组，组里只这一条。
pub fn stop_hook_slots(command: &str) -> Vec<(Vec<&'static str>, Value)> {
    vec![(
        vec!["hooks", "Stop"],
        json!({"hooks": [{"type": "command", "command": command}]}),
    )]
}

/// 一条 Stop 钩子的信任：它的键 · 照它算的哈希 · `config.toml` 里记的对不对得上 · 要贴进 `config.toml` 的那一节（哈希算好）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopHookTrust {
    pub command: String,
    pub key: String,
    pub hash: String,
    pub trusted: bool,
    pub line: String,
}

/// `hooks_json`（那份钩子文件的路径，键的头一段）· 它解析出来的样子 · 同一层 `config.toml` 的原文 ⇒ Stop 底下每一条的信任。
/// `config.toml` 读不懂 ⇒ 每一条都当没信任。哈希只照这一条的命令与超时算（带 `async` · `statusMessage` · `matcher` 的那几条、Windows 上写了 `commandWindows` 的那几条算出来会对不上，照实报没信任）。
pub fn stop_hook_trust(hooks_json: &Path, hooks: &Value, config: &str) -> Vec<StopHookTrust> {
    let doc = config_doc(config).ok();
    stop_handlers(hooks)
        .into_iter()
        .filter_map(|(gi, hi, h)| {
            let command = h.get("command")?.as_str()?.to_string();
            let timeout = h
                .get("timeout")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .max(1);
            let key = format!("{}:{STOP_LABEL}:{gi}:{hi}", hooks_json.display());
            let hash = super::hook_trust::trusted_hash(STOP_LABEL, &command, timeout);
            let trusted = doc.as_ref().is_some_and(|d| {
                string_at(d, &["hooks", "state", key.as_str(), "trusted_hash"])
                    == Some(hash.as_str())
            });
            let line = format!(
                "[hooks.state.{}]\ntrusted_hash = {}\n",
                toml_quote(&key),
                toml_quote(&hash)
            );
            Some(StopHookTrust {
                command,
                key,
                hash,
                trusted,
                line,
            })
        })
        .collect()
}

/// cc-bus Stop 钩子的信任（足迹面 `hook_trust`）：记在同一层 `config.toml` 的 `hooks.state."<键>".trusted_hash`，
/// 键与哈希照 [`stop_hook_trust`]（键带那份 `hooks.json` 的路径与组序 · 条序）。
const HOOK_TRUST: crate::agents::HookTrust = crate::agents::HookTrust {
    file: |codex_home| codex_home.join(CONFIG_FILE),
    expected: |hooks_json, hooks| {
        stop_hook_trust(hooks_json, hooks, "")
            .into_iter()
            .map(|t| (t.command, t.key, t.hash))
            .collect()
    },
    recorded: |config, key| {
        string_at(
            &config_doc(config).ok()?,
            &["hooks", "state", key, "trusted_hash"],
        )
        .map(str::to_string)
    },
    merge: trust_merge,
};

/// 把 `hooks.state."<键>".trusted_hash = "<值>"` 并进那份 `config.toml`（[`set_string_at`]：已有那一格只换值；
/// 那一项是表头 · 点号键 · 行内表照它原来的写法补；都没有 ⇒ 末尾加一节）。读不懂 · 半路那一格不是表 ⇒ `None`。别的节与键一个字不动。
fn trust_merge(config: &str, key: &str, value: &str) -> Option<String> {
    set_string_at(config, &["hooks", "state", key, "trusted_hash"], value)
}

/// 注册表里 Codex 那一行的足迹面（`agents::Adapter::footprint`）。
/// `tools` · `env`：要新的文案键（工具名），这一份没加 ⇒ 空（交回里列着要加的那几条）。
/// `under_agent_home`：核心只认一个 agent 家（缺省那一家的配置根）⇒ 这一家不认领 `~/.codex/…`，那几条照家目录解析。
pub const FACE: crate::agents::FootprintFace = crate::agents::FootprintFace {
    tools: &[],
    env: &[],
    under_agent_home: |_| None,
    user_settings,
    stop_hook_commands,
    stop_hook_slots,
    hook_trust: Some(HOOK_TRUST),
};

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/footprint_tests.rs"]
mod tests;
