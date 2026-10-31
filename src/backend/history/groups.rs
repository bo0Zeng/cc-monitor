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
//! groups-read {machines}（发问那台）⇒ remote_ask::ask_each 问各台 groups-here、在这里合一次（[`merge`]）：
//!     名字取 named 最新的那一片 · 组员照各台 · 组头计数与字在这里数一次；某台没答 ⇒ 照它上一次答的那份、标上次所知（那台的组员画成看不见）；
//!     从没答过 ⇒ 那台读不到；哪一片都没有那一组的记录、只有组员指着它 ⇒ 名字不知道（组照出、组员不漏）
//! groups-write ⇒ make 只起 id；move 只写组员那一台（进组抄记录 · 最后一位出组删记录）；
//!     rename · dissolve 由收到的那台发给各片（ask_each），没送到的那几台照实列进回包（下次连上补发这一步还没做）
//! ```
//!
//! - 组 id 全局不重（起它的那一台随机起），所以注解上只记 id，不记别的台（机器名是相对问的那一端的，记了也对不上）。
//! - 上次所知那一份只在本进程内存里（缓存，按机器名），本机后端重启后第一问之前没有。
//! - 等多久只归发起方（请求信封 `within_ms`）；问到别的台而它缺 ⇒ `bad_args`。

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::wire::WireLife;
use cmd_wire::cells::Words;
use cmd_wire::frame::{SessionDot, SessionFace};
use cmd_wire::Machine;
use serde::{Deserialize, Serialize};

use super::history_annotations::{self as notes, AnnotationErr, Book, GroupRec, Named, Table};
use super::history_browse::Read;
use crate::dial::remote_ask::{Remote, Said};
use crate::wire::typed::{codes, Command};
use crate::wire::SessionFaceExt as _;

// ───────────────────────── 这台那一半：groups-here ─────────────────────────

/// 这一片里的一组：id · 名字 · 几时建的 · 这个名字几时起的（毫秒）。进组时整份抄进组员那一台。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HereGroup {
    pub(crate) id: String,
    pub(crate) name: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) made: i64,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) named: i64,
}

/// 复活一个标签要的那几格（重启后那条会话没有活进程，照它造灰标签）：这台照自己那份清单那一行给。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct MemberFacts {
    /// 显示的标题（改过的 ＞ 标题 ＞ 第一句 ＞ 清单那一句「没名字」）。
    #[cfg_attr(test, ts(type = "string"))]
    pub(crate) title: Words,
    /// 项目目录（清单里没有那一条 ⇒ 缺）。
    pub(crate) cwd: Option<String>,
    /// 记录文件（resume 照它；清单里没有 ⇒ 缺）。
    pub(crate) record: Option<String>,
    /// 上次跑在哪个号下（没记 ⇒ 缺）。
    pub(crate) account: Option<String>,
    /// 后台会话。
    pub(crate) bg: bool,
    /// 最后活动（毫秒；清单里没有 ⇒ 缺）。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) at: Option<i64>,
}

/// 这台一条带组或固定的会话：在哪几组 · 固没固定 · 这台此刻给它画的样子 · 复活那几格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HereMember {
    pub(crate) sid: String,
    pub(crate) groups: Vec<String>,
    pub(crate) pinned: bool,
    pub(crate) face: SessionFace,
    pub(crate) facts: MemberFacts,
}

/// `groups-here` 的回包：这台那一片。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct GroupsHere {
    pub(crate) groups: Vec<HereGroup>,
    pub(crate) members: Vec<HereMember>,
}

/// 一条组员此刻的样子 ＋ 复活那几格（这台判的）。
pub(crate) type Seen = (SessionFace, MemberFacts);

/// 这台那一半（注解 · 组本 · 每条的样子与那几格由调用方给）：组照 id 排；会话只带有组或固定的，照 sid 排。
pub(crate) fn here_of(table: &Table, book: &Book, seen: impl Fn(&str) -> Seen) -> GroupsHere {
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
pub(crate) fn unlisted(entry: Option<&cmd_wire::listing::Entry>, life: WireLife) -> Seen {
    let label = super::search_rules::session_label(&super::search_rules::LabelFacts {
        custom: entry.and_then(|e| e.custom_title.clone()),
        bg_name: None,
        title: None,
        first: String::new(),
        project_dir: None,
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
pub(crate) fn mark(table: &Table, book: &Book) -> u64 {
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
    pub(crate) enum GroupsHereErr {
        /// 推不出家 · 注解文件读不懂。
        Failed = "failed",
    }
}

/// `groups-here`：这台那一半（别的后端经 `--resident-call` 问它，本机后端的 `groups-read` 同进程问它）。
pub(crate) struct GroupsHereCmd;

impl Command for GroupsHereCmd {
    const NAME: &'static str = "groups-here";
    type Req = crate::wire::typed::NoArgs;
    type Reply = GroupsHere;
    type Err = GroupsHereErr;
}

impl crate::wire::typed::Handle for GroupsHereCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<GroupsHere, GroupsHereErr>> + Send + 'static {
        async move { here_now().map_err(GroupsHereErr::Failed) }
    }
}

// ───────────────────────── 几台并成一份：groups-read ─────────────────────────

/// `groups-read` 的入参：问哪几台（本机逐字 `"<local>"`）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct GroupsReadReq {
    #[cfg_attr(test, ts(as = "Vec<String>"))]
    pub(crate) machines: Vec<Machine>,
}

/// 一组的名字此刻从哪来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "is", rename_all = "snake_case")]
pub(crate) enum GroupKnown {
    /// 名字取自这一问答了的那一片。
    Now,
    /// 名字取自那台上一次答的那一片（那台这一问没答；`at` ＝ 那一份几时读到的，写好的字）。
    Before {
        #[cfg_attr(test, ts(type = "string"))]
        at: Words,
    },
    /// 哪一片都没有这一组的记录（只有组员指着它）：名字缺。
    Unknown,
}

/// 组里一条会话：哪台 · 哪条 · 固没固定 · 那张脸（那台这一问没答 ⇒ 看不见的那一张）· 复活那几格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct GroupMember {
    #[cfg_attr(test, ts(as = "String"))]
    pub(crate) origin: Machine,
    pub(crate) sid: String,
    pub(crate) pinned: bool,
    pub(crate) face: SessionFace,
    pub(crate) facts: MemberFacts,
}

/// 组头那一颗点：几条 ＋ 那颗点的名字（照组员那张脸上的字）。只出有的那几样，序：需手动 · 在跑 · 后台。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct GroupCount {
    pub(crate) dot: SessionDot,
    pub(crate) n: u32,
    #[cfg_attr(test, ts(type = "string"))]
    pub(crate) word: Words,
    /// 组头那一句（「需手动 2」那样，出口照摆）。
    #[cfg_attr(test, ts(type = "string"))]
    pub(crate) text: Words,
}

/// 并好的一组。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct GroupRead {
    pub(crate) id: String,
    /// 名字：各片里 `named` 最新的那一份（哪一片都没有记录 ⇒ 缺）。
    pub(crate) name: Option<String>,
    /// 叫它什么（出口照摆）：有名字 ⇒ 名字；没有 ⇒ 照实说不知道。
    #[cfg_attr(test, ts(type = "string"))]
    pub(crate) title: Words,
    /// 各片里最早的那个建组时刻（没有记录 ⇒ 缺）。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) made: Option<i64>,
    /// 取到的那个名字几时起的（没有记录 ⇒ 缺）：进组时同名字一起抄进组员那一台。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) named: Option<i64>,
    pub(crate) known: GroupKnown,
    /// 组员：照问的台序、再照 sid。
    pub(crate) members: Vec<GroupMember>,
    pub(crate) counts: Vec<GroupCount>,
}

/// 一台这一问的结局（同历史页那一列那一格：答了 · 上次所知 · 读不到）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct GroupsMachine {
    #[cfg_attr(test, ts(as = "String"))]
    pub(crate) origin: Machine,
    pub(crate) read: Read,
}

/// `groups-read` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct GroupsRead {
    /// 各组：照建的先后、再照 id。
    pub(crate) groups: Vec<GroupRead>,
    /// 各台固定的会话（不论在不在组里），同组员那一形。
    pub(crate) pinned: Vec<GroupMember>,
    /// 问的每一台这一问的结局（照问的先后）。
    pub(crate) machines: Vec<GroupsMachine>,
}

codes! {
    /// `groups-read` 自己没有别的码（哪台没问到只进回包的 `machines`）。
    pub(crate) enum GroupsReadErr {}
}

/// 各台上一次答的那一半 ＋ 那一刻（毫秒）：缓存，只在本进程。
#[derive(Default)]
pub(crate) struct Kept(Mutex<BTreeMap<Machine, (GroupsHere, i64)>>);

impl Kept {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<Machine, (GroupsHere, i64)>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

static KEPT: std::sync::LazyLock<Kept> = std::sync::LazyLock::new(Kept::default);

/// `groups-read`：本机后端问到各台的那一半（本机同进程、别的台经 `--resident-call` 问它的常驻 `groups-here`），并成一份再答。
pub(crate) struct GroupsReadCmd;

impl Command for GroupsReadCmd {
    const NAME: &'static str = "groups-read";
    type Req = GroupsReadReq;
    type Reply = GroupsRead;
    type Err = GroupsReadErr;
}

impl crate::wire::typed::Handle for GroupsReadCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: GroupsReadReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<GroupsRead, GroupsReadErr>> + Send + 'static {
        async move {
            let local = async {
                tokio::task::spawn_blocking(here_now)
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
                    .map_err(|message| Said {
                        code: None,
                        message,
                    })
            };
            read_with(
                &req.machines,
                local,
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
                &KEPT,
                crate::common::time::now_ms(),
            )
            .await
        }
    }
}

/// 只问这台的 `groups-read`（一次性进程里问不到别的台时用：agent 自问那一条的退路）。
pub(crate) async fn read_here(tz: crate::common::time::Tz) -> Result<GroupsRead, String> {
    let local = async {
        tokio::task::spawn_blocking(here_now)
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r)
            .map_err(|message| Said {
                code: None,
                message,
            })
    };
    read_with(
        &[Machine::named(cmd_wire::LOCAL)],
        local,
        &tz,
        None,
        &crate::dial::remote_ask::REACH,
        &crate::dial::remote_ask::DialRemote,
        &KEPT,
        crate::common::time::now_ms(),
    )
    .await
    .map_err(|e| format!("{e:?}"))
}

/// 一台这一问拿到的那一片：这一问答的 · 上一次答的（带那一份几时读到的，写好的字）· 没有。
pub(crate) enum Half {
    Now(GroupsHere),
    Before(GroupsHere, Words),
    None,
}

/// [`GroupsReadCmd`] 的本体（本机那一台怎么答 · 可达表 · 对面 · 缓存 · 此刻由调用方给）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn read_with(
    machines: &[Machine],
    local: impl std::future::Future<Output = Result<GroupsHere, Said>> + Send,
    tz: &crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &crate::dial::remote_ask::Table,
    remote: &dyn Remote,
    kept: &Kept,
    now_ms: i64,
) -> Result<GroupsRead, GroupsReadErr> {
    let got = crate::dial::remote_ask::ask_each::<GroupsHereCmd, _>(
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
    let mut halves: Vec<(Machine, Half)> = Vec::new();
    let mut heads = Vec::new();
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
                        (
                            Half::Before(h.clone(), at.clone()),
                            Read::Stale { why: s.message, at },
                        )
                    }
                    None => (Half::None, Read::Failed { why: s.message }),
                },
            };
            heads.push(GroupsMachine {
                origin: origin.clone(),
                read,
            });
            halves.push((origin, half));
        }
    }
    Ok(merge(&halves, heads))
}

/// 合：各台那一片（照问的先后）＋ 这一问各台的结局 ⇒ 并好的一份。判定 · 计数只在这里；纯函数（不碰盘、不问网），
/// 手机核心照 A1-8-4 挪走时整个搬过去，本机后端与手机核心调同一份。
/// - 名字：各片里同一组 `named` 最新的那份；同一刻 ⇒ 这一问答了的那片优先，再照问的先后。`made` 取最早的。
/// - 哪一片都没有那一组的记录、只有组员指着它 ⇒ 照出，名字缺。
pub(crate) fn merge(halves: &[(Machine, Half)], machines: Vec<GroupsMachine>) -> GroupsRead {
    // 组 id ⇒ (取到名字的那一份, 名字从哪来, 各片里最早的建组时刻)
    let mut records: BTreeMap<&str, (&HereGroup, GroupKnown, i64)> = BTreeMap::new();
    for (_, half) in halves {
        let (here, known) = match half {
            Half::Now(h) => (h, GroupKnown::Now),
            Half::Before(h, at) => (h, GroupKnown::Before { at: at.clone() }),
            Half::None => continue,
        };
        for g in &here.groups {
            let fresh = known == GroupKnown::Now;
            match records.get_mut(g.id.as_str()) {
                None => {
                    records.insert(g.id.as_str(), (g, known.clone(), g.made));
                }
                Some((best, k, made)) => {
                    *made = (*made).min(g.made);
                    let best_fresh = *k == GroupKnown::Now;
                    if (g.named, fresh) > (best.named, best_fresh) {
                        *best = g;
                        *k = known.clone();
                    }
                }
            }
        }
    }
    let mut members: BTreeMap<&str, Vec<GroupMember>> = BTreeMap::new();
    let mut pinned = Vec::new();
    for (origin, half) in halves {
        let (here, seen) = match half {
            Half::Now(h) => (h, true),
            Half::Before(h, _) => (h, false),
            Half::None => continue,
        };
        for m in &here.members {
            // 这一问答了 ⇒ 那台画的样子；上次所知 ⇒ 看不见的那一张（不冒充在跑）。
            let face = if seen {
                m.face.clone()
            } else {
                resident_client::book::unseen(Words(comms_inward::chan::said::offline(
                    &origin.to_string(),
                )))
            };
            let row = GroupMember {
                origin: origin.clone(),
                sid: m.sid.clone(),
                pinned: m.pinned,
                face,
                facts: m.facts.clone(),
            };
            if m.pinned {
                pinned.push(row.clone());
            }
            for g in &m.groups {
                members.entry(g.as_str()).or_default().push(row.clone());
            }
        }
    }
    let mut ids: Vec<&str> = records.keys().copied().collect();
    ids.extend(
        members
            .keys()
            .copied()
            .filter(|id| !records.contains_key(id)),
    );
    let mut groups: Vec<GroupRead> = ids
        .into_iter()
        .map(|id| {
            let rows = members.remove(id).unwrap_or_default();
            let counts = counts_of(&rows);
            match records.get(id) {
                Some((g, known, made)) => GroupRead {
                    id: id.to_string(),
                    name: Some(g.name.clone()),
                    title: Words(g.name.clone()),
                    made: Some(*made),
                    named: Some(g.named),
                    known: known.clone(),
                    members: rows,
                    counts,
                },
                None => GroupRead {
                    id: id.to_string(),
                    name: None,
                    title: Words(copy_core::copy_text("beGroups.name.unknown", &[])),
                    made: None,
                    named: None,
                    known: GroupKnown::Unknown,
                    members: rows,
                    counts,
                },
            }
        })
        .collect();
    groups.sort_by(|a, b| {
        (a.made.unwrap_or(i64::MAX), &a.id).cmp(&(b.made.unwrap_or(i64::MAX), &b.id))
    });
    GroupsRead {
        groups,
        pinned,
        machines,
    }
}

/// 组头那几颗点：需手动 · 在跑 · 后台各几条，名字照第一条那张脸上的字。
fn counts_of(rows: &[GroupMember]) -> Vec<GroupCount> {
    [
        SessionDot::NeedsYou,
        SessionDot::Running,
        SessionDot::Background,
    ]
    .into_iter()
    .filter_map(|dot| {
        let mut hit = rows.iter().filter(|r| r.face.dot == dot);
        let first = hit.next()?;
        let n = u32::try_from(1 + hit.count()).unwrap_or(u32::MAX);
        let n_text = n.to_string();
        let n_text = n_text.as_str();
        let text = match dot {
            SessionDot::NeedsYou => copy_core::copy_text("beGroups.count.needs", &[("n", n_text)]),
            SessionDot::Running => copy_core::copy_text("beGroups.count.running", &[("n", n_text)]),
            _ => copy_core::copy_text("beGroups.count.background", &[("n", n_text)]),
        };
        Some(GroupCount {
            dot,
            n,
            word: first.face.name.clone(),
            text: Words(text),
        })
    })
    .collect()
}

/// 格目录的样本（`faces/cells_catalog.rs`）：真合（[`merge`]）出来的一份 —— 名字取自这一问答的那片 · 上次那片各一组，
/// 各组组头都有计数；那台这一问的结局四种各一台。可缺的格都填上、列表都不空。
pub(crate) fn specimen() -> GroupsRead {
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
            (Machine::named("b"), Half::Before(before, w("t"))),
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum GroupsWriteReq {
    Make {
        name: String,
    },
    Move {
        sid: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        join: Option<HereGroup>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        leave: Option<String>,
    },
    Rename {
        id: String,
        name: String,
        #[cfg_attr(test, ts(as = "Vec<String>"))]
        machines: Vec<Machine>,
    },
    Dissolve {
        id: String,
        #[cfg_attr(test, ts(as = "Vec<String>"))]
        machines: Vec<Machine>,
    },
}

/// 一台没送到：哪台 · 为什么（那台这一问说的那一句）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct GroupMissed {
    #[cfg_attr(test, ts(as = "String"))]
    pub(crate) origin: Machine,
    pub(crate) why: String,
}

/// `groups-write` 的回包：起好的那一组 · 那条会话此刻在哪几组 · 发给各片的结局（没送到的那几台，照问的先后）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "is", rename_all = "snake_case")]
pub(crate) enum GroupsWritten {
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
    pub(crate) enum GroupsWriteErr {
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
pub(crate) fn write_at(
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
#[allow(clippy::too_many_arguments)]
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
            let f = e.into_said();
            Said {
                code: Some(f.code),
                message: f.message,
            }
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
                why: s.message,
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
pub(crate) struct GroupsWriteCmd;

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
            send_with(
                req,
                |r| write_at(&own, r, crate::common::time::now_ms()),
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/history/groups_tests.rs"]
mod tests;
