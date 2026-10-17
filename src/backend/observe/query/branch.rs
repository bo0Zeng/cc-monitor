//! `history-branch`：主线外清单，从扫描图上切一页。

use super::locate;
use super::{jlen, Pager};
use std::path::Path;

/// `history-branch` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BranchReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 从清单第几条起（缺 ⇒ 0；下一页传 `next`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) from: Option<u64>,
    /// 可选：一页最多几条（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) limit: Option<u64>,
}

/// `history-branch` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Branch {
    /// 回退掉的那几条记录的 `id`（只含进界面的；文件序），这一页是清单里从 `from` 起的那几条；这一家的记录没有链 ⇒ 恒空。
    pub(crate) off: Vec<String>,
    /// 最后一个完整行的末字节（之后的由实时帧 `session_branch` 接着说）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    /// 下一页从清单第几条起；没有下一页 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) next: Option<u64>,
}

/// `history-branch` 的一页，上限是参数。清单出自扫描图（同一张），按条切页。
pub(crate) fn branch_page(home: &Path, req: BranchReq, cap: usize) -> Result<Branch, String> {
    let map = locate::cold_scan(home, &req.path)?;
    let from = usize::try_from(req.from.unwrap_or(0)).unwrap_or(usize::MAX);
    let fixed = jlen(&Branch {
        off: Vec::new(),
        end: u64::MAX,
        next: Some(u64::MAX),
    });
    let mut pager = Pager::new(cap, fixed, req.limit);
    let rest = map.off.get(from..).unwrap_or_default();
    let n = rest
        .iter()
        .take_while(|id| pager.admit(jlen(*id) + 1))
        .count();
    Ok(Branch {
        off: rest[..n].to_vec(),
        end: map.end,
        next: (n < rest.len()).then(|| (from + n) as u64),
    })
}
