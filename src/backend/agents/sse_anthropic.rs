//! Anthropic Messages 那一种上游协议的流面：SSE 的 `data:` 原文 ⇒ 归一事件（[`super::StreamEv`]）。
//!
//! 只认四种：应答开始（带对账键）· 一块开始（文字 / 思考 / 工具 ＋ 工具名）· 文字增量 · 收尾（正常 / 报错）。
//! 其余（心跳、用量增量、工具入参增量、不认识的新事件、读不懂的原文）一律不出事件。
//! 每发用量另读（[`spent`]）：开头那一件的输入 · 缓存读写，收尾前那一件的输出。

use super::{BlockKind, Spent, StreamEv};
use serde_json::Value;

/// 这个协议的流面（走这个协议的那几家在 `DefaultUpstream::stream` 里登记它）。
/// 这个协议的事件都在上限之内；截断的那一件不认。
pub(crate) const FACE: super::StreamFace = super::StreamFace {
    fold,
    fold_clipped: |_| Vec::new(),
    spent: Some(spent),
};

/// 各类 token 折成输入 token 的比（这个协议的价目：缓存读一折、缓存写 5 分钟档 1.25 倍 · 1 小时档 2 倍、输出 5 倍；各模型同比）。
const CACHE_READ: f64 = 0.1;
const CACHE_WRITE_5M: f64 = 1.25;
const CACHE_WRITE_1H: f64 = 2.0;
const OUTPUT: f64 = 5.0;

/// 一件事件里说出的这一发用量：`message_start` 带输入 · 缓存读写（它那一格输出只是开头一两个，不算）；
/// `message_delta` 带到收尾为止的输出（一发只有一件）。别的事件 · 读不懂 ⇒ `None`。
pub(crate) fn spent(data: &str) -> Option<Spent> {
    if !data.contains("\"usage\"") {
        return None;
    }
    let v: Value = serde_json::from_str(data).ok()?;
    let n = |u: &Value, k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0) as f64;
    match v.get("type").and_then(Value::as_str)? {
        "message_start" => {
            let u = v.get("message")?.get("usage")?;
            let write = match u.get("cache_creation") {
                Some(c) if c.is_object() => {
                    n(c, "ephemeral_5m_input_tokens") * CACHE_WRITE_5M
                        + n(c, "ephemeral_1h_input_tokens") * CACHE_WRITE_1H
                }
                _ => n(u, "cache_creation_input_tokens") * CACHE_WRITE_5M,
            };
            Some(Spent {
                units: n(u, "input_tokens") + write + n(u, "cache_read_input_tokens") * CACHE_READ,
                opens: true,
            })
        }
        "message_delta" => Some(Spent {
            units: n(v.get("usage")?, "output_tokens") * OUTPUT,
            opens: false,
        }),
        _ => None,
    }
}

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
