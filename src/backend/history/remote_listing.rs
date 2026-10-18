//! **远端那台的会话清单**：那台怎么答（帧面 `history-listing`，带键集合那一形）· 本机后端怎么取、怎么存（副本）。
//!
//! ```text
//! history-list {origin}（本机后端，按会话 ID 要一行）· history-browse {remotes, fresh}（历史页那一列）
//!   ⇒ 手上没有那台的副本 · 或要同步（fresh）⇒ 经 `remote_ask::ask_resident` 问那台的常驻：history-listing {held: 手上那份的记号}
//!        ⇒ 那台答 Patch<ListingHead, RawRow>（没变 / 只给变了的那几行 / 整份）⇒ 照答换副本（`Hand::apply`，没有判断）
//!   ⇒ 副本拼回一份清单（`Listing`）交 `history_list::answer_from` 并注解 · 筛 · 排
//!   历史页那一列不带 fresh 的那几台只读手上那份（[`kept`]：没问成过 ⇒ 没读过；上一次同步没成 ⇒ 那一句），不去问
//! ```
//!
//! - 问的是那台**热着的常驻**（`--resident-call`）：那台扫过的清单缓存与这一问的备忘都在它那里；
//!   那台常驻不在 ⇒ 照实失败（`resident_absent`），不在那台起一个冷进程顶上。
//! - 副本只在内存里（按机器名），本机后端重启后第一问整份一次；不按时间过期（这一层不看钟）：历史页开页 · 刷新带 `fresh` 才同步。
//! - 等多久只归发起方：`history-list` · `history-browse` 那一发交来的截止时刻（请求信封 `within_ms`）原样往下带（本机等它、那台 `--resident-call --within-ms` 也等它）；
//!   没带 ⇒ `bad_args`，这里不自己定一个。
//! - 压缩照旧在 SSH 那一层，这里不另做。

use std::collections::BTreeMap;
use std::sync::Mutex;

use cmd_wire::listing::{Listing, ListingHead, RawRow};
use cmd_wire::sync::{Hand, Held, Patch};
use serde::{Deserialize, Serialize};

use crate::common::views::{ViewSource, Views};
use crate::dial::remote_ask::{Remote, Table};
use crate::platform::child::{Deadline, Until};

// ───────────────────────── 那台怎么答 ─────────────────────────

/// `history-listing` 的入参：手上那份的记号（没问过 ⇒ 缺 ⇒ 整份）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryListingReq {
    #[serde(default)]
    pub(crate) held: Option<Held>,
}

crate::wire::typed::codes! {
    /// `history-listing` 的码。
    pub(crate) enum HistoryListingErr {
        /// 读不了这台的记录树 · 读的那一趟没做完。
        Failed = "failed",
    }
}

/// `history-listing`：这台自己的清单（不筛不排、并好这台的注解与隐藏规则），按带键集合答（键 ＝ 会话 ID）。
/// 别的后端经 `--resident-call` 问它（问的那一方记着副本）；备忘住本进程，只在被问时比。
pub(crate) struct HistoryListingCmd;

impl crate::wire::typed::Command for HistoryListingCmd {
    const NAME: &'static str = "history-listing";
    type Req = HistoryListingReq;
    type Reply = Patch<ListingHead, RawRow>;
    type Err = HistoryListingErr;
}

/// 这台答出去的那一份备忘（问法只有一种：整份）。
static VIEWS: std::sync::LazyLock<Views> = std::sync::LazyLock::new(|| Views::new(1));

impl crate::wire::typed::Handle for HistoryListingCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryListingReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Self::Reply, HistoryListingErr>> + Send + 'static
    {
        async move {
            let listing = super::history_list::blocking(super::history_list::machine_listing)
                .await
                .map_err(crate::wire::typed::of_pair(HistoryListingErr::Failed))?;
            Ok(answer_with(&VIEWS, listing, req.held.as_ref()))
        }
    }
}

/// 一台的清单当带键集合：键 ＝ 会话 ID（下游并注解 · 判隐藏都按它认），先后照清单；头 ＝ 行以外那几格。
struct Source(ListingHead, Vec<RawRow>);

impl ViewSource for Source {
    type Head = ListingHead;
    type Item = RawRow;
    fn ask_key(&self) -> String {
        String::new()
    }
    fn now(&self) -> (ListingHead, Vec<(String, RawRow)>) {
        let mut seen = std::collections::HashSet::new();
        let rows = self
            .1
            .iter()
            .filter(|r| seen.insert(r.session_id.clone()))
            .map(|r| (r.session_id.clone(), r.clone()))
            .collect();
        (self.0.clone(), rows)
    }
}

/// 那台答一问（备忘由调用方给：生产 ＝ 本进程那一份）。
pub(crate) fn answer_with(
    views: &Views,
    listing: Listing,
    held: Option<&Held>,
) -> Patch<ListingHead, RawRow> {
    let (head, rows) = listing.split();
    views.answer(&Source(head, rows), held, None)
}

// ───────────────────────── 本机后端怎么取、怎么存 ─────────────────────────

/// 远端那几台的副本：机器名 ⇒ 手上那份（记号 ＋ 先后 ＋ 各行 ＋ 头）＋ 上一次同步没成的那一句（成了就清掉）。
#[derive(Default)]
pub(crate) struct Copies(Mutex<BTreeMap<String, Slot>>);

/// 一台的副本：没问成过 ⇒ `hand.held` 缺。
#[derive(Default)]
pub(crate) struct Slot {
    hand: Hand<ListingHead, RawRow>,
    failed: Option<String>,
    /// 手上那份落下的那一刻（毫秒）：同步没成时照它说「下面是那一刻读到的」。
    at_ms: i64,
}

impl Copies {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Slot>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 手上那份那台的清单此刻是什么样（不去问）。
#[derive(Debug)]
pub(crate) enum Kept {
    /// 拼回来的那份。
    Got(Listing),
    /// 上一次同步没成、手上有之前那份：照出、标明是旧的（那一句 ＋ 那份落下的那一刻）。
    Stale {
        listing: Listing,
        why: String,
        at_ms: i64,
    },
    /// 从没问成过、同步又没成：只有那一句。
    Failed(String),
    /// 还没问成过、这一趟也没去问。
    Unread,
}

/// 那一台手上那份（生产：本进程的副本表）。
pub(crate) fn kept(machine: &str) -> Kept {
    kept_in(&COPIES, machine)
}

/// [`kept`] 的本体（副本表由调用方给）。
pub(crate) fn kept_in(copies: &Copies, machine: &str) -> Kept {
    let book = copies.lock();
    let Some(c) = book.get(machine) else {
        return Kept::Unread;
    };
    match (&c.failed, c.hand.held.is_some()) {
        (Some(why), true) => Kept::Stale {
            listing: assembled(&c.hand),
            why: why.clone(),
            at_ms: c.at_ms,
        },
        (Some(why), false) => Kept::Failed(why.clone()),
        (None, true) => Kept::Got(assembled(&c.hand)),
        (None, false) => Kept::Unread,
    }
}

/// 本进程那一份副本表。
static COPIES: std::sync::LazyLock<Copies> = std::sync::LazyLock::new(Copies::default);

/// 那一台的清单（生产：可达表 · 池里那条 SSH · 本进程的副本表）。`until` ＝ 发起方给的截止时刻（没给 ⇒ `bad_args`）。
pub(crate) async fn listing(
    machine: &str,
    fresh: bool,
    tz: &crate::Tz,
    until: Option<Until>,
) -> Result<Listing, (&'static str, String)> {
    let Some(until) = until else {
        return Err((
            "bad_args",
            crate::common::contract::malformed(
                "`within_ms` is required when asking another machine: how long to wait belongs to the asker",
            ),
        ));
    };
    listing_with(
        machine,
        fresh,
        tz,
        Deadline::millis(until.left_ms()),
        &crate::dial::remote_ask::REACH,
        &crate::dial::remote_ask::DialRemote,
        &COPIES,
    )
    .await
}

/// [`listing`] 的本体（可达表 · 对面 · 副本表由调用方给）：手上没有 · 或 `fresh` ⇒ 带记号问那台、照答换；再拼回一份清单。
/// 问不成 ⇒ 那台的码与那一句（够不着 `unreachable` · 常驻不在 `resident_absent` · …），手上那份不动、那一句记下（[`kept_in`] 照它答），下一次问成了清掉。
pub(crate) async fn listing_with(
    machine: &str,
    fresh: bool,
    tz: &crate::Tz,
    total: Deadline,
    table: &Table,
    remote: &dyn Remote,
    copies: &Copies,
) -> Result<Listing, (&'static str, String)> {
    let held = {
        let book = copies.lock();
        match book.get(machine).map(|c| &c.hand) {
            Some(hand) if !fresh && hand.held.is_some() => return Ok(assembled(hand)),
            Some(hand) => hand.held.clone(),
            None => None,
        }
    };
    let got = crate::dial::remote_ask::ask_resident::<HistoryListingCmd>(
        machine,
        &HistoryListingReq { held },
        tz,
        total,
        table,
        remote,
    )
    .await
    .map_err(|s| (code_of(s.code.as_deref()), s.message));
    let mut book = copies.lock();
    let slot = book.entry(machine.to_string()).or_default();
    match got {
        Ok(patch) => {
            slot.failed = None;
            slot.hand.apply(patch);
            slot.at_ms = crate::common::time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64);
            Ok(assembled(&slot.hand))
        }
        Err(e) => {
            slot.failed = Some(e.1.clone());
            Err(e)
        }
    }
}

/// 那台的码落到 `history-list` 的码表上（不在表里的落 `failed`，那一句照旧）。
fn code_of(code: Option<&str>) -> &'static str {
    match code {
        Some("unreachable") | None => "unreachable",
        Some("resident_absent") => "resident_absent",
        Some(_) => "failed",
    }
}

/// 手上那份拼回一份清单（行照 `order` 的先后）。
fn assembled(hand: &Hand<ListingHead, RawRow>) -> Listing {
    let rows = hand
        .order
        .iter()
        .filter_map(|k| hand.items.get(k).cloned())
        .collect();
    let head = hand
        .head
        .as_ref()
        .map(|h| h.info.clone())
        .unwrap_or_default();
    Listing::join(head, rows)
}

#[cfg(test)]
#[path = "../../../tests/backend/history/remote_listing_tests.rs"]
mod tests;
