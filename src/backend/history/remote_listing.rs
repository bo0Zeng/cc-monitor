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
//! - 副本表是共享核心那一份（`face_core::listing_copy`，手机核心同一份）；本机后端把它落在数据目录底下
//!   `cache/remote-listing/<机器>.json`（缓存：读不懂当没有、不迁移），重启后头一问带盘上那份的记号；
//!   不按时间过期（这一层不看钟）：历史页开页 · 刷新带 `fresh` 才同步。
//! - 等多久只归发起方：`history-list` · `history-browse` 那一发交来的截止时刻（请求信封 `within_ms`）原样往下带（本机等它、那台 `--resident-call --within-ms` 也等它）；
//!   没带 ⇒ `bad_args`，这里不自己定一个。
//! - 压缩照旧在 SSH 那一层，这里不另做。

use cmd_wire::listing::{Listing, ListingHead, RawRow};
use cmd_wire::sync::{Held, Patch};
use serde::{Deserialize, Serialize};

use crate::common::views::{ViewSource, Views};
use crate::dial::remote_ask::{Remote, Table};
use crate::platform::child::{Deadline, Until};
use crate::platform::fs::{Own, OwnPath};

// ───────────────────────── 那台怎么答 ─────────────────────────

/// `history-listing` 的入参：手上那份的记号（没问过 ⇒ 缺 ⇒ 整份）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct HistoryListingReq {
    #[serde(default)]
    pub(crate) held: Option<Held>,
}

crate::wire::typed::codes! {
    /// `history-listing` 的码。
    pub enum HistoryListingErr {
        /// 读不了这台的记录树 · 读的那一趟没做完。
        Failed = "failed",
    }
}

/// `history-listing`：这台自己的清单（不筛不排、并好这台的注解与隐藏规则），按带键集合答（键 ＝ 会话 ID）。
/// 别的后端经 `--resident-call` 问它（问的那一方记着副本）；备忘住本进程，只在被问时比。
pub struct HistoryListingCmd;

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
    async fn run(
        req: HistoryListingReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<Self::Reply, HistoryListingErr> {
        let listing = super::history_list::blocking(super::history_list::machine_listing)
            .await
            .map_err(crate::wire::typed::of_fail_with(HistoryListingErr::Failed))?;
        Ok(answer_with(&VIEWS, listing, req.held.as_ref()))
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

use face_core::listing_copy::{Ask, Shelf};
pub(crate) use face_core::listing_copy::{Copies, Kept};

/// 那一台手上那份（生产：本进程的副本表）。
pub(crate) fn kept(machine: &str) -> Kept {
    kept_in(&COPIES, machine)
}

/// [`kept`] 的本体（副本表由调用方给）。
pub(crate) fn kept_in(copies: &Copies, machine: &str) -> Kept {
    copies.kept(machine)
}

/// 本进程那一份副本表：落在启动时装的那个数据目录底下（[`install`]）；没装（判据 · 一次性进程）⇒ 只在内存。
static COPIES: std::sync::LazyLock<Copies> = std::sync::LazyLock::new(|| Copies::on(Installed));

/// 副本落盘的家（启动时装一次：[`install`]）。
static HOME: std::sync::OnceLock<Own> = std::sync::OnceLock::new();

/// 启动时装一次（拿启动时造的写句柄）：之后副本落在它的数据目录底下，重启照盘上那份接着问。
pub(crate) fn install(own: &Own) {
    if HOME.set(own.clone()).is_err() {
        tracing::debug!("remote listing copy home already installed; keeping the first one");
    }
}

/// 生产那一个落盘处：每一下照装好的家（没装 ⇒ 不读不写）。
struct Installed;

impl Shelf for Installed {
    fn load(&self, machine: &str) -> Option<Vec<u8>> {
        load_at(HOME.get()?, machine)
    }
    fn store(&self, machine: &str, body: &[u8]) {
        if let Some(own) = HOME.get() {
            store_at(own, machine, body);
        }
    }
}

/// 副本落盘那一份：缓存（读不懂 · 版本对不上 ⇒ 当没有，照那台再问一次整份）。一台一份，文件名照 `listing_copy::file_stem`。
/// 落点 · 读上限照表；原样是那台交来的字节（不是我们的格式，版本在那份里头）。
const COPY_DOC: crate::common::own_state::Doc<Vec<u8>> =
    crate::common::own_state::Doc::of(&relay_route_core::REMOTE_LISTING_FILE);

/// 从数据目录往下的那几层目录，与那台的文件。
fn copy_at(own: &Own, machine: &str) -> Option<(Vec<OwnPath>, OwnPath)> {
    let rel = relay_route_core::REMOTE_LISTING_DIR_REL.strip_prefix(".cc-monitor/")?;
    let mut dirs = vec![own.root()?];
    for seg in rel.split('/') {
        let next = dirs.last()?.join(seg).ok()?;
        dirs.push(next);
    }
    let name = format!("{}.json", face_core::listing_copy::file_stem(machine));
    let file = dirs.last()?.join(&name).ok()?;
    Some((dirs, file))
}

/// 读那台落盘的那一份（不在 · 读不出来 ⇒ 当没有）。
fn load_at(own: &Own, machine: &str) -> Option<Vec<u8>> {
    let (_, file) = copy_at(own, machine)?;
    match COPY_DOC.read_with(file.as_path(), Ok) {
        crate::common::own_state::Read::Present(b) => Some(b),
        _ => None,
    }
}

/// 整份写那台的那一份。写不成只记一笔调试日志：缓存，下一次照那台整份重来。
fn store_at(own: &Own, machine: &str, body: &[u8]) {
    let written = (|| -> Result<(), String> {
        let (dirs, file) = copy_at(own, machine).ok_or("no data dir")?;
        for d in &dirs {
            own.ensure_dir(d).map_err(|e| e.to_string())?;
        }
        let dir = dirs.last().ok_or("no data dir")?;
        let held = own.lock(dir).map_err(|e| format!("{e:?}"))?;
        own.write(&file, &held, body).map_err(|e| e.said)
    })();
    if let Err(e) = written {
        tracing::debug!("remote listing copy of {machine} not stored: {e}");
    }
}

/// 那一台的清单（生产：可达表 · 池里那条 SSH · 本进程的副本表）。`until` ＝ 发起方给的截止时刻（没给 ⇒ `bad_args`）。
pub(crate) async fn listing(
    machine: &str,
    fresh: bool,
    tz: &crate::common::time::Tz,
    until: Option<Until>,
) -> Result<Listing, crate::common::said::Said> {
    let Some(until) = until else {
        return Err(crate::common::said::Said::new(
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
    tz: &crate::common::time::Tz,
    total: Deadline,
    table: &Table,
    remote: &dyn Remote,
    copies: &Copies,
) -> Result<Listing, crate::common::said::Said> {
    let held = match copies.to_ask(machine, fresh) {
        Ask::Held(listing) => return Ok(listing),
        Ask::With(held) => held,
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
    .map_err(|s| crate::common::said::Said::new(code_of(s.code.as_deref()), s.said));
    match got {
        Ok(patch) => {
            let at_ms = crate::common::time::now_ms() as i64;
            Ok(copies.took(machine, patch, at_ms))
        }
        Err(e) => {
            copies.missed(machine, e.said.clone());
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

#[cfg(test)]
#[path = "../../../tests/backend/history/remote_listing_tests.rs"]
mod tests;
