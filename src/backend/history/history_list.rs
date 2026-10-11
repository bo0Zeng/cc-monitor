//! **历史页的平铺会话清单**（帧面 `history-list`）—— 跨项目、按机器问，一次出成品：
//! 每行带显示标题 · 项目与真实目录 · 那一家 · 状态 · 注解 · 分叉 · 分身 · 条数 · 时间 ·「这一行能做什么」，
//! 外加按项目看时的分组（按真实目录分，读不了的那一组照样出、带原因）。
//!
//! # 形状
//!
//! ```text
//! history-list {origin?, listing?, raw?, sid?, query?, sort?, within_days?, hidden?, limit?}
//!   raw: true      这台自己的清单 `{rows, failed}`（不并注解、不筛不排）—— 远端那一份就是那台常驻答的它（热缓存）
//!   origin 缺席    这台；给了 = 那一台：`listing` 是界面经已开着的长连接问那台 `raw` 拿回来的原样（那台自己判活、自己读上次的号），
//!                  本进程记着它；不带 `listing` ⇒ 用记着的那份（敲字搜索），没记着 ⇒ `no_listing`（界面先问那台、再带着来）
//!   ⚠ 这台**不替界面去问那台**（原先经 `remote_ask` 在那台起一次性进程、每次开页冷扫一遍；那台常驻早就热着同一份）
//!   ⇒ 并上这台的注解（星标 · 改名 · 隐藏）⇒ 筛（隐藏 · 时间 · 搜索词）⇒ 补上被筛掉的分叉父会话（`context`）⇒ 排 ⇒ 截
//!   sid: 只要那一个会话那一行（独立查看窗按会话 ID 开任意一个会话，含已结束的、隐藏的）：别的筛一概不看、不补父会话
//!   ⇒ {rows, groups, total, truncated, notice}
//! ```
//!
//! 判定全在这里：搜什么（显示标题 · 第一句 · 项目名）· 按什么排（`at`）· 每行能不能恢复 / 分叉 / 删、不能的为什么。
//! 界面按 `at` 把各台的行并成一列（各台各自排好了），不另判。
//! 「不知道」不装成已知：判不了活（合成历史没有 pidfile）⇒ `isLive: null`、那一组 `hasLive: null`；
//! 注解读不到 / 读不懂 ⇒ 行照出（星标 · 隐藏按没有算），`notice` 说一句为什么。

use std::collections::{BTreeMap, BTreeSet};

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::common::cells::Words;
use crate::history::history_annotations::{Loaded, Table};
use crate::observe::history_query::{can_of, status_of, Can};

/// 默认最多回多少行（按时间看时超过它只列最近这些、底部「更早的用搜索找」）。
const DEFAULT_LIMIT: usize = 2000;

/// 远端清单的缓存：那台的名字 ⇒ 界面交进来的那台 `raw` 的回答。搜索框每敲一下都问一次，不能每次都再交一整份；
/// 不按时间过期（这一层不看钟）：界面开页与「刷新」再问那台、带着新的 `listing` 来，那时才换。
static REMOTE_CACHE: std::sync::Mutex<BTreeMap<String, Listing>> =
    std::sync::Mutex::new(BTreeMap::new());

// ───────────────────────── 这台自己的清单（`raw`）─────────────────────────

/// 这台此刻活着的会话（pidfile 源头）。
pub(crate) struct LiveSet(pub(crate) BTreeSet<String>);

/// 一台自己的清单（`raw` 那一形，也是远端那一支带回来的样子）：行 · 读不了的记录目录 · 那台自己的注解（或读不到它的那一句）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Listing {
    pub(crate) rows: Vec<RawRow>,
    pub(crate) failed: Vec<FailedDir>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) annotations: Option<Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) annotations_notice: Option<String>,
}

/// 读不了的一个记录目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FailedDir {
    pub(crate) project_dir: String,
    pub(crate) error: String,
}

/// 清单里的一行（这台判过活、注解格先按没有填）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawRow {
    pub(crate) agent: String,
    pub(crate) session_id: String,
    pub(crate) project_path: String,
    pub(crate) project_name: String,
    pub(crate) ai_title: Option<String>,
    pub(crate) first_user_excerpt: String,
    /// 显示标题（用户改过的另在 `customTitle`）：与全文搜索同一条规则。
    pub(crate) title: String,
    pub(crate) started_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) jsonl_path: String,
    /// 判不了活（合成历史没有 pidfile）⇒ `null`。
    pub(crate) is_live: Option<bool>,
    pub(crate) message_count_approx: u32,
    pub(crate) is_bg: bool,
    pub(crate) starred: bool,
    pub(crate) custom_title: Option<String>,
    pub(crate) hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) forked_from_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) forked_from_message_uuid: Option<String>,
    /// 读它那一组用的键（记录目录名 · 合成历史 `<kind>:<cwd>`）。
    pub(crate) project_dir: String,
    /// 分组键：哪一家 ＋ 真实目录。
    pub(crate) group: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_account: Option<String>,
}

/// `history-list` 的成品。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct HistoryList<'o> {
    pub(crate) rows: Vec<HistoryRow<'o>>,
    pub(crate) groups: Vec<HistoryGroup<'o>>,
    pub(crate) total: usize,
    pub(crate) truncated: bool,
    /// 注解没并上的那一句；并上了 ⇒ `null`。
    pub(crate) notice: Option<String>,
}

/// 成品的一行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryRow<'o> {
    pub(crate) agent: String,
    pub(crate) session_id: String,
    pub(crate) project_path: String,
    pub(crate) project_name: String,
    pub(crate) ai_title: Option<String>,
    pub(crate) first_user_excerpt: String,
    pub(crate) title: String,
    pub(crate) started_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) jsonl_path: String,
    pub(crate) message_count_approx: u32,
    pub(crate) is_bg: bool,
    pub(crate) starred: bool,
    pub(crate) custom_title: Option<String>,
    pub(crate) hidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) forked_from_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) forked_from_message_uuid: Option<String>,
    pub(crate) project_dir: String,
    pub(crate) group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_account: Option<String>,
    /// 显示的标题：用户改过的 ＞ 标题 ＞ 第一句。
    pub(crate) label: Words,
    /// 改过的标题 · 标题 · 第一句都没有（界面写「没有说过话的会话」）。
    pub(crate) untitled: bool,
    /// `live` · `ended` · `unknown`（活不活只出这一格）。
    pub(crate) status: &'static str,
    pub(crate) can: Can,
    /// 行上那一家的小牌（对用户的叫法）；默认那一家 ⇒ `null`。
    pub(crate) agent_tag: Option<Words>,
    /// 被筛掉、只为挂住分叉子会话才带上的父会话（界面淡显）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) context: Option<bool>,
    /// 排序键（`activity` ⇒ 最后活动，`created` ⇒ 开始；毫秒）。
    pub(crate) at: i64,
    pub(crate) at_text: Words,
    pub(crate) section_text: Words,
    pub(crate) span_text: Words,
    /// 问的是哪一台（与入参 `origin` 同一个机器名；这台 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) origin: Option<&'o str>,
}

/// 按项目看时的一组。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryGroup<'o> {
    pub(crate) key: String,
    pub(crate) agent: String,
    pub(crate) project_name: String,
    pub(crate) project_path: String,
    pub(crate) project_dir: String,
    pub(crate) count: u64,
    /// 有在跑的 ⇒ `true`；有答不了的 ⇒ `null`；都没在跑 ⇒ `false`。
    pub(crate) has_live: Option<bool>,
    pub(crate) starred: bool,
    pub(crate) last_activity: i64,
    /// 几台的组并成一列时按它（大的在前）。
    pub(crate) order: i64,
    /// 读不了的那个记录目录：原因。
    pub(crate) failed: Option<String>,
    /// 问的是哪一台（与入参 `origin` 同一个机器名；这台 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) origin: Option<&'o str>,
}

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

/// **这台的注解并进它自己那份清单**（`raw` 那一形也带）：读到了 ⇒ `annotations` ＝ 那张表（sid ⇒ 注解）；读不到 ⇒
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

/// 记录树里的一行（`--list-sessions` 那一形）＋ 判活 ⇒ 清单的一行（注解格先按没有填，并注解在 [`answer_from`]）。
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
        title: crate::observe::search_rules::session_title(v["aiTitle"].as_str(), excerpt, sid),
        started_at: v["startedAtMs"].as_i64().unwrap_or(0),
        updated_at: v["updatedAtMs"].as_i64().unwrap_or(0),
        jsonl_path: v["jsonlPath"].as_str().unwrap_or_default().to_string(),
        is_live: Some(live.0.contains(sid)),
        message_count_approx: v["messageCountApprox"].as_u64().unwrap_or(0) as u32,
        is_bg: v["isBg"].as_bool().unwrap_or(false),
        starred: false,
        custom_title: None,
        hidden: false,
        forked_from_session_id: fork.as_ref().map(|f| f.0.clone()),
        forked_from_message_uuid: fork.map(|f| f.1),
        project_dir: project_dir.to_string(),
        last_account: None,
    })
}

/// 合成历史的一条会话 ⇒ 清单的一行（判不了活 ⇒ `isLive: null`）。
fn synth_row(kind: &str, s: &crate::agents::SynthSession, excerpt: String) -> RawRow {
    let name = last_segment(&s.cwd).map_or_else(|| format!("({kind})"), str::to_string);
    let title = crate::observe::search_rules::session_title(None, &excerpt, &s.sid);
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
        is_live: None,
        message_count_approx: 0,
        is_bg: false,
        starred: false,
        custom_title: None,
        hidden: false,
        forked_from_session_id: None,
        forked_from_message_uuid: None,
        project_dir: format!("{kind}:{}", s.cwd),
        group: group_key(kind, &s.cwd),
        last_account: None,
    }
}

/// 一行的分组键：哪一家 ＋ 真实目录（记录目录名会撞）。
fn group_key(agent: &str, project_path: &str) -> String {
    format!("{agent}:{project_path}")
}

/// 这台的平铺清单（并上这台的注解）：记录树每个目录 ＋ 合成历史各家；判活 ＝ 这台 pidfile；上次的号 ＝ 这台的起会话账号记录。
/// `last` 读不懂 ⇒ 那一格都不带（界面不说「上次用的」，照样能恢复）。
pub(crate) fn machine_listing() -> Result<Listing, (&'static str, String)> {
    machine_listing_for(None)
}

/// [`machine_listing`]；`only` ＝ 按 sid 问的那一条：记录树里只整份扫叫这个名字的那一份（别的会话只出分组要的几格，
/// [`answer_from`] 按 sid 滤掉）⇒ 答案与整台扫逐字相同，不必把整台每份会话从头扫一遍。
fn machine_listing_for(only: Option<&str>) -> Result<Listing, (&'static str, String)> {
    let home = crate::observe::history_query::agent_home();
    let live = LiveSet(crate::observe::accounts_query::live_session_ids(&home));
    let tree = crate::observe::history_query::sessions_by_dir_for(&home, only)
        .map_err(|e| ("failed", e))?;
    let synth: Vec<(
        &'static str,
        Vec<crate::agents::SynthSession>,
        fn(&std::path::Path) -> String,
    )> = crate::agents::history_faces()
        .into_iter()
        .map(|(k, f)| (k, (f.sessions)(), f.excerpt))
        .collect();
    let last = crate::control::launch_account::answer_last_accounts()
        .ok()
        .and_then(|v| v.get("accounts").cloned())
        .unwrap_or(Value::Null);
    let mut listing = listing_from(tree.unwrap_or_default(), &synth, &live, &last);
    with_own_annotations(&mut listing, &crate::history::history_annotations::load());
    Ok(listing)
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
    }
}

// ───────────────────────── 成品 ─────────────────────────

/// 入参（帧面那几格）。
#[derive(Debug, Default)]
pub(crate) struct Ask {
    /// 只要这一个会话（别的筛不看）。
    pub sid: Option<String>,
    pub query: Option<String>,
    pub by_created: bool,
    pub within_ms: Option<i64>,
    pub hidden: bool,
    pub limit: usize,
}

fn bad(what: &str) -> (&'static str, String) {
    ("bad_args", crate::common::contract::malformed(what))
}

pub(crate) fn parse_ask(args: &Value) -> Result<Ask, (&'static str, String)> {
    // 会话 ID 的形状与 `history-record` 同一条（`[A-Za-z0-9-]`，1..=64），先于任何 IO。
    let sid = match args.get("sid") {
        None | Some(Value::Null) => None,
        Some(Value::String(s))
            if (1..=64).contains(&s.len())
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') =>
        {
            Some(s.clone())
        }
        Some(_) => return Err(bad("`sid` must be a session id ([A-Za-z0-9-], 1..=64)")),
    };
    let query = match args.get("query") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.trim().to_lowercase()).filter(|s| !s.is_empty()),
        Some(_) => return Err(bad("`query` must be a string")),
    };
    let by_created = match args.get("sort").map(|v| v.as_str()) {
        None | Some(Some("activity")) => false,
        Some(Some("created")) => true,
        _ => return Err(bad("`sort` must be `activity` or `created`")),
    };
    let within_ms = match args.get("within_days") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_u64() {
            Some(d @ 1..=3650) => Some(d as i64 * 86_400_000),
            _ => return Err(bad("`within_days` must be 1..=3650")),
        },
    };
    let flag = |k: &str| match args.get(k) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(bad(&format!("`{k}` must be a boolean"))),
    };
    let limit = match args.get("limit") {
        None | Some(Value::Null) => DEFAULT_LIMIT,
        Some(v) => match v.as_u64() {
            Some(n @ 1..=20_000) => n as usize,
            _ => return Err(bad("`limit` must be 1..=20000")),
        },
    };
    Ok(Ask {
        sid,
        query,
        by_created,
        within_ms,
        hidden: flag("hidden")?,
        limit,
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
    status: &'static str,
}

/// 并上这台的注解（那一台的行也并这台的：注解只住本机），判 `status` · `label` · `untitled`。
fn finish_row(mut raw: RawRow, ann: Option<&Table>) -> Finished {
    let meta = ann
        .and_then(|t| t.get(&raw.session_id))
        .cloned()
        .unwrap_or_default();
    raw.starred = meta.starred;
    raw.custom_title = meta.custom_title.clone();
    raw.hidden = meta.hidden;
    // 显示的标题：用户改过的 ＞ 标题 ＞ 第一句；都没有 ⇒ `untitled`（界面写「没有说过话的会话」）。
    let untitled = meta.custom_title.is_none()
        && raw.ai_title.as_deref().is_none_or(str::is_empty)
        && raw.first_user_excerpt.is_empty();
    let label = meta.custom_title.unwrap_or_else(|| raw.title.clone());
    let status = status_of(raw.is_live);
    Finished {
        raw,
        label,
        untitled,
        status,
    }
}

/// 搜索词命中：显示标题 · 第一句 · 项目名（不分大小写，子串）。
fn hits(f: &Finished, q: &str) -> bool {
    [&f.label, &f.raw.first_user_excerpt, &f.raw.project_name]
        .iter()
        .any(|s| s.to_lowercase().contains(q))
}

/// 一行成品（活不活只出 `status` 一格：`isLive` 是它的来路，两格并存读的人就得猜听哪一格）。
fn row_of<'o>(f: Finished, at: i64, context: bool, now_ms: i64, tz: &crate::Tz) -> HistoryRow<'o> {
    let r = f.raw;
    let (at_text, section_text, span_text) =
        crate::common::time::history_times(at, r.started_at, r.updated_at, now_ms, tz);
    HistoryRow {
        can: can_of(&r.agent, f.status, r.is_bg),
        agent_tag: agent_tag(&r.agent).map(Words::from),
        label: Words(f.label),
        untitled: f.untitled,
        status: f.status,
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
        hidden: r.hidden,
        forked_from_session_id: r.forked_from_session_id,
        forked_from_message_uuid: r.forked_from_message_uuid,
        project_dir: r.project_dir,
        group: r.group,
        last_account: r.last_account,
    }
}

/// 一台的清单（`raw` 那一形）＋ 注解 ＋ 入参 ⇒ 成品。`origin` ＝ 问的是哪一台（缺 ＝ 这台）。`now_ms` 由调用方给（时间筛的「现在」）；
/// `tz` ＝ 看的那一台的时区（请求信封带来的）：每行的行尾 · 分段 · 时间段三格按它写好（偏移按各自那一刻）。
pub(crate) fn answer_from<'o>(
    listing: &Listing,
    origin: Option<&'o str>,
    ann: Result<&Table, String>,
    ask: &Ask,
    now_ms: i64,
    tz: &crate::Tz,
) -> HistoryList<'o> {
    let t = ann.as_ref().ok().copied();
    let all: Vec<Finished> = listing
        .rows
        .iter()
        .cloned()
        .map(|r| finish_row(r, t))
        .collect();
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
        (ask.hidden || !f.raw.hidden)
            && ask.within_ms.is_none_or(|w| key(f) >= now_ms - w)
            && ask.query.as_deref().is_none_or(|q| hits(f, q))
    };
    let (mut out, rest): (Vec<Finished>, Vec<Finished>) = all.into_iter().partition(|f| kept(f));
    let total = out.len();
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
    let mut rows: Vec<HistoryRow<'o>> = out
        .into_iter()
        .zip(context)
        .map(|(f, cx)| {
            let at = key(&f);
            row_of(f, at, cx, now_ms, tz)
        })
        .collect();
    rows.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    let truncated = rows.len() > ask.limit;
    rows.truncate(ask.limit);
    let mut groups = groups_of(&rows, listing);
    // 问的是哪一台：每行每组都带上（界面把几台并成一列时认得出是哪台的）。
    if let Some(o) = origin {
        rows.iter_mut().for_each(|r| r.origin = Some(o));
        groups.iter_mut().for_each(|g| g.origin = Some(o));
    }
    HistoryList {
        rows,
        groups,
        total,
        truncated,
        notice: ann.err(),
    }
}

/// 一组有没有在跑的：档位（有在跑的 2 · 说不清 1 · 都没在跑 0）。
fn live_rank(g: &HistoryGroup<'_>) -> i64 {
    match g.has_live {
        Some(true) => 2,
        None => 1,
        Some(false) => 0,
    }
}

/// 按项目看时的分组：在列的那几行按 `group` 归，每组 {key · 那一家 · 名字 · 目录 · 读它的键 · 几个 · 有没有在跑的 · 最后动过 · 机器}；
/// 读不了的那几个记录目录也是一组（`failed` 带原因）。排序：有在跑的 → 有星标的 → 最近动过的。
fn groups_of<'o>(rows: &[HistoryRow<'o>], listing: &Listing) -> Vec<HistoryGroup<'o>> {
    let mut by: BTreeMap<String, HistoryGroup<'o>> = BTreeMap::new();
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
            order: 0,
            failed: None,
            origin: None,
        });
        g.count += 1;
        // 有在跑的：确定有一个就够；有答不了的就不许说「都没在跑」（同项目清单）。
        match v.status {
            "live" => g.has_live = Some(true),
            "unknown" if g.has_live != Some(true) => g.has_live = None,
            _ => {}
        }
        g.starred |= v.starred;
        g.last_activity = g.last_activity.max(v.updated_at);
    }
    let mut out: Vec<HistoryGroup<'o>> = by.into_values().collect();
    // 有在跑的 ＞ 说不清的 ＞ 都没在跑（同项目清单）。
    out.sort_by(|a, b| {
        let rank = |g: &HistoryGroup<'_>| (live_rank(g), g.starred, g.last_activity);
        rank(b).cmp(&rank(a)).then_with(|| a.key.cmp(&b.key))
    });
    // 各台各自排好，界面要把几台的组并成一列 ⇒ 把这个序压成一个数交出去（`order` 大的在前；界面只按它并，不另判）：
    //   档位 × 10^14 ＋ 有星标 × 10^13 ＋ 最后动过（毫秒，< 10^13）。
    for g in &mut out {
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
            order: 0,
            failed: Some(f.error.clone()),
            origin: None,
        });
    }
    out
}

/// 格目录与出参对拍的样本：每个可缺的格都填上、每个列表都不空。
pub(crate) fn specimen() -> HistoryList<'static> {
    let s = |v: &str| v.to_string();
    let w = |v: &str| Words(v.to_string());
    HistoryList {
        rows: vec![HistoryRow {
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
            hidden: false,
            forked_from_session_id: Some(s("f")),
            forked_from_message_uuid: Some(s("u")),
            project_dir: s("-w"),
            group: s("agent:/w"),
            last_account: Some(s("a")),
            label: w("c"),
            untitled: false,
            status: "ended",
            can: can_of("agent", "ended", false),
            agent_tag: Some(w("g")),
            context: Some(true),
            at: 2,
            at_text: w("t"),
            section_text: w("s"),
            span_text: w("p"),
            origin: Some("m"),
        }],
        groups: vec![HistoryGroup {
            key: s("agent:/w"),
            agent: s("agent"),
            project_name: s("w"),
            project_path: s("/w"),
            project_dir: s("-w"),
            count: 1,
            has_live: Some(false),
            starred: false,
            last_activity: 2,
            order: 2,
            failed: Some(s("x")),
            origin: Some("m"),
        }],
        total: 1,
        truncated: false,
        notice: Some(s("n")),
    }
}

// ───────────────────────── 帧面 ─────────────────────────

fn now_ms() -> i64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

async fn blocking<T: Send + 'static>(
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

/// 帧面 `history-list`（`tz` ＝ 看的那一台的时区：行上那几格时刻字按它写）。
pub async fn answer(args: Value, tz: crate::Tz) -> Result<Value, (&'static str, String)> {
    if args.get("raw").and_then(Value::as_bool) == Some(true) {
        return blocking(|| {
            machine_listing().map(|l| serde_json::to_value(l).unwrap_or(Value::Null))
        })
        .await;
    }
    let ask = parse_ask(&args)?;
    let origin = match args.get("origin") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(_) => return Err(bad("`origin` must be a non-empty string")),
    };
    let handed = match args.get("listing") {
        None | Some(Value::Null) => None,
        Some(v) => Some(serde_json::from_value::<Listing>(v.clone()).map_err(|_| {
            bad("`listing` must be a machine's raw listing `{rows, failed, annotations | annotationsNotice}`")
        })?),
    };
    let listing = match (&origin, handed) {
        (None, None) => {
            let only = ask.sid.clone();
            blocking(move || machine_listing_for(only.as_deref())).await?
        }
        (None, Some(_)) => return Err(bad("`listing` needs the `origin` it came from")),
        (Some(o), handed) => remote_listing(o, handed)?,
    };
    blocking(move || {
        // 并的是清单自己带来的那台的注解（本机的那份只并本机会话；远端那台自己并好了它那份）。
        let ann = listing_annotations(&listing);
        let got = answer_from(
            &listing,
            origin.as_deref(),
            ann.as_ref().map_err(Clone::clone),
            &ask,
            now_ms(),
            &tz,
        );
        Ok(serde_json::to_value(got).unwrap_or(Value::Null))
    })
    .await
}

/// 那一台的 `raw` 清单：交进来了 ⇒ 记下、就用它；没交 ⇒ 记着的那份；都没有 ⇒ `no_listing`。
fn remote_listing(
    machine: &str,
    handed: Option<Listing>,
) -> Result<Listing, (&'static str, String)> {
    let mut held = REMOTE_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = handed {
        held.insert(machine.to_string(), v.clone());
        return Ok(v);
    }
    held.get(machine).cloned().ok_or_else(|| {
        (
            "no_listing",
            copy_text("beHistoryList.remote.notHeld", &[("machine", machine)]),
        )
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/history/history_list_tests.rs"]
mod tests;
