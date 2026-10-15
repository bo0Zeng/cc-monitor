//! [`AgentKind`]：线上一家 agent 的名字（kind，如 `claude` · `codex`）—— 收的 `agent` 与回的 `agent` 都是它，线上就是一个字符串。
//! 那一家在后端里面的路由名（中转路由键 · 额度账 · 轮换簿按它记）是另一样东西、不上线：两样之间只在后端 `agents` 那一处换。

use serde::{Deserialize, Serialize};

/// 线上一家 agent 的名字（kind）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"), type = "string")
)]
pub struct AgentKind(String);

impl AgentKind {
    /// 照原样收一个 kind（后端认 kind 的那一处 · 判据用；线上进来的走反序列化）。
    pub fn new(kind: impl Into<String>) -> Self {
        AgentKind(kind.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
