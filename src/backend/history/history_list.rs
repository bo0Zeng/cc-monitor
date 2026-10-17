//! **历史页的平铺会话清单**（帧面 `history-list`）—— 跨项目、按机器问，一次出成品：
//! 每行带显示标题 · 项目与真实目录 · 那一家 · 状态 · 注解 · 分叉 · 分身 · 条数 · 时间 ·「这一行能做什么」，
//! 外加按项目看时的分组（按真实目录分，读不了的那一组照样出、带原因）。
//!
//! # 形状
//!
//! ```text
//! history-list {origin?, fresh?, sid?, query?, sort?, within_days?, hidden?, held?, range?}   （入参 · 回包 · 码都是类型）
//!   origin 缺席    这台；给了 = 那一台：那台的清单（那台自己判活、自己读上次的号、并好它自己的注解）由本进程问那台的常驻取、记着副本
//!                  （`remote_listing.rs`：带记号问 `history-listing`，只拿变了的那几行）；手上没有 · 或 `fresh`（开页 · 刷新）才去问，
//!                  敲字搜索用手上那份；那台常驻不在 ⇒ `resident_absent`（不在那台起一个冷进程顶上）
//!   ⇒ 并上那台的注解（星标 · 改名 · 隐藏）⇒ 筛（隐藏 · 时间 · 搜索词）⇒ 补上被筛掉的分叉父会话（`context`）⇒ 排 ⇒ 截
//!   sid: 只要那一个会话那一行（独立查看窗按会话 ID 开任意一个会话，含已结束的、隐藏的）：别的筛一概不看、不补父会话
//!   ⇒ 带键集合那一形（`common/views.rs`，键 ＝ 会话 ID）：`range` 那一窗（缺 ⇒ 头 2000 行）；头 ＝ {groups, notice} ＋ 总数；
//!     带着上次的 `held` ⇒ 没变 `same` · 只给变了的那几行 `up`（先后变了才带 `order`）；不带 · 对不上 ⇒ `whole`
//! ```
//!
//! 判定全在这里：搜什么（显示标题 · 第一句 · 项目名）· 按什么排（`at`）· 每行能不能恢复 / 分叉 / 删、不能的为什么。
//! 界面按 `at` 把各台的行并成一列（各台各自排好了），不另判。
//! 「不知道」不装成已知：判不了活（合成历史没有 pidfile）⇒ `isLive: null`、那一组 `hasLive: null`；
//! 注解读不到 / 读不懂 ⇒ 行照出（星标 · 隐藏按没有算），`notice` 说一句为什么。

pub(crate) use cmd_wire::listing::{
    FailedDir, HistoryGroup, HistoryHead, HistoryRow, Listing, RawRow,
};
use cmd_wire::sync::{Head, Held, Patch, Range};
use std::collections::{BTreeMap, BTreeSet};

use copy_core::copy_text;
use serde::Deserialize;
use serde_json::Value;

use crate::common::cells::Words;
use crate::common::views::{ViewSource, Views};
use crate::history::history_annotations::{Loaded, Table};
use crate::history::listing::{can_of, RowLife};
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

/// 这台此刻活着的会话（pidfile 源头）。
pub(crate) struct LiveSet(pub(crate) BTreeMap<String, Option<crate::agents::SessionActivity>>);

/// 注解这一维：读到了 ⇒ 那张表；读不到 ⇒ 为什么（给 `notice`）。
pub(crate) fn annotations(loaded: &Loaded) -> Result<&Table, String> {
    match loaded {
        Loaded::Read(t) => Ok(t),
        Loaded::NoPath => Err(copy_text("beHistoryJoin.annotations.unknown", &[]).into()),
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

/// 路径最后一段（`/` 与 `\` 都认）；空 ⇒ `None`。
fn last_segment(p: &str) -> Option<&str> {
    p.rsplit(['/', '\\']).next().filter(|s| !s.is_empty())
}

/// 记录树里的一行（会话清单那一形）＋ 判活 ⇒ 清单的一行（注解格先按没有填，并注解在 [`answer_from`]）。
/// 没有 `sessionId` ⇒ `None`。
fn session_row(v: &Value, project_dir: &str, live: &LiveSet) -> Option<RawRow> {
    let sid = v["sessionId"].as_str().filter(|s| !s.is_empty())?;
    let cwd = v["cwd"].as_str().unwrap_or_default();
    let excerpt = v["firstUserExcerpt"].as_str().unwrap_or_default();
    let agent = crate::agents::record_tree_kind()
        .unwrap_or_default()
        .to_string();
    // 分叉关系：两格都有才带。
    let fork = match (
        v["forkedFromSessionId"].as_str(),
        v["forkedFromMessageUuid"].as_str(),
    ) {
        (Some(s), Some(u)) => Some((s.to_string(), u.to_string())),
        _ => None,
    };
    Some(RawRow {
        group: group_key(&agent, cwd),
        agent,
        session_id: sid.to_string(),
        project_path: cwd.to_string(),
        project_name: last_segment(cwd).unwrap_or(project_dir).to_string(),
        ai_title: v["aiTitle"].as_str().map(str::to_string),
        first_user_excerpt: excerpt.to_string(),
        title: label_of(None, v["aiTitle"].as_str(), excerpt, cwd, sid),
        started_at: v["startedAtMs"].as_i64().unwrap_or(0),
        updated_at: v["updatedAtMs"].as_i64().unwrap_or(0),
        jsonl_path: v["jsonlPath"].as_str().unwrap_or_default().to_string(),
        life: Some(crate::history::live::life_of(&live.0, sid)),
        message_count_approx: v["messageCountApprox"].as_u64().unwrap_or(0) as u32,
        is_bg: v["isBg"].as_bool().unwrap_or(false),
        starred: false,
        custom_title: None,
        launch_tab: None,
        parent: None,
        forked_from_session_id: fork.as_ref().map(|f| f.0.clone()),
        forked_from_message_uuid: fork.map(|f| f.1),
        project_dir: project_dir.to_string(),
        last_account: None,
    })
}

/// 合成历史的一条会话 ⇒ 清单的一行（判不了活 ⇒ `isLive: null`）。
fn synth_row(kind: &str, s: &crate::agents::SynthSession, excerpt: String) -> RawRow {
    let name = last_segment(&s.cwd).map_or_else(|| format!("({kind})"), str::to_string);
    let title = label_of(None, None, &excerpt, &s.cwd, &s.sid);
    RawRow {
        agent: kind.to_string(),
        session_id: s.sid.clone(),
        project_path: s.cwd.clone(),
        project_name: name,
        ai_title: None,
        first_user_excerpt: excerpt,
        title,
        started_at: s.mtime_ms,
        updated_at: s.mtime_ms,
        jsonl_path: s.path.to_string_lossy().into_owned(),
        life: None,
        message_count_approx: 0,
        is_bg: false,
        starred: false,
        custom_title: None,
        launch_tab: None,
        parent: None,
        forked_from_session_id: None,
        forked_from_message_uuid: None,
        project_dir: format!("{kind}:{}", s.cwd),
        group: group_key(kind, &s.cwd),
        last_account: None,
    }
}

/// 叫这一行什么（[`crate::history::search_rules::session_label`]，与标签页 · 通知同一个）。
fn label_of(
    custom: Option<&str>,
    title: Option<&str>,
    first: &str,
    project: &str,
    sid: &str,
) -> String {
    let facts = crate::history::search_rules::LabelFacts {
        custom: custom.map(str::to_string),
        title: title.map(str::to_string),
        first: first.to_string(),
        project_dir: (!project.is_empty()).then(|| project.to_string()),
    };
    crate::history::search_rules::session_label(&facts).0
}

/// 一行的分组键：哪一家 ＋ 真实目录（记录目录名会撞）。
fn group_key(agent: &str, project_path: &str) -> String {
    format!("{agent}:{project_path}")
}

/// 这台的平铺清单（并上这台的注解）：记录树每个目录 ＋ 合成历史各家；判活 ＝ 这台 pidfile；上次的号 ＝ 这台的起会话账号记录。
/// `last` 读不懂 ⇒ 那一格都不带（界面不说「上次用的」，照样能恢复）。
pub(crate) fn machine_listing() -> Result<Listing, (&'static str, String)> {
    machine_listing_for(None, crate::history::list_node::node())
}

/// [`machine_listing`]；`only` ＝ 按 sid 问的那一条：记录树里只整份扫叫这个名字的那一份（别的会话只出分组要的几格，
/// [`answer_from`] 按 sid 滤掉）⇒ 答案与整台扫逐字相同，不必把整台每份会话从头扫一遍。
/// `node` ＝ 这台会话清单的节点（生产 ＝ 本进程那一份）：读之前放开。
fn machine_listing_for(
    only: Option<&str>,
    node: &crate::history::list_node::ListNode,
) -> Result<Listing, (&'static str, String)> {
    // 读之前放开清单的节点：此后再变的会话再响一次（这一份里没赶上的由下一帧叫回来重问）。
    node.asked();
    let home = crate::history::listing::agent_home();
    let live = LiveSet(crate::history::live::live_activity(&home));
    let tree = tree_for(&home, only).map_err(|e| ("failed", e))?;
    let synth: Vec<(
        &'static str,
        Vec<crate::agents::SynthSession>,
        fn(&std::path::Path) -> String,
    )> = crate::agents::history_faces()
        .into_iter()
        .map(|(k, f)| (k, (f.sessions)(), f.excerpt))
        .collect();
    let last = crate::control::launch_account::last_accounts_at(
        crate::platform::paths::data_home().as_deref(),
    )
    .ok()
    .and_then(|l| serde_json::to_value(l.accounts).ok())
    .unwrap_or(Value::Null);
    let mut listing = listing_from(tree.unwrap_or_default(), &synth, &live, &last);
    with_own_annotations(&mut listing, &crate::history::history_annotations::load());
    with_own_rules(
        &mut listing,
        &crate::control::session_hidden::Here::read_now(),
    );
    Ok(listing)
}

/// 记录树分好的目录（[`crate::history::listing::sessions_by_dir_for`]）；按 sid 问的那一条是分叉来的 ⇒ 父会话那一份也整份扫
/// （那一行要父会话的显示标题，与整台扫那一份同一个字）。
pub(crate) fn tree_for(
    home: &std::path::Path,
    only: Option<&str>,
) -> Result<Option<Vec<(String, Result<Vec<Value>, String>)>>, String> {
    let tree = crate::history::listing::sessions_by_dir_for(home, only)?;
    let (Some(sid), Some(mut tree)) = (only, tree) else {
        return crate::history::listing::sessions_by_dir_for(home, only);
    };
    let parent = tree
        .iter()
        .filter_map(|(_, r)| r.as_ref().ok())
        .flatten()
        .find(|v| v["sessionId"] == sid)
        .and_then(|v| v["forkedFromSessionId"].as_str())
        .map(str::to_string);
    let Some(p) = parent.filter(|p| p != sid) else {
        return Ok(Some(tree));
    };
    let full = crate::history::listing::sessions_by_dir_for(home, Some(&p))?.unwrap_or_default();
    for (dir, rows) in &mut tree {
        let Ok(rows) = rows else { continue };
        let Some(Ok(got)) = full.iter().find(|(d, _)| d == dir).map(|(_, r)| r) else {
            continue;
        };
        for v in rows.iter_mut().filter(|v| v["sessionId"] == p.as_str()) {
            if let Some(w) = got.iter().find(|w| w["sessionId"] == p.as_str()) {
                *v = w.clone();
            }
        }
    }
    Ok(Some(tree))
}

/// **这台隐藏要的那几样并进它自己那份清单**（同注解）：每行起会话时的 `--tab` · 主会话，整份带这台偏好那两格。
/// 单个那一格就是注解（[`with_own_annotations`]）；判定在 [`answer_from`]（`control/session_hidden.rs`）。
pub(crate) fn with_own_rules(listing: &mut Listing, here: &crate::control::session_hidden::Here) {
    for r in &mut listing.rows {
        r.launch_tab = here.ledger.tabs.get(&r.session_id).cloned();
        r.parent = here.parent_of(&r.session_id).map(str::to_string);
    }
    listing.rules = here.rules;
}

/// [`machine_listing`] 的可喂夹具那一半（记录树分好的目录 · 合成历史 · 判活 · 上次的号由调用方给；不并注解）。
pub(crate) fn listing_from(
    tree: Vec<(String, Result<Vec<Value>, String>)>,
    synth: &[(
        &'static str,
        Vec<crate::agents::SynthSession>,
        fn(&std::path::Path) -> String,
    )],
    live: &LiveSet,
    last: &Value,
) -> Listing {
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    for (dir, got) in tree {
        match got {
            Ok(list) => rows.extend(list.iter().filter_map(|v| session_row(v, &dir, live))),
            Err(why) => failed.push(FailedDir {
                project_dir: dir,
                error: why,
            }),
        }
    }
    for (kind, sessions, excerpt) in synth {
        for s in sessions {
            rows.push(synth_row(kind, s, excerpt(&s.path)));
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

/// `history-list` 的排法：按最后活动（缺省）· 按开始。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "HistorySort"
    )
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Sort {
    #[default]
    Activity,
    Created,
}

/// `history-list` 的入参（全可缺）。
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryListReq {
    /// 那台的名字（行与组都标上它）。缺 ⇒ 这台。问那一台要带期限（请求信封 `within_ms`）：等多久归发起方，没带 ⇒ `bad_args`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, as = "Option<String>"))]
    pub(crate) origin: Option<cmd_wire::Machine>,
    /// `true` ⇒ 先问那一台（`origin`）同步一次本进程记着的副本再答（开页 · 刷新）；缺 ⇒ 手上有就用手上那份（敲字搜索）。不带 `origin` ⇒ `bad_args`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fresh: Option<bool>,
    /// 只要这一个会话那一行（独立查看窗；别的筛一概不看、不补父会话）。形状同 `history-record`（`[A-Za-z0-9-]`，1..=64）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) sid: Option<String>,
    /// 只留显示标题 · 第一句 · 项目名里含这几个字的（不分大小写，子串；不比路径、不搜内容 —— 内容走 `history-search`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) query: Option<String>,
    /// 缺 ⇒ 按最后活动。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) sort: Option<Sort>,
    /// 只留那个键（同 `sort`）落在最近 N 天里的（1–3650）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) within_days: Option<u64>,
    /// `true` ⇒ 隐藏的也出（缺省不出）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) hidden: Option<bool>,
    /// 上次这一问答的记号（原样交回）：对得上 ⇒ 没变 · 只给变了的那几行；缺 · 对不上（那台重启过 · 换了问法）⇒ 整份。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) held: Option<Held>,
    /// 只要排好的那一窗（`limit` 1–20000）；缺 ⇒ 头 2000 行。总数在回包的头里。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) range: Option<Range>,
}

crate::wire::typed::codes! {
    /// `history-list` 的码。
    pub(crate) enum HistoryListErr {
        /// 读不了这台的记录树 · 读的那一趟没做完。
        Failed = "failed",
        /// 问的是那一台、可达表里没有它（还没连上过）· 拨号 / 链路不通。
        Unreachable = "unreachable",
        /// 问的是那一台、那台的常驻后端没在跑（不在那台起一个冷进程顶上）。
        ResidentAbsent = "resident_absent",
    }
}

/// `history-list`：历史页的平铺会话清单。扫记录树在阻塞池里做，并 · 筛 · 排是纯计算 ⇒ 异步档。
pub(crate) struct HistoryListCmd;

impl crate::wire::typed::Command for HistoryListCmd {
    const NAME: &'static str = "history-list";
    type Req = HistoryListReq;
    type Reply = Patch<HistoryHead, HistoryRow>;
    type Err = HistoryListErr;
}

impl crate::wire::typed::Handle for HistoryListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: HistoryListReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Patch<HistoryHead, HistoryRow>, HistoryListErr>> + Send + 'static
    {
        async move {
            answer(req, cx.tz, cx.until)
                .await
                .map_err(crate::wire::typed::of_pair(HistoryListErr::Failed))
        }
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

fn bad(what: &str) -> (&'static str, String) {
    ("bad_args", crate::common::contract::malformed(what))
}

/// 入参 ⇒ [`Ask`]：值域不对 ⇒ `bad_args`（先于任何 IO）。
pub(crate) fn parse_ask(req: &HistoryListReq) -> Result<Ask, (&'static str, String)> {
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
    let query = req
        .query
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let within_ms = match req.within_days {
        None => None,
        Some(d @ 1..=3650) => Some(d as i64 * 86_400_000),
        Some(_) => return Err(bad("`within_days` must be 1..=3650")),
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
        query,
        by_created: req.sort == Some(Sort::Created),
        within_ms,
        hidden: req.hidden == Some(true),
        range,
    })
}

/// 行上那一家的小牌（对用户的叫法，如 `Codex`）：默认那一家（Claude）不画 ⇒ `None`。
fn agent_tag(kind: &str) -> Option<&'static str> {
    if kind == crate::agents::default_kind() {
        return None;
    }
    crate::agents::launch_face_among(crate::agents::REGISTRY, kind).map(|l| l.display_name)
}

/// 一行并上注解、判好的几格（排序键与时刻字在 [`answer_from`] 定了排法之后才写）。
struct Finished {
    raw: RawRow,
    label: String,
    untitled: bool,
    face: crate::wire::SessionFace,
    /// 单个那一格（注解）：隐藏的判定在所有行都叫得出名字之后（[`answer_from`]），主会话的名字从同一份清单里取。
    pin: Option<crate::history::history_annotations::Hide>,
    hidden: Option<cmd_wire::session::Hidden>,
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
        &raw.session_id,
    );
    let face = crate::wire::SessionFace::of(crate::wire::Life::of_row(raw.life));
    Finished {
        raw,
        label,
        untitled,
        face,
        pin: meta.hide,
        hidden: None,
    }
}

/// 搜索词命中：显示标题 · 第一句 · 项目名（不分大小写，子串）。
fn hits(f: &Finished, q: &str) -> bool {
    [&f.label, &f.raw.first_user_excerpt, &f.raw.project_name]
        .iter()
        .any(|s| s.to_lowercase().contains(q))
}

/// 一行成品（活不活只在 `face` 里）。
fn row_of(
    f: Finished,
    at: i64,
    context: bool,
    now_ms: i64,
    tz: &crate::Tz,
    labels: &BTreeMap<String, String>,
) -> HistoryRow {
    let r = f.raw;
    let forked_from = r
        .forked_from_session_id
        .as_deref()
        .and_then(|p| labels.get(p))
        .map(|l| cmd_wire::listing::ForkedFrom {
            label: Words(l.clone()),
        });
    let (at_text, section_text, span_text) =
        crate::common::time::history_times(at, r.started_at, r.updated_at, now_ms, tz);
    HistoryRow {
        can: can_of(&r.agent, r.life, r.is_bg),
        agent_tag: agent_tag(&r.agent).map(Words::from),
        label: Words(f.label),
        untitled: f.untitled,
        face: f.face,
        context: context.then_some(true),
        at,
        at_text,
        section_text,
        span_text,
        origin: None,
        agent: r.agent,
        session_id: r.session_id,
        project_path: r.project_path,
        project_name: r.project_name,
        ai_title: r.ai_title,
        first_user_excerpt: r.first_user_excerpt,
        title: r.title,
        started_at: r.started_at,
        updated_at: r.updated_at,
        jsonl_path: r.jsonl_path,
        message_count_approx: r.message_count_approx,
        is_bg: r.is_bg,
        starred: r.starred,
        custom_title: r.custom_title,
        hidden: f.hidden,
        forked_from_session_id: r.forked_from_session_id,
        forked_from_message_uuid: r.forked_from_message_uuid,
        forked_from,
        project_dir: r.project_dir,
        group: r.group,
        last_account: r.last_account,
    }
}

/// 一台的清单 ＋ 注解 ＋ 入参 ⇒ 成品：头 ＋ 排好的全部行（窗口在 [`patch_from`] 切）。`origin` ＝ 问的是哪一台（缺 ＝ 这台）。`now_ms` 由调用方给（时间筛的「现在」）；
/// `tz` ＝ 看的那一台的时区（请求信封带来的）：每行的行尾 · 分段 · 时间段三格按它写好（偏移按各自那一刻）。
pub(crate) fn answer_from(
    listing: &Listing,
    origin: Option<&str>,
    ann: Result<&Table, String>,
    ask: &Ask,
    now_ms: i64,
    tz: &crate::Tz,
) -> (HistoryHead, Vec<HistoryRow>) {
    let t = ann.as_ref().ok().copied();
    let mut all: Vec<Finished> = listing
        .rows
        .iter()
        .cloned()
        .map(|r| finish_row(r, t))
        .collect();
    // 隐藏：那一台的事实（注解 · 起会话时的 `--tab` · 分身 · 主会话）＋ 那一台的偏好，主会话叫什么照这份清单里它那一行（不在 ⇒ 编号头八位）。
    let labels: BTreeMap<String, String> = all
        .iter()
        .map(|f| (f.raw.session_id.clone(), f.label.clone()))
        .collect();
    for f in &mut all {
        let parent = f.raw.parent.as_deref().map(|p| {
            labels
                .get(p)
                .cloned()
                .unwrap_or_else(|| p.chars().take(8).collect())
        });
        f.hidden = crate::control::session_hidden::judge(&crate::control::session_hidden::Facts {
            pin: f.pin,
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
            row_of(f, at, cx, now_ms, tz, &labels)
        })
        .collect();
    rows.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    let mut groups = groups_of(&rows, listing, now_ms, tz);
    // 问的是哪一台：每行每组都带上（界面把几台并成一列时认得出是哪台的）。
    if let Some(o) = origin {
        rows.iter_mut()
            .for_each(|r| r.origin = Some(cmd_wire::Machine(o.to_string())));
        groups
            .iter_mut()
            .for_each(|g| g.origin = Some(cmd_wire::Machine(o.to_string())));
    }
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
    origin: Option<&'a str>,
    ann: Result<&'a Table, String>,
    ask: &'a Ask,
    now_ms: i64,
    tz: &'a crate::Tz,
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn patch_from(
    views: &Views,
    listing: &Listing,
    origin: Option<&str>,
    ann: Result<&Table, String>,
    ask: &Ask,
    held: Option<&Held>,
    now_ms: i64,
    tz: &crate::Tz,
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
    now_ms: i64,
    tz: &crate::Tz,
) -> Vec<HistoryGroup> {
    let mut by: BTreeMap<String, HistoryGroup> = BTreeMap::new();
    for v in rows.iter().filter(|v| v.context.is_none()) {
        let g = by.entry(v.group.clone()).or_insert_with(|| HistoryGroup {
            key: v.group.clone(),
            agent: v.agent.clone(),
            project_name: v.project_name.clone(),
            project_path: v.project_path.clone(),
            project_dir: v.project_dir.clone(),
            count: 0,
            has_live: Some(false),
            starred: false,
            last_activity: 0,
            at_text: None,
            order: 0,
            failed: None,
            origin: None,
        });
        g.count += 1;
        // 有在跑的：确定有一个就够；有答不了的就不许说「都没在跑」（同项目清单）。
        match (&v.face.live, v.face.dot) {
            (crate::wire::FaceLive::Yes { .. }, _) => g.has_live = Some(true),
            (_, crate::wire::SessionDot::Unknown) if g.has_live != Some(true) => g.has_live = None,
            _ => {}
        }
        g.starred |= v.starred;
        g.last_activity = g.last_activity.max(v.updated_at);
    }
    let mut out: Vec<HistoryGroup> = by.into_values().collect();
    // 有在跑的 ＞ 说不清的 ＞ 都没在跑（同项目清单）。
    out.sort_by(|a, b| {
        let rank = |g: &HistoryGroup| (live_rank(g), g.starred, g.last_activity);
        rank(b).cmp(&rank(a)).then_with(|| a.key.cmp(&b.key))
    });
    // 各台各自排好，界面要把几台的组并成一列 ⇒ 把这个序压成一个数交出去（`order` 大的在前；界面只按它并，不另判）：
    //   档位 × 10^14 ＋ 有星标 × 10^13 ＋ 最后动过（毫秒，< 10^13）。
    for g in &mut out {
        g.at_text = Some(crate::common::time::history_at_text(
            g.last_activity,
            now_ms,
            tz,
        ));
        g.order = live_rank(g) * 100_000_000_000_000
            + i64::from(g.starred) * 10_000_000_000_000
            + g.last_activity.clamp(0, 9_999_999_999_999);
    }
    for f in &listing.failed {
        out.push(HistoryGroup {
            key: format!("dir:{}", f.project_dir),
            agent: crate::agents::record_tree_kind()
                .unwrap_or_default()
                .to_string(),
            project_name: f.project_dir.clone(),
            project_path: String::new(),
            project_dir: f.project_dir.clone(),
            count: 0,
            has_live: None,
            starred: false,
            last_activity: 0,
            at_text: None,
            order: 0,
            failed: Some(f.error.clone()),
            origin: None,
        });
    }
    out
}

/// 格目录与出参对拍的样本：每个可缺的格都填上、每个列表都不空。
pub(crate) fn specimen() -> Patch<HistoryHead, HistoryRow> {
    let s = |v: &str| v.to_string();
    let w = |v: &str| Words(v.to_string());
    let mut rows = vec![HistoryRow {
        agent: s("agent"),
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
        hidden: crate::control::session_hidden::judge(&crate::control::session_hidden::Facts {
            pin: Some(crate::history::history_annotations::Hide::On),
            ..Default::default()
        }),
        forked_from_session_id: Some(s("f")),
        forked_from_message_uuid: Some(s("u")),
        forked_from: Some(cmd_wire::listing::ForkedFrom { label: w("f") }),
        project_dir: s("-w"),
        group: s("agent:/w"),
        last_account: Some(s("a")),
        label: w("c"),
        untitled: false,
        face: crate::wire::SessionFace::of(crate::wire::Life::Live {
            activity: Some(crate::agents::SessionActivity::Working),
            tmux: None,
        }),
        can: can_of("agent", Some(RowLife::Ended), false),
        agent_tag: Some(w("g")),
        context: Some(true),
        at: 2,
        at_text: w("t"),
        section_text: w("s"),
        span_text: w("p"),
        origin: Some(cmd_wire::Machine(s("m"))),
    }];
    let groups = vec![HistoryGroup {
        key: s("agent:/w"),
        agent: s("agent"),
        project_name: s("w"),
        project_path: s("/w"),
        project_dir: s("-w"),
        count: 1,
        has_live: Some(false),
        starred: false,
        last_activity: 2,
        at_text: Some(w("t")),
        order: 2,
        failed: Some(s("x")),
        origin: Some(cmd_wire::Machine(s("m"))),
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

fn now_ms() -> i64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, (&'static str, String)> + Send + 'static,
) -> Result<T, (&'static str, String)> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        (
            "failed",
            crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(
                copy_text("beHistoryJoin.blocking.unfinished", &[]),
                e,
            )),
        )
    })?
}

/// 帧面 `history-list` 的本体（`tz` ＝ 看的那一台的时区：行上那几格时刻字按它写；`until` ＝ 发起方给的截止时刻，问那一台时照它等）。
pub(crate) async fn answer(
    req: HistoryListReq,
    tz: crate::Tz,
    until: Option<crate::platform::child::Until>,
) -> Result<Patch<HistoryHead, HistoryRow>, (&'static str, String)> {
    let ask = parse_ask(&req)?;
    let held = req.held;
    let origin = match req.origin {
        None => None,
        Some(cmd_wire::Machine(s)) if !s.is_empty() => Some(s),
        Some(_) => return Err(bad("`origin` must be a non-empty string")),
    };
    let fresh = req.fresh == Some(true);
    let listing = match &origin {
        None if fresh => return Err(bad("`fresh` needs the `origin` it syncs from")),
        None => {
            let only = ask.sid.clone();
            blocking(move || {
                machine_listing_for(only.as_deref(), crate::history::list_node::node())
            })
            .await?
        }
        Some(o) => super::remote_listing::listing(o, fresh, &tz, until).await?,
    };
    blocking(move || {
        // 并的是清单自己带来的那台的注解（本机的那份只并本机会话；远端那台自己并好了它那份）。
        let ann = listing_annotations(&listing);
        Ok(patch_from(
            views_for(&ask),
            &listing,
            origin.as_deref(),
            ann.as_ref().map_err(Clone::clone),
            &ask,
            held.as_ref(),
            now_ms(),
            &tz,
        ))
    })
    .await
}

#[cfg(test)]
#[path = "../../../tests/backend/history/history_list_tests.rs"]
mod tests;
