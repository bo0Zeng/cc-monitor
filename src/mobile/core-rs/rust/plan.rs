//! 手机计划三屏的字：Kotlin 交 `plan-list` / `plan-read` 的回包原样 ＋ 要哪一屏，这里按契约里的类型（[`cmd_wire::plan`]）解开，
//! 照成品把那一屏每一格摆好，Kotlin 照摆。
//! 状态码 · 画哪一种 · 右端那一句 · 原因 · 谁负责 · 需手动哪几条都是那台后端的成品；这里只取文、插值、照成品的先后摆，不另判。窄屏一屏一层，不给整张图。
//!
//! 入：`{layer: "slices" | "outline" | "cell", reply: <回包>, slice?, id?, sid?}`（`slices` 交 `plan-list`，另两屏交 `plan-read`）。
//! 出（读不动 · 解不开 · 片或格不在 ⇒ `None`）：
//! - `slices`：`{said, groups: [{head, rows: [{slice, line, end, need}]}]}`；`said` ＝ pb 不在那一句或「无计划」那一句（有片 ⇒ `null`）。
//! - `outline`：`{title, meta: [..], needs: [{kind, title, lines, pos}], mine: {head, rows: [{k, v}]} | null, head, rows: [行]}`。
//! - `cell`：`{label, kind, path, look, state, block, design, inside, files, agentHead, agentHint}`，三节 `{head, …}` 没有 ⇒ `null`。
//!
//! 一行（大纲顶层 · 一格的里面）：`{id, label, look, end, kids}`，`look` ∈ `done` · `open` · `undecided` · `none`（图标照它挑）。

use cmd_wire::plan::{
    PlanBlock, PlanCell, PlanList, PlanNeed, PlanRead, PlanSlice, PlanStale, PlanWho, PlanWhyCode,
};
use copy_core::copy_text;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn sep() -> String {
    copy_text("kit.text.sep", &[])
}

/// 几段非空的字用 ` · ` 连。
fn joined<'a>(parts: impl IntoIterator<Item = Option<&'a str>>) -> String {
    parts
        .into_iter()
        .flatten()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(&sep())
}

/// 过期那一句（时刻照那台写好的字）。
fn stale_line(st: Option<&PlanStale>) -> Option<String> {
    let st = st?;
    Some(copy_text(
        "plan.strip.stale",
        &[("time", st.since_text.as_deref().unwrap_or_default())],
    ))
}

// ═══ 片 ═══════════════════════════════════════════════════════════════════════

/// `plan-list` ⇒ 片那一屏：每个工作区一组，一片一行。
pub fn slices(list: &PlanList) -> Value {
    if list.pb.state != "ok" {
        return json!({ "said": list.pb.said.as_deref().unwrap_or_default(), "groups": [] });
    }
    let groups: Vec<Value> = list
        .workspaces
        .iter()
        .map(|w| {
            let rows: Vec<Value> = w
                .slices
                .iter()
                .map(|sl| {
                    let line = match sl.error.as_deref() {
                        Some(e) => copy_text("plan.strip.sliceUnreadable", &[("reason", e)]),
                        None => joined([
                            sl.domain.as_deref(),
                            stale_line(sl.stale.as_ref()).as_deref(),
                        ]),
                    };
                    let end = sl.progress.as_ref().map(|p| {
                        copy_text(
                            "plan.outline.rootProgress",
                            &[("done", &p.done.to_string()), ("of", &p.of.to_string())],
                        )
                    });
                    json!({ "slice": sl.name, "line": line, "end": end, "need": sl.need_count > 0 })
                })
                .collect();
            json!({ "workspace": w.workspace, "head": w.workspace, "rows": rows })
        })
        .collect();
    let said = list
        .workspaces
        .iter()
        .all(|w| w.slices.is_empty())
        .then(|| copy_text("plan.empty.text", &[]));
    json!({ "said": said, "groups": groups })
}

// ═══ 一片 ═════════════════════════════════════════════════════════════════════

struct Slice<'a> {
    v: &'a PlanSlice,
    cells: BTreeMap<&'a str, &'a PlanCell>,
}

impl<'a> Slice<'a> {
    fn of(read: &'a PlanRead, name: &str) -> Option<Self> {
        let v = read.slices.iter().find(|x| x.name == name)?;
        let cells = v.cells.iter().map(|c| (c.id.as_str(), c)).collect();
        Some(Self { v, cells })
    }

    fn cell(&self, id: &str) -> Option<&'a PlanCell> {
        self.cells.get(id).copied()
    }

    /// 「编号 标题」：后端拼好的那一份；没有 ⇒ 编号。
    fn label(&self, id: &str) -> String {
        self.v
            .labels
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_owned())
    }

    fn block(&self, id: &str) -> Option<&'a PlanBlock> {
        self.v.blocks.iter().find(|b| b.id == id)
    }

    /// 一行（大纲顶层 · 一格的里面）。
    fn row(&self, id: &str) -> Option<Value> {
        let c = self.cell(id)?;
        let kids = c
            .children
            .iter()
            .any(|k| self.cells.contains_key(k.as_str()));
        Some(json!({
            "id": id,
            "label": self.label(id),
            "look": c.look,
            "end": c.end.as_deref().unwrap_or_default(),
            "kids": kids,
        }))
    }
}

/// 一个 id 对到的那条会话叫什么：后端写好的名字（子 agent）；认不出 ⇒ 认不出那一句；会话本身 ⇒ 它的会话名。
fn who_name(w: &PlanWho) -> String {
    if let Some(l) = &w.label {
        return l.clone();
    }
    if w.kind == "unknown" {
        return copy_text("plan.who.unknown", &[("id", &w.id)]);
    }
    w.face.name.0.clone()
}

/// 一条需手动（照桌面那一套键）。
fn need(sl: &Slice, nd: &PlanNeed, i: usize, total: usize) -> Value {
    let blk = nd.block.as_deref().and_then(|b| sl.block(b));
    let root = nd.cell.as_deref().and_then(|c| sl.cell(c));
    let owner = blk.and_then(|b| b.owner.as_ref());
    let mut lines: Vec<String> = Vec::new();
    let title = match nd.kind.as_str() {
        "top" => {
            let (done, of) = sl.v.progress.as_ref().map_or((0, 0), |p| (p.done, p.of));
            lines.push(copy_text(
                "plan.review.topDoneWhat",
                &[
                    ("done", &done.to_string()),
                    ("of", &of.to_string()),
                    (
                        "phase",
                        blk.and_then(|b| b.phase.as_deref()).unwrap_or_default(),
                    ),
                ],
            ));
            copy_text("plan.review.topDone", &[])
        }
        "red" => {
            let red = nd
                .red
                .and_then(|k| usize::try_from(k).ok())
                .and_then(|k| sl.v.check.red.get(k));
            if let Some(w) = red.and_then(|r| r.what.as_deref()) {
                lines.push(w.to_owned());
            }
            if let Some(f) = red.and_then(|r| r.fix.as_deref()) {
                lines.push(copy_text("plan.review.redFix", &[("fix", f)]));
            }
            let block = root
                .and_then(|r| r.title.as_deref())
                .or(nd.block.as_deref())
                .unwrap_or_default();
            copy_text(
                "plan.review.red",
                &[
                    (
                        "rule",
                        red.and_then(|r| r.rule.as_deref()).unwrap_or_default(),
                    ),
                    ("block", block),
                ],
            )
        }
        "ask" => {
            lines.extend(owner.map(who_name));
            copy_text("plan.review.ask", &[])
        }
        _ => {
            lines.extend(owner.map(who_name));
            lines.push(match root.and_then(|r| r.why_code.as_ref()) {
                Some(PlanWhyCode::Inside { done, of }) => copy_text(
                    "plan.review.endedWhat",
                    &[("done", &done.to_string()), ("of", &of.to_string())],
                ),
                _ => copy_text("plan.review.endedOpen", &[]),
            });
            if owner.is_some_and(|o| o.kind == "subagent") {
                lines.push(copy_text("plan.review.endedSub", &[]));
            }
            copy_text("sessionState.planReview.ended", &[])
        }
    };
    json!({
        "kind": nd.kind,
        "title": title,
        "lines": lines,
        "pos": copy_text("plan.review.pos", &[("i", &(i + 1).to_string()), ("n", &total.to_string())]),
    })
}

/// 此会话接手的那一块（在这一片里才有）。
fn mine(sl: &Slice, read: &PlanRead, sid: &str) -> Option<Value> {
    let b = read.by_session.get(sid)?;
    if b.slice != sl.v.name {
        return None;
    }
    let name = if b.top {
        copy_text("plan.head.top", &[])
    } else {
        b.title.clone().unwrap_or_else(|| b.block.clone())
    };
    let mut rows = vec![json!({ "k": copy_text("plan.blocks.block", &[]), "v": name })];
    if let Some(p) = &b.phase {
        rows.push(json!({ "k": copy_text("plan.blocks.phase", &[]), "v": p }));
    }
    if let Some(at) = &b.at {
        rows.push(json!({ "k": copy_text("plan.blocks.at", &[]), "v": sl.label(at) }));
    }
    if let Some(k) = sl.block(&b.block).and_then(|blk| blk.inside.as_ref()) {
        let v = copy_text(
            "plan.whyShort.inside",
            &[("done", &k.done.to_string()), ("of", &k.of.to_string())],
        );
        rows.push(json!({ "k": copy_text("plan.blocks.inside", &[]), "v": v }));
    }
    Some(json!({ "head": copy_text("plan.overview.blocks", &[]), "rows": rows }))
}

/// `plan-read` 里一片 ⇒ 大纲那一屏。
pub fn outline(read: &PlanRead, slice: &str, sid: Option<&str>) -> Option<Value> {
    let sl = Slice::of(read, slice)?;
    let mut meta: Vec<String> = Vec::new();
    if let Some(e) = &sl.v.error {
        meta.push(copy_text("plan.strip.sliceUnreadable", &[("reason", e)]));
    }
    meta.extend(stale_line(read.stale.as_ref()).or_else(|| stale_line(sl.v.stale.as_ref())));
    if let Some(t) = &read.read_at_text {
        meta.push(copy_text("plan.overview.readAt", &[("time", t)]));
    }
    if let Some(p) = &sl.v.progress {
        meta.push(copy_text(
            "plan.overview.top",
            &[("done", &p.done.to_string()), ("of", &p.of.to_string())],
        ));
    }
    if sl.v.docs_only {
        meta.push(copy_text("plan.sub.docsOnly", &[]));
    }
    let open: Vec<&PlanNeed> = sl.v.needs.iter().filter(|x| !x.acked).collect();
    let needs: Vec<Value> = open
        .iter()
        .enumerate()
        .map(|(i, x)| need(&sl, x, i, open.len()))
        .collect();
    let rows: Vec<Value> = sl.v.top.iter().filter_map(|t| sl.row(t)).collect();
    Some(json!({
        "title": slice,
        "meta": meta,
        "needs": needs,
        "mine": sid.and_then(|x| mine(&sl, read, x)),
        "head": copy_text("plan.overview.progress", &[]),
        "rows": rows,
    }))
}

// ═══ 一格 ═════════════════════════════════════════════════════════════════════

/// `plan-read` 里一格 ⇒ 一格详情那一屏。
pub fn cell(read: &PlanRead, slice: &str, id: &str) -> Option<Value> {
    let sl = Slice::of(read, slice)?;
    let c = sl.cell(id)?;
    let mut up: Vec<String> = Vec::new();
    let mut at = c.parent.as_deref();
    while let Some(p) = at {
        if up.len() > sl.cells.len() {
            break;
        }
        up.push(sl.label(p));
        at = sl.cell(p).and_then(|x| x.parent.as_deref());
    }
    up.reverse();
    let state = c
        .end
        .clone()
        .unwrap_or_else(|| copy_text("plan.state.none", &[]));
    let block = sl
        .v
        .blocks
        .iter()
        .find(|b| b.id != "project" && b.cells.iter().any(|x| x == id))
        .map(|b| {
            let head = copy_text(
                "plan.cell.blockRoot",
                &[("block", c.title.as_deref().unwrap_or(id))],
            );
            let owner = b.owner.as_ref().map(who_name);
            joined([Some(head.as_str()), owner.as_deref(), b.phase.as_deref()])
        });
    let design = c
        .body
        .as_deref()
        .filter(|b| !b.trim().is_empty())
        .map(|b| json!({ "head": copy_text("plan.cell.design", &[]), "body": b }));
    let kids: Vec<Value> = c.children.iter().filter_map(|k| sl.row(k)).collect();
    let inside = (!kids.is_empty())
        .then(|| json!({ "head": copy_text("plan.cell.inside", &[]), "rows": kids }));
    let files: Vec<Value> = c
        .files
        .iter()
        .map(|f| json!({ "path": f.path, "state": f.check_text, "note": f.note }))
        .collect();
    let files = (!files.is_empty())
        .then(|| json!({ "head": copy_text("plan.cell.files", &[]), "rows": files }));
    Some(json!({
        "label": sl.label(id),
        "kind": c.kind,
        "path": (!up.is_empty()).then(|| up.join(&sep())),
        "look": c.look,
        "state": state,
        "block": block,
        "design": design,
        "inside": inside,
        "files": files,
        "agentHead": copy_text("plan.cell.agentView", &[]),
        "agentHint": copy_text("plan.cell.agentViewHint", &[]),
    }))
}

/// 一屏（见模块头）：回包照那一屏的类型解；解不开 ⇒ `None`。
pub fn face(want: &Value) -> Option<Value> {
    let reply = want.get("reply")?.clone();
    let s = |k: &str| want.get(k).and_then(Value::as_str);
    match s("layer")? {
        "slices" => Some(slices(&serde_json::from_value(reply).ok()?)),
        "outline" => outline(&serde_json::from_value(reply).ok()?, s("slice")?, s("sid")),
        "cell" => cell(&serde_json::from_value(reply).ok()?, s("slice")?, s("id")?),
        _ => None,
    }
}

/// [`face`] 的 JSON 进出；读不动 ⇒ 空串。
pub fn face_json(want: &str) -> String {
    serde_json::from_str::<Value>(want)
        .ok()
        .and_then(|w| face(&w))
        .map(|v| v.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no() -> Value {
        json!({ "can": "no", "why": "夹具" })
    }

    /// 一份完整的会话脸（形状照契约；值是中性夹具）。
    fn face_of(name: &str) -> Value {
        json!({ "dot": "idle", "life": { "is": "gone" }, "settled": true, "name": name, "tip": "", "line": "", "tone": "plain", "order": 0,
                "can": { "close": { "can": "quiet" }, "resume": no(), "term": no(), "reattach": no(), "model": no(),
                         "permission": no(), "stop": no(), "say": no(), "end": no() } })
    }

    fn who(kind: &str, label: Option<&str>) -> Value {
        json!({ "id": "a1b2c3", "kind": kind, "sid": "s1", "needs": null, "label": label, "face": face_of("会话甲") })
    }

    fn edges() -> Value {
        json!({ "to": [], "with": [], "after": [], "replaces": [] })
    }

    /// 一格：契约要的每一格都在，几样要看的由调用处给。
    fn cell_with(id: &str, extra: Value) -> Value {
        let mut c = json!({ "id": id, "title": format!("标题{id}"), "kind": "模块", "body": "做成这样", "parent": null,
                "children": [], "edges": edges(), "pointedBy": edges(), "files": [], "state": [], "stateLine": null,
                "evidencedBy": [], "statusCode": null, "undecided": false, "look": "none", "end": null,
                "whyCode": null, "whyShort": null, "sets": [], "block": null, "owner": null, "signer": null,
                "covers": [], "coveredBy": [], "refs": { "title": [], "body": [] } });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            c[k] = v;
        }
        c
    }

    fn block_with(id: &str, extra: Value) -> Value {
        let mut b = json!({ "id": id, "cells": [], "inside": null, "dir": null, "owner": null, "phase": null, "at": null,
                "listed": true, "handoff": null, "handoffError": null, "takeover": "open",
                "groups": { "groups": [], "text": "", "unread": null } });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            b[k] = v;
        }
        b
    }

    fn read_json() -> Value {
        let done = json!({ "statusCode": "done", "look": "done", "stateLine": "已签收", "end": "已签收" });
        let mut a1 = cell_with("A1", done.clone());
        a1["children"] = json!(["A1-1"]);
        let mut a11 = cell_with("A1-1", done);
        a11["parent"] = json!("A1");
        a11["files"] = json!([{ "path": "src/a.rs", "stateCode": "missing", "note": null,
                                "checkText": copy_text("plan.fileState.missing", &[]), "alsoBy": [] }]);
        let a2 = cell_with(
            "A2",
            json!({ "kind": "能力", "body": "", "statusCode": "open", "look": "open", "end": "1/3",
                    "whyShort": { "text": "1/3", "wait": false }, "whyCode": { "kind": "inside", "done": 1, "of": 3 } }),
        );
        let blocks = json!([
            block_with("project", json!({ "phase": "开发" })),
            block_with(
                "B1",
                json!({ "cells": ["A2"], "phase": "审计", "owner": who("subagent", Some("会话甲 的 agent")),
                        "inside": { "done": 1, "of": 3 } })
            ),
        ]);
        let check = json!({ "unreadable": [], "unparsed": [], "undecidable": [],
                            "red": [{ "rule": "R1", "what": "缺文件", "fix": "补上", "block": "B1", "refs": [] }] });
        let needs = json!([
            { "key": "k0", "kind": "top", "block": "project", "cell": null, "sid": null, "acked": true },
            { "key": "k1", "kind": "red", "block": "B1", "cell": "A2", "sid": null, "red": 0, "acked": false },
            { "key": "k2", "kind": "ended", "block": "B1", "cell": "A2", "sid": "s1", "acked": false },
        ]);
        let slice = json!({
            "name": "ledger", "domain": "软件", "path": null, "current": true, "docsOnly": false, "error": null,
            "stale": null, "bare": false, "kinds": [], "phases": [], "doneValue": "已签收",
            "progress": { "done": 1, "of": 2 }, "openCells": 1, "top": ["A1", "A2"],
            "labels": { "A1": "A1 标题A1", "A1-1": "A1-1 标题A1-1", "A2": "A2 标题A2" },
            "cells": [a1, a11, a2], "blocks": blocks,
            "unbuiltBlocks": [], "unowned": { "paths": null, "why": null }, "pendingSets": [],
            "check": check, "archived": [], "needCount": 2, "returned": null, "needs": needs,
        });
        let by_session = json!({
            "s1": { "slice": "ledger", "block": "B1", "cell": "A1", "title": "导出", "top": false,
                    "phase": "回看", "at": "A1-1", "atTitle": "标题A1-1", "via": "session" },
            "s2": { "slice": "site", "block": "B9", "cell": null, "title": null, "top": true,
                    "phase": "开发", "at": null, "atTitle": null, "via": "session" },
        });
        json!({
            "pb": null, "workspace": "/w", "repo": null, "auto": false, "domains": [], "rev": "r0", "readAt": 0,
            "readAtText": "02:11", "seen": null, "stale": null, "needCount": 2,
            "bySession": by_session, "slices": [slice],
        })
    }

    fn read() -> PlanRead {
        serde_json::from_value(read_json()).expect("夹具照契约的形状")
    }

    #[test]
    fn outline_puts_open_needs_first_with_the_desk_keys() {
        let o = outline(&read(), "ledger", None).expect("一片");
        let needs = o["needs"].as_array().expect("needs");
        assert_eq!(needs.len(), 2, "认可过的那条不出");
        assert_eq!(
            needs[0]["title"],
            copy_text("plan.review.red", &[("rule", "R1"), ("block", "标题A2")])
        );
        assert_eq!(
            needs[0]["lines"],
            json!([
                "缺文件",
                copy_text("plan.review.redFix", &[("fix", "补上")])
            ])
        );
        assert_eq!(
            needs[0]["pos"],
            copy_text("plan.review.pos", &[("i", "1"), ("n", "2")])
        );
        assert_eq!(
            needs[1]["title"],
            copy_text("sessionState.planReview.ended", &[])
        );
        assert_eq!(
            needs[1]["lines"],
            json!([
                "会话甲 的 agent",
                copy_text("plan.review.endedWhat", &[("done", "1"), ("of", "3")]),
                copy_text("plan.review.endedSub", &[]),
            ])
        );
    }

    #[test]
    fn outline_rows_are_the_top_layer_only() {
        let o = outline(&read(), "ledger", None).expect("一片");
        let rows = o["rows"].as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            json!({ "id": "A1", "label": "A1 标题A1", "look": "done", "end": "已签收", "kids": true })
        );
        assert_eq!(rows[1]["look"], "open");
        assert_eq!(rows[1]["end"], "1/3");
        assert_eq!(rows[1]["kids"], false);
        assert_eq!(
            o["meta"],
            json!([
                copy_text("plan.overview.readAt", &[("time", "02:11")]),
                copy_text("plan.overview.top", &[("done", "1"), ("of", "2")]),
            ])
        );
        assert!(o["mine"].is_null(), "没给会话 ⇒ 不出此会话那一节");
    }

    /// 画哪一种 · 右端那一句照后端写好的那两格摆：码说 open、后端写的是 undecided ⇒ 照后端。
    #[test]
    fn rows_take_look_and_end_from_the_backend() {
        let mut v = read_json();
        v["slices"][0]["cells"][2]["look"] = json!("undecided");
        v["slices"][0]["cells"][2]["end"] = json!("后端写的那一句");
        let o = outline(&serde_json::from_value(v).expect("形状"), "ledger", None).expect("一片");
        assert_eq!(o["rows"][1]["look"], "undecided");
        assert_eq!(o["rows"][1]["end"], "后端写的那一句");
    }

    #[test]
    fn outline_shows_the_block_this_session_holds() {
        let o = outline(&read(), "ledger", Some("s1")).expect("一片");
        let rows = o["mine"]["rows"].as_array().expect("mine");
        let kv: Vec<(String, String)> = rows
            .iter()
            .map(|r| {
                (
                    r["k"].as_str().unwrap_or_default().to_owned(),
                    r["v"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            kv,
            vec![
                (copy_text("plan.blocks.block", &[]), "导出".to_owned()),
                (copy_text("plan.blocks.phase", &[]), "回看".to_owned()),
                (copy_text("plan.blocks.at", &[]), "A1-1 标题A1-1".to_owned()),
                (
                    copy_text("plan.blocks.inside", &[]),
                    copy_text("plan.whyShort.inside", &[("done", "1"), ("of", "3")])
                ),
            ]
        );
        assert!(outline(&read(), "ledger", Some("别的")).expect("一片")["mine"].is_null());
        assert!(
            outline(&read(), "ledger", Some("s2")).expect("一片")["mine"].is_null(),
            "接手的是别的片 ⇒ 不出"
        );
    }

    #[test]
    fn cell_writes_path_state_kids_and_files() {
        let c = cell(&read(), "ledger", "A1-1").expect("一格");
        assert_eq!(c["label"], "A1-1 标题A1-1");
        assert_eq!(c["path"], "A1 标题A1");
        assert_eq!(c["state"], "已签收");
        assert!(c["inside"].is_null(), "没子格 ⇒ 不出里面那一节");
        assert_eq!(
            c["files"]["rows"][0],
            json!({ "path": "src/a.rs", "state": copy_text("plan.fileState.missing", &[]), "note": null })
        );
        assert_eq!(c["design"]["body"], "做成这样");
        let top = cell(&read(), "ledger", "A1").expect("一格");
        assert!(top["path"].is_null());
        assert_eq!(top["inside"]["rows"][0]["id"], "A1-1");
        let root = cell(&read(), "ledger", "A2").expect("一格");
        assert_eq!(
            root["block"],
            joined([
                Some(copy_text("plan.cell.blockRoot", &[("block", "标题A2")]).as_str()),
                Some("会话甲 的 agent"),
                Some("审计"),
            ])
        );
        assert!(root["design"].is_null(), "空正文不出");
        assert!(cell(&read(), "ledger", "没这格").is_none());
    }

    fn list_slice(name: &str, extra: Value) -> Value {
        let mut s = json!({ "name": name, "domain": null, "current": false, "progress": null, "needCount": 0,
                            "error": null, "stale": null });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            s[k] = v;
        }
        s
    }

    fn list(pb: Value, slices: Vec<Value>) -> PlanList {
        let ws = if slices.is_empty() {
            json!([])
        } else {
            json!([{ "workspace": "/w", "repo": null, "auto": false, "rev": "r0", "stale": null, "needCount": 0,
                     "bySession": {}, "slices": slices }])
        };
        serde_json::from_value(json!({ "pb": pb, "workspaces": ws })).expect("形状")
    }

    #[test]
    fn slices_group_by_workspace_and_say_why_empty() {
        let ok = json!({ "state": "ok", "said": null, "version": null });
        let v = slices(&list(
            ok.clone(),
            vec![
                list_slice(
                    "ledger",
                    json!({ "domain": "软件", "progress": { "done": 1, "of": 4 }, "needCount": 1 }),
                ),
                list_slice("坏的", json!({ "error": "读不成" })),
            ],
        ));
        assert!(v["said"].is_null());
        let rows = v["groups"][0]["rows"].as_array().expect("rows");
        assert_eq!(
            rows[0],
            json!({ "slice": "ledger", "line": "软件",
            "end": copy_text("plan.outline.rootProgress", &[("done", "1"), ("of", "4")]), "need": true })
        );
        assert_eq!(
            rows[1]["line"],
            copy_text("plan.strip.sliceUnreadable", &[("reason", "读不成")])
        );
        assert!(rows[1]["end"].is_null());
        assert_eq!(
            slices(&list(ok, vec![]))["said"],
            copy_text("plan.empty.text", &[])
        );
        let no_pb = slices(&list(
            json!({ "state": "missing", "said": "没装 pb", "version": null }),
            vec![],
        ));
        assert_eq!(no_pb["said"], "没装 pb");
    }

    #[test]
    fn face_json_routes_by_layer() {
        let want =
            json!({ "layer": "cell", "slice": "ledger", "id": "A1", "reply": read_json() }).to_string();
        let out: Value = serde_json::from_str(&face_json(&want)).expect("JSON");
        assert_eq!(out["label"], "A1 标题A1");
        assert_eq!(face_json("{"), "");
        assert_eq!(
            face_json(&json!({ "layer": "x", "reply": {} }).to_string()),
            ""
        );
        assert_eq!(
            face_json(&json!({ "layer": "cell", "slice": "ledger", "id": "A1", "reply": { "workspace": "/w" } }).to_string()),
            "",
            "回包不是契约的形状 ⇒ 解不开、不出"
        );
    }
}
