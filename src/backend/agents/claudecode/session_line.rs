//! Claude 会话记录的一行里，通用层要的那几格怎么读（注册表 `TextFace.line` · `TextFace.line_of` · `TextFace.shape`）：
//! - 会话级那几格（[`SessionLine`]：是谁说的一句 · 标题 · 后台会话 · 时刻 · cwd · 分叉来源）—— 历史清单那一行与全文搜索索引同一份读法；
//! - 正文的形状（[`BodyShape`]：正文几段 · 折叠单元几个）—— 骨架索引按它粗估高度。
//!
//! 字段名（`type` · `sessionKind` · `aiTitle` · `forkedFrom` · `message.content` 的块类型）只住这里；
//! 通用层的扫描器（`history::listing_scan::SessionScan`）只拿翻好的那一行累计，不认这一家的格式。

use crate::agents::{BodyShape, LineRole, SessionLine};
use serde_json::Value;
use std::borrow::Cow;

/// 一行原文（已去掉换行）⇒ 它说的会话级那几格。
///
/// 〔perfC〕先按 [`Head`] 只取要的那几格（其余跳过、不分配：一行里几 MB 的工具输出不再复制一份）；
/// 取不了（不是对象 · 同名键 · 坏行）⇒ 退回整行解成 `Value` 那一条，口径逐格相同（值不是串当没有 · 同名键取最后一个）。
/// 整行都解不成 JSON ⇒ `None`。
pub(crate) fn line_of(text: &str) -> Option<SessionLine> {
    if text.starts_with('{') {
        if let Ok(h) = serde_json::from_str::<Head>(text) {
            return Some(h.line());
        }
    }
    let v = serde_json::from_str::<Value>(text).ok()?;
    Some(Head::of(&v).line())
}

/// 已解析好的一条 ⇒ 同 [`line_of`] 那几格（全文搜索那一路本来就整条解开了）。
pub(crate) fn line_of_value(v: &Value) -> SessionLine {
    Head::of(v).line()
}

/// 一条记录正文的形状：`user` / `assistant` 在 `message.content`（串或块数组），`system` 在顶层 `content`。
/// 正文块（`text`）的字逐段交出去；工具调用 · 工具结果 · 思考 · 图片各算一个折叠单元。没有正文那一格 ⇒ `None`。
pub(crate) fn shape_of(v: &Value) -> Option<BodyShape<'_>> {
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .or_else(|| v.get("content"))?;
    let mut shape = BodyShape::default();
    match content {
        Value::String(s) => shape.prose.push(s),
        Value::Array(blocks) => {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(s) = b.get("text").and_then(Value::as_str) {
                            shape.prose.push(s);
                        }
                    }
                    Some(
                        "tool_use" | "tool_result" | "thinking" | "redacted_thinking" | "image",
                    ) => shape.folded += 1,
                    _ => {}
                }
            }
        }
        _ => return None,
    }
    Some(shape)
}

/// 一行里要的那几格；其余字段跳过（不分配）。同名键 ⇒ 解不出（退回 `Value` 那一条，那边取最后一个）。
#[derive(serde::Deserialize, Default)]
struct Head<'a> {
    #[serde(rename = "type", borrow, default)]
    kind: LooseStr<'a>,
    #[serde(rename = "sessionKind", borrow, default)]
    session_kind: LooseStr<'a>,
    #[serde(borrow, default)]
    timestamp: LooseStr<'a>,
    #[serde(borrow, default)]
    cwd: LooseStr<'a>,
    #[serde(rename = "aiTitle", borrow, default)]
    ai_title: LooseStr<'a>,
    #[serde(rename = "forkedFrom", default)]
    forked_from: Option<Value>,
}

impl<'a> Head<'a> {
    /// 从整条 `Value` 取同样那几格（口径同 `v.get(k).and_then(as_str)`）。
    fn of(v: &'a Value) -> Self {
        let s = |k: &str| LooseStr(v.get(k).and_then(Value::as_str).map(Cow::Borrowed));
        Self {
            kind: s("type"),
            session_kind: s("sessionKind"),
            timestamp: s("timestamp"),
            cwd: s("cwd"),
            ai_title: s("aiTitle"),
            forked_from: v.get("forkedFrom").cloned(),
        }
    }

    /// 翻成通用的那一行：时刻与分叉来源只认人说的 / 回话那两类记录；标题只认 agent 起的 `ai-title`：`custom-title` 是人在 Claude 里 `/rename` 写的，改名只认 monitor 那一格。
    fn line(self) -> SessionLine {
        let role = match self.kind.0.as_deref() {
            Some("user") => Some(LineRole::Said),
            Some("assistant") => Some(LineRole::Replied),
            _ => None,
        };
        let title = match self.kind.0.as_deref() {
            Some("ai-title") => self.ai_title.0,
            // 人在 Claude 里 `/rename` 写的那一条：不是这一行的标题（改名只认 monitor 那一格）。
            Some("custom-title") => None,
            _ => None,
        };
        let talk = role.is_some();
        SessionLine {
            role,
            title: title.map(Cow::into_owned),
            bg: self.session_kind.is("bg"),
            at_ms: self
                .timestamp
                .0
                .as_deref()
                .filter(|_| talk)
                .and_then(crate::common::time::parse_iso8601_ms),
            cwd: self.cwd.0.map(Cow::into_owned).filter(|c| !c.is_empty()),
            forked: self
                .forked_from
                .as_ref()
                .filter(|_| talk)
                .and_then(super::branch::forked_pair),
        }
    }
}

/// 一格「是串就要、不是串当没有」（与 `Value::as_str` 同口径）；不是串的值整个跳过，不报错。
#[derive(Default)]
struct LooseStr<'a>(Option<Cow<'a, str>>);

impl LooseStr<'_> {
    fn is(&self, want: &str) -> bool {
        self.0.as_deref() == Some(want)
    }
}

impl<'de: 'a, 'a> serde::Deserialize<'de> for LooseStr<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
        struct V<'a>(std::marker::PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
            type Value = LooseStr<'a>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E> {
                Ok(LooseStr(Some(Cow::Borrowed(v))))
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
                Ok(LooseStr(Some(Cow::Owned(v.to_owned()))))
            }
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
                Ok(LooseStr(Some(Cow::Owned(v))))
            }
            fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
                Ok(LooseStr(None))
            }
            fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
                Ok(LooseStr(None))
            }
            fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
                Ok(LooseStr(None))
            }
            fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
                Ok(LooseStr(None))
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(LooseStr(None))
            }
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(LooseStr(None))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                while a.next_element::<IgnoredAny>()?.is_some() {}
                Ok(LooseStr(None))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                while a.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(LooseStr(None))
            }
        }
        d.deserialize_any(V(std::marker::PhantomData))
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/session_line_tests.rs"]
mod tests;
