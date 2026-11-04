//! **组**：一群会话 ＋ 一个名字。桌面标签页 · 手机分组 · cc-bus 范围 · agent 自问读的都是这一份。
//!
//! 组是跨机器的集合（ARCHITECTURE「存储层」③）：没有哪一台是它的主人，跟着组员走，每台只存自己有组员的那一片。
//!
//! ```text
//! 组那一片 {id, name, made, named}  住每一台有组员的那台：注解文件 `history-metadata.json` 的 `groups` 那一节（history_annotations::GroupRec），
//!                                   名字带「几时起的」（named）抄进每一片
//! 组员关系 · 固定                   住会话那一台：那条会话注解上的 `groups` · `pinned` 两格（一条会话可在几组）
//!
//! groups-here（每台）   ⇒ 这台那一片 ＋ 这台带组或固定的会话（各带此刻活不活，问 live_table）
//! groups-read {machines}（发问那台）⇒ remote_ask::ask_each 问各台 groups-here、在这里合一次（`face_core::groups::merge`，手机核心调同一份）：
//!     名字取 named 最新的那一片 · 组员照各台 · 组头计数与字在这里数一次；某台没答 ⇒ 照它上一次答的那份、标上次所知（那台的组员画成看不见）；
//!     从没答过 ⇒ 那台读不到；哪一片都没有那一组的记录、只有组员指着它 ⇒ 名字不知道（组照出、组员不漏）
//! groups-write ⇒ make 只起 id；move 只写组员那一台（进组抄记录 · 最后一位出组删记录）；
//!     rename · dissolve 由收到的那台发给各片（ask_each），没送到的那几台照实列进回包、记进这一端的待补（`groups_owed.rs`）；
//!     那台下一次答这一端的 groups-here ⇒ 照它那一片判一次，该补的补发（只写那台自己），补成了 · 到期了消掉
//! ```
//!
//! - 组 id 全局不重（起它的那一台随机起），所以注解上只记 id，不记别的台（机器名是相对问的那一端的，记了也对不上）。
//! - 上次所知那一份按机器名记：本进程内存里一份，别的台答的那几份也记进这台的上次值（`common/last_seen_store.rs`，一样 `groups`），
//!   本机后端新起、那台没答 ⇒ 照盘上那一份出（离线那台上固定的会话开机照样出）；删机器时那台的一起清。
//! - 等多久只归发起方（请求信封 `within_ms`）；问到别的台而它缺 ⇒ `bad_args`。

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::wire::WireLife;
use cmd_wire::cells::Words;
use cmd_wire::frame::SessionFace;
use cmd_wire::groups::{GroupsHere, GroupsMachine, GroupsRead, HereGroup, HereMember, MemberFacts};
use cmd_wire::listing::HistoryMachineRead as Read;
use cmd_wire::Machine;
use face_core::groups::{merge, Half};
use serde::{Deserialize, Serialize};

use super::groups_owed as owed;
use super::history_annotations::{self as notes, AnnotationErr, Book, GroupRec, Named, Table};
use crate::common::said::Said;
use crate::dial::remote_ask::Remote;
use crate::platform::fs::Own;
use crate::wire::typed::{codes, Command};
use crate::wire::SessionFaceExt as _;

// ───────────────────────── 这台那一半：groups-here ─────────────────────────

/// 一条组员此刻的样子 ＋ 复活那几格（这台判的）。
pub type Seen = (SessionFace, MemberFacts);

/// 这台那一半（注解 · 组本 · 每条的样子与那几格由调用方给）：组照 id 排；会话只带有组或固定的，照 sid 排。
pub fn here_of(table: &Table, book: &Book, seen: impl Fn(&str) -> Seen) -> GroupsHere {
    GroupsHere {
        groups: book
            .iter()
            .map(|(id, g)| HereGroup {
                id: id.clone(),
                name: g.name.name.clone(),
                made: g.made,
                named: g.name.named,
            })
            .collect(),
        members: table
            .iter()
            .filter(|(_, e)| !e.groups.is_empty() || e.pinned)
            .map(|(sid, e)| {
                let (face, facts) = seen(sid);
                HereMember {
                    sid: sid.clone(),
                    groups: e.groups.clone(),
                    pinned: e.pinned,
                    face,
                    facts,
                }
            })
            .collect(),
    }
}

/// 这台清单的行 ⇒ 每条的样子与那几格（照清单那一行同一处判：[`super::history_list::member_facts`]）。
pub(crate) fn seen_from(
    rows: Vec<cmd_wire::listing::RawRow>,
    table: &Table,
) -> BTreeMap<String, Seen> {
    rows.into_iter()
        .filter(|r| {
            table
                .get(&r.session_id)
                .is_some_and(|e| !e.groups.is_empty() || e.pinned)
        })
        .map(|r| {
            (
                r.session_id.clone(),
                super::history_list::member_facts(r, Some(table)),
            )
        })
        .collect()
}

/// 清单里没有的那一条（记录已不在 · 还没落盘）：样子照活会话表，标题照注解（没有 ⇒ 清单那一句「没名字」）。
pub fn unlisted(entry: Option<&cmd_wire::listing::Entry>, life: WireLife) -> Seen {
    let label = super::search_rules::session_label(&super::search_rules::LabelFacts {
        custom: entry.and_then(|e| e.custom_title.clone()),
        ..Default::default()
    })
    .0;
    (
        SessionFace::of(crate::wire::Life::of_wire(&life)),
        MemberFacts {
            title: Words(label),
            cwd: None,
            record: None,
            account: None,
            bg: false,
            at: None,
        },
    )
}

/// 这台的组那一半此刻的记号（不看样子与那几格）：watcher 重读注解文件时照它判「组那一半变了没有」、变了才推 `changed {groups}`。
pub fn mark(table: &Table, book: &Book) -> u64 {
    let half = here_of(table, book, |_| {
        unlisted(None, WireLife::Ended { by: None })
    });
    let bytes = serde_json::to_vec(&half).unwrap_or_default();
    crate::common::digest::Fnv::default().part(&bytes).value()
}

/// 这台那一半（生产：这台那份注解文件 ＋ 这台的清单 ＋ 活会话表）。推不出家 · 读不懂 ⇒ 那一句。
fn here_now() -> Result<GroupsHere, String> {
    let (table, book) = notes::load_all().unwrap_or_else(|| {
        Err(copy_core::copy_text(
            "beHistoryAnnotations.needPath.unknown",
            &[("env", creds_core::store::DATA_DIR_ENV)],
        ))
    })?;
    // 清单读不了 ⇒ 每条照活会话表画、那几格缺（组照出，组员不漏）。
    let rows = super::history_list::machine_listing()
        .map(|l| l.rows)
        .unwrap_or_default();
    let seen = seen_from(rows, &table);
    let live = super::live_table::now(&super::listing::agent_home());
    Ok(here_of(&table, &book, |sid| {
        seen.get(sid)
            .cloned()
            .unwrap_or_else(|| unlisted(table.get(sid), live.life_of(sid)))
    }))
}

codes! {
    /// `groups-here` 的码。
    pub enum GroupsHereErr {
        /// 推不出家 · 注解文件读不懂。
        Failed = "failed",
    }
}

/// `groups-here`：这台那一半（别的后端经 `--resident-call` 问它，本机后端的 `groups-read` 同进程问它）。
pub struct GroupsHereCmd;

impl Command for GroupsHereCmd {
    const NAME: &'static str = "groups-here";
    type Req = crate::wire::typed::NoArgs;
    type Reply = GroupsHere;
    type Err = GroupsHereErr;
}

impl crate::wire::typed::Handle for GroupsHereCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<GroupsHere, GroupsHereErr> {
        here_now().map_err(GroupsHereErr::Failed)
    }
}

// ───────────────────────── 几台并成一份：groups-read ─────────────────────────

/// `groups-read` 的入参：问哪几台（本机逐字 `"<local>"`）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct GroupsReadReq {
    #[cfg_attr(any(test, feature = "ts"), ts(as = "Vec<String>"))]
    pub(crate) machines: Vec<Machine>,
}

codes! {
    /// `groups-read` 自己没有别的码（哪台没问到只进回包的 `machines`）。
    pub enum GroupsReadErr {}
}

/// 各台上一次答的那一半 ＋ 那一刻（毫秒）：本进程里这一份，盘上那一份（这台的上次值，`common/last_seen_store.rs`）在头一回有台没答时读进来一次。
#[derive(Default)]
pub(crate) struct Kept {
    mem: Mutex<BTreeMap<Machine, (GroupsHere, i64)>>,
    /// 盘上那一份读进来过没有（本进程只读一次；读进来的不盖这一进程里已有的）。
    from_disk: std::sync::atomic::AtomicBool,
}

impl Kept {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<Machine, (GroupsHere, i64)>> {
        self.mem.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 有台这一问没答、本进程里又没有它 ⇒ 把盘上那一份读进来（只一次）。
    async fn fill_from(&self, disk: &Own, missing: bool) {
        use std::sync::atomic::Ordering;
        if !missing || self.from_disk.swap(true, Ordering::AcqRel) {
            return;
        }
        let own = disk.clone();
        let Ok(rows) = tokio::task::spawn_blocking(move || kept_on_disk(&own)).await else {
            return;
        };
        let mut k = self.lock();
        for (origin, here, at_ms) in rows {
            k.entry(origin).or_insert((here, at_ms as i64));
        }
    }
}

/// 组的上次所知在上次值那一份里那一样的键（`groups-here` 的应答；界面那条 `last-seen-write` 的种类里没有它）。
const KEPT_KIND: &str = "groups";
/// 上次值那一份里机器名最长多少字节（同 `footprint/last_seen.rs` 收的）。
const KEPT_ORIGIN_MAX: usize = 256;

/// 这一问答了的那几台（只记远端：本机每一问都答）记进盘上那一份；记不成只留一行日志（它只是缓存）。
async fn keep_on(disk: &Own, answered: Vec<(Machine, GroupsHere)>, now_ms: i64) {
    if answered.is_empty() {
        return;
    }
    let own = disk.clone();
    let done =
        tokio::task::spawn_blocking(move || keep_on_disk(&own, &answered, now_ms.max(0) as u64))
            .await;
    if let Ok(Err(e)) = done {
        tracing::warn!(target: "groups", reason = %e.said, "last-known groups not kept");
    }
}

/// 一次锁里记完各台那一样；序列化后超过一样的封顶的那台不记。
fn keep_on_disk(own: &Own, answered: &[(Machine, GroupsHere)], now_ms: u64) -> Result<(), Said> {
    use crate::common::last_seen_store as store;
    let shape = cmd_wire::shapes::shape_of(GroupsHereCmd::NAME);
    let rows: Vec<(String, serde_json::Value)> = answered
        .iter()
        .filter(|(m, _)| m.is_remote() && m.to_string().len() <= KEPT_ORIGIN_MAX)
        .filter_map(|(m, h)| Some((m.to_string(), serde_json::to_value(h).ok()?)))
        .filter(|(_, v)| v.to_string().len() <= store::MAX_VALUE_BYTES.as_len())
        .collect();
    if rows.is_empty() {
        return Ok(());
    }
    let (dir, file) = store::made_place(own)?;
    store::edit(own, &dir, &file, |all| {
        for (origin, value) in rows {
            store::record(all, origin, KEPT_KIND, &value, shape, now_ms);
        }
        true
    })
}

/// 盘上记着的各台那一样 ＋ 记下的时刻（毫秒）：只收这一版的形状、解得回 `GroupsHere` 的，别的照没记过。
fn kept_on_disk(own: &Own) -> Vec<(Machine, GroupsHere, u64)> {
    use crate::common::last_seen_store as store;
    let Ok((_, file)) = store::place(own) else {
        return Vec::new();
    };
    let shape = cmd_wire::shapes::shape_of(GroupsHereCmd::NAME);
    store::load(file.as_path())
        .iter()
        .filter_map(|(origin, mine)| {
            let e = store::entry(Some(mine), KEPT_KIND)?;
            let here = store::this_version::<GroupsHere>(&e, shape)?;
            Some((Machine::named(origin.clone()), here, e.at_ms))
        })
        .collect()
}

static KEPT: std::sync::LazyLock<Kept> = std::sync::LazyLock::new(Kept::default);

/// `groups-read`：本机后端问到各台的那一半（本机同进程、别的台经 `--resident-call` 问它的常驻 `groups-here`），并成一份再答。
pub struct GroupsReadCmd;

impl Command for GroupsReadCmd {
    const NAME: &'static str = "groups-read";
    type Req = GroupsReadReq;
    type Reply = GroupsRead;
    type Err = GroupsReadErr;
}

impl crate::wire::typed::Handle for GroupsReadCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: GroupsReadReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<GroupsRead, GroupsReadErr>> + Send + 'static {
        let (cx, own) = cx.split();
        async move {
            let local = async {
                tokio::task::spawn_blocking(here_now)
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
                    .map_err(Said::from)
            };
            // 待补读不懂 ⇒ 这一问不补（不覆盖那一份），照常合。
            let owed = owed::load_at(&own).unwrap_or_default();
            let (read, settled) = read_with(
                &req.machines,
                local,
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
                &KEPT,
                Some(&own),
                crate::common::time::now_ms() as i64,
                &owed.owed,
            )
            .await?;
            if !settled.is_empty() {
                if let Err(e) = owed::edit_at(&own, |o| o.owed.retain(|d| !settled.contains(d))) {
                    tracing::warn!(target: "groups", reason = %e.said, "groups-owed not updated");
                }
            }
            Ok(read)
        }
    }
}

/// 只问这台的 `groups-read`（一次性进程里问不到别的台时用：agent 自问那一条的退路）。
pub async fn read_here(tz: crate::common::time::Tz) -> Result<GroupsRead, String> {
    let local = async {
        tokio::task::spawn_blocking(here_now)
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r)
            .map_err(Said::from)
    };
    read_with(
        &[Machine::named(cmd_wire::LOCAL)],
        local,
        &tz,
        None,
        &crate::dial::remote_ask::REACH,
        &crate::dial::remote_ask::DialRemote,
        &KEPT,
        None,
        crate::common::time::now_ms() as i64,
        &[],
    )
    .await
    .map(|(read, _)| read)
    .map_err(|e| format!("{e:?}"))
}

/// [`GroupsReadCmd`] 的本体（本机那一台怎么答 · 可达表 · 对面 · 缓存 · 盘上的上次所知 · 此刻 · 这一端欠各台的那几句由调用方给）。
/// `disk` 是 `None` ⇒ 上次所知只看本进程那一份（不读不写盘）。
/// 回并好的一份 ＋ 这一问了结了的那几条待补（补成了 · 到期了；调用方从盘上那一份里消掉）。
#[expect(
    clippy::too_many_arguments,
    reason = "本机那一台怎么答 · 可达表 · 对面 · 缓存 · 盘 · 期限 · 此刻 · 待补都由调用方注入（判据换替身），并成一个结构只是换个地方摆"
)]
pub(crate) async fn read_with(
    machines: &[Machine],
    local: impl std::future::Future<Output = Result<GroupsHere, Said>> + Send,
    tz: &crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &crate::dial::remote_ask::Table,
    remote: &dyn Remote,
    kept: &Kept,
    disk: Option<&Own>,
    now_ms: i64,
    owed: &[owed::Debt],
) -> Result<(GroupsRead, Vec<owed::Debt>), GroupsReadErr> {
    let mut got = crate::dial::remote_ask::ask_each::<GroupsHereCmd, _>(
        machines,
        &crate::wire::typed::NoArgs::default(),
        local,
        tz,
        until,
        table,
        remote,
    )
    .await
    .map_err(GroupsReadErr::BadArgs)?;
    // 欠的那台已不在机器表 ⇒ 到期；答了的那几台照它那一片补 · 丢。
    let mut settled: Vec<owed::Debt> = owed
        .iter()
        .filter(|d| !table.has(&d.machine.to_string()))
        .cloned()
        .collect();
    for (origin, r) in &mut got {
        let Ok(here) = r else { continue };
        for debt in owed.iter().filter(|d| d.machine == *origin) {
            if settled.contains(debt) {
                continue;
            }
            if owed::due(debt, here) == owed::Due::Drop {
                settled.push(debt.clone());
            } else if resend(debt, tz, until, table, remote).await {
                owed::apply(debt, here);
                settled.push(debt.clone());
            }
        }
    }
    let mut halves: Vec<(Machine, Half)> = Vec::new();
    let mut heads = Vec::new();
    if let Some(disk) = disk {
        let missing = {
            let k = kept.lock();
            got.iter().any(|(o, r)| r.is_err() && !k.contains_key(o))
        };
        kept.fill_from(disk, missing).await;
        let answered = got
            .iter()
            .filter_map(|(o, r)| Some((o.clone(), r.as_ref().ok()?.clone())))
            .collect();
        keep_on(disk, answered, now_ms).await;
    }
    {
        let mut k = kept.lock();
        for (origin, r) in got {
            let key = origin.clone();
            let (half, read) = match r {
                Ok(h) => {
                    k.insert(key, (h.clone(), now_ms));
                    (Half::Now(h), Read::Ok { notice: None })
                }
                Err(s) => match k.get(&key) {
                    Some((h, at_ms)) => {
                        let at = crate::common::time::history_at_text(*at_ms, now_ms, tz);
                        let offline = Words(comms_inward::chan::said::offline(&origin.to_string()));
                        (
                            Half::Before {
                                here: h.clone(),
                                at: at.clone(),
                                offline,
                            },
                            Read::Stale { why: s.said, at },
                        )
                    }
                    None => (Half::None, Read::Failed { why: s.said }),
                },
            };
            heads.push(GroupsMachine {
                origin: origin.clone(),
                read,
            });
            halves.push((origin, half));
        }
    }
    Ok((merge(&halves, heads), settled))
}

/// 补发欠那台的一句（`groups-write`，转过去的那一问只写那台自己）：那台答了、它那一片写成了 ⇒ 真。
async fn resend(
    debt: &owed::Debt,
    tz: &crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &crate::dial::remote_ask::Table,
    remote: &dyn Remote,
) -> bool {
    let here = vec![Machine::named(cmd_wire::LOCAL)];
    let req = match &debt.what {
        owed::Owe::Rename { name, .. } => GroupsWriteReq::Rename {
            id: debt.id.clone(),
            name: name.clone(),
            machines: here,
        },
        owed::Owe::Dissolve => GroupsWriteReq::Dissolve {
            id: debt.id.clone(),
            machines: here,
        },
    };
    // 欠的总是别的台：本机那一片当场就写了，不会欠。
    let local = std::future::ready(Err(Said::from("groups-owed: never owed to this machine")));
    let got = crate::dial::remote_ask::ask_each::<GroupsWriteCmd, _>(
        std::slice::from_ref(&debt.machine),
        &req,
        local,
        tz,
        until,
        table,
        remote,
    )
    .await;
    matches!(
        got.as_deref(),
        Ok([(_, Ok(GroupsWritten::Sent { missed, .. }))]) if missed.is_empty()
    )
}

/// `rename` · `dissolve` 发完（`missed` 没送到的那几台）⇒ 记进这一端的待补（[`owed::note`]）；别的几样不欠。
pub(crate) fn owe(owed: &mut owed::Owed, req: &GroupsWriteReq, missed: &[GroupMissed], now: i64) {
    let (id, machines, what) = match req {
        GroupsWriteReq::Rename { id, name, machines } => (
            id,
            machines,
            owed::Owe::Rename {
                name: name.trim().to_string(),
                named: now,
            },
        ),
        GroupsWriteReq::Dissolve { id, machines } => (id, machines, owed::Owe::Dissolve),
        _ => return,
    };
    let missed: Vec<Machine> = missed.iter().map(|m| m.origin.clone()).collect();
    let sent: Vec<Machine> = machines
        .iter()
        .filter(|m| !m.is_local() && !missed.contains(m))
        .cloned()
        .collect();
    owed::note(owed, id, &sent, &missed, what);
}

/// 格目录的样本（`faces/cells_catalog.rs`）：真合（[`merge`]）出来的一份 —— 名字取自这一问答的那片 · 上次那片各一组，
/// 各组组头都有计数；那台这一问的结局四种各一台。可缺的格都填上、列表都不空。
pub fn specimen() -> GroupsRead {
    let w = |v: &str| Words(v.to_string());
    let rec = |id: &str| HereGroup {
        id: id.to_string(),
        name: format!("n-{id}"),
        made: 1,
        named: 2,
    };
    let member = |sid: &str, groups: &[&str], activity| HereMember {
        sid: sid.to_string(),
        groups: groups.iter().map(|g| g.to_string()).collect(),
        pinned: true,
        face: <SessionFace as crate::wire::SessionFaceExt>::of(crate::wire::Life::Live {
            activity: Some(activity),
            host: None,
        }),
        facts: MemberFacts {
            title: w("t"),
            cwd: Some("/w".to_string()),
            record: Some("/w/s".to_string()),
            account: Some("a".to_string()),
            bg: false,
            at: Some(1),
        },
    };
    let now = GroupsHere {
        groups: vec![rec("g1")],
        members: vec![
            member("s1", &["g1"], crate::agents::SessionActivity::NeedsYou),
            member("s2", &["g2"], crate::agents::SessionActivity::Working),
        ],
    };
    // 上次所知那片的组员画成看不见那一张（灰的那几格 `can.*.why` 才露得出来）。
    let before = GroupsHere {
        groups: vec![rec("g2")],
        members: vec![member(
            "s3",
            &["g2"],
            crate::agents::SessionActivity::Working,
        )],
    };
    let at = |m: &str| GroupsMachine {
        origin: Machine::named(m),
        read: Read::Unread,
    };
    merge(
        &[
            (Machine::named("a"), Half::Now(now)),
            (
                Machine::named("b"),
                Half::Before {
                    here: before,
                    at: w("t"),
                    offline: w("o"),
                },
            ),
        ],
        vec![
            GroupsMachine {
                read: Read::Ok {
                    notice: Some("n".to_string()),
                },
                ..at("a")
            },
            GroupsMachine {
                read: Read::Stale {
                    why: "w".to_string(),
                    at: w("t"),
                },
                ..at("b")
            },
            GroupsMachine {
                read: Read::Failed {
                    why: "w".to_string(),
                },
                ..at("c")
            },
            at("d"),
        ],
    )
}

// ───────────────────────── 写：groups-write ─────────────────────────

/// 组名至多几个字。
pub(crate) const NAME_MAX: usize = 80;

/// `groups-write` 的入参。
/// - `make`：起一个新组的 id 与时刻，不写盘（一个组员都没有的组不住任何一台）；回的那一份随第一位组员的 `move` 抄进它那一台。
/// - `move`：这台一条会话出 `leave` 那一组、进 `join` 那一组（带整份记录抄进这一片；这一片已有 ⇒ 名字照 `named` 新的那份）；
///   出组后这一片没人在那一组 ⇒ 那一组的记录也删。只写收到它的那一台：问组员所在那台。
/// - `rename` · `dissolve`：收到的那台发给 `machines` 里的每一片（本机逐字 `"<local>"`；转给别的台时只写那台自己），
///   没有那一组的片什么都不做；解散在那一片摘掉每条会话身上的那一组。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum GroupsWriteReq {
    Make {
        name: String,
    },
    Move {
        sid: String,
        #[serde(default)]
        #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
        join: Option<HereGroup>,
        #[serde(default)]
        #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
        leave: Option<String>,
    },
    Rename {
        id: String,
        name: String,
        #[cfg_attr(any(test, feature = "ts"), ts(as = "Vec<String>"))]
        machines: Vec<Machine>,
    },
    Dissolve {
        id: String,
        #[cfg_attr(any(test, feature = "ts"), ts(as = "Vec<String>"))]
        machines: Vec<Machine>,
    },
}

/// 一台没送到：哪台 · 为什么（那台这一问说的那一句）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct GroupMissed {
    #[cfg_attr(any(test, feature = "ts"), ts(as = "String"))]
    pub(crate) origin: Machine,
    pub(crate) why: String,
}

/// `groups-write` 的回包：起好的那一组 · 那条会话此刻在哪几组 · 发给各片的结局（没送到的那几台，照问的先后）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum GroupsWritten {
    Made(HereGroup),
    Moved {
        sid: String,
        groups: Vec<String>,
    },
    Sent {
        id: String,
        missed: Vec<GroupMissed>,
    },
}

codes! {
    /// `groups-write` 的码（同注解那几条：读不懂拒写 · 写不下去 · 推不出家）。
    pub enum GroupsWriteErr {
        Unreadable = "annotations_unreadable",
        IoFailed = "io_failed",
        NoAnnotations = "no_annotations",
    }
}

fn write_err(e: AnnotationErr) -> GroupsWriteErr {
    match e {
        AnnotationErr::BadArgs(s) => GroupsWriteErr::BadArgs(s),
        AnnotationErr::Unreadable(s) => GroupsWriteErr::Unreadable(s),
        AnnotationErr::IoFailed(s) => GroupsWriteErr::IoFailed(s),
        AnnotationErr::NoAnnotations(s) => GroupsWriteErr::NoAnnotations(s),
    }
}

/// 名字：去掉首尾空白，空 · 超过 [`NAME_MAX`] 个字 · 带控制字符 ⇒ `bad_args`。
fn checked_name(name: &str) -> Result<String, AnnotationErr> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > NAME_MAX || n.chars().any(char::is_control) {
        return Err(AnnotationErr::BadArgs(crate::common::contract::malformed(
            &format!("a group name must be 1..={NAME_MAX} characters on one line"),
        )));
    }
    Ok(n.to_string())
}

/// 新组的 id：进程随机种子 ＋ 进程内计数 ＋ 此刻，不可猜、不重。
fn new_id(now: i64) -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(SEQ.fetch_add(1, Ordering::Relaxed));
    h.write_i64(now);
    h.write_u32(std::process::id());
    format!("g{:016x}", h.finish())
}

fn group_id(id: &str) -> Result<String, AnnotationErr> {
    notes::checked_group_id(id)
        .map(str::to_string)
        .map_err(AnnotationErr::BadArgs)
}

/// 这一片里一条会话出 `leave`、进 `join`（见 [`GroupsWriteReq::Move`]）。
fn move_here(
    table: &mut Table,
    book: &mut Book,
    sid: &str,
    join: Option<HereGroup>,
    leave: Option<String>,
) -> Result<GroupsWritten, AnnotationErr> {
    let sid = notes::checked_sid(sid)?.to_string();
    let join = join
        .map(|g| {
            Ok::<_, AnnotationErr>(HereGroup {
                id: group_id(&g.id)?,
                name: checked_name(&g.name)?,
                ..g
            })
        })
        .transpose()?;
    let leave = leave.as_deref().map(group_id).transpose()?;
    let had = table.contains_key(&sid);
    let entry = table.entry(sid.clone()).or_default();
    if let Some(id) = &leave {
        entry.groups.retain(|g| g != id);
    }
    if let Some(g) = &join {
        if !entry.groups.contains(&g.id) {
            entry.groups.push(g.id.clone());
        }
    }
    let groups = entry.groups.clone();
    if !had && *entry == cmd_wire::listing::Entry::default() {
        table.remove(&sid);
    }
    if let Some(g) = join {
        match book.get_mut(&g.id) {
            None => {
                book.insert(
                    g.id,
                    GroupRec {
                        name: Named {
                            name: g.name,
                            named: g.named,
                        },
                        made: g.made,
                    },
                );
            }
            Some(r) => {
                r.made = r.made.min(g.made);
                if g.named > r.name.named {
                    r.name = Named {
                        name: g.name,
                        named: g.named,
                    };
                }
            }
        }
    }
    if let Some(id) = leave {
        if !table.values().any(|e| e.groups.contains(&id)) {
            book.remove(&id);
        }
    }
    Ok(GroupsWritten::Moved { sid, groups })
}

/// 这一片改名：有那一组 ⇒ 换名字、`named` 记成此刻（不早于这一片原来那份，这一片里新改的总算最新）；没有 ⇒ 不动。
fn rename_here(book: &mut Book, id: &str, name: &str, now: i64) {
    if let Some(r) = book.get_mut(id) {
        r.name = Named {
            name: name.to_string(),
            named: now.max(r.name.named + 1),
        };
    }
}

/// 这一片解散：那一组的记录删掉、每条会话身上的那一组摘掉（会话一条不动）。
fn dissolve_here(table: &mut Table, book: &mut Book, id: &str) {
    book.remove(id);
    for e in table.values_mut() {
        e.groups.retain(|g| g != id);
    }
}

/// 只写这一台的那几样（`make` · `move` · 这一片的 `rename` · `dissolve`）：「现在」由调用方给。
pub fn write_at(
    own: &crate::platform::fs::Own,
    req: GroupsWriteReq,
    now: i64,
) -> Result<GroupsWritten, GroupsWriteErr> {
    match req {
        GroupsWriteReq::Make { name } => {
            let name = checked_name(&name).map_err(write_err)?;
            Ok(GroupsWritten::Made(HereGroup {
                id: new_id(now),
                name,
                made: now,
                named: now,
            }))
        }
        GroupsWriteReq::Move { sid, join, leave } => {
            notes::edit_slice_at(own, now, |table, book| {
                move_here(table, book, &sid, join, leave)
            })
            .map_err(write_err)
        }
        GroupsWriteReq::Rename { id, name, .. } => {
            let id = group_id(&id).map_err(write_err)?;
            let name = checked_name(&name).map_err(write_err)?;
            notes::edit_slice_at(own, now, |_, book| {
                rename_here(book, &id, &name, now);
                Ok(GroupsWritten::Sent {
                    id: id.clone(),
                    missed: vec![],
                })
            })
            .map_err(write_err)
        }
        GroupsWriteReq::Dissolve { id, .. } => {
            let id = group_id(&id).map_err(write_err)?;
            notes::edit_slice_at(own, now, |table, book| {
                dissolve_here(table, book, &id);
                Ok(GroupsWritten::Sent {
                    id: id.clone(),
                    missed: vec![],
                })
            })
            .map_err(write_err)
        }
    }
}

/// `rename` · `dissolve` 发给各片（本机那一片怎么写 · 可达表 · 对面由调用方给）：先核 id 与名字，再 `ask_each`；
/// 转给别的台的那一问 `machines` 只写那台自己。没送到的那几台照问的先后列进回包。
pub(crate) async fn send_with(
    req: GroupsWriteReq,
    local: impl FnOnce(GroupsWriteReq) -> Result<GroupsWritten, GroupsWriteErr> + Send,
    tz: &crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &crate::dial::remote_ask::Table,
    remote: &dyn Remote,
) -> Result<GroupsWritten, GroupsWriteErr> {
    let here = vec![Machine::named(cmd_wire::LOCAL)];
    let (machines, id, fwd) = match req {
        GroupsWriteReq::Rename { id, name, machines } => {
            let id = group_id(&id).map_err(write_err)?;
            let name = checked_name(&name).map_err(write_err)?;
            let fwd = GroupsWriteReq::Rename {
                id: id.clone(),
                name,
                machines: here,
            };
            (machines, id, fwd)
        }
        GroupsWriteReq::Dissolve { id, machines } => {
            let id = group_id(&id).map_err(write_err)?;
            let fwd = GroupsWriteReq::Dissolve {
                id: id.clone(),
                machines: here,
            };
            (machines, id, fwd)
        }
        other => return local(other),
    };
    let mine = fwd.clone();
    let local = async move {
        local(mine).map_err(|e| {
            use crate::wire::typed::Codes as _;
            e.into_said()
        })
    };
    let got = crate::dial::remote_ask::ask_each::<GroupsWriteCmd, _>(
        &machines, &fwd, local, tz, until, table, remote,
    )
    .await
    .map_err(GroupsWriteErr::BadArgs)?;
    // 那台答了却说它自己那一片没写成（转过去的那一问只写它自己）⇒ 也算没送到。
    let missed = got
        .into_iter()
        .filter_map(|(origin, r)| match r {
            Err(s) => Some(GroupMissed {
                origin,
                why: s.said,
            }),
            Ok(GroupsWritten::Sent { missed, .. }) => missed
                .into_iter()
                .next()
                .map(|m| GroupMissed { origin, why: m.why }),
            Ok(_) => None,
        })
        .collect();
    Ok(GroupsWritten::Sent { id, missed })
}

/// `groups-write`：起组 · 进出组（只写这一台）· 改名 · 解散（发给各片）。
pub struct GroupsWriteCmd;

impl Command for GroupsWriteCmd {
    const NAME: &'static str = "groups-write";
    type Req = GroupsWriteReq;
    type Reply = GroupsWritten;
    type Err = GroupsWriteErr;
}

impl crate::wire::typed::Handle for GroupsWriteCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: GroupsWriteReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<GroupsWritten, GroupsWriteErr>> + Send + 'static
    {
        let (cx, own) = cx.split();
        async move {
            let now = crate::common::time::now_ms() as i64;
            let got = send_with(
                req.clone(),
                |r| write_at(&own, r, now),
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await?;
            // 没送到的那几台记进待补（那台再答 `groups-here` 时补发）；记不下照实留一行日志，这一问照答。
            let other = match &req {
                GroupsWriteReq::Rename { machines, .. }
                | GroupsWriteReq::Dissolve { machines, .. } => {
                    machines.iter().any(|m| !m.is_local())
                }
                _ => false,
            };
            if let (true, GroupsWritten::Sent { missed, .. }) = (other, &got) {
                if let Err(e) = owed::edit_at(&own, |o| owe(o, &req, missed, now)) {
                    tracing::warn!(target: "groups", reason = %e.said, "groups-owed not noted");
                }
            }
            Ok(got)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/history/groups_tests.rs"]
mod tests;
