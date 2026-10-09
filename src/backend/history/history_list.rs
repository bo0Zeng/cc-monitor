//! **历史页的平铺会话清单**（帧面 `history-list`）—— 跨项目、按机器问，一次出成品：
//! 每行带显示标题 · 项目与真实目录 · 那一家 · 状态 · 注解 · 分叉 · 分身 · 条数 · 时间 ·「这一行能做什么」，
//! 外加按项目看时的分组（按真实目录分，读不了的那一组照样出、带原因）。
//!
//! # 形状
//!
//! ```text
//! history-list {origin?, raw?, fresh?, sid?, query?, sort?, within_days?, hidden?, limit?}
//!   raw: true      这台自己的清单（不并注解、不筛不排）——远端那一支问的就是它（那台的 CLI 面 `--history-list`）
//!   origin 缺席    这台；给了 = 可达表里那一台：问那台 `raw`（它自己判活、自己读上次的号），结果在本进程记着、`fresh` 才再问
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
use serde_json::{json, Map, Value};

use crate::history::history_annotations::{Loaded, Table};
use crate::observe::history_query::{can_of, status_of};

/// 默认最多回多少行（按时间看时超过它只列最近这些、底部「更早的用搜索找」）。
const DEFAULT_LIMIT: usize = 2000;

/// 远端清单的缓存：那台的名字 ⇒ 那台 `raw` 的回答。搜索框每敲一下都问一次，不能每次都去那台整份扫；
/// 不按时间过期（这一层不看钟）：界面开页与「刷新」带 `fresh`，那时才再问那台。
static REMOTE_CACHE: std::sync::Mutex<BTreeMap<String, Value>> =
    std::sync::Mutex::new(BTreeMap::new());

// ───────────────────────── 这台自己的清单（`raw`）─────────────────────────

/// 这台此刻活着的会话（pidfile 源头）。
pub(crate) struct LiveSet(pub(crate) BTreeSet<String>);

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

/// 路径最后一段（`/` 与 `\` 都认）；空 ⇒ `None`。
fn last_segment(p: &str) -> Option<&str> {
    p.rsplit(['/', '\\']).next().filter(|s| !s.is_empty())
}

/// 记录树里的一行（`--list-sessions` 那一形）＋ 判活 ⇒ 清单的一行（注解格先按没有填，并注解在 [`answer_from`]）。
/// 没有 `sessionId` ⇒ `None`。
fn session_row(v: &Value, project_dir: &str, live: &LiveSet) -> Option<Value> {
    let sid = v["sessionId"].as_str().filter(|s| !s.is_empty())?;
    let cwd = v["cwd"].as_str().unwrap_or_default();
    let excerpt = v["firstUserExcerpt"].as_str().unwrap_or_default();
    let mut o = Map::new();
    o.insert(
        "agent".into(),
        json!(crate::agents::record_tree_kind().unwrap_or_default()),
    );
    o.insert("sessionId".into(), json!(sid));
    o.insert("projectPath".into(), json!(cwd));
    o.insert(
        "projectName".into(),
        json!(last_segment(cwd).unwrap_or(project_dir)),
    );
    o.insert(
        "aiTitle".into(),
        v["aiTitle"].as_str().map_or(Value::Null, |s| json!(s)),
    );
    o.insert("firstUserExcerpt".into(), json!(excerpt));
    // 显示标题（用户改过的另在 `customTitle`，界面先看它）：与全文搜索同一条规则。
    o.insert(
        "title".into(),
        json!(crate::observe::search_rules::session_title(
            v["aiTitle"].as_str(),
            excerpt,
            sid
        )),
    );
    o.insert(
        "startedAt".into(),
        json!(v["startedAtMs"].as_i64().unwrap_or(0)),
    );
    o.insert(
        "updatedAt".into(),
        json!(v["updatedAtMs"].as_i64().unwrap_or(0)),
    );
    o.insert(
        "jsonlPath".into(),
        json!(v["jsonlPath"].as_str().unwrap_or_default()),
    );
    o.insert("isLive".into(), json!(live.0.contains(sid)));
    o.insert(
        "messageCountApprox".into(),
        json!(v["messageCountApprox"].as_u64().unwrap_or(0) as u32),
    );
    o.insert("isBg".into(), json!(v["isBg"].as_bool().unwrap_or(false)));
    o.insert("starred".into(), json!(false));
    o.insert("customTitle".into(), Value::Null);
    o.insert("hidden".into(), json!(false));
    // 分叉关系：有才带。
    if let (Some(s), Some(u)) = (
        v["forkedFromSessionId"].as_str(),
        v["forkedFromMessageUuid"].as_str(),
    ) {
        o.insert("forkedFromSessionId".into(), json!(s));
        o.insert("forkedFromMessageUuid".into(), json!(u));
    }
    Some(Value::Object(o))
}

/// 合成历史的一条会话 ⇒ 清单的一行（判不了活 ⇒ `isLive: null`）。
fn synth_row(kind: &str, s: &crate::agents::SynthSession, excerpt: String) -> Value {
    let name = last_segment(&s.cwd).map_or_else(|| format!("({kind})"), str::to_string);
    let title = crate::observe::search_rules::session_title(None, &excerpt, &s.sid);
    json!({
        "agent": kind,
        "sessionId": s.sid,
        "projectPath": s.cwd,
        "projectName": name,
        "aiTitle": null,
        "firstUserExcerpt": excerpt,
        "title": title,
        "startedAt": s.mtime_ms,
        "updatedAt": s.mtime_ms,
        "jsonlPath": s.path.to_string_lossy(),
        "isLive": null,
        "messageCountApprox": 0,
        "isBg": false,
        "starred": false,
        "customTitle": null,
        "hidden": false,
    })
}

/// 一行的分组键：哪一家 ＋ 真实目录（记录目录名会撞）。
fn group_key(agent: &str, project_path: &str) -> String {
    format!("{agent}:{project_path}")
}

/// 这台的平铺清单（不并注解）：记录树每个目录 ＋ 合成历史各家；判活 ＝ 这台 pidfile；上次的号 ＝ 这台的起会话账号记录。
/// `last` 读不懂 ⇒ 那一格都不带（界面不说「上次用的」，照样能恢复）。
pub(crate) fn machine_listing() -> Result<Value, (&'static str, String)> {
    machine_listing_for(None)
}

/// [`machine_listing`]；`only` ＝ 按 sid 问的那一条：记录树里只整份扫叫这个名字的那一份（别的会话只出分组要的几格，
/// [`answer_from`] 按 sid 滤掉）⇒ 答案与整台扫逐字相同，不必把整台每份会话从头扫一遍。
fn machine_listing_for(only: Option<&str>) -> Result<Value, (&'static str, String)> {
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
    Ok(listing_from(tree.unwrap_or_default(), &synth, &live, &last))
}

/// [`machine_listing`] 的可喂夹具那一半（记录树分好的目录 · 合成历史 · 判活 · 上次的号由调用方给）。
pub(crate) fn listing_from(
    tree: Vec<(String, Result<Vec<Value>, String>)>,
    synth: &[(
        &'static str,
        Vec<crate::agents::SynthSession>,
        fn(&std::path::Path) -> String,
    )],
    live: &LiveSet,
    last: &Value,
) -> Value {
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    for (dir, got) in tree {
        match got {
            Ok(list) => rows.extend(
                list.iter()
                    .filter_map(|v| session_row(v, &dir, live))
                    .map(|v| with_group(v, &dir)),
            ),
            Err(why) => failed.push(json!({ "projectDir": dir, "error": why })),
        }
    }
    for (kind, sessions, excerpt) in synth {
        for s in sessions {
            let dir = format!("{kind}:{}", s.cwd);
            rows.push(with_group(synth_row(kind, s, excerpt(&s.path)), &dir));
        }
    }
    for r in &mut rows {
        if let Some(name) = r["sessionId"]
            .as_str()
            .and_then(|sid| last.get(sid))
            .and_then(Value::as_str)
        {
            r["lastAccount"] = json!(name);
        }
    }
    json!({ "rows": rows, "failed": failed })
}

/// 给一行补上 `projectDir`（读它那一组用的键）与 `group`（分组键）。
fn with_group(mut v: Value, dir: &str) -> Value {
    let key = group_key(
        v["agent"].as_str().unwrap_or_default(),
        v["projectPath"].as_str().unwrap_or_default(),
    );
    v["projectDir"] = json!(dir);
    v["group"] = json!(key);
    v
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

/// 并上这台的注解（那一台的行也并这台的：注解只住本机），补 `status` · `label` · `untitled` · `can`。
fn finish_row(mut v: Value, ann: Option<&Table>) -> Value {
    let sid = v["sessionId"].as_str().unwrap_or_default().to_string();
    let meta = ann.and_then(|t| t.get(&sid)).cloned().unwrap_or_default();
    v["starred"] = json!(meta.starred);
    v["customTitle"] = json!(meta.custom_title);
    v["hidden"] = json!(meta.hidden);
    // 显示的标题：用户改过的 ＞ 标题 ＞ 第一句；都没有 ⇒ `untitled`（界面写「没有说过话的会话」）。
    let untitled = meta.custom_title.is_none()
        && v["aiTitle"].as_str().is_none_or(str::is_empty)
        && v["firstUserExcerpt"].as_str().is_none_or(str::is_empty);
    let label = meta
        .custom_title
        .clone()
        .unwrap_or_else(|| v["title"].as_str().unwrap_or_default().to_string());
    v["label"] = json!(label);
    v["untitled"] = json!(untitled);
    let status = status_of(v["isLive"].as_bool());
    v["status"] = json!(status);
    v["can"] = can_of(
        v["agent"].as_str().unwrap_or_default(),
        status,
        v["isBg"].as_bool().unwrap_or(false),
    );
    v["agentTag"] =
        agent_tag(v["agent"].as_str().unwrap_or_default()).map_or(Value::Null, |t| json!(t));
    // 活不活只出 `status` 一格（`isLive` 是它的来路；两格并存，读的人就得猜听哪一格）。
    if let Some(o) = v.as_object_mut() {
        o.remove("isLive");
    }
    v
}

/// 搜索词命中：显示标题 · 第一句 · 项目名（不分大小写，子串）。
fn hits(v: &Value, q: &str) -> bool {
    ["label", "firstUserExcerpt", "projectName"]
        .iter()
        .any(|k| v[*k].as_str().is_some_and(|s| s.to_lowercase().contains(q)))
}

/// 一台的清单（`raw` 那一形，远端的已补 `origin`）＋ 注解 ＋ 入参 ⇒ 成品。`now_ms` 由调用方给（时间筛的「现在」）；
/// `local` 把 unix 秒按那一刻的偏移排成本地钟秒数（生产里是这台的 [`crate::common::time::local_secs`]）：每行的行尾 · 分段 · 时间段三格按它写好。
pub(crate) fn answer_from(
    listing: &Value,
    origin: Option<&str>,
    ann: Result<&Table, String>,
    ask: &Ask,
    now_ms: i64,
    local: &dyn Fn(i64) -> i64,
) -> Value {
    let t = ann.as_ref().ok().copied();
    let all: Vec<Value> = listing["rows"]
        .as_array()
        .map(|a| a.iter().cloned().map(|v| finish_row(v, t)).collect())
        .unwrap_or_default();
    let key = |v: &Value| {
        let k = if ask.by_created {
            "startedAt"
        } else {
            "updatedAt"
        };
        v[k].as_i64().unwrap_or(0)
    };
    let kept = |v: &Value| {
        if let Some(sid) = ask.sid.as_deref() {
            return v["sessionId"].as_str() == Some(sid);
        }
        (ask.hidden || !v["hidden"].as_bool().unwrap_or(false))
            && ask.within_ms.is_none_or(|w| key(v) >= now_ms - w)
            && ask.query.as_deref().is_none_or(|q| hits(v, q))
    };
    let by_sid: BTreeMap<&str, &Value> = all
        .iter()
        .filter_map(|v| Some((v["sessionId"].as_str()?, v)))
        .collect();
    let mut out: Vec<Value> = all.iter().filter(|v| kept(v)).cloned().collect();
    let total = out.len();
    // 分叉的父会话被隐藏 / 被筛掉 ⇒ 照样带上、标 `context`（界面淡显，子会话照样挂在它名下）。
    let present: BTreeSet<String> = out
        .iter()
        .filter_map(|v| v["sessionId"].as_str().map(str::to_string))
        .collect();
    let mut parents = BTreeSet::new();
    // 按会话 ID 要一行 ⇒ 不补父会话。
    let forks: &[Value] = if ask.sid.is_some() { &[] } else { &out };
    for v in forks {
        if let Some(p) = v["forkedFromSessionId"].as_str() {
            if !present.contains(p) && by_sid.contains_key(p) {
                parents.insert(p.to_string());
            }
        }
    }
    for p in &parents {
        let mut v = (*by_sid[p.as_str()]).clone();
        v["context"] = json!(true);
        out.push(v);
    }
    for v in &mut out {
        v["at"] = json!(key(v));
        crate::common::time::history_texts(v, now_ms, local);
    }
    out.sort_by(|a, b| {
        key(b)
            .cmp(&key(a))
            .then_with(|| a["sessionId"].as_str().cmp(&b["sessionId"].as_str()))
    });
    let truncated = out.len() > ask.limit;
    out.truncate(ask.limit);
    let groups = groups_of(&out, listing, origin);
    if let Some(o) = origin {
        for v in &mut out {
            v["origin"] = json!(o);
        }
    }
    json!({
        "rows": out,
        "groups": groups,
        "total": total,
        "truncated": truncated,
        "notice": ann.err(),
    })
}

/// 按项目看时的分组：在列的那几行按 `group` 归，每组 {key · 那一家 · 名字 · 目录 · 读它的键 · 几个 · 有没有在跑的 · 最后动过 · 机器}；
/// 读不了的那几个记录目录也是一组（`failed` 带原因）。排序：有在跑的 → 有星标的 → 最近动过的。
fn groups_of(rows: &[Value], listing: &Value, origin: Option<&str>) -> Vec<Value> {
    let mut by: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for v in rows.iter().filter(|v| v.get("context").is_none()) {
        let key = v["group"].as_str().unwrap_or_default().to_string();
        let g = by.entry(key.clone()).or_insert_with(|| {
            let mut g = Map::new();
            g.insert("key".into(), json!(key));
            g.insert("agent".into(), v["agent"].clone());
            g.insert("projectName".into(), v["projectName"].clone());
            g.insert("projectPath".into(), v["projectPath"].clone());
            g.insert("projectDir".into(), v["projectDir"].clone());
            g.insert("count".into(), json!(0));
            g.insert("hasLive".into(), json!(false));
            g.insert("starred".into(), json!(false));
            g.insert("lastActivity".into(), json!(0));
            g.insert("failed".into(), Value::Null);
            g
        });
        g["count"] = json!(g["count"].as_u64().unwrap_or(0) + 1);
        // 有在跑的：确定有一个就够；有答不了的就不许说「都没在跑」（同项目清单）。
        match v["status"].as_str() {
            Some("live") => g["hasLive"] = json!(true),
            Some("unknown") if g["hasLive"] != true => g["hasLive"] = Value::Null,
            _ => {}
        }
        if v["starred"] == true {
            g["starred"] = json!(true);
        }
        let at = v["updatedAt"].as_i64().unwrap_or(0);
        if at > g["lastActivity"].as_i64().unwrap_or(0) {
            g["lastActivity"] = json!(at);
        }
    }
    let mut out: Vec<Value> = by.into_values().map(Value::Object).collect();
    out.sort_by(|a, b| {
        // 有在跑的 ＞ 说不清的 ＞ 都没在跑（同项目清单）。
        let live = |g: &Value| match g["hasLive"].as_bool() {
            Some(true) => 2,
            None => 1,
            Some(false) => 0,
        };
        let rank = |g: &Value| {
            (
                live(g),
                g["starred"] == true,
                g["lastActivity"].as_i64().unwrap_or(0),
            )
        };
        rank(b)
            .cmp(&rank(a))
            .then_with(|| a["key"].as_str().cmp(&b["key"].as_str()))
    });
    // 各台各自排好，界面要把几台的组并成一列 ⇒ 把这个序压成一个数交出去（`order` 大的在前；界面只按它并，不另判）：
    //   档位（有在跑的 2 · 说不清 1 · 都没在跑 0）× 10^14 ＋ 有星标 × 10^13 ＋ 最后动过（毫秒，< 10^13）。
    for g in &mut out {
        let live = match g["hasLive"].as_bool() {
            Some(true) => 2,
            None => 1,
            Some(false) => 0,
        };
        let star = i64::from(g["starred"] == true);
        let last = g["lastActivity"]
            .as_i64()
            .unwrap_or(0)
            .clamp(0, 9_999_999_999_999);
        g["order"] = json!(live * 100_000_000_000_000 + star * 10_000_000_000_000 + last);
    }
    for f in listing["failed"].as_array().into_iter().flatten() {
        let dir = f["projectDir"].as_str().unwrap_or_default();
        out.push(json!({
            "key": format!("dir:{dir}"),
            "agent": crate::agents::record_tree_kind().unwrap_or_default(),
            "projectName": dir,
            "projectPath": "",
            "projectDir": dir,
            "count": 0,
            "hasLive": null,
            "starred": false,
            "lastActivity": 0,
            "order": 0,
            "failed": f["error"],
        }));
    }
    if let Some(o) = origin {
        for g in &mut out {
            g["origin"] = json!(o);
        }
    }
    out
}

// ───────────────────────── 帧面 ─────────────────────────

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, (&'static str, String)> + Send + 'static,
) -> Result<T, (&'static str, String)> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        (
            "failed",
            copy_text(
                "beHistoryJoin.blocking.unfinished",
                &[("e", &e.to_string())],
            ),
        )
    })?
}

/// 帧面 `history-list`（生产入口：进程里那张可达表 ＋ 真拨号）。
pub async fn answer(args: Value) -> Result<Value, (&'static str, String)> {
    answer_with(
        args,
        &crate::dial::remote_ask::REACH,
        &crate::dial::remote_ask::DialRemote,
    )
    .await
}

/// [`answer`] 的可喂夹具那一半。
pub async fn answer_with(
    args: Value,
    table: &crate::dial::remote_ask::Table,
    remote: &dyn crate::dial::remote_ask::Remote,
) -> Result<Value, (&'static str, String)> {
    if args.get("raw").and_then(Value::as_bool) == Some(true) {
        return blocking(machine_listing).await;
    }
    let ask = parse_ask(&args)?;
    let origin = match args.get("origin") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(_) => return Err(bad("`origin` must be a non-empty string")),
    };
    let fresh = args.get("fresh").and_then(Value::as_bool) == Some(true);
    let listing = match &origin {
        None => {
            let only = ask.sid.clone();
            blocking(move || machine_listing_for(only.as_deref())).await?
        }
        Some(o) => remote_listing(o, fresh, table, remote).await?,
    };
    blocking(move || {
        let loaded = crate::history::history_annotations::load();
        Ok(answer_from(
            &listing,
            origin.as_deref(),
            annotations(&loaded),
            &ask,
            now_ms(),
            &crate::common::time::local_secs,
        ))
    })
    .await
}

/// 那一台的 `raw` 清单：记着的就用（`fresh` 不用），否则问那台（`--history-list`，它自己判活、读上次的号）。
async fn remote_listing(
    machine: &str,
    fresh: bool,
    table: &crate::dial::remote_ask::Table,
    remote: &dyn crate::dial::remote_ask::Remote,
) -> Result<Value, (&'static str, String)> {
    let lock = || REMOTE_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if !fresh {
        if let Some(v) = lock().get(machine) {
            return Ok(v.clone());
        }
    }
    let v = crate::dial::remote_ask::ask_json(
        machine,
        "history-list",
        &json!({"raw": true}),
        table,
        remote,
    )
    .await
    .map_err(|s| ("unreachable", s.message))?;
    if !v["rows"].is_array() {
        return Err((
            "unreachable",
            copy_text("beHistoryList.remote.unreadable", &[("machine", machine)]),
        ));
    }
    lock().insert(machine.to_string(), v.clone());
    Ok(v)
}

#[cfg(test)]
#[path = "../../../tests/backend/history/history_list_tests.rs"]
mod tests;
