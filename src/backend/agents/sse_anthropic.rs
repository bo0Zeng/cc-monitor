//! Anthropic Messages 那一种上游协议的流面：SSE 的 `data:` 原文 ⇒ 归一事件（[`super::StreamEv`]）。
//!
//! 只认四种：应答开始（带对账键）· 一块开始（文字 / 思考 / 工具 ＋ 工具名）· 文字增量 · 收尾（正常 / 报错）。
//! 其余（心跳、用量增量、工具入参增量、不认识的新事件、读不懂的原文）一律不出事件。

use super::{BlockKind, StreamEv};
use serde_json::Value;

/// 这个协议的流面（走这个协议的那几家在 `DefaultUpstream::stream` 里登记它）。
pub(crate) const FACE: super::StreamFace = super::StreamFace { fold };

/// 一个原始事件 ⇒ 零或一个归一事件。
pub(crate) fn fold(data: &str) -> Vec<StreamEv> {
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return Vec::new();
    };
    let index = || v.get("index").and_then(Value::as_u64);
    let ev = match v.get("type").and_then(Value::as_str) {
        Some("message_start") => v
            .get("message")
            .and_then(|m| m.get("id"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(|id| StreamEv::Start {
                rid: id.to_string(),
            }),
        Some("content_block_start") => index().and_then(|i| {
            let b = v.get("content_block")?;
            let kind = match b.get("type").and_then(Value::as_str) {
                Some("text") => BlockKind::Text,
                Some("thinking") => BlockKind::Thinking,
                Some("tool_use") => BlockKind::Tool,
                _ => BlockKind::Other,
            };
            let tool = (kind == BlockKind::Tool)
                .then(|| b.get("name").and_then(Value::as_str).map(str::to_string))
                .flatten();
            Some(StreamEv::Block { i, kind, tool })
        }),
        Some("content_block_delta") => index().and_then(|i| {
            let d = v.get("delta")?;
            if d.get("type").and_then(Value::as_str) != Some("text_delta") {
                return None;
            }
            let s = d.get("text").and_then(Value::as_str)?;
            Some(StreamEv::Text {
                i,
                s: s.to_string(),
            })
        }),
        Some("message_stop") => Some(StreamEv::Stop { ok: true }),
        Some("error") => Some(StreamEv::Stop { ok: false }),
        _ => None,
    };
    ev.into_iter().collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/sse_anthropic_tests.rs"]
mod tests;
