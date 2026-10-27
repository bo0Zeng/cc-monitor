//! Codex 的一条 rollout 记录 ⇒ 一拍（[`crate::agents::beat`]）。分类与「谁说的」用 [`super::record`] 那一份，不另写判别。
//!
//! | 记录 | 一拍 |
//! |---|---|
//! | `payload.id` · 信封 `timestamp` | `id` · `at` |
//! | `response_item.message`：「谁说的」答得出（user / developer / 别家来信）⇒ 用户那侧 `Said`；答不出 ⇒ 代理那侧 | `side`；系统注入 ⇒ `injected` |
//! | message 的 `content` 拍平 | `Text`；`input_image` 那几项 ⇒ `Image` |
//! | assistant 的 `phase == "final_answer"` | `Stop(Done)` |
//! | `reasoning` | `Thinking`（代理那侧） |
//! | 工具调用（`call_id` · `name`） | `Call`（代理那侧）；主参数 · 写了哪个文件：这一家的工具词表没考据 ⇒ 不答 |
//! | 工具输出（`call_id`） | `Result`（用户那侧）；失败与否记录里看不出 ⇒ 不算失败 |
//! | `agent_message` · `inter_agent_communication` | 用户那侧 `Said`（来信） |
//! | 顶层 `token_usage_record` 的 `usage` ＋ `response_id` | 代理那侧，只带 `usage`：新输入 ＝ `input_tokens − cached_input_tokens`（输入含读缓存：`total ＝ input ＋ output`），读缓存 ＝ `cached_input_tokens`；写缓存是否含在输入里看不出 ⇒ 不答 |
//! | 别的（元记录 · 事件） | 簿记 |
//!
//! 型号（`turn_context.model`）不在带用量的那一条上 ⇒ 不答；花费 · 许可档 · 分叉今天不答。

use std::borrow::Cow;

use serde_json::Value;

use super::parse::unwrap_envelope;
use super::record::{call_id, classify, flatten_text, message_said, CodexRecordKind as K};
use crate::agents::beat::{contains, Act, Beat, BeatFace, CallAsk, Side, StopWhy, Usage, Watch};
use crate::agents::Speaker;

/// 注册表 `RecordFace.beats` 那一格。
pub(crate) const BEATS: BeatFace = BeatFace {
    beat: beat_of,
    hint: could_matter,
    // 这一家等人回答 · 批准计划的工具今天没考据 ⇒ 都是别的。
    ask: |_| CallAsk::Plain,
};

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// 正文拍平成一段（空 ⇒ 不出）· 图那几项各一个。
fn content_acts(content: Option<&Value>, acts: &mut Vec<Act<'_>>) {
    let Some(content) = content else {
        return;
    };
    let text = flatten_text(content);
    if !text.is_empty() {
        acts.push(Act::Text(Cow::Owned(text)));
    }
    if let Some(items) = content.as_array() {
        for it in items {
            if str_of(it, "type") == Some("input_image") {
                acts.push(Act::Image);
            }
        }
    }
}

/// 一条已解析的记录 ⇒ 一拍（恒有：不说话的记录就是簿记）。
pub(crate) fn beat_of(v: &Value) -> Option<Beat<'_>> {
    let payload = unwrap_envelope(v).map_or(&Value::Null, |(_, p)| p);
    let mut acts = Vec::new();
    let mut usage = None;
    let side = match classify(v) {
        K::Message => {
            content_acts(payload.get("content"), &mut acts);
            match message_said(payload) {
                Some(who) => {
                    acts.push(Act::Said(who));
                    Side::Input
                }
                None => {
                    if str_of(payload, "phase") == Some("final_answer") {
                        acts.push(Act::Stop(StopWhy::Done));
                    }
                    Side::Reply
                }
            }
        }
        K::AgentMail => {
            content_acts(payload.get("content"), &mut acts);
            acts.push(Act::Said(super::record::agent_mail_said(payload)));
            Side::Input
        }
        K::Reasoning => {
            acts.push(Act::Thinking);
            Side::Reply
        }
        K::ToolCall => {
            acts.push(Act::Call {
                id: Cow::Borrowed(call_id(v).unwrap_or_default()),
                name: Cow::Borrowed(str_of(payload, "name").unwrap_or_default()),
                what: None,
                edits: None,
            });
            Side::Reply
        }
        K::ToolResult => {
            acts.push(Act::Result {
                of: call_id(v).map(Cow::Borrowed),
                failed: false,
            });
            acts.push(Act::Said(crate::agents::UserText::of(Speaker::ToolResult)));
            Side::Input
        }
        K::TokenUsage => {
            usage = payload
                .get("usage")
                .filter(|u| u.is_object())
                .map(|u| usage_of(u, payload));
            Side::Reply
        }
        _ => Side::Note,
    };
    let injected = side == Side::Input
        && acts
            .iter()
            .any(|a| matches!(a, Act::Said(u) if matches!(u.speaker, Speaker::System { .. })));
    Some(Beat {
        id: str_of(payload, "id")
            .filter(|id| !id.is_empty())
            .map(Cow::Borrowed),
        at: super::parse::envelope_ts(v).map(Cow::Borrowed),
        side,
        injected,
        acts,
        usage,
        model: None,
        cost: None,
        mode: None,
        forked_from: None,
    })
}

/// `token_usage_record.usage` ＋ `response_id` ⇒ 用量（输入含读缓存 ⇒ 减掉；写缓存分不出 ⇒ 0）。
fn usage_of<'a>(u: &Value, payload: &'a Value) -> Usage<'a> {
    let num = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    let cached = num("cached_input_tokens");
    Usage {
        input: num("input_tokens").saturating_sub(cached),
        cache_read: cached,
        cache_write: 0,
        write_5m: None,
        write_1h: None,
        output: num("output_tokens"),
        request: str_of(payload, "response_id").map(Cow::Borrowed),
    }
}

/// 这一行有没有可能改动会话事实（快路，`BeatFace.hint`）。**只许放过、不许误拦**：
/// 不是簿记的那几类一拍只出自这三种顶层类型（Codex 写 JSON 不转义 ASCII 字母）；这一家不说 MCP。
fn could_matter(line: &[u8], w: &Watch) -> bool {
    !w.mcp_only
        && (contains(line, b"\"response_item\"")
            || contains(line, b"\"token_usage_record\"")
            || contains(line, b"\"inter_agent_communication\""))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/beat_tests.rs"]
mod tests;
