//! OpenAI Responses 那一种上游协议的流面：SSE 的 `data:` 原文 ⇒ 归一事件（[`super::StreamEv`]）。
//!
//! 认的：应答开始（`response.id` 当对账键）· 一项输出开始（`output_index` 当块号；消息 / 思考 / 工具 ＋ 工具名）·
//! 文字增量（正文 · 思考摘要 · 思考原文）· 收尾（正常 / 报错 / 没说完）。
//! 其余（工具入参增量、项完成、用量、不认识的新事件、读不懂的原文）一律不出事件。

use super::{BlockKind, StreamEv};
use serde_json::Value;

/// 这个协议的流面（走这个协议的那几家在 `DefaultUpstream::stream` 里登记它）。
pub(crate) const FACE: super::StreamFace = super::StreamFace { fold };

/// 一个原始事件 ⇒ 零或一个归一事件。
pub(crate) fn fold(data: &str) -> Vec<StreamEv> {
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return Vec::new();
    };
    let index = || v.get("output_index").and_then(Value::as_u64);
    let text = || {
        let i = index()?;
        let s = v.get("delta").and_then(Value::as_str)?;
        Some(StreamEv::Text {
            i,
            s: s.to_string(),
        })
    };
    let ev = match v.get("type").and_then(Value::as_str) {
        Some("response.created") => v
            .get("response")
            .and_then(|r| r.get("id"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(|id| StreamEv::Start {
                rid: id.to_string(),
            }),
        Some("response.output_item.added") => index().and_then(|i| {
            let item = v.get("item")?;
            let kind = match item.get("type").and_then(Value::as_str) {
                Some("message") => BlockKind::Text,
                Some("reasoning") => BlockKind::Thinking,
                Some(t) if t.ends_with("_call") => BlockKind::Tool,
                _ => BlockKind::Other,
            };
            let tool = (kind == BlockKind::Tool)
                .then(|| item.get("name").and_then(Value::as_str).map(str::to_string))
                .flatten();
            Some(StreamEv::Block { i, kind, tool })
        }),
        Some(
            "response.output_text.delta"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_text.delta",
        ) => text(),
        Some("response.completed") => Some(StreamEv::Stop { ok: true }),
        Some("response.failed" | "response.incomplete" | "error") => {
            Some(StreamEv::Stop { ok: false })
        }
        _ => None,
    };
    ev.into_iter().collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/sse_openai_responses_tests.rs"]
mod tests;
