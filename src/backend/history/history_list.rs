//! **历史页的平铺会话清单**（帧面 `history-list`）—— 跨项目、按机器问，一次出成品：
//! 每行带显示标题 · 项目与真实目录 · 那一家 · 状态 · 注解 · 分叉 · 分身 · 条数 · 时间 ·「这一行能做什么」，
//! 外加按项目看时的分组（按真实目录分，读不了的那一组照样出、带原因）。
//!
//! # 形状
//!
//! ```text
//! history-list {origin, sid?, query?, sort?, within_days?, hidden?, held?, range?}   （入参 · 回包 · 码都是类型）
//!   origin `"<local>"` ⇒ 这台；别的名字 = 那一台（空白名 ⇒ `bad_args`）：那台的清单（那台自己判活、自己读上次的号、并好它自己的注解）由本进程问那台的常驻取、记着副本
//!                  （`remote_listing.rs`：带记号问 `history-listing`，只拿变了的那几行）；手上有就用手上那份，没有才去问
//!                  （历史页开页 · 刷新时同步副本归几台并成一列那一问，`history_browse.rs`）；那台常驻不在 ⇒ `resident_absent`（不在那台起一个冷进程顶上）
//!   ⇒ 并上那台的注解（星标 · 改名 · 隐藏）⇒ 筛（隐藏 · 时间 · 搜索词）⇒ 补上被筛掉的分叉父会话（`context`）⇒ 排 ⇒ 截
//!   sid: 只要那一个会话那一行（独立查看窗按会话 ID 开任意一个会话，含已结束的、隐藏的）：别的筛一概不看、不补父会话
//!   ⇒ 带键集合那一形（`common/views.rs`，键 ＝ 会话 ID）：`range` 那一窗（缺 ⇒ 头 2000 行）；头 ＝ {groups, notice} ＋ 总数；
//!     带着上次的 `held` ⇒ 没变 `same` · 只给变了的那几行 `up`（先后变了才带 `order`）；不带 · 对不上 ⇒ `whole`
//! ```
//!
//! 判定全在这里：搜什么（显示标题 · 第一句 · 项目名）· 按什么排（`at`）· 每行能不能恢复 / 分叉 / 删、不能的为什么。
//! 历史页要的是几台并成一列的那一份（`history_browse.rs`，照这里每台的成品并）；这里答一台的（手机 · 独立查看窗按会话 ID）。
//! 「不知道」不装成已知：判不了活（合成历史没有 pidfile）⇒ `isLive: null`、那一组 `hasLive: null`；
//! 注解读不到 / 读不懂 ⇒ 行照出（星标 · 隐藏按没有算），`notice` 说一句为什么。

pub(crate) use cmd_wire::listing::{
    FailedDir, HistoryGroup, HistoryHead, HistoryListReq, HistoryRow, HistorySort as Sort, Listing,
    RawRow,
};
use cmd_wire::sync::{Head, Held, Patch, Range};
use std::collections::{BTreeMap, BTreeSet};

use crate::history::listing_scan::SessionRow;
use copy_core::copy_text;
use serde_json::Value;

use crate::common::cells::Words;
use crate::common::said::Said;
use crate::common::views::{ViewSource, Views};
use crate::history::history_annotations::{Loaded, Table};
use crate::history::listing::can_of;
use crate::wire::SessionFaceExt as _;

/// 不带 `range` 时一窗回几行：排好的头这些行（按时间看时超过它只列最近这些、底部「更早的用搜索找」）。
const DEFAULT_LIMIT: u32 = 2000;
const DEFAULT_WINDOW: Range = Range {
    from: 0,
    limit: DEFAULT_LIMIT,
};

/// 每种问法一份备忘（`common/views.rs`）：浏览那几问（不带搜索词 · 不按会话 ID）8 份；带搜索词 · 按会话 ID 的另 4 份，敲字不挤掉浏览的。
static BROWSE: std::sync::LazyLock<Views> = std::sync::LazyLock::new(|| Views::new(8));
static NARROW: std::sync::LazyLock<Views> = std::sync::LazyLock::new(|| Views::new(4));

// ───────────────────────── 这台自己的清单（`history-listing` 答的就是它）─────────────────────────

/// 注解这一维：读到了 ⇒ 那张表；读不到 ⇒ 为什么（给 `notice`）。
pub(crate) fn annotations(loaded: &Loaded) -> Result<&Table, String> {
    match loaded {
        Loaded::Read(t) => Ok(t),
        Loaded::NoPath => Err(copy_text("beHistoryJoin.annotations.unknown", &[])),
        Loaded::Unreadable(why) => Err(copy_text(
            "beHistoryJoin.annotations.unparsable",
            &[("why", &why.to_string())],
        )),
    }
}

/// **这台的注解并进它自己那份清单**：读到了 ⇒ `annotations` ＝ 那张表（sid ⇒ 注解）；读不到 ⇒
/// `annotationsNotice` 一句为什么。注解跟着会话住在那台：每台只并自己那份，本机后端代问远端清单时照那台带来的出、不拿本机那份去盖。
pub(crate) fn with_own_annotations(listing: &mut Listing, loaded: &Loaded) {
    match annotations(loaded) {
        Ok(t) => listing.annotations = Some(t.clone()),
        Err(why) => listing.annotations_notice = Some(why),
    }
}

/// 一份清单带来的那台的注解（[`with_own_annotations`] 那两格）；两格都没有（那台没并）⇒ 空表。
pub(crate) fn listing_annotations(listing: &Listing) -> Result<Table, String> {
    if let Some(why) = &listing.annotations_notice {
        return Err(why.clone());
    }
    Ok(listing.annotations.clone().unwrap_or_default())
}

/// 记录树里的一行（会话清单那一形）＋ 判活 ⇒ 清单的一行（注解格先按没有填，并注解在 [`answer_from`]）。
/// 没有 sid ⇒ `None`。
fn session_row(
    r: &SessionRow,
    project_dir: &str,
    live: &crate::history::live_table::LiveTable,
) -> Option<RawRow> {
    if r.session_id.is_empty() {
        return None;
    }
    let sid = r.session_id.as_str();
    let cwd = r.cwd.as_deref().unwrap_or_default();
    // 哪一家跟着这一行走：照它的记录文件落在哪棵树下认（`agents::record_kind_of`）。认不出 ⇒ 行照出、家那一格缺
    // （小牌说认不出是哪一家的，恢复那一项是 `no`）。
    let SessionRow { jsonl_path, .. } = r;
    let record = std::path::Path::new(jsonl_path);
    let agent = crate::agents::record_kind_of(record);
    // 分叉关系：两格都有才带。
    let fork = r
        .forked_from_session_id
        .clone()
        .zip(r.forked_from_message_uuid.clone());
    Some(RawRow {
        group: group_key(agent, cwd),
        agent: agent.map(Into::into),
        session_id: sid.to_string(),
        project_path: cwd.to_string(),
        project_name: plain_path_core::last_segment(cwd)
            .unwrap_or(project_dir)
            .to_string(),
        title: label_of(None, r.ai_title.as_deref(), &r.first_user_excerpt, cwd),
        ai_title: r.ai_title.clone(),
        first_user_excerpt: r.first_user_excerpt.clone(),
        started_at: r.started_at_ms,
        updated_at: r.updated_at_ms,
        jsonl_path: jsonl_path.clone(),
        life: live.life_of(sid),
        housed: None,
        asks: false,
        message_count_approx: r.message_count_approx,
        is_bg: r.is_bg,
        starred: false,
        custom_title: None,
        launch_tab: None,
        parent: None,
        forked_from_session_id: fork.map(|f| f.0),
        project_dir: project_dir.to_string(),
        last_account: None,
    })
}

/// 合成历史的一条会话 ⇒ 清单的一行（判不了活 ⇒ `isLive: null`；工作目录取不出最后一段 ⇒ 项目名照实说目录不明，不画 kind）。
fn synth_row(kind: &str, s: &crate::agents::SynthSession, excerpt: String) -> RawRow {
    let name = plain_path_core::last_segment(&s.cwd).map_or_else(
        || copy_core::copy_text("history.row.noDir", &[]),
        str::to_string,
    );
    let title = label_of(None, None, &excerpt, &s.cwd);
    RawRow {
        agent: Some(kind.into()),
        session_id: s.sid.clone(),
        project_path: s.cwd.clone(),
        project_name: name,
        ai_title: None,
        first_user_excerpt: excerpt,
        title,
        started_at: s.mtime_ms,
        updated_at: s.mtime_ms,
        jsonl_path: s.path.to_string_lossy().into_owned(),
        life: crate::wire::WireLife::Unclear,
        housed: None,
        asks: false,
        message_count_approx: 0,
        is_bg: false,
        starred: false,
        custom_title: None,
        launch_tab: None,
        parent: None,
        forked_from_session_id: None,
        project_dir: format!("{kind}:{}", s.cwd),
        group: group_key(Some(kind), &s.cwd),
        last_account: None,
    }
}

/// 叫这一行什么（[`crate::history::search_rules::session_label`]，与标签页 · 通知同一个）。
fn label_of(custom: Option<&str>, title: Option<&str>, first: &str, project: &str) -> String {
    let facts = crate::history::search_rules::LabelFacts {
        custom: custom.map(str::to_string),
        title: title.map(str::to_string),
        first: first.to_string(),
        project_dir: (!project.is_empty()).then(|| project.to_string()),
        ..Default::default()
    };
    crate::history::search_rules::session_label(&facts).0
}

/// 一行的分组键：哪一家 ＋ 真实目录（记录目录名会撞）；认不出是哪一家的那几行同目录归一组（键里那一家那一截空着，键只是键）。
fn group_key(agent: Option<&str>, project_path: &str) -> String {
    format!("{}:{project_path}", agent.unwrap_or_default())
}

/// 这台的平铺清单（并上这台的注解）：记录树每个目录 ＋ 合成历史各家；判活 ＝ 这台 pidfile；上次的号 ＝ 这台的起会话账号记录。
/// `last` 读不懂 ⇒ 那一格都不带（界面不说「上次用的」，照样能恢复）。
pub fn machine_listing() -> Result<Listing, Said> {
    machine_listing_in(&crate::host::Housing::here())
}

/// [`machine_listing`]，活会话住哪问调用方那一份（同一趟里别处也要问住哪的，共用一份、名单只列一次）。
pub fn machine_listing_in(housing: &crate::host::Housing) -> Result<Listing, Said> {
    machine_listing_at(None, crate::history::list_node::node(), housing)
}

/// [`machine_listing`]；`only` ＝ 按 sid 问的那一条：记录树里只整份扫叫这个名字的那一份（别的会话只出分组要的几格，
/// [`answer_from`] 按 sid 滤掉）⇒ 答案与整台扫逐字相同，不必把整台每份会话从头扫一遍。
/// `node` ＝ 这台会话清单的节点（生产 ＝ 本进程那一份）：读之前放开。
fn machine_listing_for(
    only: Option<&str>,
    node: &crate::history::list_node::ListNode,
) -> Result<Listing, Said> {
    machine_listing_at(only, node, &crate::host::Housing::here())
}

/// [`machine_listing_for`] 的本体，活会话住哪问 `housing`。
fn machine_listing_at(
    only: Option<&str>,
    node: &crate::history::list_node::ListNode,
    housing: &crate::host::Housing,
) -> Result<Listing, Said> {
    // 读之前放开清单的节点：此后再变的会话再响一次（这一份里没赶上的由下一帧叫回来重问）。
    node.asked();
    let homes = crate::agents::local_homes();
    let live = crate::history::live_table::now(&homes);
    let tree = tree_for(&homes, only).map_err(|e| Said::new("failed", e))?;
    let synth: Vec<Synth> = crate::agents::history_faces()
        .into_iter()
        .map(|(k, f)| {
            (
                k,
                (f.sessions)(&crate::agents::homes_of(k)),
                f.excerpt,
                crate::agents::run_faces(k),
            )
        })
        .collect();
    let last = super::launch_book::last_accounts_at(crate::platform::paths::data_home().as_deref())
        .ok()
        .and_then(|l| serde_json::to_value(l.accounts).ok())
        .unwrap_or(Value::Null);
    let mut listing = listing_from(tree.unwrap_or_default(), &synth, &live, &last);
    with_own_housing(&mut listing, housing);
    with_own_asks(&mut listing, |sid| {
        crate::history::needs_changed::pending_of(sid)
            .into_iter()
            .next()
    });
    with_own_annotations(&mut listing, &crate::history::history_annotations::load());
    with_own_rules(
        &mut listing,
        &crate::history::session_hidden::Here::read_now(),
    );
    Ok(listing)
}

/// 记录树分好的目录：每个目录一项（目录名 · 那一目录的会话行，读不了 ⇒ [`Unread`]）。
pub(crate) type Tree = crate::history::listing::DirSessions;

pub(crate) use crate::history::listing::Unread;

/// 一家的合成历史：那一家叫什么 · 它的会话 · 第一句怎么取 · 它的运行面（哪一份是子运行的，[`crate::agents::RunFaces::owner`]）。
pub(crate) type Synth = (
    &'static str,
    Vec<crate::agents::SynthSession>,
    fn(&std::path::Path) -> String,
    crate::agents::RunFaces,
);

/// 记录树分好的目录（[`crate::history::listing::sessions_by_dir_among`]：几家一趟扫，读不了的目录带上扫它的那一家）；
/// 按 sid 问的那一条是分叉来的 ⇒ 父会话那一份也整份扫（那一行要父会话的显示标题，与整台扫那一份同一个字）。
/// 没有一家的记录树在 ⇒ `Ok(None)`。
pub(crate) fn tree_for(
    home: impl crate::agents::IntoHomes,
    only: Option<&str>,
) -> Result<Option<Tree>, String> {
    let home_v = crate::agents::IntoHomes::into_homes(home);
    tree_for_among(crate::agents::REGISTRY, &home_v, only)
}

/// [`tree_for`] 的可喂注册表那一半。
pub(crate) fn tree_for_among(
    registry: &[crate::agents::Adapter],
    homes: &[crate::agents::AgentHome],
    only: Option<&str>,
) -> Result<Option<Tree>, String> {
    use crate::history::listing::sessions_by_dir_among;
    let Some(mut tree) = sessions_by_dir_among(registry, homes, only)? else {
        return Ok(None);
    };
    let parent = only.and_then(|sid| {
        tree.iter()
            .filter_map(|(_, r)| r.as_ref().ok())
            .flatten()
            .find(|r| r.session_id == sid)
            .and_then(|r| r.forked_from_session_id.as_deref())
            .filter(|p| *p != sid)
            .map(str::to_string)
    });
    let Some(p) = parent else {
        return Ok(Some(tree));
    };
    // 父会话那一行整份扫的那一份：照「目录名 ＋ sid」配（sid 只住一家的一个目录），同名目录几家都有也不串。
    let full = sessions_by_dir_among(registry, homes, Some(&p))?.unwrap_or_default();
    let whole: Vec<(&str, &SessionRow)> = full
        .iter()
        .filter_map(|(d, r)| r.as_ref().ok().map(|rows| (d.as_str(), rows)))
        .flat_map(|(d, rows)| {
            rows.iter()
                .filter(|w| w.session_id == p)
                .map(move |w| (d, w))
        })
        .collect();
    for (dir, rows) in &mut tree {
        let Ok(rows) = rows else { continue };
        for v in rows.iter_mut().filter(|r| r.session_id == p) {
            if let Some((_, w)) = whole.iter().find(|(d, _)| d == dir) {
                *v = (*w).clone();
            }
        }
    }
    Ok(Some(tree))
}

/// **这台隐藏要的那几样并进它自己那份清单**（同注解）：每行起会话时的 `--tab` · 主会话，整份带这台偏好那两格。
/// 单个那一格就是注解（[`with_own_annotations`]）；判定在 [`answer_from`]（`history/session_hidden.rs`）。
pub(crate) fn with_own_rules(listing: &mut Listing, here: &crate::history::session_hidden::Here) {
    for r in &mut listing.rows {
        r.launch_tab = here.ledger.tabs.get(&r.session_id).cloned();
        r.parent = here.parent_of(&r.session_id).map(str::to_string);
    }
    listing.rules = here.rules;
}

/// **这台判的宿主并进它自己那份清单**（同注解）：活着的那几行住在哪（[`crate::host::Housing::live`]，一份清单至多列一次名单）；
/// 看的那一台照这一格造脸（[`finish_row`]），不另判。
pub(crate) fn with_own_housing(listing: &mut Listing, housing: &crate::host::Housing) {
    for r in &mut listing.rows {
        r.housed = housing.live(&r.session_id, &r.life);
    }
}

/// [`machine_listing`] 的可喂夹具那一半（记录树分好的目录 · 合成历史 · 判活 · 上次的号由调用方给；不并注解）。
pub(crate) fn listing_from(
    tree: Tree,
    synth: &[Synth],
    live: &crate::history::live_table::LiveTable,
    last: &Value,
) -> Listing {
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    for (dir, got) in tree {
        match got {
            Ok(list) => rows.extend(list.iter().filter_map(|v| session_row(v, &dir, live))),
            Err(Unread { kind, why }) => failed.push(FailedDir {
                agent: kind.into(),
                project_dir: dir,
                error: why,
            }),
        }
    }
    for (kind, sessions, excerpt, runs) in synth {
        // 那一家答得出父的那几份是子运行的记录（[`crate::agents::RunFaces::owner`]）：不是会话，不进清单；父已不在的孤儿那一家答不出父 ⇒ 照独立会话列出。
        for s in sessions.iter().filter(|s| runs.owner(&s.path).is_none()) {
            rows.push(synth_row(
                kind,
                s,
                crate::history::search_rules::excerpt_of(&excerpt(&s.path)),
            ));
        }
    }
    for r in &mut rows {
        r.last_account = last
            .get(&r.session_id)
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    Listing {
        rows,
        failed,
        annotations: None,
        annotations_notice: None,
        rules: Default::default(),
    }
}

// ───────────────────────── 成品 ─────────────────────────

// `history-list` 的入参（`HistoryListReq` · 排法 `Sort`）住 `cmd-wire::listing`（名字与回包挂在 `cmd_wire::Call` 上）。

crate::wire::typed::codes! {
    /// `history-list` 的码。
    pub enum HistoryListErr {
        /// 读不了这台的记录树 · 读的那一趟没做完。
        Failed = "failed",
        /// 问的是那一台、可达表里没有它（还没连上过）· 拨号 / 链路不通。
        Unreachable = "unreachable",
        /// 问的是那一台、那台的常驻后端没在跑（不在那台起一个冷进程顶上）。
        ResidentAbsent = "resident_absent",
    }
}

/// `history-list`：历史页的平铺会话清单。扫记录树在阻塞池里做，并 · 筛 · 排是纯计算 ⇒ 异步档。
pub struct HistoryListCmd;

impl crate::wire::typed::Command for HistoryListCmd {
    crate::wire::typed::by_call!(HistoryListReq);
    type Err = HistoryListErr;
}

impl crate::wire::typed::Handle for HistoryListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: HistoryListReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<Patch<HistoryHead, HistoryRow>, HistoryListErr> {
        answer(req, cx.tz, cx.until)
            .await
            .map_err(crate::wire::typed::of_fail_with(HistoryListErr::Failed))
    }
}

/// 筛排那几格（入参解好、值域核过）。
#[derive(Debug)]
pub(crate) struct Ask {
    /// 只要这一个会话（别的筛不看）。
    pub sid: Option<String>,
    pub query: Option<String>,
    pub by_created: bool,
    pub within_ms: Option<i64>,
    pub hidden: bool,
    /// 答哪一窗（不进问法的键：几只手各看各的窗共用一份备忘）。
    pub range: Range,
}

fn bad(what: &str) -> Said {
    Said::new("bad_args", crate::common::contract::malformed(what))
}

/// 入参 ⇒ [`Ask`]：值域不对 ⇒ `bad_args`（先于任何 IO）。
pub(crate) fn parse_ask(req: &HistoryListReq) -> Result<Ask, Said> {
    // 会话 ID 的形状与 `history-record` 同一条（`[A-Za-z0-9-]`，1..=64）。
    let sid = match req.sid.as_deref() {
        None => None,
        Some(s)
            if (1..=64).contains(&s.len())
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') =>
        {
            Some(s.to_string())
        }
        Some(_) => return Err(bad("`sid` must be a session id ([A-Za-z0-9-], 1..=64)")),
    };
    let range = match req.range {
        None => DEFAULT_WINDOW,
        Some(
            r @ Range {
                limit: 1..=20_000, ..
            },
        ) => r,
        Some(_) => return Err(bad("`range.limit` must be 1..=20000")),
    };
    Ok(Ask {
        sid,
        range,
        ..filters(req.query.as_deref(), req.sort, req.within_days, req.hidden)?
    })
}

/// 筛排那几格（搜索词 · 排法 · 时间窗 · 隐藏的出不出）⇒ [`Ask`]（不按会话 ID、窗口取缺省）：值域不对 ⇒ `bad_args`。
/// `history-list` 与几台并成一列那一问（`history_browse.rs`）共用。
pub(crate) fn filters(
    query: Option<&str>,
    sort: Option<Sort>,
    within_days: Option<u64>,
    hidden: Option<bool>,
) -> Result<Ask, Said> {
    let query = query
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let within_ms = match within_days {
        None => None,
        Some(d @ 1..=3650) => Some(d as i64 * 86_400_000),
        Some(_) => return Err(bad("`within_days` must be 1..=3650")),
    };
    Ok(Ask {
        sid: None,
        query,
        by_created: sort == Some(Sort::Created),
        within_ms,
        hidden: hidden == Some(true),
        range: DEFAULT_WINDOW,
    })
}

/// 行上那一家的小牌（对用户的叫法）：起会话默认的那一家不画 ⇒ `None`；认不出是哪一家的（`kind` 缺）⇒ 照实说那一句。
fn agent_tag(kind: Option<&str>) -> Option<String> {
    agent_tag_among(crate::agents::REGISTRY, kind)
}

/// [`agent_tag`] 照一份注册表判。「默认那一家不画」判在 [`crate::agents::unless_default`]；这里交的默认是起会话默认的那一家
/// （[`crate::agents::default_launch_among`]，不看资产面；资产那几问交的是资产默认，见那一处）。
fn agent_tag_among(registry: &[crate::agents::Adapter], kind: Option<&str>) -> Option<String> {
    let Some(kind) = kind else {
        return Some(copy_core::copy_text("rsAgents.family.unknown", &[]));
    };
    let default = crate::agents::default_launch_among(registry).map(|(k, _)| k);
    crate::agents::unless_default(kind, default)
        .map(|k| crate::agents::family_said_among(registry, k))
}

/// 一行并上注解、判好的几格（排序键与时刻字在 [`answer_from`] 定了排法之后才写）。
struct Finished {
    raw: RawRow,
    label: String,
    untitled: bool,
    face: crate::wire::SessionFace,
    /// 单个那一格（注解）：隐藏的判定在所有行都叫得出名字之后（[`answer_from`]），主会话的名字从同一份清单里取。
    hide: Option<crate::history::history_annotations::Hide>,
    hidden: Option<cmd_wire::session::Hidden>,
}

/// 一行 ⇒ 组员那一行的样子与复活那几格（与清单这一行同一处判，`history/groups.rs`）。
pub(crate) fn member_facts(
    raw: RawRow,
    ann: Option<&Table>,
) -> (crate::wire::SessionFace, cmd_wire::groups::MemberFacts) {
    let f = finish_row(raw, ann);
    // 记录文件的路径照清单那一格原样交（解构取，同 [`row_of`]）。
    let RawRow {
        jsonl_path: record,
        project_path,
        last_account,
        is_bg,
        updated_at,
        ..
    } = f.raw;
    let facts = cmd_wire::groups::MemberFacts {
        title: Words(f.label),
        cwd: (!project_path.is_empty()).then_some(project_path),
        record: (!record.is_empty()).then_some(record),
        account: last_account,
        bg: is_bg,
        at: Some(updated_at),
    };
    (f.face, facts)
}

/// **这台判的「挂着的那一条界面答得了」并进它自己那份清单**（同住哪 [`with_own_housing`]）：每行照 `pending`（某条会话此刻挂着的、最早挂上的那一条）
/// 经 [`crate::wire::answerable`] 写 `asks`。远端那台的请求只挂在那台的登记表里，所以由会话所在那台写、看的那一台照抄（[`listed_face`]）。
pub(crate) fn with_own_asks(
    listing: &mut Listing,
    pending: impl Fn(&str) -> Option<crate::agents::asks::Asked>,
) {
    for r in &mut listing.rows {
        r.asks = crate::wire::answerable(pending(&r.session_id).as_ref());
    }
}

/// 清单那一行的脸：那台判的一生 ＋ 住哪；在等人且那台说挂着的那一条界面答得了（`asks`，[`with_own_asks`]）⇒ 待答请求那一颗（同标签页那张脸）。
fn listed_face(raw: &RawRow) -> crate::wire::SessionFace {
    use crate::wire::SessionFaceExt as _;
    crate::wire::SessionFace::listed(&raw.life, raw.housed.as_ref()).asking(raw.asks)
}

/// 并上这台的注解（那一台的行也并这台的：注解只住本机），判 `face` · `label` · `untitled`。
fn finish_row(mut raw: RawRow, ann: Option<&Table>) -> Finished {
    let meta = ann
        .and_then(|t| t.get(&raw.session_id))
        .cloned()
        .unwrap_or_default();
    raw.starred = meta.starred;
    raw.custom_title = meta.custom_title.clone();
    // 显示的标题：用户改过的 ＞ 标题 ＞ 第一句；都没有 ⇒ `untitled`（界面写「没有说过话的会话」）。
    let untitled = meta.custom_title.is_none()
        && raw.ai_title.as_deref().is_none_or(str::is_empty)
        && raw.first_user_excerpt.is_empty();
    let label = label_of(
        meta.custom_title.as_deref(),
        raw.ai_title.as_deref(),
        &raw.first_user_excerpt,
        &raw.project_path,
    );
    let face = listed_face(&raw);
    Finished {
        raw,
        label,
        untitled,
        face,
        hide: meta.hide,
        hidden: None,
    }
}

/// 搜索词命中：显示标题 · 第一句 · 项目名（不分大小写，子串）。
fn hits(f: &Finished, q: &str) -> bool {
    [&f.label, &f.raw.first_user_excerpt, &f.raw.project_name]
        .iter()
        .any(|s| s.to_lowercase().contains(q))
}

/// 一行成品（活不活只在 `face` 里）；`origin` ＝ 问的是哪一台。
fn row_of(
    f: Finished,
    origin: &cmd_wire::Machine,
    at: i64,
    context: bool,
    now_ms: i64,
    tz: &crate::common::time::Tz,
) -> HistoryRow {
    let mut r = f.raw;
    // 记录文件的路径先拿出来（其余各格下面逐格搬过去）。
    let record_path = {
        let RawRow { jsonl_path, .. } = &mut r;
        std::mem::take(jsonl_path)
    };
    let (at_text, section_text, span_text) =
        crate::common::time::history_times(at, r.started_at, r.updated_at, now_ms, tz);
    let can = can_of(r.agent.as_deref(), &r.life, r.is_bg);
    let archive =
        crate::history::listing::archive_item(std::path::Path::new(&record_path), &f.label);
    HistoryRow {
        archive,
        reveal: face_core::history_asks::reveal_of(origin, &r.project_path),
        delete_ask: face_core::history_asks::delete_ask(origin, can.delete),
        can,
        agent_tag: agent_tag(r.agent.as_deref()).map(Words::from),
        label: Words(f.label),
        untitled: f.untitled,
        face: f.face,
        context: context.then_some(true),
        at,
        at_text,
        section_text,
        span_text,
        origin: origin.clone(),
        agent: r.agent,
        session_id: r.session_id,
        project_path: r.project_path,
        project_name: r.project_name,
        ai_title: r.ai_title,
        first_user_excerpt: r.first_user_excerpt,
        title: r.title,
        started_at: r.started_at,
        updated_at: r.updated_at,
        jsonl_path: record_path,
        message_count_approx: r.message_count_approx,
        is_bg: r.is_bg,
        starred: r.starred,
        custom_title: r.custom_title,
        hidden: f.hidden,
        forked_from_session_id: r.forked_from_session_id,
        project_dir: r.project_dir,
        group: r.group,
        last_account: r.last_account,
    }
}

/// 一台的清单 ＋ 注解 ＋ 入参 ⇒ 成品：头 ＋ 排好的全部行（窗口在 [`patch_from`] 切）。`origin` ＝ 问的是哪一台。`now_ms` 由调用方给（时间筛的「现在」）；
/// `tz` ＝ 看的那一台的时区（请求信封带来的）：每行的行尾 · 分段 · 时间段三格按它写好（偏移按各自那一刻）。
pub(crate) fn answer_from(
    listing: &Listing,
    origin: &cmd_wire::Machine,
    ann: Result<&Table, String>,
    ask: &Ask,
    now_ms: i64,
    tz: &crate::common::time::Tz,
) -> (HistoryHead, Vec<HistoryRow>) {
    let t = ann.as_ref().ok().copied();
    let mut all: Vec<Finished> = listing
        .rows
        .iter()
        .cloned()
        .map(|r| finish_row(r, t))
        .collect();
    // 隐藏：那一台的事实（注解 · 起会话时的 `--tab` · 分身 · 主会话）＋ 那一台的偏好，主会话叫什么照这份清单里它那一行
    // （不在 ⇒ 「记录没读到」那一档，[`crate::history::search_rules::session_label`]）。
    let labels: BTreeMap<String, String> = all
        .iter()
        .map(|f| (f.raw.session_id.clone(), f.label.clone()))
        .collect();
    for f in &mut all {
        let parent = f.raw.parent.as_deref().map(|p| {
            labels.get(p).cloned().unwrap_or_else(|| {
                crate::history::search_rules::session_label(
                    &crate::history::search_rules::LabelFacts::unread(p, None),
                )
                .0
            })
        });
        f.hidden = crate::history::session_hidden::judge(&crate::history::session_hidden::Facts {
            hide: f.hide,
            launch: f.raw.launch_tab.as_ref(),
            bg: f.raw.is_bg,
            parent: parent.as_deref(),
            rules: listing.rules,
        });
    }
    let key = |f: &Finished| {
        if ask.by_created {
            f.raw.started_at
        } else {
            f.raw.updated_at
        }
    };
    let kept = |f: &Finished| {
        if let Some(sid) = ask.sid.as_deref() {
            return f.raw.session_id == sid;
        }
        (ask.hidden || f.hidden.is_none())
            && ask.within_ms.is_none_or(|w| key(f) >= now_ms - w)
            && ask.query.as_deref().is_none_or(|q| hits(f, q))
    };
    let (mut out, rest): (Vec<Finished>, Vec<Finished>) = all.into_iter().partition(|f| kept(f));
    // 分叉的父会话被隐藏 / 被筛掉 ⇒ 照样带上、标 `context`（界面淡显，子会话照样挂在它名下）。按会话 ID 要一行 ⇒ 不补父会话。
    let present: BTreeSet<&str> = out.iter().map(|f| f.raw.session_id.as_str()).collect();
    let parents: BTreeSet<String> = if ask.sid.is_some() {
        BTreeSet::new()
    } else {
        out.iter()
            .filter_map(|f| f.raw.forked_from_session_id.as_deref())
            .filter(|p| !present.contains(p))
            .map(str::to_string)
            .collect()
    };
    let mut context = vec![false; out.len()];
    for f in rest {
        if parents.contains(&f.raw.session_id) {
            out.push(f);
            context.push(true);
        }
    }
    let mut rows: Vec<HistoryRow> = out
        .into_iter()
        .zip(context)
        .map(|(f, cx)| {
            let at = key(&f);
            row_of(f, origin, at, cx, now_ms, tz)
        })
        .collect();
    rows.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    // 问的是哪一台：每行每组都带上（界面把几台并成一列时认得出是哪台的）。
    let groups = groups_of(&rows, listing, origin, now_ms, tz);
    (
        HistoryHead {
            groups,
            notice: ann.err(),
        },
        rows,
    )
}

/// 一问在数据层眼里的样子：问法的键 ＝ 决定集合的那几格（哪台 · 会话 ID · 搜索词 · 排法 · 时间窗 · 隐藏的出不出；不含窗口），
/// 此刻的全部项 ＝ [`answer_from`] 排好的行（键 ＝ 会话 ID）。
struct Source<'a> {
    listing: &'a Listing,
    origin: &'a cmd_wire::Machine,
    ann: Result<&'a Table, String>,
    ask: &'a Ask,
    now_ms: i64,
    tz: &'a crate::common::time::Tz,
}

impl ViewSource for Source<'_> {
    type Head = HistoryHead;
    type Item = HistoryRow;
    fn ask_key(&self) -> String {
        let a = self.ask;
        serde_json::json!([
            self.origin,
            a.sid,
            a.query,
            a.by_created,
            a.within_ms,
            a.hidden
        ])
        .to_string()
    }
    fn now(&self) -> (HistoryHead, Vec<(String, HistoryRow)>) {
        let (head, rows) = answer_from(
            self.listing,
            self.origin,
            self.ann.clone(),
            self.ask,
            self.now_ms,
            self.tz,
        );
        let rows = rows
            .into_iter()
            .map(|r| (r.session_id.clone(), r))
            .collect();
        (head, rows)
    }
}

/// 浏览那几问 ⇒ [`BROWSE`]；带搜索词 · 按会话 ID ⇒ [`NARROW`]。
fn views_for(ask: &Ask) -> &'static Views {
    if ask.query.is_some() || ask.sid.is_some() {
        &NARROW
    } else {
        &BROWSE
    }
}

/// [`answer_from`] 那一份经数据层答成 `range` 那一窗：带着对得上的 `held` ⇒ 没变 / 只给变了的；否则整份。
#[expect(
    clippy::too_many_arguments,
    reason = "备忘 · 清单 · 哪台 · 注解 · 筛法 · 记号 · 此刻 · 时区各是这一答的一维，由帧面那一臂与判据各自给"
)]
pub(crate) fn patch_from(
    views: &Views,
    listing: &Listing,
    origin: &cmd_wire::Machine,
    ann: Result<&Table, String>,
    ask: &Ask,
    held: Option<&Held>,
    now_ms: i64,
    tz: &crate::common::time::Tz,
) -> Patch<HistoryHead, HistoryRow> {
    let src = Source {
        listing,
        origin,
        ann,
        ask,
        now_ms,
        tz,
    };
    views.answer(&src, held, Some(ask.range))
}

/// 一组有没有在跑的：档位（有在跑的 2 · 说不清 1 · 都没在跑 0）。
fn live_rank(g: &HistoryGroup) -> i64 {
    match g.has_live {
        Some(true) => 2,
        None => 1,
        Some(false) => 0,
    }
}

/// 按项目看时的分组：在列的那几行按 `group` 归，每组 {key · 那一家 · 名字 · 目录 · 读它的键 · 几个 · 有没有在跑的 · 最后动过 · 机器}；
/// 读不了的那几个记录目录也是一组（`failed` 带原因）。排序：有在跑的 → 有星标的 → 最近动过的。
fn groups_of(
    rows: &[HistoryRow],
    listing: &Listing,
    origin: &cmd_wire::Machine,
    now_ms: i64,
    tz: &crate::common::time::Tz,
) -> Vec<HistoryGroup> {
    // 一组在后端自己用的那一份：线上那一组 ＋ 最近动静（毫秒；排先后 · 写 `at_text` · 压 `order` 用，不上线）。
    let mut by: BTreeMap<String, (HistoryGroup, i64)> = BTreeMap::new();
    for v in rows.iter().filter(|v| v.context.is_none()) {
        let (g, last) = by.entry(v.group.clone()).or_insert_with(|| {
            (
                HistoryGroup {
                    key: v.group.clone(),
                    agent: v.agent.clone(),
                    project_name: v.project_name.clone(),
                    project_path: v.project_path.clone(),
                    project_dir: v.project_dir.clone(),
                    count: 0,
                    count_text: String::new(),
                    has_live: Some(false),
                    starred: false,
                    at_text: None,
                    order: 0,
                    failed: None,
                    origin: origin.clone(),
                },
                0,
            )
        });
        g.count += 1;
        // 有在跑的：确定有一个就够；有答不了的就不许说「都没在跑」（同项目清单）。
        match (&v.face.life, v.face.dot) {
            (crate::wire::WireLife::Live { .. }, _) => g.has_live = Some(true),
            (_, crate::wire::SessionDot::Unknown) if g.has_live != Some(true) => g.has_live = None,
            _ => {}
        }
        g.starred |= v.starred;
        *last = (*last).max(v.updated_at);
    }
    let mut ranked: Vec<(HistoryGroup, i64)> = by.into_values().collect();
    // 有在跑的 ＞ 说不清的 ＞ 都没在跑（同项目清单）。
    ranked.sort_by(|(a, la), (b, lb)| {
        (live_rank(b), b.starred, lb)
            .cmp(&(live_rank(a), a.starred, la))
            .then_with(|| a.key.cmp(&b.key))
    });
    // 各台各自排好，几台的组并成一列（`history_browse.rs`）⇒ 把这个序压成一个数（`order` 大的在前；并的时候只按它，不另判）：
    //   档位 × 10^14 ＋ 有星标 × 10^13 ＋ 最后动过（毫秒，< 10^13）。出口那一下只留线上那一组。
    let mut out: Vec<HistoryGroup> = ranked
        .into_iter()
        .map(|(mut g, last)| {
            g.count_text =
                copy_core::copy_text("history.group.count", &[("n", &g.count.to_string())]);
            g.at_text = Some(crate::common::time::history_at_text(last, now_ms, tz));
            g.order = live_rank(&g) * 100_000_000_000_000
                + i64::from(g.starred) * 10_000_000_000_000
                + last.clamp(0, 9_999_999_999_999);
            g
        })
        .collect();
    for f in &listing.failed {
        out.push(HistoryGroup {
            // 键带上那一家：同名目录几家树下都读不了也是几组、不撞（读得了的组键本来就带家）。
            key: format!("dir:{}:{}", f.agent.as_str(), f.project_dir),
            // 那一家是扫它的那台扫时手里那一根的（[`Unread`]）：同名目录几家树下都有也不混；不照看的这一台的记录根另认。
            agent: Some(f.agent.clone()),
            project_name: f.project_dir.clone(),
            project_path: String::new(),
            project_dir: f.project_dir.clone(),
            count: 0,
            count_text: String::new(),
            has_live: None,
            starred: false,
            at_text: None,
            order: 0,
            failed: Some(f.error.clone()),
            origin: origin.clone(),
        });
    }
    out
}

/// 格目录与出参对拍的样本：每个可缺的格都填上、每个列表都不空。
pub fn specimen() -> Patch<HistoryHead, HistoryRow> {
    let s = |v: &str| v.to_string();
    let w = |v: &str| Words(v.to_string());
    let mut rows = vec![HistoryRow {
        agent: Some(s("agent").into()),
        session_id: s("s"),
        project_path: s("/w"),
        project_name: s("w"),
        ai_title: Some(s("t")),
        first_user_excerpt: s("e"),
        title: s("t"),
        started_at: 1,
        updated_at: 2,
        jsonl_path: s("/p"),
        message_count_approx: 1,
        is_bg: false,
        starred: true,
        custom_title: Some(s("c")),
        hidden: crate::history::session_hidden::judge(&crate::history::session_hidden::Facts {
            hide: Some(crate::history::history_annotations::Hide::On),
            ..Default::default()
        }),
        forked_from_session_id: Some(s("f")),
        project_dir: s("-w"),
        group: s("agent:/w"),
        last_account: Some(s("a")),
        label: w("c"),
        untitled: false,
        face: crate::wire::SessionFace::of(crate::wire::Life::Live {
            activity: Some(crate::agents::SessionActivity::Working),
            host: None,
        }),
        // 分身那一形：恢复不给时那一句（`resumeWhy`）也填上。
        can: can_of(
            Some("agent"),
            &crate::wire::WireLife::Ended { by: None },
            true,
        ),
        archive: Some(cmd_wire::listing::RowArchive {
            archived: true,
            label: w("a"),
            failed: w("f"),
        }),
        agent_tag: Some(w("g")),
        context: Some(true),
        at: 2,
        at_text: w("t"),
        section_text: w("s"),
        span_text: w("p"),
        origin: cmd_wire::Machine::named(s("m")),
        reveal: false,
        delete_ask: w("d"),
    }];
    let groups = vec![HistoryGroup {
        key: s("agent:/w"),
        agent: Some(s("agent").into()),
        project_name: s("w"),
        project_path: s("/w"),
        project_dir: s("-w"),
        count: 1,
        count_text: s("c"),
        has_live: Some(false),
        starred: false,
        at_text: Some(w("t")),
        order: 2,
        failed: Some(s("x")),
        origin: cmd_wire::Machine::named(s("m")),
    }];
    // 离开活的那两形也各露一行（能做什么那几格的「不能」与为什么：已结束只灰终端那两格，记录已不在连换模型 · 权限档也灰）。
    for (sid, life) in [
        ("e", crate::wire::Life::Ended),
        ("g", crate::wire::Life::Gone),
    ] {
        let mut left = rows[0].clone();
        left.session_id = s(sid);
        left.face = crate::wire::SessionFace::of(life);
        rows.push(left);
    }
    Patch {
        held: Held(s("h")),
        whole: true,
        same: false,
        order: Some(rows.iter().map(|r| r.session_id.clone()).collect()),
        head: Some(Head {
            total: rows.len() as u32,
            info: HistoryHead {
                groups,
                notice: Some(s("n")),
            },
        }),
        up: rows
            .into_iter()
            .map(|r| (r.session_id.clone(), r))
            .collect(),
    }
}

// ───────────────────────── 帧面 ─────────────────────────

pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, Said> + Send + 'static,
) -> Result<T, Said> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        Said::new(
            "failed",
            crate::common::said::IntoNote::into_note(Said::with_raw(
                copy_text("beHistoryJoin.blocking.unfinished", &[]),
                e,
            )),
        )
    })?
}

/// 帧面 `history-list` 的本体（`tz` ＝ 看的那一台的时区：行上那几格时刻字按它写；`until` ＝ 发起方给的截止时刻，问那一台时照它等）。
pub(crate) async fn answer(
    req: HistoryListReq,
    tz: crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
) -> Result<Patch<HistoryHead, HistoryRow>, Said> {
    let ask = parse_ask(&req)?;
    let held = req.held;
    let origin = req.origin;
    let listing = match origin.route("history-list") {
        Err(_) => return Err(bad("`origin` must be a non-empty string")),
        Ok(cmd_wire::Route::Local) => {
            let only = ask.sid.clone();
            blocking(move || {
                machine_listing_for(only.as_deref(), crate::history::list_node::node())
            })
            .await?
        }
        Ok(cmd_wire::Route::Remote(o)) => {
            super::remote_listing::listing(o, false, &tz, until).await?
        }
    };
    blocking(move || {
        // 并的是清单自己带来的那台的注解（本机的那份只并本机会话；远端那台自己并好了它那份）。
        let ann = listing_annotations(&listing);
        Ok(patch_from(
            views_for(&ask),
            &listing,
            &origin,
            ann.as_ref().map_err(Clone::clone),
            &ask,
            held.as_ref(),
            crate::common::time::now_ms() as i64,
            &tz,
        ))
    })
    .await
}

// 只给测试用的那几样（这台记录树的串行锁）：本包判据与上层判据（`test-support`）共用。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/backend/history/history_list_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../../tests/backend/history/history_list_tests.rs"]
mod tests;
