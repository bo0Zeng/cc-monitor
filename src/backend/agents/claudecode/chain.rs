//! Claude 记录 ⇒ 链事实（注册表 `RecordFace.chain` 那一格；主线怎么算住通用层 `agents::mainline`）。
//!
//! 链字段：`uuid` · `parentUuid` · `timestamp` · `type`。进链的有 user / assistant / system / attachment，外加看不懂但带身份的记录
//! （它缺席的话它的孩子指向集合外、成了根，被当死胡同整棵折掉）；`queue-operation` 的 `enqueue` 给排队原文（豁免那片裸叶）。
//! 只有 user 记录解析整行（判是不是打断、取原文）；别的只读那几格短串。

use crate::agents::mainline::{ChainFact, Link};
use serde::de::IgnoredAny;
use serde::Deserialize;

#[derive(Deserialize)]
struct Probe {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(rename = "parentUuid", default)]
    parent: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    content: Option<Loose>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Loose {
    Str(String),
    Other(IgnoredAny),
}

/// 一行原文 ⇒ 链事实；不在链上 / 解不出 ⇒ `None`。
pub(crate) fn chain_fact(raw: &str) -> Option<ChainFact> {
    let raw = raw.trim_start_matches('\u{feff}').trim();
    // user 行要整行（判打断、取原文）⇒ 按文本认得出是 user 行就只整行解析这一次，那几格短串从这一份里取；
    // 别的行只解那几格短串（认错了也对：不是 user 行照样从整行里取短串，只是白解了整行）。
    let full: Option<serde_json::Value> = if USER_NEEDLES.iter().any(|n| raw.contains(n)) {
        Some(parsed(serde_json::from_str(raw)).ok()?)
    } else {
        None
    };
    let p: Probe = match &full {
        Some(v) => Probe::deserialize(v).ok()?,
        None => parsed(serde_json::from_str(raw)).ok()?,
    };
    let kind = p.kind.as_deref().unwrap_or("");
    if kind == "queue-operation" {
        return match (p.operation.as_deref(), p.content) {
            (Some("enqueue"), Some(Loose::Str(t))) => Some(ChainFact::Queued(t.trim().to_string())),
            _ => None,
        };
    }
    let (Some(id), Some(at)) = (p.uuid.filter(|u| !u.is_empty()), p.timestamp) else {
        return None;
    };
    let parent = p.parent.filter(|q| !q.is_empty());
    let (said, reply, shown) = match kind {
        "user" => (true, false, true),
        "assistant" => (false, true, true),
        "system" => (false, false, p.subtype.as_deref() == Some("api_error")),
        _ => (false, false, false),
    };
    let (interrupt, text) = if said {
        let v: serde_json::Value = match full {
            Some(v) => v,
            None => parsed(serde_json::from_str(raw)).ok()?,
        };
        let content = v
            .get("message")
            .and_then(|m| m.get("content"))
            .unwrap_or(&serde_json::Value::Null);
        let interrupt = super::text::user_text_of_record(&v)
            .is_some_and(|t| matches!(t.speaker, crate::agents::Speaker::Interrupt));
        (interrupt, Some(first_text(content).trim().to_string()))
    } else {
        (false, None)
    };
    Some(ChainFact::Node(Link {
        id,
        parent,
        at,
        said,
        reply,
        interrupt,
        text,
        shown,
    }))
}

/// user 行在原文里一定带这两截之一（那一家写的是不带空格的 JSON；冒号后带一个空格的也认）。
const USER_NEEDLES: [&str; 2] = [r#""type":"user""#, r#""type": "user""#];

/// 一次整行 / 短串的文本解析：记一笔（本线程解析过几次文本，判据读它：一行至多解析一次）。
fn parsed<T>(r: serde_json::Result<T>) -> serde_json::Result<T> {
    PARSES.with(|c| c.set(c.get() + 1));
    r
}

thread_local! {
    /// 本线程到此刻为链事实解析过几次文本（[`parsed`] 记）。
    pub(crate) static PARSES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// 正文：字符串本身，或第一个带 `text` 的块。
fn first_text(content: &serde_json::Value) -> &str {
    match content {
        serde_json::Value::String(s) => s,
        serde_json::Value::Array(a) => a
            .iter()
            .find_map(|b| b.get("text").and_then(serde_json::Value::as_str))
            .unwrap_or(""),
        _ => "",
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/chain_tests.rs"]
mod tests;
