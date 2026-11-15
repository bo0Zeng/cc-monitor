//! Codex 钩子信任：非托管来源的钩子（`-c` 会话层 · 用户层 `hooks.json`）要 `hooks.state."<键>".trusted_hash`
//! 对上才跑。这一份只管 Codex 内部的那三样规则：信任键怎么拼 · 信任哈希怎么算 · 照哪一版核过（版本门）。
//! 挂法（那一张 `-c hooks={…}` 怎么写）与钩子事件住 [`super::asks`]。
//!
//! 依据（codex-rs，核到 tag `rust-v0.167.0`）：
//! - 键 ＝ `<来源路径>:<事件的小写下划线名>:<组序>:<条序>`（hooks 包的 `hook_key`）；
//!   会话层的来源路径是合成的 `<session-flags>/config.toml` 落在根上（hooks 包找钩子那一段）。
//! - 哈希 ＝ 规范化后那一条（`NormalizedHookIdentity`：事件名 ＋ 那一组，只留这一条、超时补成实值）转 JSON、
//!   键全排序、紧凑写出的 sha256，写成 `sha256:<64 位小写十六进制>`（配置指纹 `codex_config::fingerprint::version_for_toml`）。
//!
//! 这套规则一升版可能就变 ⇒ 版本门（[`GATE`]）＋ 照那一版算的夹具判据钉住。

use crate::agents::HookGate;
use serde_json::{json, Value};

/// 版本门：键名与哈希规则照这一版核过、判据钉过（[`MIN_VERSION`]）；装的那一版认不出 · 更低 ⇒ 不挂。
pub(crate) const GATE: HookGate = HookGate {
    min_version: MIN_VERSION,
    command: super::resume::DEFAULT_COMMAND,
    // npm 装的那一形往上找这个包的 `package.json`（读法是通用的 `agents::installed_version`）。
    package: "@openai/codex",
};

/// 照它核过钩子信任规则的那一版。
pub(crate) const MIN_VERSION: [u64; 3] = [0, 167, 0];

/// 会话层的来源路径（键的头一段）：合成的 `<session-flags>/config.toml` 落在这台的根上。
#[cfg(not(windows))]
pub(crate) const SESSION_SOURCE: &str = "/<session-flags>/config.toml";
/// 同上（Windows 上落在 `C:\`；⚠ 只读过源码，没在真 Windows 上核过）。
#[cfg(windows)]
pub(crate) const SESSION_SOURCE: &str = r"C:\<session-flags>\config.toml";

/// 会话层第 0 组第 0 条那一种的键（每种只挂一组一条）；`label` 是事件的小写下划线名（`stop` · `permission_request` …）。
pub(crate) fn hook_key(label: &str) -> String {
    format!("{SESSION_SOURCE}:{label}:0:0")
}

/// 那一条的信任哈希：规范化后的那一份（事件名 ＋ 只这一条、超时实值、`async` 默认假；没写的可空格不出现）。
pub(crate) fn trusted_hash(label: &str, command: &str, timeout: u64) -> String {
    fingerprint(&json!({
        "event_name": label,
        "hooks": [{ "type": "command", "command": command, "timeout": timeout, "async": false }],
    }))
}

/// Codex 的配置指纹：键全排序、紧凑写出的 JSON 的 sha256，`sha256:` 打头。
pub(crate) fn fingerprint(v: &Value) -> String {
    format!(
        "sha256:{}",
        crate::common::digest::sha256_hex(canonical(v).as_bytes())
    )
}

/// 键全排序、紧凑写出（不靠 `serde_json` 的键序特性）。
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", Value::String(k.clone()), canonical(&m[k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        other => other.to_string(),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/hook_trust_tests.rs"]
mod tests;
