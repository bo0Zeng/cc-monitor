//! 全文搜索那一族的线上形（`history-search` 一台的回包 · `history-search-merge` 几台合好的那一份）。
//! 「几台合一份」那一步连结果头那几句住 `face_core::search::merge`（本机后端与手机嵌的 Rust 核心调同一份），
//! 最近优先那一种先后住 `face_core::column::sort_by_recency`；这里只留类型。

use crate::cells::Words;
use crate::listing::{Can, ForkedFrom};
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
    /// 分叉来的：父会话的显示标题（同历史清单那一格）；不是分叉来的 · 父会话这一趟没走到 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub forked_from: Option<ForkedFrom>,
    /// 父会话的会话 id（只给那台扫的那一层填上面那一格，不上线）。
    #[serde(skip)]
    #[cfg_attr(any(test, feature = "ts"), ts(skip))]
    pub parent: Option<String>,
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
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub ts_ms: i64,
    /// `user` · `assistant` · `tool` · `report`。
    pub kind: String,
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
    pub session_count: usize,
    /// 任一行 `hitsTruncated`（每一台都被自己的 `limit` 砍过）。
    pub truncated: bool,
    /// 各台的会话行，按 `updatedAt` 倒序（同一刻照台的先后）。
    pub sessions: Vec<MergedRow>,
    /// 结果头底下那几句，写好的（两端照摆）：答了的那几台读不动的会话记录份数之和（0 份不写）；
    /// 再是内容搜索不覆盖、只按标题的那几家各一句（各台说的，去重、照先说的那台的先后）。
    pub notes: Vec<String>,
    /// 没答上的那几台（它们的会话没搜到）：哪台 · 那一句。
    pub down: Vec<SearchDown>,
    /// 结果头那一句（搜的词 · 命中几处 · 几个会话；截过 ⇒ 截过那一句）：两端照摆。
    pub summary: String,
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
    /// 只要这一刻（毫秒）之后的。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub after_ms: Option<u64>,
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
