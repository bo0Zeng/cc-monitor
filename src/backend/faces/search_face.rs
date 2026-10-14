//! 全文搜索那两条帧命令（typed，`stream/inbound/typed.rs`）：`history-search`（这台搜一遍）· `history-search-merge`（各台的会话行合一份）。
//! 本体（索引 · 口径 · 合并规则）在 `observe/search_query.rs`；这里只有线上那三个类型和接线。

use crate::observe::search_query::{self, MachineHits, Merged, SearchOpts, SessionHits};
use crate::stream::inbound::typed::{codes, Async, Blocking, Command, Cx};

/// `history-search` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistorySearchReq {
    /// 搜索词（不分大小写，子串）。
    pub(crate) query: String,
    /// 工具调用与结果也搜。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) include_tools: Option<bool>,
    /// 只搜哪一种：人说的话 · agent 的回答 · agent 回报；缺 ⇒ 全部。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) scope: Option<SearchScope>,
    /// 只要这一刻（毫秒）之后的。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) after_ms: Option<u64>,
    /// 全局片段上限（`hitCount` 仍报全量）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) limit: Option<u64>,
    /// 只比会话标题与第一句（不搜内容）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) titles: Option<bool>,
}

/// [`HistorySearchReq::scope`] 的三个词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum SearchScope {
    User,
    Assistant,
    Report,
}

impl SearchScope {
    /// 口径那一层（`search_query::in_scope`）认的那个词。
    pub(crate) fn word(self) -> &'static str {
        match self {
            SearchScope::User => "user",
            SearchScope::Assistant => "assistant",
            SearchScope::Report => "report",
        }
    }
}

/// `history-search` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistorySearchReply {
    /// 每命中会话一行，最近优先。
    pub(crate) sessions: Vec<SessionHits>,
    /// 这一趟有几份会话记录读不动、没搜到。
    pub(crate) unreadable: usize,
    /// 内容搜索不覆盖、这台上又有它的会话记录的那几家（对用户的叫法）。
    pub(crate) skipped: Vec<String>,
}

codes! {
    /// `history-search` 自己的码。
    pub(crate) enum HistorySearchErr {
        /// 扫的那一趟失败（围栏 · 读目录）。
        Failed = "failed",
        /// 整份回包过了上限（不截断：截断的清单会被当成完整的用）。
        TooLarge = "too_large",
    }
}

/// `history-search`：在这台的记录树里全文搜索。
pub(crate) struct HistorySearch;

impl Command for HistorySearch {
    const NAME: &'static str = "history-search";
    type Req = HistorySearchReq;
    type Reply = HistorySearchReply;
    type Err = HistorySearchErr;
}

impl Blocking for HistorySearch {
    fn run(req: HistorySearchReq, _cx: &Cx) -> Result<HistorySearchReply, HistorySearchErr> {
        search_at(
            &crate::history::listing::agent_home(),
            req,
            super::read_face::LINES_CAP_BYTES,
        )
    }
}

/// [`HistorySearch`] 的本体，家目录与回包上限是参数（判据拿夹具喂它、拿小上限验「超了就拒、不截断」）。
/// 生产只有一个值：`read_face::LINES_CAP_BYTES`。
pub(crate) fn search_at(
    home: &std::path::Path,
    req: HistorySearchReq,
    cap: usize,
) -> Result<HistorySearchReply, HistorySearchErr> {
    let opts = SearchOpts {
        include_tools: req.include_tools == Some(true),
        scope: req.scope.map(SearchScope::word),
        after_ms: req
            .after_ms
            .map_or(0, |n| i64::try_from(n).unwrap_or(i64::MAX)),
        limit: SearchOpts::limit_of(req.limit),
        titles: req.titles == Some(true),
    };
    let mut sessions = Vec::new();
    let mut size = 0usize;
    let unreadable = search_query::search_into(home, &req.query, &opts, &mut |row| {
        // 上限照逐行那一形算（每行一份 JSON ＋ 换行）。
        size += serde_json::to_vec(&row).map_or(0, |b| b.len()) + 1;
        sessions.push(row);
        Ok(())
    })
    .map_err(HistorySearchErr::Failed)?;
    if size > cap {
        return Err(HistorySearchErr::TooLarge(
            super::read_face::too_large_at(size, cap).1,
        ));
    }
    Ok(HistorySearchReply {
        sessions,
        unreadable,
        skipped: crate::agents::content_search_skips()
            .into_iter()
            .map(str::to_string)
            .collect(),
    })
}

/// `history-search-merge` 的入参：各台答回来的会话行，按台。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistorySearchMergeReq {
    pub(crate) machines: Vec<MachineHits>,
}

codes! {
    /// `history-search-merge` 自己没有码（只有入参形状不对的 `bad_args`）。
    pub(crate) enum HistorySearchMergeErr {}
}

/// `history-search-merge`：把各台的搜索结果合成一份（纯计算，不读盘 ⇒ 不进阻塞档）。
pub(crate) struct HistorySearchMerge;

impl Command for HistorySearchMerge {
    const NAME: &'static str = "history-search-merge";
    type Req = HistorySearchMergeReq;
    type Reply = Merged;
    type Err = HistorySearchMergeErr;
}

impl Async for HistorySearchMerge {
    fn run(
        req: HistorySearchMergeReq,
        cx: Cx,
    ) -> impl std::future::Future<Output = Result<Merged, HistorySearchMergeErr>> + Send + 'static
    {
        let now_ms = crate::common::time::now_secs().saturating_mul(1_000);
        std::future::ready(Ok(search_query::merge_at(req.machines, now_ms, &cx.tz)))
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/search_face_tests.rs"]
mod tests;
