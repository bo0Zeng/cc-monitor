//! 一份会话记录的会话级那几格怎么逐行累计（[`SessionScan`]）：历史清单那一行与全文搜索索引**同一个扫描器**。
//! 每一行原文先交给那一家的适配层翻成一行 [`SessionLine`]（`agents::session_line`，字段名只住适配层），这里只累计、不认格式。
//! 口径（每一格取什么）逐格说明在 `history::listing::analyze_session` 的头注；缓存与「变长只扫尾巴」在 `history::listing` 的清单缓存那一格。

use crate::agents::{LineRole, SessionLine};
use std::borrow::Cow;
use std::path::Path;

/// 扫一份会话记录时逐行累计的那几格（[`analyze_session`] 的口径，逐格说明在它的头注）。清单缓存与搜索索引都存它。
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionScan {
    /// 读到过一行不是合法 UTF-8 ⇒ 那一行和之后的都不算（逐行读到读不动就停，与整份逐行读同一个口径）。
    pub(super) stopped: bool,
    count: u32,
    excerpt: String,
    ai_title: Option<String>,
    started_at: Option<i64>,
    forked: Option<(String, String)>,
    /// 第一条写着工作目录的记录说的那个（全文搜索的项目名从这里取；清单那一行照适配层的项目目录）。
    cwd: Option<String>,
    // CC 2.1.x 后台分身会话（←/bg/退出转后台 fork 出的 worker）——
    // 记录级的后台标记是官方 resume 选择器同款识别信号（内部字段无兼容
    // 承诺，缺失=false 安全降级）。历史列表标 ⚙ 徽标防 resume 选错克隆。
    is_bg: bool,
}

impl SessionScan {
    /// 吸收一行原始字节（带不带换行都行）。不是合法 UTF-8 ⇒ 停（之后的行都不算）。`kind` ＝ 这份记录是哪一家的（`agents::record_kind_of`，跟着这份文件走）。
    pub(super) fn absorb_bytes(&mut self, kind: &str, line: &[u8]) {
        if self.stopped || line.is_empty() {
            return;
        }
        match std::str::from_utf8(line) {
            Ok(s) => self.absorb(kind, s),
            Err(_) => self.stopped = true,
        }
    }

    /// 前面这一段（`self`）接上后面那一段（`later`）：按段序合并与整份顺扫相等。
    pub fn joined(&self, later: &Self) -> Self {
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
        if out.cwd.is_none() {
            out.cwd.clone_from(&later.cwd);
        }
        out.is_bg |= later.is_bg;
        out.stopped = later.stopped;
        out
    }

    /// 吸收一行原文：那一家翻成 [`SessionLine`]（〔perfC〕适配层只取要的那几格，一行里几 MB 的工具输出不复制）；
    /// 解不成 ⇒ 只数行。首条人说的话要看整条记录 ⇒ 还没取到摘录时，人说的那几行另解一次（通常第一条就取到）。
    pub(super) fn absorb(&mut self, kind: &str, line: &str) {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            return;
        }
        self.count += 1;
        let Some(l) = crate::agents::session_line(kind, trimmed) else {
            return;
        };
        self.take(kind, l, || {
            serde_json::from_str::<serde_json::Value>(trimmed)
                .ok()
                .map(Cow::Owned)
        });
    }

    /// 吸收已解析好的一条（全文搜索那一路本来就整条解开了）：同 [`SessionScan::absorb`] 那一套。
    pub fn absorb_value(&mut self, kind: &str, v: &serde_json::Value) {
        self.count += 1;
        let l = crate::agents::session_line_of(kind, v);
        self.take(kind, l, || Some(Cow::Borrowed(v)));
    }

    /// 一行翻好的那几格并进来：后台标记任一行有就算 · 时刻与分叉来源取第一条 · 标题取最新（持续覆盖）·
    /// 工作目录取第一个 · 首句摘要取第一条人说的话（那一家判「谁说的」，`agents::human_speech`）。
    fn take<'v>(
        &mut self,
        kind: &str,
        l: SessionLine,
        whole: impl FnOnce() -> Option<Cow<'v, serde_json::Value>>,
    ) {
        self.is_bg |= l.bg;
        if self.started_at.is_none() {
            self.started_at = l.at_ms;
        }
        if self.forked.is_none() {
            self.forked = l.forked;
        }
        if self.cwd.is_none() {
            self.cwd = l.cwd;
        }
        if l.title.is_some() {
            self.ai_title = l.title;
        }
        if l.role == Some(LineRole::Said) && self.excerpt.is_empty() {
            if let Some(said) = whole().and_then(|v| crate::agents::human_speech(kind, &v)) {
                self.excerpt = copy_core::clip_flat(&said, super::search_rules::EXCERPT_CHARS);
            }
        }
    }

    /// 首条人说的话（截好的）。
    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }

    /// 最新的标题。
    pub fn title(&self) -> Option<&str> {
        self.ai_title.as_deref()
    }

    /// 第一条写着的工作目录。
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// 是后台分身会话。
    pub fn bg(&self) -> bool {
        self.is_bg
    }

    /// 分叉来的：父会话 id。
    pub fn forked_from(&self) -> Option<&str> {
        self.forked.as_ref().map(|(s, _)| s.as_str())
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
        serde_json::to_value(SessionRow {
            session_id,
            record_path: p.to_string_lossy().into_owned(),
            started_at_ms: self
                .started_at
                .unwrap_or_else(|| super::listing::created_ms_or_mtime(p)),
            updated_at_ms: crate::platform::fs::Ro::mtime_ms(p),
            message_count_approx: self.count,
            first_user_excerpt: self.excerpt.clone(),
            ai_title: self.ai_title.clone(),
            cwd: crate::agents::project_dir_of(p),
            is_bg: self.is_bg,
            forked_from_session_id: forked_sid,
            forked_from_message_uuid: forked_uuid,
        })
        .unwrap_or_default()
    }
}

/// 清单里的一行（[`SessionScan::row`] 写、`history_list` · 叫一条会话什么那几处读）：清单缓存与协议之间那一形，**是我们自己的格式**，
/// 不是那一家的记录（读写两头都经这个类型，不按键名取字）。按 sid 只要分组那几格的行只带三格，其余照缺省。
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SessionRow {
    pub(crate) session_id: String,
    /// 记录文件的路径（字符串）。
    #[serde(rename = "jsonlPath")]
    pub(crate) record_path: String,
    pub(crate) started_at_ms: i64,
    pub(crate) updated_at_ms: i64,
    pub(crate) message_count_approx: u32,
    pub(crate) first_user_excerpt: String,
    pub(crate) ai_title: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) is_bg: bool,
    pub(crate) forked_from_session_id: Option<String>,
    pub(crate) forked_from_message_uuid: Option<String>,
}
