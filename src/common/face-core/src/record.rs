//! **认不出那一条**的句子与判法：后端适配层认不出的那一行（[`unread_body`]）· 出记录页并条时重写的那一句（[`unread_text`]）·
//! 手机核心解不开的那一条（[`OrUnread`] · 交 Kotlin 之前改那一格的 [`mend_records`]）同这一份。
//! 线上形（`Body::Unread` · `UnreadWhy` · `t` 的闭集 · 摘录的截法）住 `cmd_wire::record`。

use cmd_wire::cells::{Tone, Words};
use cmd_wire::record::{excerpt_of, records_cell, Body, Draw, Record, UnreadWhy, BODY_TAGS};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 认不出那一条的一句：「认不出的记录 · {type}」/「记录形状变了 · {type}」，并了几条 ⇒ 后面带「 · {n} 条」。
pub fn unread_text(why: UnreadWhy, kind: Option<&str>, count: u32) -> Words {
    let kind = kind
        .filter(|k| !k.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| copy_text("record.unread.noType", &[]));
    let n = count.to_string();
    let text = match (why, count > 1) {
        (UnreadWhy::UnknownType, false) => {
            copy_text("record.unread.unknownType", &[("type", &kind)])
        }
        (UnreadWhy::UnknownType, true) => copy_text(
            "record.unread.unknownTypeMany",
            &[("type", &kind), ("n", &n)],
        ),
        (UnreadWhy::ParseFailed, false) => {
            copy_text("record.unread.parseFailed", &[("type", &kind)])
        }
        (UnreadWhy::ParseFailed, true) => copy_text(
            "record.unread.parseFailedMany",
            &[("type", &kind), ("n", &n)],
        ),
    };
    Words(text)
}

/// 认不出的一行（每行一条，`count` 1）：一句 ＋ 原文摘录。
pub fn unread_body(why: UnreadWhy, kind: Option<String>, raw: &str) -> Body {
    Body::Unread {
        why,
        text: unread_text(why, kind.as_deref(), 1),
        kind,
        tone: Tone::Warn,
        excerpt: Words(excerpt_of(raw)),
        count: 1,
    }
}

/// 客户端解记录那一层：[`Record`] 外面包一层，解得开 ⇒ 那一条原样；解不开 ⇒ 认不出那一支（[`unread_body`]，带原样的类名与摘录），
/// 不让整份回包 / 整帧失败（后端照原样的一团写记录，记录的形状不进形状指纹：那一家新出一种记录不该让整页读不出）。
/// 线上照 [`Record`] 写（`transparent`）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct OrUnread(pub Record);

impl OrUnread {
    /// 一团 JSON ⇒ 一条（解不开 ⇒ 认不出那一条：类在 `t` 的闭集里 ⇒ 形状变了，否则没见过的类；
    /// 身份原样有就用，没有 ⇒ 按原文摘一个稳定的；时刻那几格型对就带上）。
    pub fn of(raw: Value) -> Self {
        if let Ok(r) = Record::deserialize(&raw) {
            return Self(r);
        }
        let s = |k: &str| raw.get(k).and_then(Value::as_str).map(str::to_string);
        let kind = s("t");
        let why = match kind.as_deref() {
            Some(t) if BODY_TAGS.contains(&t) => UnreadWhy::ParseFailed,
            _ => UnreadWhy::UnknownType,
        };
        let text = raw.to_string();
        let id = s("id").filter(|i| !i.is_empty()).unwrap_or_else(|| {
            format!(
                "unread:{:016x}",
                cmd_wire::fnv::fnv1a64(text.as_bytes())
            )
        });
        Self(Record {
            agent: s("agent").unwrap_or_default().into(),
            id,
            at: s("at"),
            time_text: s("timeText").map(Words),
            at_ms: raw.get("atMs").and_then(Value::as_i64),
            // 认不出的那一条恒画成一张卡（一行 warn 细条），不判。
            draw: Draw::Card,
            human: false,
            body: unread_body(why, kind, &text),
        })
    }
}

impl<'de> Deserialize<'de> for OrUnread {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Value::deserialize(d).map(Self::of)
    }
}

/// 带记录的那几条回包（`cmd_wire::record::records_cell`）：记录那一格解不开完整记录的，换成认不出那一条（[`OrUnread::of`]：
/// 带原样的类名与摘录，同桌面那几句）；解得开的、别的格都原样不动。
/// 后端照原样的一团写记录、记录的形状不进形状指纹：那一家新出一种记录不该让整页读不出，这一条判定只在这里（手机核心交 Kotlin 之前）。
pub fn mend_records(cmd: &str, data: &mut Value) {
    let Some(list) = records_cell(cmd) else {
        return;
    };
    let Some(items) = data.get_mut(list).and_then(Value::as_array_mut) else {
        return;
    };
    for rec in items.iter_mut().filter_map(|i| i.get_mut("record")) {
        if Record::deserialize(&*rec).is_err() {
            let mended = OrUnread::of(std::mem::take(rec));
            *rec = serde_json::to_value(&mended).unwrap_or_default();
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/record_tests.rs"]
mod tests;
