//! `history-user-inputs`：「人说过的话」清单，从扫描图上切一页。

use super::locate;
use super::{jlen, Pager};
use std::path::Path;

/// `history-user-inputs` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct UserInputsReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 起点（缺 ⇒ 0；增量传上次的 `end`，下一页传 `next`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) from: Option<u64>,
    /// 可选：一页最多几条（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) limit: Option<u64>,
}

/// `history-user-inputs` 的回包：「人说过的话」清单。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct UserInputs {
    /// 这份清单从哪个字节起（＝ 请求的 `from`）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
    /// 这一页管到哪个字节 ＝ 下一次增量该带的 `from`（最后一页 ⇒ 最后一个完整行的末字节）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    /// 下一页从哪个字节起（＝ 下一页第一条那一行的起点）；没有下一页 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) next: Option<u64>,
    /// 每条用户输入，对话序。
    pub(crate) entries: Vec<crate::observe::recognize::user_inputs::UserInputRow>,
}

/// `history-user-inputs` 的一页，上限是参数。一律从扫描图上切（同 [`super::index::index_page`]）。
pub(crate) fn user_inputs_page(
    home: &Path,
    req: UserInputsReq,
    cap: usize,
) -> Result<UserInputs, String> {
    let from = req.from.unwrap_or(0);
    let fixed = jlen(&UserInputs {
        from: u64::MAX,
        end: u64::MAX,
        next: Some(u64::MAX),
        entries: Vec::new(),
    });
    let mut pager = Pager::new(cap, fixed, req.limit);
    let mut rows = Vec::new();
    let mut next = None;
    let map = locate::scan_from(home, &req.path, from)?;
    let start = map.inputs.partition_point(|r| r.at < from);
    for r in &map.inputs[start..] {
        if !pager.admit(jlen(r) + 1) {
            next = Some(r.at);
            break;
        }
        rows.push(r.clone());
    }
    let end = map.end;
    Ok(UserInputs {
        from,
        end: next.unwrap_or(end),
        next,
        entries: rows,
    })
}
