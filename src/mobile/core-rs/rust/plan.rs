//! 手机计划三屏的字：Kotlin 交 `plan-list` / `plan-read` 的回包原样 ＋ 要哪一屏，这里写好那一屏每一格的字，Kotlin 照摆。
//! 状态码 · 原因 · 谁负责 · 需手动哪几条都是那台后端的成品；这里只取文、插值、照成品的先后摆，不另判。窄屏一屏一层，不给整张图。
//!
//! 入：`{layer: "slices" | "outline" | "cell", reply: <回包>, slice?, id?, sid?}`（`slices` 交 `plan-list`，另两屏交 `plan-read`）。
//! 出（读不动 · 片或格不在 ⇒ `None`）：
//! - `slices`：`{said, groups: [{head, rows: [{slice, line, end, need}]}]}`；`said` ＝ pb 不在那一句或「无计划」那一句（有片 ⇒ `null`）。
//! - `outline`：`{title, meta: [..], needs: [{kind, title, lines, pos}], mine: {head, rows: [{k, v}]} | null, head, rows: [行]}`。
//! - `cell`：`{label, kind, path, look, state, block, design, inside, files, agentHead, agentHint}`，三节 `{head, …}` 没有 ⇒ `null`。
//!
//! 一行（大纲顶层 · 一格的里面）：`{id, label, look, end, kids}`，`look` ∈ `done` · `open` · `undecided` · `none`（图标照它挑）。

use copy_core::copy_text;
use serde_json::{json, Map, Value};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn n(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}

fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

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
fn stale_line(v: &Value) -> Option<String> {
    let st = v.get("stale").filter(|x| !x.is_null())?;
    Some(copy_text(
        "plan.strip.stale",
        &[("time", s(st, "sinceText").unwrap_or_default())],
    ))
}

fn done_of(key: &str, p: &Value) -> String {
    copy_text(
        key,
        &[
            ("done", &n(p, "done").to_string()),
            ("of", &n(p, "of").to_string()),
        ],
    )
}

// ═══ 片 ═══════════════════════════════════════════════════════════════════════

/// `plan-list` ⇒ 片那一屏：每个工作区一组，一片一行。
pub fn slices(list: &Value) -> Option<Value> {
    let pb = list.get("pb")?;
    if s(pb, "state") != Some("ok") {
        return Some(json!({ "said": s(pb, "said").unwrap_or_default(), "groups": [] }));
    }
    let groups: Vec<Value> = arr(list, "workspaces")
        .iter()
        .map(|w| {
            let ws = s(w, "workspace").unwrap_or_default();
            let rows: Vec<Value> = arr(w, "slices")
                .iter()
                .map(|sl| {
                    let line = match s(sl, "error") {
                        Some(e) => copy_text("plan.strip.sliceUnreadable", &[("reason", e)]),
                        None => joined([s(sl, "domain"), stale_line(sl).as_deref()]),
                    };
                    let end = sl
                        .get("progress")
                        .filter(|p| !p.is_null())
                        .map(|p| done_of("plan.outline.rootProgress", p));
                    json!({
                        "slice": s(sl, "name").unwrap_or_default(),
                        "line": line,
                        "end": end,
                        "need": n(sl, "needCount") > 0,
                    })
                })
                .collect();
            json!({ "workspace": ws, "head": ws, "rows": rows })
        })
        .collect();
    let said = groups
        .iter()
        .all(|g| arr(g, "rows").is_empty())
        .then(|| copy_text("plan.empty.text", &[]));
    Some(json!({ "said": said, "groups": groups }))
}

// ═══ 一片 ═════════════════════════════════════════════════════════════════════

struct Slice<'a> {
    v: &'a Value,
    cells: Map<String, Value>,
}

impl<'a> Slice<'a> {
    fn of(read: &'a Value, name: &str) -> Option<Self> {
        let v = arr(read, "slices")
            .iter()
            .find(|x| s(x, "name") == Some(name))?;
        let cells = arr(v, "cells")
            .iter()
            .filter_map(|c| Some((s(c, "id")?.to_owned(), c.clone())))
            .collect();
        Some(Self { v, cells })
    }

    fn cell(&self, id: &str) -> Option<&Value> {
        self.cells.get(id)
    }

    /// 「编号 标题」：后端拼好的那一份；没有 ⇒ 编号。
    fn label(&self, id: &str) -> String {
        self.v
            .get("labels")
            .and_then(|l| s(l, id))
            .unwrap_or(id)
            .to_owned()
    }

    fn block(&self, id: &str) -> Option<&Value> {
        arr(self.v, "blocks")
            .iter()
            .find(|b| s(b, "id") == Some(id))
    }

    /// 一行（大纲顶层 · 一格的里面）。
    fn row(&self, id: &str) -> Option<Value> {
        let c = self.cell(id)?;
        let kids = arr(c, "children")
            .iter()
            .any(|k| k.as_str().is_some_and(|k| self.cells.contains_key(k)));
        Some(json!({
            "id": id,
            "label": self.label(id),
            "look": look(c),
            "end": state_text(c),
            "kids": kids,
        }))
    }
}

/// 状态画哪一种：照后端的码；待人定另画一种；类不许记收的值 ⇒ `none`。
fn look(c: &Value) -> &str {
    if c.get("undecided").and_then(Value::as_bool) == Some(true) {
        return "undecided";
    }
    s(c, "statusCode").unwrap_or("none")
}

/// 状态那一句：pb 的状态行 ＋ 后端写好的原因。
fn state_text(c: &Value) -> String {
    joined([
        s(c, "stateLine"),
        c.get("whyShort").and_then(|w| s(w, "text")),
    ])
}

/// 一个 id 对到的那条会话叫什么：后端写好的名字（子 agent）；会话本身 ⇒ 它的会话名；认不出 ⇒ 认不出那一句。
fn who_name(w: &Value) -> String {
    if let Some(l) = s(w, "label") {
        return l.to_owned();
    }
    match w.get("face").and_then(|f| s(f, "name")) {
        Some(name) if s(w, "kind") != Some("unknown") => name.to_owned(),
        _ => copy_text(
            "plan.who.unknown",
            &[("id", s(w, "id").unwrap_or_default())],
        ),
    }
}

/// 一条需手动（照桌面那一套键）。
fn need(sl: &Slice, nd: &Value, i: usize, total: usize) -> Value {
    let kind = s(nd, "kind").unwrap_or_default();
    let blk = s(nd, "block").and_then(|b| sl.block(b));
    let root = s(nd, "cell").and_then(|c| sl.cell(c));
    let owner = blk.and_then(|b| b.get("owner")).filter(|o| !o.is_null());
    let mut lines: Vec<String> = Vec::new();
    let title = match kind {
        "top" => {
            let p = sl.v.get("progress").cloned().unwrap_or(Value::Null);
            lines.push(copy_text(
                "plan.review.topDoneWhat",
                &[
                    ("done", &n(&p, "done").to_string()),
                    ("of", &n(&p, "of").to_string()),
                    ("phase", blk.and_then(|b| s(b, "phase")).unwrap_or_default()),
                ],
            ));
            copy_text("plan.review.topDone", &[])
        }
        "red" => {
            let red = nd.get("red").and_then(Value::as_u64).and_then(|k| {
                sl.v.get("check")
                    .map(|c| arr(c, "red"))
                    .and_then(|r| r.get(usize::try_from(k).ok()?))
            });
            if let Some(w) = red.and_then(|r| s(r, "what")) {
                lines.push(w.to_owned());
            }
            if let Some(f) = red.and_then(|r| s(r, "fix")) {
                lines.push(copy_text("plan.review.redFix", &[("fix", f)]));
            }
            let block = root
                .and_then(|r| s(r, "title"))
                .or_else(|| s(nd, "block"))
                .unwrap_or_default();
            copy_text(
                "plan.review.red",
                &[
                    ("rule", red.and_then(|r| s(r, "rule")).unwrap_or_default()),
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
            let why = root.and_then(|r| r.get("whyCode"));
            lines.push(match why.filter(|w| s(w, "kind") == Some("inside")) {
                Some(w) => done_of("plan.review.endedWhat", w),
                None => copy_text("plan.review.endedOpen", &[]),
            });
            if owner.and_then(|o| s(o, "kind")) == Some("subagent") {
                lines.push(copy_text("plan.review.endedSub", &[]));
            }
            copy_text("sessionState.planReview.ended", &[])
        }
    };
    json!({
        "kind": kind,
        "title": title,
        "lines": lines,
        "pos": copy_text("plan.review.pos", &[("i", &(i + 1).to_string()), ("n", &total.to_string())]),
    })
}

/// 此会话接手的那一块（在这一片里才有）。
fn mine(sl: &Slice, read: &Value, sid: &str) -> Option<Value> {
    let b = read.get("bySession")?.get(sid)?;
    if s(b, "slice") != s(sl.v, "name") {
        return None;
    }
    let name = if b.get("top").and_then(Value::as_bool) == Some(true) {
        copy_text("plan.head.top", &[])
    } else {
        s(b, "title")
            .or_else(|| s(b, "block"))
            .unwrap_or_default()
            .to_owned()
    };
    let mut rows = vec![json!({ "k": copy_text("plan.blocks.block", &[]), "v": name })];
    if let Some(p) = s(b, "phase") {
        rows.push(json!({ "k": copy_text("plan.blocks.phase", &[]), "v": p }));
    }
    if let Some(at) = s(b, "at") {
        rows.push(json!({ "k": copy_text("plan.blocks.at", &[]), "v": sl.label(at) }));
    }
    let inside = s(b, "block")
        .and_then(|id| sl.block(id))
        .and_then(|blk| blk.get("inside"))
        .filter(|x| !x.is_null());
    if let Some(k) = inside {
        rows.push(json!({ "k": copy_text("plan.blocks.inside", &[]), "v": done_of("plan.whyShort.inside", k) }));
    }
    Some(json!({ "head": copy_text("plan.overview.blocks", &[]), "rows": rows }))
}

/// `plan-read` 里一片 ⇒ 大纲那一屏。
pub fn outline(read: &Value, slice: &str, sid: Option<&str>) -> Option<Value> {
    let sl = Slice::of(read, slice)?;
    let mut meta: Vec<String> = Vec::new();
    if let Some(e) = s(sl.v, "error") {
        meta.push(copy_text("plan.strip.sliceUnreadable", &[("reason", e)]));
    }
    meta.extend(stale_line(read).or_else(|| stale_line(sl.v)));
    if let Some(t) = s(read, "readAtText") {
        meta.push(copy_text("plan.overview.readAt", &[("time", t)]));
    }
    if let Some(p) = sl.v.get("progress").filter(|p| !p.is_null()) {
        meta.push(done_of("plan.overview.top", p));
    }
    if sl.v.get("docsOnly").and_then(Value::as_bool) == Some(true) {
        meta.push(copy_text("plan.sub.docsOnly", &[]));
    }
    let open: Vec<&Value> = arr(sl.v, "needs")
        .iter()
        .filter(|x| x.get("acked").and_then(Value::as_bool) != Some(true))
        .collect();
    let needs: Vec<Value> = open
        .iter()
        .enumerate()
        .map(|(i, x)| need(&sl, x, i, open.len()))
        .collect();
    let rows: Vec<Value> = arr(sl.v, "top")
        .iter()
        .filter_map(|t| sl.row(t.as_str()?))
        .collect();
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

fn file_word(code: Option<&str>) -> Option<String> {
    let key = match code? {
        "ok" => "plan.fileState.ok",
        "missing" => "plan.fileState.missing",
        "empty" => "plan.fileState.empty",
        "broken" => "plan.fileState.broken",
        _ => return None,
    };
    Some(copy_text(key, &[]))
}

/// `plan-read` 里一格 ⇒ 一格详情那一屏。
pub fn cell(read: &Value, slice: &str, id: &str) -> Option<Value> {
    let sl = Slice::of(read, slice)?;
    let c = sl.cell(id)?;
    let mut up: Vec<String> = Vec::new();
    let mut at = s(c, "parent");
    while let Some(p) = at {
        if up.len() > sl.cells.len() {
            break;
        }
        up.push(sl.label(p));
        at = sl.cell(p).and_then(|x| s(x, "parent"));
    }
    up.reverse();
    let state = Some(state_text(c))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| copy_text("plan.state.none", &[]));
    let block = arr(sl.v, "blocks")
        .iter()
        .find(|b| {
            s(b, "id") != Some("project") && arr(b, "cells").iter().any(|x| x.as_str() == Some(id))
        })
        .map(|b| {
            let head = copy_text(
                "plan.cell.blockRoot",
                &[("block", s(c, "title").unwrap_or(id))],
            );
            let owner = b.get("owner").filter(|o| !o.is_null()).map(who_name);
            joined([Some(head.as_str()), owner.as_deref(), s(b, "phase")])
        });
    let design = s(c, "body")
        .filter(|b| !b.trim().is_empty())
        .map(|b| json!({ "head": copy_text("plan.cell.design", &[]), "body": b }));
    let kids: Vec<Value> = arr(c, "children")
        .iter()
        .filter_map(|k| sl.row(k.as_str()?))
        .collect();
    let inside = (!kids.is_empty())
        .then(|| json!({ "head": copy_text("plan.cell.inside", &[]), "rows": kids }));
    let files: Vec<Value> = arr(c, "files")
        .iter()
        .map(|f| {
            json!({
                "path": s(f, "path").unwrap_or_default(),
                "state": file_word(s(f, "stateCode")),
                "note": s(f, "note"),
            })
        })
        .collect();
    let files = (!files.is_empty())
        .then(|| json!({ "head": copy_text("plan.cell.files", &[]), "rows": files }));
    Some(json!({
        "label": sl.label(id),
        "kind": s(c, "kind"),
        "path": (!up.is_empty()).then(|| up.join(&sep())),
        "look": look(c),
        "state": state,
        "block": block,
        "design": design,
        "inside": inside,
        "files": files,
        "agentHead": copy_text("plan.cell.agentView", &[]),
        "agentHint": copy_text("plan.cell.agentViewHint", &[]),
    }))
}

/// 一屏（见模块头）。
pub fn face(want: &Value) -> Option<Value> {
    let reply = want.get("reply")?;
    match s(want, "layer")? {
        "slices" => slices(reply),
        "outline" => outline(reply, s(want, "slice")?, s(want, "sid")),
        "cell" => cell(reply, s(want, "slice")?, s(want, "id")?),
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

    fn who(kind: &str, label: Option<&str>) -> Value {
        json!({ "id": "a1b2c3", "kind": kind, "sid": "s1", "needs": null, "label": label,
                "face": { "name": "会话甲", "dot": "run", "life": "live", "settled": true, "tip": "", "line": "", "tone": "plain", "order": 0, "can": {} } })
    }

    fn cell_of(id: &str, parent: Option<&str>, kids: &[&str], status: Option<&str>) -> Value {
        json!({ "id": id, "title": format!("标题{id}"), "kind": "模块", "body": "做成这样", "parent": parent,
                "children": kids, "statusCode": status, "undecided": false, "stateLine": status.map(|_| "已签收"),
                "whyShort": null, "whyCode": null,
                "files": [{ "path": "src/a.rs", "stateCode": "missing", "note": null, "checkText": null, "alsoBy": [] }] })
    }

    fn read() -> Value {
        json!({
            "workspace": "/w", "readAtText": "02:11", "stale": null,
            "bySession": { "s1": { "slice": "ledger", "block": "B1", "cell": "A1", "title": "导出", "top": false,
                                   "phase": "回看", "at": "A1-1", "atTitle": "标题A1-1", "via": "session" },
                           "s2": { "slice": "site", "block": "B9", "cell": null, "title": null, "top": true,
                                   "phase": "开发", "at": null, "atTitle": null, "via": "session" } },
            "slices": [{
                "name": "ledger", "domain": "软件", "error": null, "stale": null, "docsOnly": false,
                "progress": { "done": 1, "of": 2 },
                "top": ["A1", "A2"],
                "labels": { "A1": "A1 标题A1", "A1-1": "A1-1 标题A1-1", "A2": "A2 标题A2" },
                "cells": [
                    cell_of("A1", None, &["A1-1"], Some("done")),
                    cell_of("A1-1", Some("A1"), &[], Some("done")),
                    json!({ "id": "A2", "title": "标题A2", "kind": "能力", "body": "", "parent": null, "children": [],
                            "statusCode": "open", "undecided": false, "stateLine": null,
                            "whyShort": { "text": "1/3", "wait": false }, "whyCode": { "kind": "inside", "done": 1, "of": 3 }, "files": [] }),
                ],
                "blocks": [
                    { "id": "project", "cells": [], "phase": "开发", "owner": null, "inside": null },
                    { "id": "B1", "cells": ["A2"], "phase": "审计", "owner": who("subagent", Some("会话甲 的 agent")),
                      "inside": { "done": 1, "of": 3 } },
                ],
                "check": { "red": [{ "rule": "R1", "what": "缺文件", "fix": "补上", "block": "B1", "refs": [] }] },
                "needs": [
                    { "key": "k0", "kind": "top", "block": "project", "cell": null, "sid": null, "acked": true },
                    { "key": "k1", "kind": "red", "block": "B1", "cell": "A2", "sid": null, "red": 0, "acked": false },
                    { "key": "k2", "kind": "ended", "block": "B1", "cell": "A2", "sid": "s1", "acked": false },
                ],
            }],
        })
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

    #[test]
    fn slices_group_by_workspace_and_say_why_empty() {
        let list = json!({ "pb": { "state": "ok", "said": null }, "workspaces": [{
            "workspace": "/w", "slices": [
                { "name": "ledger", "domain": "软件", "progress": { "done": 1, "of": 4 }, "needCount": 1, "error": null, "stale": null },
                { "name": "坏的", "domain": null, "progress": null, "needCount": 0, "error": "读不成", "stale": null },
            ] }] });
        let v = slices(&list).expect("片");
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
        let none = slices(&json!({ "pb": { "state": "ok" }, "workspaces": [] })).expect("片");
        assert_eq!(none["said"], copy_text("plan.empty.text", &[]));
        let no_pb =
            slices(&json!({ "pb": { "state": "missing", "said": "没装 pb" }, "workspaces": [] }))
                .expect("片");
        assert_eq!(no_pb["said"], "没装 pb");
    }

    #[test]
    fn face_json_routes_by_layer() {
        let want =
            json!({ "layer": "cell", "slice": "ledger", "id": "A1", "reply": read() }).to_string();
        let out: Value = serde_json::from_str(&face_json(&want)).expect("JSON");
        assert_eq!(out["label"], "A1 标题A1");
        assert_eq!(face_json("{"), "");
        assert_eq!(
            face_json(&json!({ "layer": "x", "reply": {} }).to_string()),
            ""
        );
    }
}
