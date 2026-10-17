//! 全文搜索那两条帧命令（typed，`wire/typed.rs`）：`history-search`（这台搜一遍）· `history-search-merge`（各台的会话行合一份）。
//! 本体（索引 · 口径）在 `observe/search_query.rs`，线上形与几台合一份那一步在 `cmd_wire::search`（手机核心调同一份）；
//! 这里只有入参 · 码 · 接线：`history-search-merge` 由本机后端自己扇出（本机同进程答、别的台经 `--resident-call` 问它的常驻），并好再答。

use crate::observe::search_query::{self, SearchOpts};
use crate::wire::typed::{codes, Command};
use cmd_wire::search::{HistorySearchReply, Merged, SearchDown, SearchRow};

/// `history-search` 的入参。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    /// 全局片段上限（没给的照样记进 `moreHits`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) limit: Option<u64>,
    /// 只比会话标题与第一句（不搜内容）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) titles: Option<bool>,
}

/// [`HistorySearchReq::scope`] 的三个词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

impl crate::wire::typed::Handle for HistorySearch {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistorySearchReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<HistorySearchReply, HistorySearchErr>> + Send + 'static
    {
        async move { search_here(req, cx.tz).await }
    }
}

/// 这台搜一遍（生产那一份：这台的家 · 生产上限 · 此刻的钟）；扫盘挪到阻塞线程池。
async fn search_here(
    req: HistorySearchReq,
    tz: crate::common::time::Tz,
) -> Result<HistorySearchReply, HistorySearchErr> {
    tokio::task::spawn_blocking(move || {
        let annotations = match crate::history::history_annotations::load() {
            crate::history::history_annotations::Loaded::Read(t) => t,
            _ => Default::default(),
        };
        let here = crate::control::session_hidden::Here::read_now();
        let mark = |row: &cmd_wire::search::SessionHits| Marks {
            label: annotations
                .get(&row.session_id)
                .and_then(|e| e.custom_title.clone())
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| row.title.clone()),
            starred: annotations.get(&row.session_id).is_some_and(|e| e.starred),
            hidden: crate::history::session_hidden::hidden_here(
                &here,
                &annotations,
                &row.session_id,
                row.is_bg,
                &|p| crate::history::session_hidden::title_here(p, &annotations),
            ),
        };
        search_at(
            &crate::history::listing::agent_home(),
            req,
            super::read_face::LINES_CAP_BYTES,
            crate::common::time::now_ms(),
            &tz,
            &mark,
        )
    })
    .await
    .unwrap_or_else(|e| Err(HistorySearchErr::Failed(e.to_string())))
}

/// 一行添上的那台的说法：显示的名字 · 星标 · 隐藏那一格（生产 ＝ 这台的注解 ＋ 隐藏规则，同历史清单那一行）。
pub(crate) struct Marks {
    pub(crate) label: String,
    pub(crate) starred: bool,
    pub(crate) hidden: Option<cmd_wire::session::Hidden>,
}

/// [`HistorySearch`] 的本体，家目录 · 回包上限 · 此刻 · 看的那一台的时区 · 每行那台的说法是参数（判据拿夹具喂它、拿小上限验「超了就拒、不截断」）。
/// 生产只有一个上限：`read_face::LINES_CAP_BYTES`。每行添上行尾 `atText` 与内容头 `spanText`（都按 `updatedAt`，按 `tz` 写）与 [`Marks`]。
pub(crate) fn search_at(
    home: &std::path::Path,
    req: HistorySearchReq,
    cap: usize,
    now_ms: i64,
    tz: &crate::common::time::Tz,
    mark: &dyn Fn(&cmd_wire::search::SessionHits) -> Marks,
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
        let at = row.updated_at;
        let (at_text, _, span_text) = crate::common::time::history_times(at, at, at, now_ms, tz);
        let m = mark(&row);
        let row = SearchRow {
            row,
            at_text: at_text.0,
            span_text: span_text.0,
            label: m.label,
            starred: m.starred,
            hidden: m.hidden,
        };
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

/// `history-search-merge` 的入参：问哪几台（本机逐字 `"<local>"`）· 每台怎么搜（同 `history-search`）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistorySearchMergeReq {
    pub(crate) machines: Vec<cmd_wire::Machine>,
    pub(crate) search: HistorySearchReq,
}

codes! {
    /// `history-search-merge` 的码：本机那一台是必答的那一台，它没答上 ⇒ 整次照它的码失败；别的台没答上只进 `down`。
    pub(crate) enum HistorySearchMergeErr {
        /// 本机扫的那一趟失败（围栏 · 读目录）。
        Failed = "failed",
        /// 本机那一份回包过了上限。
        TooLarge = "too_large",
    }
}

/// `history-search-merge`：本机后端自己问到各台（本机同进程搜、别的台经 `--resident-call` 问它的常驻 `history-search`），
/// 几台的结果合成一份再答（`cmd_wire::search::merge`）。没答上的那几台照实进 `down`，别台照常。
pub(crate) struct HistorySearchMerge;

impl Command for HistorySearchMerge {
    const NAME: &'static str = "history-search-merge";
    type Req = HistorySearchMergeReq;
    type Reply = Merged;
    type Err = HistorySearchMergeErr;
}

impl crate::wire::typed::Handle for HistorySearchMerge {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistorySearchMergeReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Merged, HistorySearchMergeErr>> + Send + 'static
    {
        async move {
            let local = search_here(req.search.clone(), cx.tz.clone());
            merge_with(
                req,
                local,
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
        }
    }
}

/// [`HistorySearchMerge`] 的本体（本机那一台怎么答 · 可达表 · 对面由调用方给）。
pub(crate) async fn merge_with(
    req: HistorySearchMergeReq,
    local: impl std::future::Future<Output = Result<HistorySearchReply, HistorySearchErr>> + Send,
    tz: &crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &crate::dial::remote_ask::Table,
    remote: &dyn crate::dial::remote_ask::Remote,
) -> Result<Merged, HistorySearchMergeErr> {
    let local_err = std::sync::Mutex::new(None);
    let local = async {
        // 本机没答上 ⇒ 整次照它的码失败（下面取出来）：这一格只占位。
        local.await.map_err(|e| {
            *local_err.lock().unwrap_or_else(|p| p.into_inner()) = Some(e);
            crate::dial::remote_ask::Said {
                code: None,
                message: String::new(),
            }
        })
    };
    let got = crate::dial::remote_ask::ask_each::<HistorySearch, _>(
        &req.machines,
        &req.search,
        local,
        tz,
        until,
        table,
        remote,
    )
    .await
    .map_err(HistorySearchMergeErr::BadArgs)?;
    if let Some(e) = local_err.into_inner().unwrap_or_else(|p| p.into_inner()) {
        return Err(match e {
            HistorySearchErr::Failed(m) => HistorySearchMergeErr::Failed(m),
            HistorySearchErr::TooLarge(m) => HistorySearchMergeErr::TooLarge(m),
            HistorySearchErr::BadArgs(m) => HistorySearchMergeErr::BadArgs(m),
        });
    }
    let mut answered = Vec::new();
    let mut down = Vec::new();
    for (origin, r) in got {
        match r {
            Ok(reply) => answered.push((origin, reply)),
            Err(s) => down.push(SearchDown {
                origin,
                said: s.message,
            }),
        }
    }
    Ok(cmd_wire::search::merge(answered, down))
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/search_face_tests.rs"]
mod tests;
