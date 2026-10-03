//! **历史跨机 join 的唯一的家** —— 项目 / 会话清单 ＋ 注解 ＋ 判活，在本机常驻后端里并成成品。
//!
//! # 裁决与出处
//!
//! 要求：「本机注解的**读写者**换成本机常驻后端 ——
//! 文件留在原处、同一路径，不迁移、一条不丢；本机后端经 `remote_ask` 问远端那台的项目 / 会话清单、并上注解、出成品；
//! 前端经 `chan.call`。codex 合成的项目与会话一起进后端（join 只一个家）」。`D1`：一个判定一个家。
//! 「观测方沿它本来就拥有的那条连接去拉被观测方」—— 远端那一跳是 [`crate::stream::remote_ask`]。
//!
//! # 形状
//!
//! ```text
//! history-projects {origin?}            origin 缺席 = 这台（本机常驻后端自己）；给了 = 可达表里的那一台
//!   这台：记录树（`--list-projects` 那一行）＋ 合成历史（注册表 `Adapter.history`：Codex）＋ 自己判活（pidfile）
//!   远端：remote_ask(origin, ["--list-projects"])（远端 CLI 老子命令，stdout 一个字节不变 ⇒ 远端不必升级）；
//! 判活再问那台一条 `--session-accounts`（那台后端答）；问不到 ⇒「不知道」
//!   ⇒ 并上这台的注解（`history_annotations`）⇒ {rows:[HistoryProject…], notice}
//! history-sessions {project_dir, origin?}  同上两支（`--list-sessions <dir>`；`<kind>:<cwd>` 那一形 = 合成历史）
//!   ⇒ {rows:[HistorySessionEntry…], notice}
//! ```
//!
//! 成品的逐格形状 == 从前 monitor 线上的 `HistoryProject` / `HistorySessionEntry`（camelCase；本机不带 `origin`、远端带那台的名字），
//! 界面消费代码不改语义；两侧对拍走跨语言金样 `tests/__fixtures__/history-products.golden.json`。
//!
//! # 「不知道」不许装成 0（`K-R83` / `K-R92` 从 monitor 搬来，判定一字不改）
//!
//! - 项目那三个数（星标数 · 隐藏数 · 有没有活会话）是 [`Counted`]：算得出 = `Known`（含真的是 0），算不出 = `Unknown(为什么)`，
//!   过线时 `Unknown` ⇒ `null`。远端那一行不带 sid 清单（老后端）/ 清单与条数对不上（行坏了）⇒ 三个数都「不知道」；
//! - 判活由 [`Liveness`] 答：这台 = pidfile 源头（`observe::accounts_query::live_session_ids`）；远端 = 那台 `--session-accounts`
//!   的 `alive`（问不到 ⇒「不知道」）；合成历史 = 「不知道」；
//! - 注解读不懂 / 没交路径 ⇒ 星标数 · 隐藏数「不知道」，`notice` 说一句为什么（从前 monitor 那份是当空、说成 0）。
//!
//! # 买不到
//!
//! - 🔴 真远端：`remote_ask` 那一跳（capture）没对真 sshd 跑过；远端这一支在判据里是替身对面。

use copy_core::copy_text;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::history::history_annotations::{Loaded, Table};

/// backend 那一行里装着**每项目会话 sid 清单**的字段名（`--list-projects`，`K-R83`）。
pub(crate) const SESSION_IDS_FIELD: &str = "sessionIds";

/// 一个数**算出来了没有**（`K-R83` / `K-R92`，从 monitor `remote_history.rs` 搬来）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Counted<T> {
    Known(T),
    Unknown(WhyUnknown),
}

/// 为什么算不出来 —— 每一档都说得出人话（进 `notice` 与日志）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhyUnknown {
    /// 那一行没带会话 sid 清单 —— 那台的后端太旧（`K-R83` 之前）。
    NoSessionIdList,
    /// 带了，但**清单长度与 `sessionCount` 对不上**：这一行坏了（不按拿得到的那几个算一个像真值的少数）。
    ListDisagreesWithCount,
    /// 「此刻活没活」这条路上答不了：远端（这台后端只认自己的 pidfile）· 合成历史（没有 pidfile）。
    NoLivenessOracle,
    /// 注解不可用（没交路径 / 读不懂）⇒ 星标数、隐藏数答不了。
    NoAnnotations,
}

impl WhyUnknown {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::NoSessionIdList => "那台的后端没带会话 id 清单（版本旧）",
            Self::ListDisagreesWithCount => "那一行的会话 id 清单与会话数对不上（行坏了）",
            Self::NoLivenessOracle => "这条路上答不了会话此刻活没活",
            Self::NoAnnotations => "星标 / 隐藏的记录读不到",
        }
    }
}

impl<T> Counted<T> {
    /// 过线：`Known(v)` ⇒ `Some(v)` · `Unknown(_)` ⇒ `None`（线上 `null`）。**这一步不丢那一维。**
    pub(crate) fn known(self) -> Option<T> {
        match self {
            Self::Known(v) => Some(v),
            Self::Unknown(_) => None,
        }
    }
}

/// 「这个会话此刻活没活」谁来答（`K-R92`：答不出的那一步是入参，判据喂得进真会答话的替身）。
pub(crate) trait Liveness {
    fn is_live(&self, sid: &str) -> Counted<bool>;
}

/// 这台机器：pidfile 源头答得出真值。
pub(crate) struct LiveSet(pub(crate) BTreeSet<String>);

impl Liveness for LiveSet {
    fn is_live(&self, sid: &str) -> Counted<bool> {
        Counted::Known(self.0.contains(sid))
    }
}

/// 答不了（远端 / 合成历史）—— 如实答「不知道」，不是 `false`。
pub(crate) struct NoLiveness;

impl Liveness for NoLiveness {
    fn is_live(&self, _sid: &str) -> Counted<bool> {
        Counted::Unknown(WhyUnknown::NoLivenessOracle)
    }
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

/// 一个项目那三个数的来路。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectCounts {
    pub starred: Counted<u32>,
    pub hidden: Counted<u32>,
    pub has_live: Counted<bool>,
}

/// 一组 sid ＋ 注解 ＋ 判活 ⇒ 三个数（汇总口径照 `K-R92`：确定活着一个就够；有答不出的就不许说「都没活」）。
pub(crate) fn counts_over(
    sids: &[&str],
    ann: Option<&Table>,
    live: &dyn Liveness,
) -> ProjectCounts {
    let (mut starred, mut hidden) = (0u32, 0u32);
    let mut has_live = Counted::Known(false);
    for sid in sids {
        if let Some(m) = ann.and_then(|t| t.get(*sid)) {
            starred += u32::from(m.starred);
            hidden += u32::from(m.hidden);
        }
        match live.is_live(sid) {
            Counted::Known(true) => has_live = Counted::Known(true),
            Counted::Known(false) => {}
            Counted::Unknown(w) => {
                if has_live != Counted::Known(true) {
                    has_live = Counted::Unknown(w)
                }
            }
        }
    }
    let ann_count = |n: u32| match ann {
        Some(_) => Counted::Known(n),
        None => Counted::Unknown(WhyUnknown::NoAnnotations),
    };
    ProjectCounts {
        starred: ann_count(starred),
        hidden: ann_count(hidden),
        has_live,
    }
}

/// `--list-projects` 的一行 ⇒ 那三个数（`K-R83`：sid 清单与 `sessionCount` 必须恒等长，否则「不知道」）。
pub(crate) fn project_counts(
    row: &Value,
    ann: Option<&Table>,
    live: &dyn Liveness,
) -> ProjectCounts {
    let all = |w| ProjectCounts {
        starred: Counted::Unknown(w),
        hidden: Counted::Unknown(w),
        has_live: Counted::Unknown(w),
    };
    let session_count = row["sessionCount"].as_u64().unwrap_or(0);
    let Some(ids) = row.get(SESSION_IDS_FIELD).and_then(Value::as_array) else {
        return all(WhyUnknown::NoSessionIdList);
    };
    let sids: Vec<&str> = ids.iter().filter_map(Value::as_str).collect();
    if sids.len() as u64 != session_count || sids.len() != ids.len() {
        return all(WhyUnknown::ListDisagreesWithCount);
    }
    counts_over(&sids, ann, live)
}

/// 把 `origin`（`Some` = 远端那台的名字）写进一行；本机那一行**不带**这一格（线上从前就是省略）。
fn with_origin(mut obj: Map<String, Value>, origin: Option<&str>) -> Value {
    if let Some(o) = origin {
        obj.insert("origin".into(), json!(o));
    }
    Value::Object(obj)
}

/// 路径最后一段（`/` 与 `\` 都认）；空 ⇒ `None`。
fn last_segment(p: &str) -> Option<&str> {
    p.rsplit(['/', '\\']).next().filter(|s| !s.is_empty())
}

/// 记录树那一支（本机 · 远端）的行是哪一家：注册表里记录树那一家。
fn record_tree_agent() -> &'static str {
    crate::agents::record_tree_kind().unwrap_or_default()
}

/// `--list-projects` 的一行 ＋ 注解 ＋ 判活 ⇒ **一条项目行**（`HistoryProject` 那一形）。没有 `dirName` ⇒ `None`（拿不到懒加载的键）。
pub(crate) fn project_from_row(
    v: &Value,
    ann: Option<&Table>,
    origin: Option<&str>,
    live: &dyn Liveness,
) -> Option<(Value, ProjectCounts)> {
    let dir_name = v["dirName"].as_str().filter(|s| !s.is_empty())?.to_string();
    let project_path = v["projectPath"].as_str().unwrap_or_default().to_string();
    let project_name = last_segment(&project_path).unwrap_or(&dir_name).to_string();
    let c = project_counts(v, ann, live);
    let mut o = Map::new();
    o.insert("agent".into(), json!(record_tree_agent()));
    o.insert("projectPath".into(), json!(project_path));
    o.insert("projectName".into(), json!(project_name));
    o.insert("projectDir".into(), json!(dir_name));
    o.insert(
        "sessionCount".into(),
        json!(v["sessionCount"].as_u64().unwrap_or(0) as u32),
    );
    o.insert("starredCount".into(), json!(c.starred.known()));
    o.insert("hiddenCount".into(), json!(c.hidden.known()));
    o.insert(
        "lastActivity".into(),
        json!(v["lastActivityMs"].as_i64().unwrap_or(0)),
    );
    o.insert("hasLive".into(), json!(c.has_live.known()));
    Some((with_origin(o, origin), c))
}

/// `--list-sessions` 的一行 ＋ 注解 ＋ 判活 ⇒ **一条会话行**（`HistorySessionEntry` 那一形）。没有 `sessionId` ⇒ `None`。
pub(crate) fn session_from_row(
    v: &Value,
    project_dir: &str,
    ann: Option<&Table>,
    origin: Option<&str>,
    live: &dyn Liveness,
) -> Option<Value> {
    let sid = v["sessionId"]
        .as_str()
        .filter(|s| !s.is_empty())?
        .to_string();
    let cwd = v["cwd"].as_str().unwrap_or_default().to_string();
    let project_name = last_segment(&cwd).unwrap_or(project_dir).to_string();
    let meta = ann.and_then(|t| t.get(&sid)).cloned().unwrap_or_default();
    let mut o = Map::new();
    o.insert("agent".into(), json!(record_tree_agent()));
    o.insert("sessionId".into(), json!(sid));
    o.insert("projectPath".into(), json!(cwd));
    o.insert("projectName".into(), json!(project_name));
    o.insert(
        "aiTitle".into(),
        v["aiTitle"].as_str().map_or(Value::Null, |s| json!(s)),
    );
    o.insert(
        "firstUserExcerpt".into(),
        json!(v["firstUserExcerpt"].as_str().unwrap_or_default()),
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
    o.insert("isLive".into(), json!(live.is_live(&sid).known()));
    o.insert(
        "messageCountApprox".into(),
        json!(v["messageCountApprox"].as_u64().unwrap_or(0) as u32),
    );
    o.insert("isBg".into(), json!(v["isBg"].as_bool().unwrap_or(false)));
    o.insert("starred".into(), json!(meta.starred));
    o.insert("customTitle".into(), json!(meta.custom_title));
    o.insert("hidden".into(), json!(meta.hidden));
    // fork 关系：有才带（线上从前就是省略）。
    if let (Some(s), Some(u)) = (
        v["forkedFromSessionId"].as_str(),
        v["forkedFromMessageUuid"].as_str(),
    ) {
        o.insert("forkedFromSessionId".into(), json!(s));
        o.insert("forkedFromMessageUuid".into(), json!(u));
    }
    Some(with_origin(o, origin))
}

/// 合成历史（`<kind>:<cwd>` 分组）的项目行。
pub(crate) fn synth_projects(
    kind: &str,
    sessions: &[crate::agents::SynthSession],
    ann: Option<&Table>,
) -> Vec<(Value, ProjectCounts)> {
    let mut groups: BTreeMap<&str, (Vec<&str>, i64)> = BTreeMap::new();
    for s in sessions {
        let g = groups.entry(s.cwd.as_str()).or_default();
        g.0.push(s.sid.as_str());
        g.1 = g.1.max(s.mtime_ms);
    }
    groups
        .into_iter()
        .map(|(cwd, (sids, last))| {
            let c = counts_over(&sids, ann, &NoLiveness);
            let name = last_segment(cwd).map_or_else(|| format!("({kind})"), str::to_string);
            let mut o = Map::new();
            o.insert("agent".into(), json!(kind));
            o.insert("projectPath".into(), json!(cwd));
            o.insert("projectName".into(), json!(name));
            o.insert("projectDir".into(), json!(format!("{kind}:{cwd}")));
            o.insert("sessionCount".into(), json!(sids.len() as u32));
            o.insert("starredCount".into(), json!(c.starred.known()));
            o.insert("hiddenCount".into(), json!(c.hidden.known()));
            o.insert("lastActivity".into(), json!(last));
            o.insert("hasLive".into(), json!(c.has_live.known()));
            (Value::Object(o), c)
        })
        .collect()
}

/// 合成历史的一条会话行。
pub(crate) fn synth_session(
    kind: &str,
    s: &crate::agents::SynthSession,
    excerpt: String,
    ann: Option<&Table>,
) -> Value {
    let meta = ann.and_then(|t| t.get(&s.sid)).cloned().unwrap_or_default();
    let name = last_segment(&s.cwd).map_or_else(|| format!("({kind})"), str::to_string);
    json!({
        "agent": kind,
        "sessionId": s.sid,
        "projectPath": s.cwd,
        "projectName": name,
        "aiTitle": null,
        "firstUserExcerpt": excerpt,
        "startedAt": s.mtime_ms,
        "updatedAt": s.mtime_ms,
        "jsonlPath": s.path.to_string_lossy(),
        "isLive": Counted::<bool>::Unknown(WhyUnknown::NoLivenessOracle).known(),
        "messageCountApprox": 0,
        "isBg": false,
        "starred": meta.starred,
        "customTitle": meta.custom_title,
        "hidden": meta.hidden,
    })
}

/// 档位（`K-R92`：确定有 > 不知道 > 确定没有）—— 与前端 `views/counted.ts` 同一套。
fn rank_live(v: &Value) -> u8 {
    match v.as_bool() {
        Some(true) => 2,
        None => 1,
        Some(false) => 0,
    }
}

fn rank_star(v: &Value) -> u8 {
    match v.as_u64() {
        Some(n) if n > 0 => 2,
        None => 1,
        Some(_) => 0,
    }
}

/// 排序：活的 → 有星标的 → 最近动过的（同 monitor 从前本机那条路的默认顺序）。
fn sort_projects(rows: &mut [Value]) {
    rows.sort_by(|a, b| {
        rank_live(&b["hasLive"])
            .cmp(&rank_live(&a["hasLive"]))
            .then_with(|| rank_star(&b["starredCount"]).cmp(&rank_star(&a["starredCount"])))
            .then_with(|| {
                b["lastActivity"]
                    .as_i64()
                    .unwrap_or(0)
                    .cmp(&a["lastActivity"].as_i64().unwrap_or(0))
            })
    });
}

/// 「不知道」要出声：按理由汇总一条日志（不是每个项目一条）。
fn log_unknowns(what: &str, rows: &[(Value, ProjectCounts)]) {
    let mut by: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (_, c) in rows {
        for w in [c.starred, c.hidden]
            .into_iter()
            .filter_map(|x| match x {
                Counted::Unknown(w) => Some(w),
                _ => None,
            })
            .chain(match c.has_live {
                Counted::Unknown(w) => Some(w),
                _ => None,
            })
        {
            *by.entry(w.reason()).or_default() += 1;
        }
    }
    for (why, n) in by {
        tracing::info!("{what}：{n} 处数**不知道**（不是 0）—— {why}");
    }
}

fn nonempty_lines(text: &str) -> impl Iterator<Item = Value> + '_ {
    text.lines().filter_map(|l| {
        let l = l.trim();
        if l.is_empty() {
            return None;
        }
        match serde_json::from_str::<Value>(l) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("历史清单：一行读不懂（跳过）：{e}");
                None
            }
        }
    })
}

fn capped(v: Value) -> Result<Value, (&'static str, String)> {
    let n = v.to_string().len();
    // 上限与按行那几条同一个数（`read_face::LINES_CAP_BYTES`：monitor 那头单帧 64 MiB，JSON 转义留一半余量）。
    let cap = crate::faces::read_face::LINES_CAP_BYTES;
    if n > cap {
        return Err((
            "too_large",
            copy_text(
                "beHistoryJoin.capped.tooLarge",
                &[("n", &n.to_string()), ("cap", &cap.to_string())],
            ),
        ));
    }
    Ok(v)
}

/// 本机 ＋ 注解 ⇒ 项目成品（阻塞：扫记录树 ＋ 合成历史 ＋ pidfile）。**可喂夹具**：家目录、合成历史、注解、判活由调用方给。
pub(crate) fn local_projects_with(
    home: &Path,
    synth: &[(&'static str, Vec<crate::agents::SynthSession>)],
    loaded: &Loaded,
    live: &dyn Liveness,
) -> Result<Value, (&'static str, String)> {
    let mut buf = Vec::new();
    // 记录树根不在 ⇒ 零个记录树项目（合成历史照并）：界面照空态画，不整页失败。
    let _has_records = crate::observe::history_query::list_projects_into(home, &mut buf)
        .map_err(|e| ("failed", e))?;
    let text = String::from_utf8_lossy(&buf);
    let ann = annotations(loaded);
    let t = ann.as_ref().ok().copied();
    let mut rows: Vec<(Value, ProjectCounts)> = nonempty_lines(&text)
        .filter_map(|v| project_from_row(&v, t, None, live))
        .collect();
    for (kind, sessions) in synth {
        rows.extend(synth_projects(kind, sessions, t));
    }
    log_unknowns("本机项目清单", &rows);
    let mut projects: Vec<Value> = rows.into_iter().map(|(p, _)| p).collect();
    sort_projects(&mut projects);
    capped(json!({ "rows": projects, "notice": ann.err() }))
}

/// 那台 `--session-accounts` 的 stdout ⇒ 此刻活着的 sid（`alive:true` 且有 `sessionId` 的那几行）。
pub(crate) fn live_from_session_accounts(stdout: &str) -> LiveSet {
    LiveSet(
        nonempty_lines(stdout)
            .filter(|v| v.get("alive") == Some(&Value::Bool(true)))
            .filter_map(|v| {
                v.get("sessionId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

/// 那台的判活：问 `--session-accounts`；问不到 ⇒ `None`（调用方用 [`NoLiveness`]，不当成「全死」）。
async fn remote_liveness(
    machine: &str,
    table: &crate::stream::remote_ask::Table,
    remote: &dyn crate::stream::remote_ask::Remote,
) -> Option<LiveSet> {
    crate::stream::remote_ask::ask_with(machine, &["--session-accounts"], table, remote)
        .await
        .ok()
        .map(|out| live_from_session_accounts(&out))
}

/// 远端那一台的 `--list-projects` stdout ＋ 注解 ⇒ 项目成品。
pub(crate) fn remote_projects_from(
    machine: &str,
    stdout: &str,
    loaded: &Loaded,
    live: &dyn Liveness,
) -> Result<Value, (&'static str, String)> {
    let ann = annotations(loaded);
    let t = ann.as_ref().ok().copied();
    let rows: Vec<(Value, ProjectCounts)> = nonempty_lines(stdout)
        .filter_map(|v| project_from_row(&v, t, Some(machine), live))
        .collect();
    log_unknowns("远端项目清单", &rows);
    let mut projects: Vec<Value> = rows.into_iter().map(|(p, _)| p).collect();
    sort_projects(&mut projects);
    capped(json!({ "rows": projects, "notice": ann.err() }))
}

/// 项目目录名的形状闸（`/` `\` `..` 一律拒 —— 与 `history_query::list_sessions_into` 第一道同形；合成历史那一形 `<kind>:<cwd>` 另走）。
fn plain_dir_ok(dir: &str) -> bool {
    !dir.is_empty() && !dir.contains('/') && !dir.contains('\\') && !dir.contains("..")
}

/// 本机某个项目的会话成品。`<kind>:<cwd>` ⇒ 合成历史那一家按 cwd 过滤；否则记录树那一个项目目录。
pub(crate) fn local_sessions_with(
    home: &Path,
    project_dir: &str,
    synth: &[(&'static str, crate::agents::HistoryFace)],
    loaded: &Loaded,
    live: &dyn Liveness,
) -> Result<Value, (&'static str, String)> {
    let ann = annotations(loaded);
    let t = ann.as_ref().ok().copied();
    if let Some((kind, cwd)) = project_dir.split_once(':') {
        let face = synth
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, f)| *f)
            .ok_or((
                "bad_args",
                copy_text(
                    "beHistoryJoin.localSessionsWith.unknownProject",
                    &[
                        ("kind", &kind.to_string()),
                        ("project", &project_dir.to_string()),
                    ],
                ),
            ))?;
        let sessions: Vec<Value> = (face.sessions)()
            .iter()
            .filter(|s| s.cwd == cwd)
            .map(|s| synth_session(kind, s, (face.excerpt)(&s.path), t))
            .collect();
        return capped(json!({ "rows": sessions, "notice": ann.err() }));
    }
    if !plain_dir_ok(project_dir) {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "invalid project directory name: {project_dir}"
            )),
        ));
    }
    let mut buf = Vec::new();
    crate::observe::history_query::list_sessions_into(home, project_dir, &mut buf)
        .map_err(|e| ("failed", e))?;
    let text = String::from_utf8_lossy(&buf);
    let sessions: Vec<Value> = nonempty_lines(&text)
        .filter_map(|v| session_from_row(&v, project_dir, t, None, live))
        .collect();
    capped(json!({ "rows": sessions, "notice": ann.err() }))
}

/// 远端那一台的 `--list-sessions <dir>` stdout ＋ 注解 ⇒ 会话成品。
pub(crate) fn remote_sessions_from(
    machine: &str,
    project_dir: &str,
    stdout: &str,
    loaded: &Loaded,
    live: &dyn Liveness,
) -> Result<Value, (&'static str, String)> {
    let ann = annotations(loaded);
    let t = ann.as_ref().ok().copied();
    let sessions: Vec<Value> = nonempty_lines(stdout)
        .filter_map(|v| session_from_row(&v, project_dir, t, Some(machine), live))
        .collect();
    capped(json!({ "rows": sessions, "notice": ann.err() }))
}

/// 入参里的 `origin`：缺席 / `null` = 这台；空串拒；其余 = 可达表的键。
fn origin_arg(args: &Value) -> Result<Option<&str>, (&'static str, String)> {
    match args.get("origin") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.as_str())),
        Some(_) => Err((
            "bad_args",
            crate::common::contract::malformed(
                "`origin` must be a machine name (absent = this machine)",
            ),
        )),
    }
}

/// 在阻塞线程池上跑一段（扫盘不占 tokio worker）。
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

/// 那台的一次性子命令问不出来 ⇒ `unreachable`（带那台的名字与原因）。
fn asked(r: Result<String, String>) -> Result<String, (&'static str, String)> {
    r.map_err(|e| ("unreachable", e))
}

/// 帧面 `history-projects`（生产入口：进程里那张可达表 ＋ 真拨号）。
pub async fn answer_projects(args: Value) -> Result<Value, (&'static str, String)> {
    answer_projects_with(
        args,
        &crate::stream::remote_ask::REACH,
        &crate::stream::remote_ask::DialRemote,
    )
    .await
}

/// [`answer_projects`] 的可喂夹具那一半：远端那一跳的表与对面由调用方给（`KR83D3`：数得出「一台问了几次」）。
pub async fn answer_projects_with(
    args: Value,
    table: &crate::stream::remote_ask::Table,
    remote: &dyn crate::stream::remote_ask::Remote,
) -> Result<Value, (&'static str, String)> {
    match origin_arg(&args)? {
        None => {
            blocking(|| {
                let home = crate::observe::history_query::agent_home();
                let synth: Vec<(&'static str, Vec<crate::agents::SynthSession>)> =
                    crate::agents::history_faces()
                        .into_iter()
                        .map(|(k, f)| (k, (f.sessions)()))
                        .collect();
                let live = LiveSet(crate::observe::accounts_query::live_session_ids(&home));
                local_projects_with(
                    &home,
                    &synth,
                    &crate::history::history_annotations::load(),
                    &live,
                )
            })
            .await
        }
        Some(o) => {
            let o = o.to_string();
            // 那台没起过会话：它的 CLI 出声、但带码 `no_record_tree` ⇒ 零个项目（界面画「这台还没有会话记录」），
            //   不并进「部分远端没加载上」。认的是码，不是话。
            let out = match crate::stream::remote_ask::ask_with_coded(
                &o,
                &["--list-projects"],
                table,
                remote,
            )
            .await
            {
                Ok(out) => out,
                Err(s)
                    if s.code.as_deref() == Some(crate::observe::history_query::NO_RECORD_TREE) =>
                {
                    String::new()
                }
                Err(s) => return Err(("unreachable", s.message)),
            };
            let live = remote_liveness(&o, table, remote).await;
            blocking(move || {
                let ann = crate::history::history_annotations::load();
                match &live {
                    Some(l) => remote_projects_from(&o, &out, &ann, l),
                    None => remote_projects_from(&o, &out, &ann, &NoLiveness),
                }
            })
            .await
        }
    }
}

/// 帧面 `history-sessions`（生产入口）。
pub async fn answer_sessions(args: Value) -> Result<Value, (&'static str, String)> {
    answer_sessions_with(
        args,
        &crate::stream::remote_ask::REACH,
        &crate::stream::remote_ask::DialRemote,
    )
    .await
}

/// [`answer_sessions`] 的可喂夹具那一半。
pub async fn answer_sessions_with(
    args: Value,
    table: &crate::stream::remote_ask::Table,
    remote: &dyn crate::stream::remote_ask::Remote,
) -> Result<Value, (&'static str, String)> {
    let dir = args
        .get("project_dir")
        .and_then(Value::as_str)
        .filter(|d| !d.is_empty())
        .ok_or((
            "bad_args",
            crate::common::contract::malformed("missing `project_dir`"),
        ))?
        .to_string();
    match origin_arg(&args)? {
        None => {
            blocking(move || {
                let home = crate::observe::history_query::agent_home();
                let live = LiveSet(crate::observe::accounts_query::live_session_ids(&home));
                local_sessions_with(
                    &home,
                    &dir,
                    &crate::agents::history_faces(),
                    &crate::history::history_annotations::load(),
                    &live,
                )
            })
            .await
        }
        Some(o) => {
            if !plain_dir_ok(&dir) {
                return Err((
                    "bad_args",
                    crate::common::contract::malformed(&format!(
                        "invalid project directory name: {dir}"
                    )),
                ));
            }
            let o = o.to_string();
            let out = asked(
                crate::stream::remote_ask::ask_with(&o, &["--list-sessions", &dir], table, remote)
                    .await,
            )?;
            let live = remote_liveness(&o, table, remote).await;
            blocking(move || {
                let ann = crate::history::history_annotations::load();
                match &live {
                    Some(l) => remote_sessions_from(&o, &dir, &out, &ann, l),
                    None => remote_sessions_from(&o, &dir, &out, &ann, &NoLiveness),
                }
            })
            .await
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/history/history_join_tests.rs"]
mod tests;
