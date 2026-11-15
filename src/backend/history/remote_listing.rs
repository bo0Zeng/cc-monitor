//! **远端那台的会话清单**：那台怎么答（帧面 `history-listing`，带键集合那一形）· 本机后端怎么取、怎么存（副本）。
//!
//! ```text
//! history-list {origin}（本机后端，按会话 ID 要一行）· history-browse {remotes, fresh}（历史页那一列）
//!   ⇒ 手上没有那台的副本 · 或要同步（fresh）⇒ 经 `remote_ask::ask_resident` 问那台的常驻：history-listing {held: 手上那份的记号}
//!        ⇒ 那台答 Patch<ListingHead, RawRow>（没变 / 只给变了的那几行 / 整份）⇒ 照答换副本（`Hand::apply`，没有判断）
//!   ⇒ 副本拼回一份清单（`Listing`）交 `history_list::answer_from` 并注解 · 筛 · 排
//!   历史页那一列不带 fresh 的那几台只读手上那份（[`kept_seen`]：没问成过 ⇒ 没读过；上一次同步没成 ⇒ 那一句），不去问
//! ```
//!
//! - 问的是那台**热着的常驻**（`--resident-call`）：那台扫过的清单缓存与这一问的备忘都在它那里；
//!   那台常驻不在 ⇒ 照实失败（`resident_absent`），不在那台起一个冷进程顶上。
//! - 副本表住这里（[`Copies`]）；本机后端把它落在数据目录底下
//!   `cache/remote-listing/<机器>.json`（缓存：读不懂当没有、不迁移），重启后头一问带盘上那份的记号；
//!   不按时间过期（这一层不看钟）：历史页开页 · 刷新带 `fresh` 才同步。
//! - 等多久只归发起方：`history-list` · `history-browse` 那一发交来的截止时刻（请求信封 `within_ms`）原样往下带（本机等它、那台 `--resident-call --within-ms` 也等它）；
//!   没带 ⇒ `bad_args`，这里不自己定一个。
//! - 压缩照旧在 SSH 那一层，这里不另做。

use cmd_wire::listing::{Listing, ListingHead, RawRow};
use cmd_wire::sync::{Hand, Held, Patch};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

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

// 副本表：手上那份此刻什么样 · 要问时带的记号 · 照答换副本 · 落盘的那一形。这一层不看钟、不碰文件系统：落下的那一刻由调用方给，
// 字节存哪由 [`Shelf`] 的实现定（生产：[`Installed`]）。
//
//   要问那台 ⇒ [`Copies::to_ask`]：手上有、不 fresh ⇒ 直接给手上那份；否则给记号去问
//     问成了 ⇒ [`Copies::took`]（照答换 · 记下落下的那一刻 · 整份写进 [`Shelf`]）；没成 ⇒ [`Copies::missed`]（手上那份不动、记下那一句）
//   只读不问 ⇒ [`Copies::kept_seen`]；副本表头一次碰那台 ⇒ 先从 [`Shelf`] 装回那台的那一份（读不懂 · 版本对不上 ⇒ 当没有）

/// 落盘那一形的版本（形状变了就加一；对不上 ⇒ 当没有）。
pub(crate) const VERSION: u32 = 1;

/// 副本的字节存哪：按机器取一份 · 存一份。取不到 ⇒ `None`；存不成由实现自己记，不往上报（缓存）。
pub(crate) trait Shelf: Send + Sync {
    fn load(&self, machine: &str) -> Option<Vec<u8>>;
    fn store(&self, machine: &str, body: &[u8]);
}

/// 一台的副本：没问成过 ⇒ `hand.held` 缺。
#[derive(Default)]
struct Slot {
    hand: Hand<ListingHead, RawRow>,
    failed: Option<String>,
    /// 手上那份落下的那一刻（毫秒）：同步没成时照它说「下面是那一刻读到的」。
    at_ms: i64,
    /// 这一台副本的代号：每换一次（问成了 · 问不成记下那一句）就变大（取那一刻的毫秒、至少比上一个大 1；从盘上装回时取装回那一刻）。
    /// 回包把它带给界面（[`kept_seen`]），界面照它认同一台哪一份新、旧的那一包到了不盖新的。
    seen: i64,
}

/// 盘上一份（读那一形）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    version: u32,
    at_ms: i64,
    hand: Hand<ListingHead, RawRow>,
}

/// 盘上一份（写那一形，借着手上那份不抄）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedRef<'a> {
    version: u32,
    at_ms: i64,
    hand: &'a Hand<ListingHead, RawRow>,
}

/// 远端那几台的副本：机器名 ⇒ 手上那份 ＋ 上一次同步没成的那一句（成了就清掉）。
#[derive(Default)]
pub(crate) struct Copies {
    book: Mutex<BTreeMap<String, Slot>>,
    shelf: Option<Box<dyn Shelf>>,
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

/// 要不要问那台。
#[derive(Debug)]
pub(crate) enum Ask {
    /// 手上有、不 fresh：就用手上那份。
    Held(Listing),
    /// 去问，带这个记号（缺 ⇒ 整份）。
    With(Option<Held>),
}

impl Copies {
    /// 落盘的副本表：头一次碰一台先从 `shelf` 装回，每次同步成了整份写回。
    pub(crate) fn on(shelf: impl Shelf + 'static) -> Copies {
        Copies {
            book: Mutex::default(),
            shelf: Some(Box::new(shelf)),
        }
    }

    /// 那一台手上那份此刻什么样（不去问），连那一份的代号（同一把锁里读：代号说的就是交出去的这一份）；从没换过 ⇒ `None`。
    pub(crate) fn kept_seen(&self, machine: &str) -> (Kept, Option<i64>) {
        let mut book = self.lock();
        let Some(c) = self.slot(&mut book, machine) else {
            return (Kept::Unread, None);
        };
        let kept = match (&c.failed, c.hand.held.is_some()) {
            (Some(why), true) => Kept::Stale {
                listing: assembled(&c.hand),
                why: why.clone(),
                at_ms: c.at_ms,
            },
            (Some(why), false) => Kept::Failed(why.clone()),
            (None, true) => Kept::Got(assembled(&c.hand)),
            (None, false) => Kept::Unread,
        };
        (kept, Some(c.seen))
    }

    /// 要不要问那台：手上有、不 `fresh` ⇒ 手上那份；否则去问、带手上那份的记号。
    pub(crate) fn to_ask(&self, machine: &str, fresh: bool) -> Ask {
        let mut book = self.lock();
        match self.slot(&mut book, machine).map(|c| &c.hand) {
            Some(hand) if !fresh && hand.held.is_some() => Ask::Held(assembled(hand)),
            Some(hand) => Ask::With(hand.held.clone()),
            None => Ask::With(None),
        }
    }

    /// 问成了：照答换（没有判断）、清掉上一次没成的那一句、记下落下的那一刻、整份写进落盘处；交拼回来的那份。
    pub(crate) fn took(
        &self,
        machine: &str,
        patch: Patch<ListingHead, RawRow>,
        at_ms: i64,
    ) -> Listing {
        let mut book = self.lock();
        let slot = book.entry(machine.to_string()).or_default();
        slot.failed = None;
        slot.hand.apply(patch);
        slot.at_ms = at_ms;
        slot.seen = next_seen(slot.seen, at_ms);
        if let Some(shelf) = &self.shelf {
            if let Some(body) = encoded(slot) {
                shelf.store(machine, &body);
            }
        }
        assembled(&slot.hand)
    }

    /// 问不成：手上那份不动、记下那一句（[`Copies::kept_seen`] 照它答），下一次问成了清掉。
    pub(crate) fn missed(&self, machine: &str, why: String) {
        let mut book = self.lock();
        match self.slot(&mut book, machine) {
            Some(slot) => {
                slot.failed = Some(why);
                slot.seen = next_seen(slot.seen, crate::common::time::now_ms() as i64);
            }
            None => {
                book.insert(
                    machine.to_string(),
                    Slot {
                        failed: Some(why),
                        seen: crate::common::time::now_ms() as i64,
                        ..Slot::default()
                    },
                );
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Slot>> {
        self.book.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 表里那一台；表里没有、落盘处有读得懂的一份 ⇒ 先装回。
    fn slot<'b>(
        &self,
        book: &'b mut BTreeMap<String, Slot>,
        machine: &str,
    ) -> Option<&'b mut Slot> {
        if !book.contains_key(machine) {
            let loaded = self
                .shelf
                .as_ref()
                .and_then(|s| s.load(machine))
                .and_then(|b| decoded(&b))?;
            book.insert(machine.to_string(), loaded);
        }
        book.get_mut(machine)
    }
}

/// 下一个代号：那一刻的毫秒，至少比上一个大 1（钟往回拨也不倒退）。
fn next_seen(prev: i64, now_ms: i64) -> i64 {
    now_ms.max(prev.saturating_add(1))
}

/// 落盘那一份的字节（紧凑 JSON ＋ 收尾换行）。
fn encoded(slot: &Slot) -> Option<Vec<u8>> {
    let mut body = serde_json::to_vec(&SavedRef {
        version: VERSION,
        at_ms: slot.at_ms,
        hand: &slot.hand,
    })
    .ok()?;
    body.push(b'\n');
    Some(body)
}

/// 盘上那一份装回：读不懂 · 版本对不上 · 没有记号 ⇒ 当没有。
fn decoded(body: &[u8]) -> Option<Slot> {
    let saved: Saved = serde_json::from_slice(body).ok()?;
    if saved.version != VERSION || saved.hand.held.is_none() {
        return None;
    }
    Some(Slot {
        hand: saved.hand,
        failed: None,
        at_ms: saved.at_ms,
        // 装回那一刻：比上一回进程里交出去的代号都大（那时的钟早于此刻），界面不会把装回的这一份当旧的。
        seen: crate::common::time::now_ms() as i64,
    })
}

/// 一台落盘的文件名（不带扩展名）：字母数字与 `-` `_` 照写，别的（连打头的 `.`）写成 `%XX`，一台一个、不会拼出路径。
pub(crate) fn file_stem(machine: &str) -> String {
    let mut out = String::with_capacity(machine.len());
    for b in machine.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    if out.is_empty() {
        out.push('%');
    }
    out
}

/// 手上那份拼回一份清单（行照 `order` 的先后）。
fn assembled(hand: &Hand<ListingHead, RawRow>) -> Listing {
    let rows = hand.in_order().cloned().collect();
    let head = hand
        .head
        .as_ref()
        .map(|h| h.info.clone())
        .unwrap_or_default();
    Listing::join(head, rows)
}

/// 那一台手上那份（生产：本进程的副本表），连那一份的代号（[`Copies::kept_seen`]）。
pub(crate) fn kept_seen(machine: &str) -> (Kept, Option<i64>) {
    COPIES.kept_seen(machine)
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

/// 副本落盘那一份：缓存（读不懂 · 版本对不上 ⇒ 当没有，照那台再问一次整份）。一台一份，文件名照 [`file_stem`]。
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
    let name = format!("{}.json", file_stem(machine));
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
/// 问不成 ⇒ 那台的码与那一句（够不着 `unreachable` · 常驻不在 `resident_absent` · …），手上那份不动、那一句记下（[`Copies::kept_seen`] 照它答），下一次问成了清掉。
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
