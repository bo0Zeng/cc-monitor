//! `history-turns`：一轮的摘要，从扫描图上切一页；还没收尾的最后一轮由调用方按此刻的会话事实补字。

use super::locate;
use super::{jlen, Pager};
use std::path::Path;

/// `history-turns` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryTurnsReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 从这个字节起扫（缺 ⇒ 0；续取传还在跑的那一轮的 `at`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) from: Option<u64>,
    /// 可选：一页最多几轮（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) limit: Option<u64>,
}

/// `history-turns` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistoryTurns {
    /// 起点字节（＝ 请求的 `from`）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
    /// 这一页管到哪个字节（有下一页 ⇒ ＝ `next`；最后一页 ⇒ 最后一个完整行的末字节，残尾不计）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    /// 下一页从哪个字节起（＝ 下一页第一轮的 `at`）；没有下一页 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) next: Option<u64>,
    /// 这一段里的每一轮，文件序；`startText` · `endText` 是看的那一台钟上的 `HH:MM`（界面照抄；解不出 ⇒ 空串）。
    pub(crate) turns: Vec<crate::observe::recognize::turns::TurnRow>,
}

/// `history-turns` 的一页，上限是参数。来源同 [`super::index::index_page`]。每一轮先按看的那一台的钟写好再算字节；
/// 还没收尾的最后一轮只在最后一页、交 `dress` 补字（补完超了这一页 ⇒ 挪到下一页）。
pub(crate) fn turns_page(
    home: &Path,
    req: HistoryTurnsReq,
    tz: &crate::Tz,
    cap: usize,
    dress: &dyn Fn(&mut crate::observe::recognize::turns::TurnRow),
) -> Result<HistoryTurns, String> {
    let path = req.path.as_str();
    let from = req.from.unwrap_or(0);
    let fixed = jlen(&HistoryTurns {
        from: u64::MAX,
        end: u64::MAX,
        next: Some(u64::MAX),
        turns: Vec::new(),
    });
    let mut pager = Pager::new(cap, fixed, req.limit);
    let mut turns: Vec<crate::observe::recognize::turns::TurnRow> = Vec::new();
    let mut costs: Vec<usize> = Vec::new();
    let mut next = None;
    let mut take =
        |row: &crate::observe::recognize::turns::TurnRow, next: &mut Option<u64>| -> bool {
            let mut t = row.clone();
            t.stamp(tz);
            let c = jlen(&t) + 1;
            if !pager.admit(c) {
                *next = Some(t.at);
                return false;
            }
            turns.push(t);
            costs.push(c);
            true
        };
    let map = locate::scan_from(home, path, from)?;
    let start = map.turns.partition_point(|t| t.at < from);
    for t in &map.turns[start..] {
        if !take(t, &mut next) {
            break;
        }
    }
    let end = map.end;
    drop(take);
    // 还没收尾的最后一轮（只会在最后一页）：「现在在做哪一步 / 在等什么」由 `dress` 按这台此刻的会话事实拼进过程行；拼完超了这一页 ⇒ 挪到下一页。
    if next.is_none() {
        if let Some(last) = turns.last_mut().filter(|t| !t.done) {
            dress(last);
            let old = costs.last().copied().unwrap_or(0);
            if !pager.redo_last(old, jlen(&*last) + 1) {
                next = turns.pop().map(|t| t.at);
            }
        }
    }
    let reply = HistoryTurns {
        from,
        end: next.unwrap_or(end),
        next,
        turns,
    };
    Ok(reply)
}
