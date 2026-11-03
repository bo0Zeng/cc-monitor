//! `history-index`：会话骨架索引，从扫描图上切一页、按列排。

use super::locate;
use super::{digits, jlen, Pager};
use std::path::Path;

/// `history-index` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct HistoryIndexReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub path: String,
    /// 从哪个字节起（缺 ⇒ 0；续传带上次的 `end`）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number | null"))]
    pub offset: Option<u64>,
    /// 可选：只收起点 `< until` 的行。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number | null"))]
    pub until: Option<u64>,
    /// 可选：一页最多几行（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number | null"))]
    pub limit: Option<u64>,
}

/// 一张稀疏小表的行号列：每项与上一项的行号差（第一项与 −1 比；连着 ＝ 0）。
fn gap(last: &mut i64, i: usize) -> u64 {
    let i = i as i64;
    let d = (i - *last - 1) as u64;
    *last = i;
    d
}

/// 骨架索引的正文那五个数（五个全 0 的行不进表）。
#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct IndexBody {
    /// 行号差。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Array<number>"))]
    pub(crate) at: Vec<u64>,
    pub(crate) ch: Vec<u32>,
    pub(crate) cj: Vec<u32>,
    pub(crate) pl: Vec<u32>,
    pub(crate) cb: Vec<u32>,
    pub(crate) cl: Vec<u32>,
}

/// 骨架索引里用户输入的 uuid · 摘要 · 时刻（没有时刻 ⇒ 空串）：大纲清单从它出，跳过去时按 uuid 找卡、按行号物化。
#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct IndexInputs {
    /// 行号差。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Array<number>"))]
    pub(crate) at: Vec<u64>,
    pub(crate) u: Vec<String>,
    pub(crate) x: Vec<String>,
    pub(crate) ts: Vec<String>,
}

/// **骨架索引按列排**（17 MB 的会话一份索引六千多行，逐行对象里键名占一大半）：
///
/// - 每行都有的几格各一列：`o`（与上一行末尾的字节差，第一行与 `from` 比；连着 ＝ 0）· `n` · `t`（`classes` 里的号，
///   从 1 起，0 ＝ 不进界面）· `fd`（0 ＝ 没有）。行号就是下标（第一行 ＝ 这一页从第几行起），不运每行的 uuid。
///   `dr`（`draws` 里的号，从 1 起，0 ＝ 不画）。
/// - 稀疏的几格各一张小表（`body` · `inputs`），`at` 是与上一项的行号差；`hu` 是人说的那几行，也按行号差。
/// - `sc` 不上帧面（CLI 那一臂照旧逐行出 [`crate::observe::recognize::index_row::IndexRow`]）。
///
/// 界面照列收进骨架账本（`live-window.ts::SkeletonLedger.append`），跨语言金样钉两侧。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct HistoryIndex {
    /// 起点字节。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    /// 这一页最后一行的末字节 ＝ 下一次续传该带的 `offset`（最后一页 ⇒ 最后一个完整行的末字节）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub end: u64,
    /// 下一页从哪个字节起（＝ 下一页第一行的起点）；没有下一页 ⇒ `null`。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub next: Option<u64>,
    /// 行数（每一列都这么长）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub count: usize,
    /// 本份里出现过的记录类（`t` 列的号从 1 起指它）。
    pub(crate) classes: Vec<crate::agents::record::RecordClass>,
    /// 本份里出现过的画法（`dr` 列的号从 1 起指它；不画 ⇒ 0）。
    pub(crate) draws: Vec<crate::agents::record::Draw>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Array<number>"))]
    pub(crate) o: Vec<u64>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Array<number>"))]
    pub(crate) n: Vec<u64>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Array<number>"))]
    pub(crate) t: Vec<usize>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Array<number>"))]
    pub(crate) dr: Vec<usize>,
    pub(crate) fd: Vec<u32>,
    pub(crate) body: IndexBody,
    /// 人说的那几行（行号差）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Array<number>"))]
    pub(crate) hu: Vec<u64>,
    pub(crate) inputs: IndexInputs,
}

/// 字典列的号：`v` 在 `dict` 里排第几（从 1 起；第一次见 ⇒ 记进去）。
fn code_of<T: PartialEq + Copy>(dict: &mut Vec<T>, v: T) -> usize {
    dict.iter().position(|x| *x == v).unwrap_or_else(|| {
        dict.push(v);
        dict.len() - 1
    }) + 1
}

/// 逐行的骨架 ⇒ 按列排的那一份（见 [`HistoryIndex`]）。
pub fn pack_index(
    from: u64,
    end: u64,
    rows: &[crate::observe::recognize::index_row::IndexRow],
) -> HistoryIndex {
    use crate::agents::record::Draw;
    let mut ix = HistoryIndex {
        from,
        end,
        next: None,
        count: rows.len(),
        classes: Vec::new(),
        draws: Vec::new(),
        o: Vec::new(),
        n: Vec::new(),
        t: Vec::new(),
        dr: Vec::new(),
        fd: Vec::new(),
        body: IndexBody::default(),
        hu: Vec::new(),
        inputs: IndexInputs::default(),
    };
    let (mut lb, mut lh, mut li) = (-1i64, -1i64, -1i64);
    let mut prev_end = from;
    for (i, r) in rows.iter().enumerate() {
        ix.o.push(r.o.saturating_sub(prev_end));
        ix.n.push(r.n);
        prev_end = r.o + r.n;
        ix.t.push(r.t.map_or(0, |c| code_of(&mut ix.classes, c)));
        ix.dr.push(match r.dr {
            Draw::None => 0,
            d => code_of(&mut ix.draws, d),
        });
        ix.fd.push(r.fd);
        if [r.ch, r.cj, r.pl, r.cb, r.cl].iter().any(|n| *n != 0) {
            let b = &mut ix.body;
            b.at.push(gap(&mut lb, i));
            b.ch.push(r.ch);
            b.cj.push(r.cj);
            b.pl.push(r.pl);
            b.cb.push(r.cb);
            b.cl.push(r.cl);
        }
        if r.hu {
            ix.hu.push(gap(&mut lh, i));
        }
        if let Some(x) = &r.x {
            ix.inputs.at.push(gap(&mut li, i));
            ix.inputs.u.push(r.u.clone().unwrap_or_default());
            ix.inputs.x.push(x.clone());
            ix.inputs.ts.push(r.ts.clone().unwrap_or_default());
        }
    }
    ix
}

/// 字典列（`classes` · `draws`）里已经见过的。
#[derive(Default)]
struct Seen {
    classes: Vec<crate::agents::record::RecordClass>,
    draws: Vec<crate::agents::record::Draw>,
}

/// 字典列一格至多几个字节：第一次见的那一行另算它在字典里那一项。
fn dict_cost<T: PartialEq + Copy + serde::Serialize>(seen: &mut Vec<T>, v: Option<T>) -> usize {
    match v {
        Some(c) if !seen.contains(&c) => {
            seen.push(c);
            jlen(&c) + 1 + digits(seen.len() as u64)
        }
        Some(_) => digits(seen.len() as u64),
        None => 1,
    }
}

/// 一行骨架索引在按列排的那一份里至多占几个字节（`i` ＝ 它在这一页里排第几；行号差 ≤ `i`，偏移差 ≤ 偏移本身）。
fn index_row_cost(
    r: &crate::observe::recognize::index_row::IndexRow,
    i: usize,
    seen: &mut Seen,
) -> usize {
    let at = digits(i as u64) + 1;
    let mut n = digits(r.o) + digits(r.n) + digits(r.fd.into()) + 4 + 5;
    n += dict_cost(&mut seen.classes, r.t);
    n += dict_cost(
        &mut seen.draws,
        Some(r.dr).filter(|d| *d != crate::agents::record::Draw::None),
    );
    let body = [r.ch, r.cj, r.pl, r.cb, r.cl];
    if body.iter().any(|v| *v != 0) {
        n += at + body.iter().map(|v| digits((*v).into()) + 1).sum::<usize>();
    }
    if r.hu {
        n += at;
    }
    if let Some(x) = &r.x {
        n += at
            + jlen(r.u.as_deref().unwrap_or(""))
            + 1
            + jlen(x)
            + 1
            + jlen(r.ts.as_deref().unwrap_or(""))
            + 1;
    }
    n
}

/// `history-index` 的一页，上限是参数（判据拿小上限喂）。
///
/// 行从哪来：从 0 起 · 那一份的图留着 ⇒ 从扫描图上切；从中间起而图不在（界面冷开先要尾段那一截）⇒ 只读那一截
/// （[`locate::index_rows`]）。
pub fn index_page(home: &Path, req: HistoryIndexReq, cap: usize) -> Result<HistoryIndex, String> {
    let offset = req.offset.unwrap_or(0);
    let fixed = jlen(&pack_index(u64::MAX, u64::MAX, &[])) + digits(u64::MAX);
    let mut pager = Pager::new(cap, fixed, req.limit);
    let mut seen = Seen::default();
    let mut rows: Vec<crate::observe::recognize::index_row::IndexRow> = Vec::new();
    let mut next = None;
    // `until`：只收起点在它之前的行；末端是界外第一行的起点（没有 ⇒ 读到的末端）。
    let end = locate::index_rows(home, &req.path, offset, |r| {
        if req.until.is_some_and(|u| r.o >= u) {
            return false;
        }
        if !pager.admit(index_row_cost(r, rows.len(), &mut seen)) {
            next = Some(r.o);
            return false;
        }
        rows.push(r.clone());
        true
    })?;
    let mut ix = pack_index(offset, next.unwrap_or(end), &rows);
    ix.next = next;
    Ok(ix)
}
