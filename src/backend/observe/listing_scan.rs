//! 历史清单那一行的逐行扫描：扫一份会话记录时累计的那几格（[`SessionScan`]）与读每一行用的窄探针（[`ListingHead`]）。
//! 口径（每一格取什么）逐格说明在 `history_query::analyze_session` 的头注；缓存与「变长只扫尾巴」在 `history_query` 的清单缓存那一格。
//! 单住一份：探针是读记录文件 schema 的反序列化类型，不是协议类型（不进生成的协议参考）。

use std::path::Path;

/// 扫一份会话记录时逐行累计的那几格（[`analyze_session`] 的口径，逐格说明在它的头注）。
#[derive(Default, Clone)]
pub(super) struct SessionScan {
    /// 读到过一行不是合法 UTF-8 ⇒ 那一行和之后的都不算（逐行读到读不动就停，与整份逐行读同一个口径）。
    pub(super) stopped: bool,
    count: u32,
    excerpt: String,
    ai_title: Option<String>,
    started_at: Option<i64>,
    forked: Option<(String, String)>,
    // Batch11-F32：CC 2.1.x 后台分身会话（←/bg/退出转后台 fork 出的 worker）——
    // 记录级 sessionKind:"bg" 是官方 resume 选择器同款识别信号（内部字段无兼容
    // 承诺，缺失=false 安全降级）。历史列表标 ⚙ 徽标防 resume 选错克隆。
    is_bg: bool,
}

impl SessionScan {
    /// 吸收一行原始字节（带不带换行都行）。不是合法 UTF-8 ⇒ 停（之后的行都不算）。
    pub(super) fn absorb_bytes(&mut self, line: &[u8]) {
        if self.stopped || line.is_empty() {
            return;
        }
        match std::str::from_utf8(line) {
            Ok(s) => self.absorb(s),
            Err(_) => self.stopped = true,
        }
    }

    /// 前面这一段（`self`）接上后面那一段（`later`）：按段序合并与整份顺扫相等。
    pub(super) fn joined(&self, later: &Self) -> Self {
        let mut out = self.clone();
        if out.stopped {
            return out;
        }
        out.count += later.count;
        if out.excerpt.is_empty() {
            out.excerpt.clone_from(&later.excerpt);
        }
        if later.ai_title.is_some() {
            out.ai_title.clone_from(&later.ai_title);
        }
        if out.started_at.is_none() {
            out.started_at = later.started_at;
        }
        if out.forked.is_none() {
            out.forked.clone_from(&later.forked);
        }
        out.is_bg |= later.is_bg;
        out.stopped = later.stopped;
        out
    }

    /// 吸收一行。
    ///
    /// 〔perfC〕先按 [`ListingHead`] 只取要的那几格（其余跳过、不分配：一行里几 MB 的工具输出不再复制一份）；
    /// 取不了（不是对象 · 同名键 · 坏行）⇒ 退回整行解成 `Value` 那一条，口径逐格相同（值不是串当没有 · 同名键取最后一个）。
    /// 首条人说的话要看整条记录 ⇒ 还没取到摘录时，`user` 那几行另解一次 `Value`（通常第一条就取到）。
    pub(super) fn absorb(&mut self, line: &str) {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            return;
        }
        self.count += 1;
        let head = if trimmed.starts_with('{') {
            serde_json::from_str::<ListingHead>(trimmed).ok()
        } else {
            None
        };
        let full;
        let (head, whole) = match head {
            Some(h) => (h, None),
            None => {
                full = match serde_json::from_str::<serde_json::Value>(trimmed) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                (ListingHead::of(&full), Some(&full))
            }
        };
        if !self.is_bg && head.session_kind.is("bg") {
            self.is_bg = true;
        }
        let kind = head.kind.0.as_deref();
        if matches!(kind, Some("user") | Some("assistant")) {
            if self.started_at.is_none() {
                self.started_at = head
                    .timestamp
                    .0
                    .as_deref()
                    .and_then(crate::common::time::parse_iso8601_ms);
            }
            if self.forked.is_none() {
                self.forked = head
                    .forked_from
                    .as_ref()
                    .and_then(super::history_query::forked_pair);
            }
        }
        match kind {
            Some("ai-title") => {
                if let Some(t) = head.ai_title.0 {
                    self.ai_title = Some(t.into_owned()); // 取最新（持续覆盖）
                }
            }
            // CC v2.1.x 起标题记录改名 `custom-title` / `customTitle`（旧的 `ai-title` 在历史记录里仍会出现，两个都认）。
            Some("custom-title") => {
                if let Some(t) = head.custom_title.0 {
                    self.ai_title = Some(t.into_owned());
                }
            }
            Some("user") if self.excerpt.is_empty() => {
                let parsed;
                let v = match whole {
                    Some(v) => v,
                    None => match serde_json::from_str::<serde_json::Value>(trimmed) {
                        Ok(v) => {
                            parsed = v;
                            &parsed
                        }
                        Err(_) => return,
                    },
                };
                let kind = crate::agents::record_tree_kind().unwrap_or_default();
                if let Some(said) = crate::agents::human_speech(kind, v) {
                    self.excerpt = super::search_rules::truncate_excerpt(&said, 120);
                }
            }
            _ => {}
        }
    }

    /// 这一份的清单行。
    pub(super) fn row(&self, p: &Path) -> serde_json::Value {
        let session_id = p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (forked_sid, forked_uuid) = match &self.forked {
            Some((s, u)) => (Some(s.clone()), Some(u.clone())),
            None => (None, None),
        };
        serde_json::json!({
            "sessionId": session_id,
            "jsonlPath": p.to_string_lossy(),
            "startedAtMs": self.started_at.unwrap_or_else(|| super::history_query::created_ms_or_mtime(p)),
            "updatedAtMs": super::fs::mtime_ms(p),
            "messageCountApprox": self.count,
            "firstUserExcerpt": self.excerpt,
            "aiTitle": self.ai_title,
            "cwd": crate::agents::project_dir_of(p),
            "isBg": self.is_bg,
            "forkedFromSessionId": forked_sid,
            "forkedFromMessageUuid": forked_uuid,
        })
    }
}

/// 扫清单时一条记录里要的那几格；其余字段跳过（不分配）。同名键 ⇒ 解不出（退回 `Value` 那一条，那边取最后一个）。
#[derive(serde::Deserialize, Default)]
pub(super) struct ListingHead<'a> {
    #[serde(rename = "type", borrow, default)]
    kind: LooseStr<'a>,
    #[serde(rename = "sessionKind", borrow, default)]
    session_kind: LooseStr<'a>,
    #[serde(borrow, default)]
    timestamp: LooseStr<'a>,
    #[serde(rename = "aiTitle", borrow, default)]
    ai_title: LooseStr<'a>,
    #[serde(rename = "customTitle", borrow, default)]
    custom_title: LooseStr<'a>,
    #[serde(rename = "forkedFrom", default)]
    forked_from: Option<serde_json::Value>,
}

impl<'a> ListingHead<'a> {
    /// 从整条 `Value` 取同样那几格（退回那一条用；口径同 `v.get(k).and_then(as_str)`）。
    fn of(v: &'a serde_json::Value) -> Self {
        let s = |k: &str| {
            LooseStr(
                v.get(k)
                    .and_then(serde_json::Value::as_str)
                    .map(std::borrow::Cow::Borrowed),
            )
        };
        Self {
            kind: s("type"),
            session_kind: s("sessionKind"),
            timestamp: s("timestamp"),
            ai_title: s("aiTitle"),
            custom_title: s("customTitle"),
            forked_from: v.get("forkedFrom").cloned(),
        }
    }
}

/// 一格「是串就要、不是串当没有」（与 `Value::as_str` 同口径）；不是串的值整个跳过，不报错。
#[derive(Default)]
struct LooseStr<'a>(Option<std::borrow::Cow<'a, str>>);

impl LooseStr<'_> {
    fn is(&self, want: &str) -> bool {
        self.0.as_deref() == Some(want)
    }
}

impl<'de: 'a, 'a> serde::Deserialize<'de> for LooseStr<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
        use std::borrow::Cow;
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
