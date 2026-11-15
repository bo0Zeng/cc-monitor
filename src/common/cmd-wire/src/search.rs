//! 全文搜索那一族的线上形（`history-search` 一台的回包 · `history-search-merge` 几台合好的那一份）。
//! 「几台合一份」那一步连结果头那几句住 `face_core::search::merge`（本机后端与手机嵌的 Rust 核心调同一份），
//! 最近优先那一种先后住 `face_core::column::sort_by_recency`；这里只留类型。

use crate::cells::Words;
use crate::listing::Can;
use crate::Machine;
use serde::{Deserialize, Serialize};

/// 一个命中会话的那一行（那台扫出来的；`history-search` 回包里一行的主体）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionHits {
    /// 这个会话是哪一家（线上的 kind）：恢复按它起。全文搜索只扫记录树 ⇒ 都是记录树那一家。
    pub agent: crate::AgentKind,
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub jsonl_path: String,
    /// 显示标题（ai-title / 首条 user 摘要 / sid 前 8 位之一）。
    pub title: String,
    /// 记录文件最后改动的毫秒。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub updated_at: i64,
    pub hits: Vec<SearchHit>,
    /// `hits` 之外这个会话还有几处命中。
    pub more_hits: u32,
    /// 本会话有命中被「全局 snippet 预算用完」挡下了。
    pub hits_truncated: bool,
    /// 后台分身会话。
    pub is_bg: bool,
    /// 那台判的：这个会话能做什么（与历史清单的行同一张规则）。
    pub can: Can,
    /// 那台此刻对这个会话的说法（同历史清单那一行的 `face`）。
    pub face: crate::frame::SessionFace,
}

/// 一条命中。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchHit {
    /// 消息 uuid：打开查看器后据此滚动定位、高亮。
    pub uuid: String,
    /// 谁说的（那台写好的那个词：用户 · 那一家 · 工具 · agent 回报），出口照摆。
    pub who: String,
    pub before: String,
    pub matched: String,
    pub after: String,
    /// 它落在哪次工具调用的卡里（调用 id，同会话内查找的 `tool`）；缺 ⇒ 落在它自己那张卡上。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub tool: Option<String>,
    /// 这条记录那一行在记录文件里的起点（字节；交给 `history-read` 的 `offset` 就从它读起）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub offset: u64,
    /// 这条记录的行号（与实时行的 `seq` 同一个空间；打开查看器后按它物化、按 `uuid` 找卡）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub line: u64,
}

/// `history-search` 回包里的一行：那台扫出来的会话行 ＋ 那台按问的那一方的时区写好的两格时刻字（都按 `updatedAt`）
/// ＋ 那台此刻对这条会话的注解与隐藏（同历史清单那一行，界面不再拿清单去对）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct SearchRow {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub row: SessionHits,
    /// 行尾那一格。
    pub at_text: String,
    /// 内容头那一段。
    pub span_text: String,
    /// 显示的名字：改过的名 ＞ 那台扫出的标题（同历史清单那一行的 `label`）。
    pub label: String,
    /// 星标（那台的注解）。
    pub starred: bool,
    /// 隐藏那一格（那台判的，与历史清单那一行同一条规则）；不隐藏 ⇒ `null`。
    pub hidden: Option<crate::session::Hidden>,
}

/// `history-search` 的回包（一台）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct HistorySearchReply {
    /// 每命中会话一行，最近优先。
    pub sessions: Vec<SearchRow>,
    /// 这一趟有几份会话记录读不动、没搜到。
    pub unreadable: usize,
    /// 内容搜索不覆盖、这台上又有它的会话记录的那几家（对用户的叫法）。
    pub skipped: Vec<String>,
}

/// `history-search-merge` 的回包：几台合好的一份。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Merged {
    /// 各会话命中数（`hits` 条数 ＋ `moreHits`）之和。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub total_hits: u64,
    /// 任一行 `hitsTruncated`（每一台都被自己的 `limit` 砍过）。
    pub truncated: bool,
    /// 各台的会话行，按 `updatedAt` 倒序（同一刻照台的先后）。
    pub sessions: Vec<MergedRow>,
    /// 结果头底下那几句，写好的（两端照摆）：答了的那几台读不动的会话记录份数之和（0 份不写）；
    /// 再是内容搜索不覆盖、只按标题的那几家各一句（各台说的，去重、照先说的那台的先后）。
    pub notes: Vec<String>,
    /// 没答上的那几台（它们的会话没搜到）：哪台 · 那一句。
    pub down: Vec<SearchDown>,
    /// 结果头那一句（搜的词 · 命中几处 · 几个会话；截过 ⇒ 截过那一句）：两端照摆；一台都没答上 ⇒ 缺（没问到不当没有，两端不出结果头）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub summary: Option<String>,
    /// 一行都没有时空态那一句：一台都没答上 ⇒ 搜不了那一句（没问到不当没有）；答了、一处没中 ⇒ 没有匹配那一句；有行 ⇒ 缺。
    /// 两端照摆，不再按 `sessions` 为空自己挑字。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub empty: Option<String>,
}

/// 合好的一行：那台答的那一行 ＋ 哪台。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct MergedRow {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub row: SearchRow,
    /// 哪一台答的这一行（这台逐字 `"<local>"`）。
    pub origin: Machine,
    /// 这一行的项目目录在问的这一端打得开：合的那一步（`face_core::search::merge`）只放缺省（`false`），本机后端合完照哪台答的补（`face_core::history_asks::reveal_of`）；
    /// 手机核心合的那一份不读这一格，不补。
    pub reveal: bool,
    /// 删除确认的正文：同上，合的那一步只放缺省（空），本机后端合完补（`face_core::history_asks::delete_ask`）。
    pub delete_ask: Words,
    /// 「+N 处」那颗的字（还有没摆出来的命中才有，`moreHits` 为 0 ⇒ 缺）：合的那一步写好，界面照抄。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub more_text: Option<String>,
}

/// 没答上的一台：哪台 · 那一句（够不着 · 那台常驻不在 · 那台答了失败 …）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct SearchDown {
    pub origin: Machine,
    pub said: String,
}

/// `history-search` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct HistorySearchReq {
    /// 搜索词（不分大小写，子串）。
    pub query: String,
    /// 工具调用与结果也搜。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub include_tools: Option<bool>,
    /// 只搜哪一种：人说的话 · agent 的回答 · agent 回报；缺 ⇒ 全部。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub scope: Option<SearchScope>,
    /// 只要最近几天的（同清单的 `within_days`）：那台照自己的钟换成时刻。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub within_days: Option<u64>,
    /// 全局片段上限（没给的照样记进 `moreHits`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub limit: Option<u64>,
    /// 只比会话标题与第一句（不搜内容）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub titles: Option<bool>,
}

/// [`HistorySearchReq::scope`] 的三个词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub enum SearchScope {
    User,
    Assistant,
    Report,
}

impl SearchScope {
    /// 口径那一层（后端 `search_query::in_scope`）认的那个词。
    pub fn word(self) -> &'static str {
        match self {
            SearchScope::User => "user",
            SearchScope::Assistant => "assistant",
            SearchScope::Report => "report",
        }
    }
}
