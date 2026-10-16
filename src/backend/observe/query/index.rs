//! `history-index`：会话骨架索引，从扫描图上切一页、按列排。

use super::locate;
use super::{digits, jlen, Pager};
use std::path::Path;

/// `history-index` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryIndexReq {
    /// jsonl 路径（围栏同 `history-read`）。
    pub(crate) path: String,
    /// 从哪个字节起（缺 ⇒ 0；续传带上次的 `end`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) offset: Option<u64>,
    /// 可选：只收起点 `< until` 的行。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) until: Option<u64>,
    /// 可选：一页最多几行（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) limit: Option<u64>,
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct IndexBody {
    /// 行号差。
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) at: Vec<u64>,
    pub(crate) ch: Vec<u32>,
    pub(crate) cj: Vec<u32>,
    pub(crate) pl: Vec<u32>,
    pub(crate) cb: Vec<u32>,
    pub(crate) cl: Vec<u32>,
}

/// 骨架索引的来源那一列（只有带来源的行）。
#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct IndexSpeaker {
    /// 行号差。
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) at: Vec<u64>,
    pub(crate) v: Vec<String>,
}

/// 骨架索引里用户输入的摘要与时刻（没有时刻 ⇒ 空串）。
#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct IndexInputs {
    /// 行号差。
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) at: Vec<u64>,
    pub(crate) x: Vec<String>,
    pub(crate) ts: Vec<String>,
}

/// **骨架索引按列排**（17 MB 的会话一份索引六千多行，逐行对象里键名占一大半）：
///
/// - 每行都有的几格各一列：`o`（与上一行末尾的字节差，第一行与 `from` 比；连着 ＝ 0）· `n` · `t`（`classes` 里的号，
///   从 1 起，0 ＝ 不进界面）· `fd`（0 ＝ 没有）· `u`（没有 ⇒ `null`）。
/// - 稀疏的几格各一张小表（`body` · `sp` · `inputs`），`at` 是与上一项的行号差。
/// - `sc` 不上帧面（CLI 那一臂照旧逐行出 [`crate::observe::history_query::IndexRow`]）。
///
/// 界面读法只在 `session-reads.ts::unpackIndex` 一处（装回逐行，下游照旧），跨语言金样钉两侧。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistoryIndex {
    /// 起点字节。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
    /// 这一页最后一行的末字节 ＝ 下一次续传该带的 `offset`（最后一页 ⇒ 最后一个完整行的末字节）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    /// 下一页从哪个字节起（＝ 下一页第一行的起点）；没有下一页 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) next: Option<u64>,
    /// 行数（每一列都这么长）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) count: usize,
    /// 本份里出现过的记录类（`t` 列的号从 1 起指它）。
    pub(crate) classes: Vec<crate::agents::record::RecordClass>,
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) o: Vec<u64>,
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) n: Vec<u64>,
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) t: Vec<usize>,
    pub(crate) fd: Vec<u32>,
    pub(crate) u: Vec<Option<String>>,
    pub(crate) body: IndexBody,
    pub(crate) sp: IndexSpeaker,
    pub(crate) inputs: IndexInputs,
}

/// 逐行的骨架 ⇒ 按列排的那一份（见 [`HistoryIndex`]）。
pub(crate) fn pack_index(
    from: u64,
    end: u64,
    rows: &[crate::observe::recognize::index_row::IndexRow],
) -> HistoryIndex {
    let mut ix = HistoryIndex {
        from,
        end,
        next: None,
        count: rows.len(),
        classes: Vec::new(),
        o: Vec::new(),
        n: Vec::new(),
        t: Vec::new(),
        fd: Vec::new(),
        u: Vec::new(),
        body: IndexBody::default(),
        sp: IndexSpeaker::default(),
        inputs: IndexInputs::default(),
    };
    let (mut lb, mut ls, mut li) = (-1i64, -1i64, -1i64);
    let mut prev_end = from;
    for (i, r) in rows.iter().enumerate() {
        ix.o.push(r.o.saturating_sub(prev_end));
        ix.n.push(r.n);
        prev_end = r.o + r.n;
        ix.t.push(match r.t {
            Some(c) => {
                let k = ix.classes.iter().position(|x| *x == c).unwrap_or_else(|| {
                    ix.classes.push(c);
                    ix.classes.len() - 1
                });
                k + 1
            }
            None => 0,
        });
        ix.fd.push(r.fd);
        ix.u.push(r.u.clone());
        if [r.ch, r.cj, r.pl, r.cb, r.cl].iter().any(|n| *n != 0) {
            let b = &mut ix.body;
            b.at.push(gap(&mut lb, i));
            b.ch.push(r.ch);
            b.cj.push(r.cj);
            b.pl.push(r.pl);
            b.cb.push(r.cb);
            b.cl.push(r.cl);
        }
        if let Some(v) = r.sp {
            ix.sp.at.push(gap(&mut ls, i));
            ix.sp.v.push(v.to_string());
        }
        if let Some(x) = &r.x {
            ix.inputs.at.push(gap(&mut li, i));
            ix.inputs.x.push(x.clone());
            ix.inputs.ts.push(r.ts.clone().unwrap_or_default());
        }
    }
    ix
}

/// 一行骨架索引在按列排的那一份里至多占几个字节（`i` ＝ 它在这一页里排第几；行号差 ≤ `i`，偏移差 ≤ 偏移本身）。
/// 记录类第一次出现那一行另算它在 `classes` 里那一项。
fn index_row_cost(
    r: &crate::observe::recognize::index_row::IndexRow,
    i: usize,
    seen: &mut Vec<crate::agents::record::RecordClass>,
) -> usize {
    let at = digits(i as u64) + 1;
    let mut n = digits(r.o) + digits(r.n) + digits(r.fd.into()) + jlen(&r.u) + 3 + 5;
    n += match r.t {
        Some(c) if !seen.contains(&c) => {
            seen.push(c);
            jlen(&c) + 1 + digits(seen.len() as u64)
        }
        Some(_) => digits(seen.len() as u64),
        None => 1,
    };
    let body = [r.ch, r.cj, r.pl, r.cb, r.cl];
    if body.iter().any(|v| *v != 0) {
        n += at + body.iter().map(|v| digits((*v).into()) + 1).sum::<usize>();
    }
    if let Some(v) = r.sp {
        n += at + jlen(v) + 1;
    }
    if let Some(x) = &r.x {
        n += at + jlen(x) + 1 + jlen(r.ts.as_deref().unwrap_or("")) + 1;
    }
    n
}

/// `history-index` 的一页，上限是参数（判据拿小上限喂）。
///
/// 行从哪来：一律从那一份的扫描图上切（从 0 起 · 从中间续都一样；图不在了照骨架起步或重扫，见 [`locate::scan_from`]）。
pub(crate) fn index_page(
    home: &Path,
    req: HistoryIndexReq,
    cap: usize,
) -> Result<HistoryIndex, String> {
    let offset = req.offset.unwrap_or(0);
    let fixed = jlen(&pack_index(u64::MAX, u64::MAX, &[])) + digits(u64::MAX);
    let mut pager = Pager::new(cap, fixed, req.limit);
    let mut seen = Vec::new();
    let mut rows: Vec<crate::observe::recognize::index_row::IndexRow> = Vec::new();
    let mut next = None;
    let map = locate::scan_from(home, &req.path, offset)?;
    let start = map.index.partition_point(|r| r.o < offset);
    // `until`：只收起点在它之前的行；末端是界外第一行的起点（没有 ⇒ 扫到的末端）。
    let stop = req
        .until
        .map_or(map.index.len(), |u| map.index.partition_point(|r| r.o < u));
    let end = map.index.get(stop).map_or(map.end, |r| r.o);
    for r in &map.index[start..stop.max(start)] {
        if !pager.admit(index_row_cost(r, rows.len(), &mut seen)) {
            next = Some(r.o);
            break;
        }
        rows.push(r.clone());
    }
    let mut ix = pack_index(offset, next.unwrap_or(end), &rows);
    ix.next = next;
    Ok(ix)
}
