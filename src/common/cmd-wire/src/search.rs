//! 全文搜索那一族的线上形（`history-search` 一台的回包 · `history-search-merge` 几台合好的那一份）与「几台合一份」那一步（[`merge`]）——
//! 两侧调同一份：本机后端扇出问完各台（`faces/search_face.rs`）· 手机嵌的 Rust 核心逐台直连问完（`mobcore::search_merge_json`）。
//!
//! 只并不判：每台的行由那台排好（最近优先）、写好时刻字（按问的那一方的时区）；这里只把几列按 `updatedAt` 并成一列（同一刻照台的先后）、
//! 命中数 · 读不动的份数相加、不覆盖的那几家去重、远端的行添上那台的名字。

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
    pub agent: String,
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

/// `history-search` 回包里的一行：那台扫出来的会话行 ＋ 那台按问的那一方的时区写好的两格时刻字（都按 `updatedAt`）。
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
    /// 答了的那几台读不动、没搜到的会话记录份数之和。
    pub unreadable: usize,
    /// 内容搜索不覆盖的那几家（各台说的，去重、照先说的那台的先后）。
    pub skipped: Vec<String>,
    /// 没答上的那几台（它们的会话没搜到）：哪台 · 那一句。
    pub down: Vec<SearchDown>,
}

/// 合好的一行：那台答的那一行 ＋ 哪台（本机缺）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct MergedRow {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub row: SearchRow,
    /// 那台远端的名字；本机 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<String>"))]
    pub origin: Option<Machine>,
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

/// 照「最近优先」排（`updatedAt` 大的在前，稳定）：每台花 snippet 预算的顺序与几台合一份的顺序都是它。
pub fn sort_by_recency<T>(items: &mut [T], key: impl Fn(&T) -> i64) {
    items.sort_by(|a, b| key(b).cmp(&key(a)));
}

/// **几台的搜索结果合成一份**：`answered` 照台的先后（每台 `(名字, 它答回来的那一份)`），`down` 是没答上的那几台（原样带上）。
/// 纯计算：不读盘、不起进程、不看钟。
pub fn merge(answered: Vec<(Machine, HistorySearchReply)>, down: Vec<SearchDown>) -> Merged {
    let mut sessions: Vec<MergedRow> = Vec::new();
    let mut total_hits: u64 = 0;
    let mut truncated = false;
    let mut unreadable = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    for (machine, reply) in answered {
        let origin = (!machine.is_local()).then_some(machine);
        unreadable = unreadable.saturating_add(reply.unreadable);
        for s in reply.skipped {
            if !skipped.contains(&s) {
                skipped.push(s);
            }
        }
        for row in reply.sessions {
            total_hits = total_hits
                .saturating_add(row.row.hits.len() as u64)
                .saturating_add(u64::from(row.row.more_hits));
            truncated |= row.row.hits_truncated;
            sessions.push(MergedRow {
                row,
                origin: origin.clone(),
            });
        }
    }
    sort_by_recency(&mut sessions, |r| r.row.row.updated_at);
    Merged {
        total_hits,
        session_count: sessions.len(),
        truncated,
        sessions,
        unreadable,
        skipped,
        down,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/search_tests.rs"]
mod tests;
