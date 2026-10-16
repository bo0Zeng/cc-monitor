//! 一份 `pb dump`（形状 1）⇒ 界面直接排版的成品。纯函数：不读盘、不起进程。
//!
//! 换个方向排、换个名字，再把界面要的数与先后算好（界面照画、不数不排）：
//! - 摘掉每格的 `agent_view`（研究盘上它占输出的一半），另交一张表，点到那一格再给；
//! - 每格「指着它的」边：把全片的边倒过来排；每份文件还挂在哪几格：把全片的文件声明倒过来查；
//! - 块的接手 · 每格在长它的 · 每条签收的「由」：经 [`super::WhoPort`] 对到会话；
//! - 顶层进度：顶层格按 pb 给的状态数一数；块根格的直接子做完几格 · 没做完的格数 · 签收新的在上 · 行尾原因短词，也在这里数好、排好、写好。
//!
//! 键名一律换成线上惯例（小驼峰）；pb 那边多出来的键不带过来（形状版本没变就不该多，多了说明该升形状版本了）。

use super::{Live, WhoPort, Whose};
use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// 四种边（pb 的边名，也是线上的键名）。
pub(crate) const EDGES: [&str; 4] = ["to", "with", "after", "replaces"];

/// pb 给的状态里「做完了」「不做了」那两个值（进度要数它们；其余都算没做完）。
pub(crate) const DONE: &str = "做完了";
pub(crate) const DROPPED: &str = "不做了";

/// pb 的三种状态 ⇒ 线上的码（`statusCode`）；pb 给了别的字 ⇒ `None`（界面照出原话、当没做完画）。
pub(crate) fn status_code(s: Option<&str>) -> Option<&'static str> {
    match s? {
        DONE => Some("done"),
        "没做完" => Some("open"),
        DROPPED => Some("dropped"),
        _ => None,
    }
}

/// 没做完的原因 ⇒ `whyCode`：`没签` ⇒ `{kind: nosign}` · `等上一级收下` ⇒ `{kind: upper}` ·
/// `里面 d/m 做完了` ⇒ `{kind: inside, done, of}`；认不出 ⇒ `null`（界面照出原话）。拆 pb 原话只在这一处。
pub(crate) fn why_code(s: Option<&str>) -> Value {
    let Some(s) = s else { return Value::Null };
    match s {
        "没签" => return json!({"kind": "nosign"}),
        "等上一级收下" => return json!({"kind": "upper"}),
        _ => {}
    }
    let inside = s
        .strip_prefix("里面 ")
        .and_then(|r| r.strip_suffix(" 做完了"))
        .and_then(|r| r.split_once('/'))
        .and_then(|(d, m)| Some((d.trim().parse::<u64>().ok()?, m.trim().parse::<u64>().ok()?)));
    match inside {
        Some((done, of)) => json!({"kind": "inside", "done": done, "of": of}),
        None => Value::Null,
    }
}

/// 行尾那一个原因短词（`whyShort`）：没签 · d/m · 等收下（`wait`：等收下用正文色）；认不出 ⇒ pb 原话；没有原因 ⇒ `null`。
fn why_short(s: Option<&str>) -> Value {
    let code = why_code(s);
    let (text, wait) = match code.get("kind").and_then(Value::as_str) {
        Some("nosign") => (copy_text("plan.whyShort.nosign", &[]), false),
        Some("upper") => (copy_text("plan.whyShort.upper", &[]), true),
        Some("inside") => (
            copy_text(
                "plan.whyShort.inside",
                &[
                    ("done", &code["done"].to_string()),
                    ("of", &code["of"].to_string()),
                ],
            ),
            false,
        ),
        _ => match s {
            Some(raw) => (raw.to_string(), false),
            None => return Value::Null,
        },
    };
    json!({"text": text, "wait": wait})
}

/// 对账的四种（在 · 缺 · 空 · 坏）⇒ `stateCode`；认不出 ⇒ `None`。
pub(crate) fn file_code(s: Option<&str>) -> Option<&'static str> {
    match s? {
        "在" => Some("ok"),
        "缺" => Some("missing"),
        "空" => Some("empty"),
        "坏" => Some("broken"),
        _ => None,
    }
}

/// 一片里每格的 `agent_view`：`(片, 格) ⇒ 原文`。
pub(crate) type Views = BTreeMap<(String, String), String>;

/// 加工好的一份：成品 ＋ 摘下来的 `agent_view`。
pub(crate) struct Made {
    pub(crate) doc: Value,
    pub(crate) views: Views,
}

fn s(v: &Value, k: &str) -> Value {
    match v.get(k) {
        Some(Value::String(x)) => Value::String(x.clone()),
        _ => Value::Null,
    }
}

fn strs(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 一个 id 对到的会话（线上那一格）。`null` 的 id ⇒ `null`。
pub(crate) fn who_json(id: Option<&str>, who: WhoPort) -> Value {
    let Some(id) = id.filter(|i| !i.is_empty()) else {
        return Value::Null;
    };
    let live_json = |l: &Option<Live>| match l {
        Some(l) => (
            true,
            serde_json::to_value(l.activity).unwrap_or(Value::Null),
            serde_json::to_value(l.needs).unwrap_or(Value::Null),
        ),
        None => (false, Value::Null, Value::Null),
    };
    let (kind, sid, live) = match who(id) {
        Whose::Session { sid, live } => ("session", Some(sid), live_json(&live)),
        Whose::Subagent { parent, live } => ("subagent", Some(parent), live_json(&live)),
        Whose::Unknown => ("unknown", None, (false, Value::Null, Value::Null)),
    };
    json!({
        "id": id,
        "kind": kind,
        "sid": sid,
        "alive": live.0,
        "activity": live.1,
        "needs": live.2,
    })
}

/// 整份加工。`repo` 是工作区 `.env` 里的当前片（`current` 那一格照它）。
pub(crate) fn make(doc: &Value, who: WhoPort) -> Made {
    let repo = doc.get("repo").and_then(Value::as_str);
    let mut views = Views::new();
    let slices: Vec<Value> = doc
        .get("slices")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|sl| slice(sl, repo, who, &mut views))
                .collect()
        })
        .unwrap_or_default();
    let by_session = by_session(&slices);
    Made {
        doc: json!({
            "pb": s(doc, "pb"),
            "workspace": s(doc, "workspace"),
            "repo": s(doc, "repo"),
            "auto": doc.get("auto").and_then(Value::as_bool).unwrap_or(false),
            "slices": slices,
            "bySession": by_session,
        }),
        views,
    }
}

/// 会话 ⇒ 它接手的那一块（会话头那一枚标 · 「这个会话是哪一片哪一块的接手」）：
/// `{片, 块, 块根格（编号）, 块根格标题（顶块 ⇒ null）, 顶块不顶块, 阶段, 站在哪一格（编号 ＋ 标题）, via: session|subagent}`。
/// 子 agent 接的块记在父会话名下；一个会话名下几块 ⇒ 自己接的压过子 agent 替它接的，同档取片与块的先后里第一块。
/// 对不上会话的接手不进表。
pub(crate) fn by_session(slices: &[Value]) -> Value {
    let mut out = serde_json::Map::new();
    for pass in ["session", "subagent"] {
        for sl in slices {
            let title_of = |id: &str| {
                sl.get("cells")
                    .and_then(Value::as_array)
                    .and_then(|cs| cs.iter().find(|c| c["id"] == id))
                    .map_or(Value::Null, |c| c["title"].clone())
            };
            for b in sl
                .get("blocks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let owner = &b["owner"];
                let Some(sid) = owner["sid"].as_str() else {
                    continue;
                };
                if owner["kind"] != pass || out.contains_key(sid) {
                    continue;
                }
                let top = b["id"] == crate::plan::needs::TOP_BLOCK;
                let root = b["cells"]
                    .as_array()
                    .and_then(|c| c.first())
                    .and_then(Value::as_str);
                let at = b["at"].as_str();
                out.insert(
                    sid.to_string(),
                    json!({
                        "slice": sl["name"],
                        "block": b["id"],
                        "cell": root.map_or(Value::Null, |r| json!(r)),
                        "title": if top { Value::Null } else { root.map_or(Value::Null, title_of) },
                        "top": top,
                        "phase": b["phase"],
                        "at": b["at"],
                        "atTitle": at.map_or(Value::Null, title_of),
                        "via": pass,
                    }),
                );
            }
        }
    }
    Value::Object(out)
}

/// 一片。带 `error` 的片只出名字、领域与那一句（pb 那时给不出别的）。
pub(crate) fn slice(sl: &Value, repo: Option<&str>, who: WhoPort, views: &mut Views) -> Value {
    let name = sl
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let head = |error: Value| -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("name".into(), json!(name));
        m.insert("domain".into(), s(sl, "domain"));
        // 仓库相对工作区根在哪（`pb repo <路径>` 登记的）；pb 没给（旧 pb）⇒ 片名（仓库贴着工作区根）。只在这一处补。
        let path = sl
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
            .unwrap_or(&name);
        m.insert("path".into(), json!(path));
        m.insert("current".into(), json!(repo == Some(name.as_str())));
        // 只写文档：仓库还没接过来（人 `pb take` 之后才守）；pb 没给 ⇒ 接过了。
        let docs_only = sl.get("docs_only").and_then(Value::as_bool) == Some(true);
        m.insert("docsOnly".into(), json!(docs_only));
        m.insert("error".into(), error);
        m.insert("stale".into(), Value::Null);
        m
    };
    if let Some(e) = sl.get("error").filter(|e| !e.is_null()) {
        // 读不成又没读好过：形状照常（空的），`bare` 说「只有头几格」。
        let mut m = head(e.clone());
        m.insert("bare".into(), json!(true));
        for k in ["kinds", "phases", "top", "blocks", "cells", "archived"] {
            m.insert(k.into(), json!([]));
        }
        m.insert("done".into(), json!(false));
        m.insert("progress".into(), Value::Null);
        m.insert("openCells".into(), json!(0));
        m.insert("recentSigns".into(), json!([]));
        m.insert(
            "check".into(),
            json!({"unreadable": [], "red": [], "undecidable": []}),
        );
        return Value::Object(m);
    }
    let cells_in: Vec<&Value> = sl
        .get("cells")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();

    // 倒过来的两张索引：谁的边指着它 · 同一份文件还挂在哪几格。
    let mut pointed: BTreeMap<String, BTreeMap<&str, Vec<String>>> = BTreeMap::new();
    let mut by_path: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in &cells_in {
        let id = c.get("id").and_then(Value::as_str).unwrap_or_default();
        for k in EDGES {
            for t in strs(c.get("edges").and_then(|e| e.get(k))) {
                pointed
                    .entry(t)
                    .or_default()
                    .entry(k)
                    .or_default()
                    .push(id.to_string());
            }
        }
        for f in c
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(p) = f.get("path").and_then(Value::as_str) {
                by_path
                    .entry(p.to_string())
                    .or_default()
                    .push(id.to_string());
            }
        }
    }

    let cells: Vec<Value> = cells_in
        .iter()
        .map(|c| {
            let id = c.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            let has_view = match c.get("agent_view").and_then(Value::as_str) {
                Some(v) => {
                    views.insert((name.clone(), id.clone()), v.to_string());
                    true
                }
                None => false,
            };
            let edges: Map<String, Value> = EDGES
                .iter()
                .map(|k| ((*k).to_string(), json!(strs(c.get("edges").and_then(|e| e.get(*k))))))
                .collect();
            let back = pointed.get(&id);
            let pointed_by: Map<String, Value> = EDGES
                .iter()
                .map(|k| {
                    let v = back.and_then(|b| b.get(k)).cloned().unwrap_or_default();
                    ((*k).to_string(), json!(v))
                })
                .collect();
            let files: Vec<Value> = c
                .get("files")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|f| {
                    let path = f.get("path").and_then(Value::as_str).unwrap_or_default();
                    let also: Vec<&String> = by_path
                        .get(path)
                        .into_iter()
                        .flatten()
                        .filter(|o| **o != id)
                        .collect();
                    let state = f.get("state").and_then(Value::as_str);
                    json!({"path": path, "state": s(f, "state"), "stateCode": file_code(state), "note": s(f, "note"), "alsoBy": also})
                })
                .collect();
            let signs: Vec<Value> = c
                .get("signs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|g| {
                    json!({
                        "at": s(g, "at"),
                        "by": who_json(g.get("by").and_then(Value::as_str), who),
                        "reason": s(g, "reason"),
                        "refs": g.get("refs").cloned().unwrap_or_else(|| json!([])),
                    })
                })
                .collect();
            json!({
                "id": id,
                "title": s(c, "title"),
                "kind": s(c, "kind"),
                "body": s(c, "body"),
                "parent": s(c, "parent"),
                "children": strs(c.get("children")),
                "edges": edges,
                "pointedBy": pointed_by,
                "files": files,
                "status": s(c, "status"),
                "statusCode": status_code(c.get("status").and_then(Value::as_str)),
                "why": s(c, "why"),
                "whyCode": why_code(c.get("why").and_then(Value::as_str)),
                "whyShort": why_short(c.get("why").and_then(Value::as_str)),
                "signs": signs,
                "owner": who_json(c.get("owner").and_then(Value::as_str), who),
                // 签它的那一位（pb 还没给 ⇒ `null`；请求单第 2 条）。
                "signer": who_json(c.get("signer").and_then(Value::as_str), who),
                "refs": c.get("refs").cloned().unwrap_or_else(|| json!({"title": [], "body": []})),
                "hasView": has_view,
            })
        })
        .collect();

    let top = strs(sl.get("top"));
    let status_of = |id: &str| {
        cells_in
            .iter()
            .find(|c| c.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|c| c.get("status").and_then(Value::as_str))
    };
    let (mut done, mut dropped, mut open) = (0u64, 0u64, 0u64);
    for t in &top {
        match status_of(t) {
            Some(DONE) => done += 1,
            Some(DROPPED) => dropped += 1,
            _ => open += 1,
        }
    }

    // 没做完的格（不是做完了、也不是不做了）。
    let open_cells = cells_in
        .iter()
        .filter(|c| {
            !matches!(
                c.get("status").and_then(Value::as_str),
                Some(DONE | DROPPED)
            )
        })
        .count();
    // 全片的签收，新的在上；读不出时刻的排后（同为读不出的照原来的先后）。
    let mut recent: Vec<(Option<i64>, &str, usize)> = cells_in
        .iter()
        .flat_map(|c| {
            let id = c.get("id").and_then(Value::as_str).unwrap_or_default();
            c.get("signs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
                .map(move |(i, g)| {
                    let ms = g
                        .get("at")
                        .and_then(Value::as_str)
                        .and_then(crate::common::time::parse_iso8601_ms);
                    (ms, id, i)
                })
        })
        .collect();
    recent.sort_by(|a, b| b.0.cmp(&a.0));
    let recent_signs: Vec<Value> = recent
        .iter()
        .map(|(_, cell, i)| json!({"cell": cell, "i": i}))
        .collect();

    let kinds: Vec<Value> = sl
        .get("kinds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|k| {
            let w = k.get("edge_words");
            let words: Map<String, Value> = EDGES
                .iter()
                .map(|e| ((*e).to_string(), w.map_or(Value::Null, |w| s(w, e))))
                .collect();
            json!({"name": s(k, "name"), "edgeWords": words})
        })
        .collect();
    let phases: Vec<Value> = sl
        .get("phases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|p| json!({"name": s(p, "name"), "does": s(p, "does"), "marks": strs(p.get("marks"))}))
        .collect();
    let blocks: Vec<Value> = sl
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|b| {
            let cells = strs(b.get("cells"));
            let inside = cells
                .first()
                .and_then(|r| {
                    cells_in
                        .iter()
                        .find(|c| c.get("id").and_then(Value::as_str) == Some(r))
                })
                .map_or(Value::Null, |root| kids_done(&cells_in, root));
            json!({
                "id": s(b, "id"),
                "cells": cells,
                "inside": inside,
                "dir": s(b, "dir"),
                "owner": who_json(b.get("owner").and_then(Value::as_str), who),
                "phase": s(b, "phase"),
                "at": s(b, "at"),
                "row": b.get("row").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect();
    let check = sl.get("check");
    let red: Vec<Value> = check
        .and_then(|c| c.get("red"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|r| json!({"rule": s(r, "rule"), "what": s(r, "what"), "block": s(r, "block"), "fix": s(r, "fix")}))
        .collect();
    let undecidable: Vec<Value> = check
        .and_then(|c| c.get("undecidable"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|r| json!({"rule": s(r, "rule"), "why": s(r, "why")}))
        .collect();
    let archived: Vec<Value> = sl
        .get("archived")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|a| json!({"id": s(a, "id"), "title": s(a, "title"), "kind": s(a, "kind"), "replacedBy": s(a, "replaced_by")}))
        .collect();

    let mut m = head(Value::Null);
    m.insert("bare".into(), json!(false));
    m.insert("kinds".into(), json!(kinds));
    m.insert("phases".into(), json!(phases));
    m.insert(
        "done".into(),
        json!(sl.get("done").and_then(Value::as_bool).unwrap_or(false)),
    );
    m.insert("top".into(), json!(top));
    m.insert(
        "progress".into(),
        json!({"done": done, "open": open, "dropped": dropped}),
    );
    m.insert("openCells".into(), json!(open_cells));
    m.insert("recentSigns".into(), json!(recent_signs));
    m.insert("blocks".into(), json!(blocks));
    m.insert(
        "check".into(),
        json!({
            "unreadable": check.and_then(|c| c.get("unreadable")).cloned().unwrap_or_else(|| json!([])),
            "red": red,
            "undecidable": undecidable,
        }),
    );
    m.insert("cells".into(), json!(cells));
    m.insert("archived".into(), json!(archived));
    Value::Object(m)
}

/// 一格的直接子里做完了几格 / 一共几格（不做了的不算）。
fn kids_done(cells: &[&Value], root: &Value) -> Value {
    let (mut done, mut of) = (0u32, 0u32);
    for k in strs(root.get("children")) {
        let status = cells
            .iter()
            .find(|c| c.get("id").and_then(Value::as_str) == Some(k.as_str()))
            .map(|c| c.get("status").and_then(Value::as_str));
        match status {
            None | Some(Some(DROPPED)) => {}
            Some(Some(DONE)) => {
                done += 1;
                of += 1;
            }
            Some(_) => of += 1,
        }
    }
    json!({"done": done, "of": of})
}

/// `plan-list` 里一个工作区那一行（标过认可的成品 ⇒ 去掉格与签收，只留切换与计数要的）。
pub(crate) fn list_row(v: &Value) -> Value {
    json!({
        "workspace": v["workspace"],
        "repo": v["repo"],
        "auto": v["auto"],
        "rev": v["rev"],
        "stale": v["stale"],
        "needCount": v["needCount"],
        "bySession": v["bySession"],
        "slices": v["slices"].as_array().map(|a| a.iter().map(slice_summary).collect::<Vec<_>>()).unwrap_or_default(),
    })
}

/// 一片的一句话摘要（`plan-list` 那一行要的）：工作区 · 片名 · 领域 · 当前片没有 · 顶层进度 · 要你看的数（标过认可之后才有）· 读不成的那一句。
pub(crate) fn slice_summary(sl: &Value) -> Value {
    json!({
        "name": sl.get("name").cloned().unwrap_or(Value::Null),
        "domain": sl.get("domain").cloned().unwrap_or(Value::Null),
        "current": sl.get("current").cloned().unwrap_or(Value::Bool(false)),
        "progress": sl.get("progress").cloned().unwrap_or(Value::Null),
        "needCount": sl.get("needCount").cloned().unwrap_or(Value::Null),
        "error": sl.get("error").cloned().unwrap_or(Value::Null),
        "stale": sl.get("stale").cloned().unwrap_or(Value::Null),
    })
}

/// 时刻写成给人看的字（界面照抄，不换算）：`readAt` · `since` · `at`（毫秒整数，或签收那种 ISO 串）旁边添 `<键>Text`，
/// 按 `now_ms` 与看的那一台的时区经 [`crate::common::time::fmt_at`] 写。`at` 是串却读不成时刻（块的站位编号）⇒ 不添。
pub(crate) fn with_time_texts(v: &mut Value, now_ms: i64, tz: &crate::Tz) {
    match v {
        Value::Object(m) => {
            let adds: Vec<(String, String)> = ["readAt", "since", "at"]
                .iter()
                .filter_map(|k| {
                    let ms = match m.get(*k)? {
                        Value::Number(n) => n.as_i64(),
                        Value::String(s) => crate::common::time::parse_iso8601_ms(s),
                        _ => None,
                    }?;
                    Some((
                        format!("{k}Text"),
                        crate::common::time::fmt_at(
                            ms.div_euclid(1000),
                            now_ms.div_euclid(1000),
                            tz,
                        ),
                    ))
                })
                .collect();
            for x in m.values_mut() {
                with_time_texts(x, now_ms, tz);
            }
            for (k, t) in adds {
                m.insert(k, Value::String(t));
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| with_time_texts(x, now_ms, tz)),
        _ => {}
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/product_tests.rs"]
mod tests;
