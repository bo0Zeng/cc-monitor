//! OpenAI Responses 那一种上游协议的流面：SSE 的 `data:` 原文 ⇒ 归一事件（[`super::StreamEv`]）。
//!
//! 认的：应答开始（`response.id` 当对账键）· 一项输出开始（`output_index` 当块号；消息 / 思考 / 工具 ＋ 工具名）·
//! 文字增量（正文 · 思考摘要 · 思考原文）· 收尾（正常 / 报错 / 没说完）。
//! 其余（工具入参增量、项完成、用量、不认识的新事件、读不懂的原文）一律不出事件。

use super::{BlockKind, StreamEv};
use serde_json::Value;

/// 这个协议的流面（走这个协议的那几家在 `DefaultUpstream::stream` 里登记它）。
pub(crate) const FACE: super::StreamFace = super::StreamFace { fold, fold_clipped };

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

/// 一件被截断的事件（只有开头那一截）：开头 / 收尾那几件带整份应答对象，常常超上限。
/// 只认开头就有的两格 —— 顶层 `type` 与 `response.id` —— 折出开始 / 收尾；别的类型、认不出 ⇒ 空。不出字（半截内容不是内容）。
pub(crate) fn fold_clipped(head: &str) -> Vec<StreamEv> {
    let (ty, rid) = head_fields(head);
    let ev = match ty.as_deref() {
        Some("response.created") => rid
            .filter(|id| !id.is_empty())
            .map(|rid| StreamEv::Start { rid }),
        Some("response.completed") => Some(StreamEv::Stop { ok: true }),
        Some("response.failed" | "response.incomplete") => Some(StreamEv::Stop { ok: false }),
        _ => None,
    };
    ev.into_iter().collect()
}

/// 半截 JSON 对象的开头里顶层 `type` 与 `response.id`（截断处之前读到多少算多少；形状不对就停）。
fn head_fields(head: &str) -> (Option<String>, Option<String>) {
    let mut s = Scan {
        b: head.as_bytes(),
        i: 0,
    };
    let (mut ty, mut rid) = (None, None);
    let _ = (|| -> Option<()> {
        s.expect(b'{')?;
        loop {
            let key = s.string()?;
            s.expect(b':')?;
            match key.as_str() {
                "type" => ty = Some(s.string()?),
                "response" => {
                    s.expect(b'{')?;
                    loop {
                        let k = s.string()?;
                        s.expect(b':')?;
                        if k == "id" {
                            rid = Some(s.string()?);
                            return Some(());
                        }
                        s.skip_value()?;
                        if !s.comma()? {
                            return Some(());
                        }
                    }
                }
                _ => s.skip_value()?,
            }
            if !s.comma()? {
                return Some(());
            }
        }
    })();
    (ty, rid)
}

/// 只够读开头几格的 JSON 扫描器：读到截断处（或形状不对）就答 `None`。
struct Scan<'a> {
    b: &'a [u8],
    i: usize,
}

impl Scan<'_> {
    fn ws(&mut self) {
        while self.b.get(self.i).is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }
    fn expect(&mut self, c: u8) -> Option<()> {
        self.ws();
        (self.b.get(self.i) == Some(&c)).then(|| self.i += 1)
    }
    /// 逗号 ⇒ `true`；收尾括号 ⇒ `false`；别的 / 截断 ⇒ `None`。
    fn comma(&mut self) -> Option<bool> {
        self.ws();
        match self.b.get(self.i)? {
            b',' => {
                self.i += 1;
                Some(true)
            }
            b'}' => Some(false),
            _ => None,
        }
    }
    /// 一个完整的串（带转义）；没收尾 ⇒ `None`。
    fn string(&mut self) -> Option<String> {
        self.ws();
        let start = self.i;
        if self.b.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        loop {
            match self.b.get(self.i)? {
                b'\\' => self.i += 2,
                b'"' => {
                    self.i += 1;
                    return serde_json::from_slice(&self.b[start..self.i]).ok();
                }
                _ => self.i += 1,
            }
        }
    }
    /// 跳过一个值（串 · 数 · 字面量 · 嵌套的对象与数组）；截断 ⇒ `None`。
    fn skip_value(&mut self) -> Option<()> {
        self.ws();
        match *self.b.get(self.i)? {
            b'"' => self.string().map(|_| ()),
            b'{' | b'[' => {
                let mut depth = 0usize;
                loop {
                    match *self.b.get(self.i)? {
                        b'"' => {
                            self.string()?;
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                self.i += 1;
                                return Some(());
                            }
                        }
                        _ => {}
                    }
                    self.i += 1;
                }
            }
            _ => {
                while self
                    .b
                    .get(self.i)
                    .is_some_and(|c| !matches!(c, b',' | b'}' | b']'))
                {
                    self.i += 1;
                }
                self.b.get(self.i).map(|_| ())
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/sse_openai_responses_tests.rs"]
mod tests;
