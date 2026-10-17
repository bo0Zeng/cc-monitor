//! **历史页那一列**（帧面 `history-browse`）：几台的会话清单在本机后端并成一列，界面只要看得见的那一窗。
//!
//! ```text
//! history-browse {here, remotes, fresh, query?, sort?, within_days?, hidden?, by, groups, forks, range}
//!   每台一份成品（`history_list::answer_from`：并注解 · 筛 · 补父会话 · 排）：这台现扫；远端那台读本进程记着的副本
//!     （`remote_listing.rs`；`fresh` 里的那几台先同步一次 —— 开页 · 刷新 · 勾回那一台；别的不去问，没读过就说没读过）
//!   ⇒ 并成一列：行按 `at` 并（同一刻按 here · remotes 的先后）、组按 `order` 并
//!   ⇒ 摊成一项一项：按时间看 ＝ 顶层的行（挂在父会话下的分叉跟在展开了的父会话后面），段头记在头里（落在第几项之前）；
//!     按项目看 ＝ 组头 ＋ 开着的组里那几行（搜着字时组都开着）
//!   ⇒ 带键集合那一形（`common/views.rs`）答 `range` 那一窗；头 ＝ 各台几行 · 读没读到 ＋ 几个项目 · 组里几个会话 ＋ 段头
//! ```
//!
//! 判定都在这里与 `history_list.rs`：界面不并、不排、不数，只照项画、照头里的段头与各台那一格画列表顶。
//! 一台读不到不挡别的台：读不到的那台在头里说为什么（它的行不出）；同步慢的那一台由界面另发一问带它的 `fresh`，这一问照手上的答。

use std::collections::{BTreeMap, BTreeSet};

use cmd_wire::cells::Words;
use cmd_wire::listing::{HistoryGroup, HistoryHead, HistoryRow};
use cmd_wire::sync::{Patch, Range};
use cmd_wire::Machine;
use serde::{Deserialize, Serialize};

use super::history_list::{answer_from, blocking, filters, listing_annotations, Sort};
use super::remote_listing::Kept;
use crate::common::views::{ViewSource, Views};

/// 一窗至多几项（界面只问看得见的那几十项）。
const WINDOW_MAX: u32 = 2000;

/// 每种问法一份备忘（界面不带记号问，备忘只为这一形；几种看法来回切不互相挤）。
static VIEWS: std::sync::LazyLock<Views> = std::sync::LazyLock::new(|| Views::new(4));

// ───────────────────────── 线上形 ─────────────────────────

/// 怎么看：按时间（缺省）· 按项目。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "HistoryBy"
    )
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum By {
    #[default]
    Time,
    Project,
}

/// 一台上的一组（按项目看时开合那一格认它）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "HistoryGroupId"
    )
)]
#[serde(deny_unknown_fields)]
pub(crate) struct GroupId {
    /// 哪台（这台 ⇒ 缺）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, as = "Option<String>"))]
    pub(crate) origin: Option<Machine>,
    pub(crate) key: String,
}

/// 一台上的一个会话（展开了分叉的那几行认它）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "HistoryRowId"
    )
)]
#[serde(deny_unknown_fields)]
pub(crate) struct RowId {
    /// 哪台（这台 ⇒ 缺）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, as = "Option<String>"))]
    pub(crate) origin: Option<Machine>,
    pub(crate) sid: String,
}

/// 按项目看时哪几组开着：`open` 是全部的样子，`except` 里那几组反过来。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "HistoryFold"
    )
)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fold {
    pub(crate) open: bool,
    pub(crate) except: Vec<GroupId>,
}

/// `history-browse` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryBrowseReq {
    /// 带不带这台。
    pub(crate) here: bool,
    /// 远端那几台（并列时同一刻的行照这台 · 再照这里的先后）。
    #[cfg_attr(test, ts(as = "Vec<String>"))]
    pub(crate) remotes: Vec<Machine>,
    /// 先同步这几台的副本再答（开页 · 刷新 · 勾回一台；别的时候空）；须在 `remotes` 里。带了要带期限（请求信封 `within_ms`）：等多久归发起方。
    #[cfg_attr(test, ts(as = "Vec<String>"))]
    pub(crate) fresh: Vec<Machine>,
    /// 同 `history-list`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) query: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) sort: Option<Sort>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) within_days: Option<u64>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) hidden: Option<bool>,
    /// 怎么看。
    pub(crate) by: By,
    /// 按项目看时哪几组开着（搜着字时一律开着）。
    pub(crate) groups: Fold,
    /// 展开了分叉的那几行。
    pub(crate) forks: Vec<RowId>,
    /// 要哪一窗（`limit` 1–2000）。总数在回包的头里。
    pub(crate) range: Range,
}

/// 一台此刻读没读到。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "HistoryMachineRead"
    )
)]
#[serde(tag = "is", rename_all = "snake_case")]
pub(crate) enum Read {
    /// 读到了；注解没并上 ⇒ 那一句。
    Ok { notice: Option<String> },
    /// 读不到：那一句（它的行不出）。
    Failed { why: String },
    /// 远端那台还没同步过、这一问也没带它的 `fresh`。
    Unread,
}

/// 一台那一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "HistoryMachine"
    )
)]
pub(crate) struct MachineHead {
    /// 哪台（这台 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, as = "Option<String>"))]
    pub(crate) origin: Option<Machine>,
    /// 那台筛好的几行（算上只为挂住分叉才带上的父会话）。
    pub(crate) total: u32,
    pub(crate) read: Read,
}

/// 按时间看的一个段头：落在第 `at` 项之前（字是那台写好的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "HistorySection"
    )
)]
pub(crate) struct Section {
    pub(crate) at: u32,
    #[cfg_attr(test, ts(type = "string"))]
    pub(crate) text: Words,
}

/// 这一列的头。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HistoryBrowseHead {
    /// 问到的每台（先后同入参：这台在前）。
    pub(crate) machines: Vec<MachineHead>,
    /// 几个项目 · 组里一共几个会话（按项目看时列表头那一句）。
    pub(crate) projects: u32,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) grouped: u64,
    /// 按时间看的段头（按项目看 ⇒ 空）。
    pub(crate) sections: Vec<Section>,
}

/// 这一列的一项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum HistoryLine {
    /// 按项目看的组头。
    Group { group: HistoryGroup, open: bool },
    /// 一行会话：`child` ＝ 挂在展开了的父会话下；`forks` ＝ 它下面挂着几个分叉（`forksOpen` 展没展开）；
    /// `orphan` ＝ 它是分叉、父会话不在这一列里。
    Row {
        row: HistoryRow,
        child: bool,
        forks: u32,
        #[serde(rename = "forksOpen")]
        forks_open: bool,
        orphan: bool,
    },
}

crate::wire::typed::codes! {
    /// `history-browse` 的码（一台读不到不算失败：在头里那一格说）。
    pub(crate) enum HistoryBrowseErr {
        /// 并的那一趟没做完。
        Failed = "failed",
    }
}

/// `history-browse`：历史页那一列。扫这台在阻塞池里、同步远端那几台并着等，并 · 摊是纯计算 ⇒ 异步档。
pub(crate) struct HistoryBrowseCmd;

impl crate::wire::typed::Command for HistoryBrowseCmd {
    const NAME: &'static str = "history-browse";
    type Req = HistoryBrowseReq;
    type Reply = Patch<HistoryBrowseHead, HistoryLine>;
    type Err = HistoryBrowseErr;
}

impl crate::wire::typed::Handle for HistoryBrowseCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryBrowseReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Self::Reply, HistoryBrowseErr>> + Send + 'static
    {
        async move {
            answer(req, cx.tz, cx.until)
                .await
                .map_err(crate::wire::typed::of_pair(HistoryBrowseErr::Failed))
        }
    }
}

// ───────────────────────── 问各台 ─────────────────────────

fn bad(what: &str) -> (&'static str, String) {
    ("bad_args", crate::common::contract::malformed(what))
}

/// 一台读到的那一份，或读不到 / 没读过。
pub(crate) enum Got {
    Listing(cmd_wire::listing::Listing),
    Failed(String),
    Unread,
}

/// 帧面 `history-browse` 的本体（`tz` ＝ 看的那一台的时区；`until` ＝ 发起方给的截止时刻，同步远端那几台时照它等）。
pub(crate) async fn answer(
    req: HistoryBrowseReq,
    tz: crate::Tz,
    until: Option<crate::platform::child::Until>,
) -> Result<Patch<HistoryBrowseHead, HistoryLine>, (&'static str, String)> {
    let ask = filters(req.query.as_deref(), req.sort, req.within_days, req.hidden)?;
    if !(1..=WINDOW_MAX).contains(&req.range.limit) {
        return Err(bad("`range.limit` must be 1..=2000"));
    }
    let mut seen = BTreeSet::new();
    for m in &req.remotes {
        if m.0.is_empty() || !seen.insert(m.0.as_str()) {
            return Err(bad("`remotes` must be distinct non-empty machine names"));
        }
    }
    if let Some(m) = req.fresh.iter().find(|m| !seen.contains(m.0.as_str())) {
        return Err(bad(&format!(
            "`fresh` names `{}`, which is not in `remotes`",
            m.0
        )));
    }
    if !req.fresh.is_empty() && until.is_none() {
        return Err(bad(
            "`within_ms` is required when `fresh` asks another machine: how long to wait belongs to the asker",
        ));
    }
    // 远端那几台并着同步（各自一个任务），这台同时在阻塞池里扫。
    let syncs: Vec<_> = req
        .remotes
        .iter()
        .map(|m| {
            let (m, tz) = (m.0.clone(), tz.clone());
            let sync = req.fresh.iter().any(|f| f.0 == m);
            tokio::spawn(async move {
                // 同步那几台照同步的结局（没成的那一句副本表也记下了，下一问不带它的 `fresh` 照样说）；别的读手上那份。
                let got = if sync {
                    match super::remote_listing::listing(&m, true, &tz, until).await {
                        Ok(l) => Got::Listing(l),
                        Err((_, why)) => Got::Failed(why),
                    }
                } else {
                    match super::remote_listing::kept(&m) {
                        Kept::Got(l) => Got::Listing(l),
                        Kept::Failed(why) => Got::Failed(why),
                        Kept::Unread => Got::Unread,
                    }
                };
                (m, got)
            })
        })
        .collect();
    let here = if req.here {
        Some(match blocking(super::history_list::machine_listing).await {
            Ok(l) => Got::Listing(l),
            Err((_, why)) => Got::Failed(why),
        })
    } else {
        None
    };
    let mut got: Vec<(Option<String>, Got)> = here.map(|g| (None, g)).into_iter().collect();
    for s in syncs {
        let (m, g) = s.await.map_err(|e| {
            (
                "failed",
                crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(
                    copy_core::copy_text("beHistoryJoin.blocking.unfinished", &[]),
                    e,
                )),
            )
        })?;
        got.push((Some(m), g));
    }
    let shape = Shape {
        by: req.by,
        all_open: ask.query.is_some(),
        fold: req.groups,
        forks: req
            .forks
            .into_iter()
            .map(|r| (r.origin.map(|m| m.0), r.sid))
            .collect(),
    };
    let range = req.range;
    blocking(move || {
        let now = crate::common::time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let (machines, answered) = per_machine(got, &ask, now, &tz);
        let filters = serde_json::json!([ask.query, ask.by_created, ask.within_ms, ask.hidden]);
        Ok(patch_from(
            &VIEWS,
            &filters.to_string(),
            machines,
            &answered,
            &shape,
            Some(range),
        ))
    })
    .await
}

/// 一台的成品（读到了的那几台）。
pub(crate) struct Answered {
    pub(crate) origin: Option<String>,
    pub(crate) head: HistoryHead,
    pub(crate) rows: Vec<HistoryRow>,
}

/// 各台读到的 ⇒ 各台那一格 ＋ 读到了的那几台的成品（`history_list::answer_from`，一台一份）。
pub(crate) fn per_machine(
    got: Vec<(Option<String>, Got)>,
    ask: &super::history_list::Ask,
    now_ms: i64,
    tz: &crate::Tz,
) -> (Vec<MachineHead>, Vec<Answered>) {
    let mut heads = Vec::new();
    let mut answered = Vec::new();
    for (origin, g) in got {
        let machine = origin.clone().map(Machine);
        match g {
            Got::Listing(l) => {
                let ann = listing_annotations(&l);
                let (head, rows) = answer_from(
                    &l,
                    origin.as_deref(),
                    ann.as_ref().map_err(Clone::clone),
                    ask,
                    now_ms,
                    tz,
                );
                heads.push(MachineHead {
                    origin: machine,
                    total: u32::try_from(rows.len()).unwrap_or(u32::MAX),
                    read: Read::Ok {
                        notice: head.notice.clone(),
                    },
                });
                answered.push(Answered { origin, head, rows });
            }
            Got::Failed(why) => heads.push(MachineHead {
                origin: machine,
                total: 0,
                read: Read::Failed { why },
            }),
            Got::Unread => heads.push(MachineHead {
                origin: machine,
                total: 0,
                read: Read::Unread,
            }),
        }
    }
    (heads, answered)
}

// ───────────────────────── 并成一列 ─────────────────────────

/// 摊成一项一项要的那几格（入参里决定这一列长什么样的，问法键里也是它们）。
pub(crate) struct Shape {
    pub(crate) by: By,
    /// 搜着字 ⇒ 组都开着（命中的就在眼前）。
    pub(crate) all_open: bool,
    pub(crate) fold: Fold,
    pub(crate) forks: BTreeSet<(Option<String>, String)>,
}

impl Shape {
    fn open(&self, g: &HistoryGroup) -> bool {
        let id = GroupId {
            origin: g.origin.clone(),
            key: g.key.clone(),
        };
        self.all_open || (self.fold.open != self.fold.except.contains(&id))
    }
}

/// 各台各自排好（`key` 大的在前）的几列 ⇒ 并成一列；同值照台的先后（稳定）。只并、不另判。
fn merge_by<'a, T>(lists: &[&'a [T]], key: impl Fn(&T) -> i64) -> Vec<&'a T> {
    let mut at = vec![0usize; lists.len()];
    let mut out = Vec::with_capacity(lists.iter().map(|l| l.len()).sum());
    loop {
        let mut best: Option<usize> = None;
        for (i, l) in lists.iter().enumerate() {
            let Some(x) = l.get(at[i]) else { continue };
            if best.is_none_or(|b| key(x) > key(&lists[b][at[b]])) {
                best = Some(i);
            }
        }
        let Some(b) = best else { return out };
        out.push(&lists[b][at[b]]);
        at[b] += 1;
    }
}

type Id = (Option<String>, String);

fn id_of(r: &HistoryRow) -> Id {
    (r.origin.as_ref().map(|m| m.0.clone()), r.session_id.clone())
}

fn parent_of(r: &HistoryRow) -> Option<Id> {
    r.forked_from_session_id
        .as_ref()
        .map(|p| (r.origin.as_ref().map(|m| m.0.clone()), p.clone()))
}

/// 一项的键（同一个会话在两台上是两项；组头另一种）。
fn key_of(line: &HistoryLine) -> String {
    let o = |m: &Option<Machine>| m.as_ref().map_or(String::new(), |m| m.0.clone());
    match line {
        HistoryLine::Group { group, .. } => {
            format!("g\u{1f}{}\u{1f}{}", o(&group.origin), group.key)
        }
        HistoryLine::Row { row, .. } => {
            format!("r\u{1f}{}\u{1f}{}", o(&row.origin), row.session_id)
        }
    }
}

/// 摊好的一列：每一项 · 段头 · 几个项目 · 组里几个会话。
pub(crate) struct Laid {
    pub(crate) lines: Vec<HistoryLine>,
    pub(crate) sections: Vec<Section>,
    pub(crate) projects: u32,
    pub(crate) grouped: u64,
}

/// 读到了的那几台 ⇒ 并成一列、摊成一项一项（[`Shape`]）。
pub(crate) fn lay(answered: &[Answered], shape: &Shape) -> Laid {
    let row_lists: Vec<&[HistoryRow]> = answered.iter().map(|a| a.rows.as_slice()).collect();
    let rows = merge_by(&row_lists, |r| r.at);
    let present: BTreeSet<Id> = rows.iter().map(|r| id_of(r)).collect();
    // 每个父会话下挂着的分叉（同一台；只认这一列里有的父会话），照并好的先后。
    let mut kids: BTreeMap<Id, Vec<&HistoryRow>> = BTreeMap::new();
    for r in &rows {
        if let Some(p) = parent_of(r).filter(|p| present.contains(p)) {
            kids.entry(p).or_default().push(r);
        }
    }
    let nested = |r: &HistoryRow| parent_of(r).is_some_and(|p| present.contains(&p));
    let mut laid = Laid {
        lines: Vec::new(),
        sections: Vec::new(),
        projects: 0,
        grouped: 0,
    };
    let push = |laid: &mut Laid, r: &HistoryRow| {
        push_row(&mut laid.lines, r, false, &kids, &present, shape);
    };
    match shape.by {
        By::Time => {
            let mut sec = String::new();
            for r in rows.iter().copied().filter(|r| !nested(r)) {
                // 分段头是那台写好的字：换了字就起新的一段。
                if r.section_text.0 != sec {
                    sec = r.section_text.0.clone();
                    laid.sections.push(Section {
                        at: u32::try_from(laid.lines.len()).unwrap_or(u32::MAX),
                        text: r.section_text.clone(),
                    });
                }
                push(&mut laid, r);
            }
        }
        By::Project => {
            let group_lists: Vec<&[HistoryGroup]> =
                answered.iter().map(|a| a.head.groups.as_slice()).collect();
            let groups = merge_by(&group_lists, |g| g.order);
            laid.projects = u32::try_from(groups.len()).unwrap_or(u32::MAX);
            laid.grouped = groups.iter().map(|g| g.count).sum();
            let mut by_group: BTreeMap<(Option<String>, &str), Vec<&HistoryRow>> = BTreeMap::new();
            for r in &rows {
                by_group
                    .entry((r.origin.as_ref().map(|m| m.0.clone()), r.group.as_str()))
                    .or_default()
                    .push(r);
            }
            for g in groups {
                let open = shape.open(g);
                laid.lines.push(HistoryLine::Group {
                    group: g.clone(),
                    open,
                });
                if !open {
                    continue;
                }
                let gk = (g.origin.as_ref().map(|m| m.0.clone()), g.key.as_str());
                for r in by_group.get(&gk).into_iter().flatten().copied() {
                    if !nested(r) {
                        push(&mut laid, r);
                    }
                }
            }
        }
    }
    laid
}

/// 一行 ＋ 展开了的话它下面的分叉（递归；分叉的分叉同样）。
fn push_row(
    out: &mut Vec<HistoryLine>,
    r: &HistoryRow,
    child: bool,
    kids: &BTreeMap<Id, Vec<&HistoryRow>>,
    present: &BTreeSet<Id>,
    shape: &Shape,
) {
    let id = id_of(r);
    let mine = kids.get(&id).map_or(&[][..], Vec::as_slice);
    let open = shape.forks.contains(&id);
    let orphan = !child && parent_of(r).is_some_and(|p| !present.contains(&p));
    out.push(HistoryLine::Row {
        row: r.clone(),
        child,
        forks: u32::try_from(mine.len()).unwrap_or(u32::MAX),
        forks_open: open,
        orphan,
    });
    if open {
        for c in mine {
            push_row(out, c, true, kids, present, shape);
        }
    }
}

/// 一问在数据层眼里的样子：问法的键 ＝ 决定这一列的那几格（不含窗口）；此刻的全部项 ＝ 摊好的那一列。
struct Source {
    key: String,
    head: HistoryBrowseHead,
    lines: Vec<(String, HistoryLine)>,
}

impl ViewSource for Source {
    type Head = HistoryBrowseHead;
    type Item = HistoryLine;
    fn ask_key(&self) -> String {
        self.key.clone()
    }
    fn now(&self) -> (HistoryBrowseHead, Vec<(String, HistoryLine)>) {
        (self.head.clone(), self.lines.clone())
    }
}

/// 各台那一格 ＋ 读到了的成品 ⇒ 经数据层答成 `range` 那一窗（界面不带记号 ⇒ 整份那一窗）。`filters` ＝ 筛排那几格（进问法的键）。
pub(crate) fn patch_from(
    views: &Views,
    filters: &str,
    machines: Vec<MachineHead>,
    answered: &[Answered],
    shape: &Shape,
    range: Option<Range>,
) -> Patch<HistoryBrowseHead, HistoryLine> {
    let laid = lay(answered, shape);
    let key = serde_json::json!([
        machines.iter().map(|m| &m.origin).collect::<Vec<_>>(),
        shape.by,
        shape.all_open,
        shape.fold,
        shape.forks,
        filters,
    ])
    .to_string();
    let src = Source {
        key,
        head: HistoryBrowseHead {
            machines,
            projects: laid.projects,
            grouped: laid.grouped,
            sections: laid.sections,
        },
        lines: laid.lines.into_iter().map(|l| (key_of(&l), l)).collect(),
    };
    views.answer(&src, None, range)
}

#[cfg(test)]
#[path = "../../../tests/backend/history/history_browse_tests.rs"]
mod tests;
