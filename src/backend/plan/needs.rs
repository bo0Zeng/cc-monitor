//! 计划这一侧「要你看」的判定：纯函数，跑在 [`super::product::make`] 之后（吃成品，不吃 pb 的原样），不读盘、不起进程。
//!
//! 只收四种（人只是顶块的上一级：子块交回由上一级 agent 收，新签收按时间排成一条，都不进这里）：
//!
//! | 种 | 什么时候出 | 键（带条目版本：版本换了，认可就失效、再出） |
//! |---|---|---|
//! | `top` | 顶块（id 为 `project` 的那一块）的阶段带「看全局」标 | `top:<进那一步那次的 rev>` |
//! | `red` | `check.red` 每条一项 | `red:<规则>@<块>` |
//! | `ended` | 某块的接手对到的会话已结束、那一块没全做完（子 agent 当接手看父会话：父会话也停了才算） | `ended:<块>@<接手 id>` |
//! | `ask` | 某块的接手对到的会话此刻在等你 | `ask:<块>@<接手 id>`（不收认可：答了自己消失；计数由会话那一侧算） |
//!
//! 另有「退回」落地的判法（[`landing`]）与送进会话的那一行（[`line`]）。

use super::product::{DONE, DROPPED};
use copy_core::copy_text;
use serde_json::{json, Value};

/// 顶块的 id（pb 的规矩：顶那一块叫 project）。
pub(crate) const TOP_BLOCK: &str = "project";

/// 阶段表里「交回的关口」那个标（pb 的协议值）。
pub(crate) const LOOK_GLOBAL: &str = "看全局";

/// 不收认可的那一种（答了自己消失）。
const ASK: &str = "ask";

/// 一次「退回」记下的那一刻：送给谁 · 送没送到 · 那一格当时的子格集合与正文摘要（判落地用）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Returned {
    /// 送出的时刻（epoch ms）。
    pub(crate) at: u64,
    /// 送到的会话 sid。
    pub(crate) to: String,
    /// `delivered` · `unsure`（送达未知）。
    pub(crate) result: String,
    /// 当时那一格的子格。
    pub(crate) children: Vec<String>,
    /// 当时那一格正文的摘要（[`body_digest`]）。
    pub(crate) body: String,
}

fn find<'a>(arr: Option<&'a Value>, id: &str) -> Option<&'a Value> {
    arr.and_then(Value::as_array)?
        .iter()
        .find(|x| x.get("id").and_then(Value::as_str) == Some(id))
}

/// 顶块此刻站在带「看全局」标的那一步上。
pub(crate) fn top_at_look_global(sl: &Value) -> bool {
    let Some(phase) = find(sl.get("blocks"), TOP_BLOCK)
        .and_then(|b| b.get("phase"))
        .and_then(Value::as_str)
    else {
        return false;
    };
    sl.get("phases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|p| {
            p.get("name").and_then(Value::as_str) == Some(phase)
                && p.get("marks")
                    .and_then(Value::as_array)
                    .is_some_and(|m| m.iter().any(|x| x.as_str() == Some(LOOK_GLOBAL)))
        })
}

/// 一块收没收尾：顶块看整片的 `done`；别的块看它的根格（`cells`）是不是都做完了 / 不做了（找不到那一格 ⇒ 算没收尾）。
fn block_finished(sl: &Value, b: &Value) -> bool {
    if b.get("id").and_then(Value::as_str) == Some(TOP_BLOCK) {
        return sl.get("done").and_then(Value::as_bool).unwrap_or(false);
    }
    b.get("cells")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .all(|id| {
            matches!(
                find(sl.get("cells"), id)
                    .and_then(|c| c.get("status"))
                    .and_then(Value::as_str),
                Some(DONE) | Some(DROPPED)
            )
        })
}

/// 一块的根格（`cells` 第一项；没有 ⇒ `null`）。
fn root_of(b: &Value) -> Value {
    b.get("cells")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .cloned()
        .unwrap_or(Value::Null)
}

fn need(kind: &str, key: String, block: Value, cell: Value, sid: Value) -> Value {
    json!({"key": key, "kind": kind, "block": block, "cell": cell, "sid": sid, "acked": false})
}

/// 一片（成品）的要你看。`entered` ＝ 顶块进「看全局」那一步那次的 rev（不知道 ⇒ 不出 `top` 那一条）。
pub(crate) fn of_slice(sl: &Value, entered: Option<&str>) -> Vec<Value> {
    let mut out = Vec::new();
    if let (true, Some(rev)) = (top_at_look_global(sl), entered) {
        let root = find(sl.get("blocks"), TOP_BLOCK).map_or(Value::Null, root_of);
        out.push(need(
            "top",
            format!("top:{rev}"),
            json!(TOP_BLOCK),
            root,
            Value::Null,
        ));
    }
    for (i, r) in sl
        .get("check")
        .and_then(|c| c.get("red"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let rule = r.get("rule").and_then(Value::as_str).unwrap_or_default();
        let block = r.get("block").and_then(Value::as_str);
        let cell = block
            .and_then(|b| find(sl.get("blocks"), b))
            .map_or(Value::Null, root_of);
        let mut n = need(
            "red",
            format!("red:{rule}@{}", block.unwrap_or_default()),
            block.map_or(Value::Null, |b| json!(b)),
            cell,
            Value::Null,
        );
        n["red"] = json!(i);
        out.push(n);
    }
    for b in sl
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = b.get("id").and_then(Value::as_str).unwrap_or_default();
        let o = b.get("owner").unwrap_or(&Value::Null);
        let (Some(who), Some(sid)) = (o.get("id").and_then(Value::as_str), o.get("sid")) else {
            continue;
        };
        if sid.is_null() {
            // 认不出 ⇒ 说不清停没停、问没问。
            continue;
        }
        let alive = o.get("alive").and_then(Value::as_bool).unwrap_or(false);
        if !alive {
            if !block_finished(sl, b) {
                out.push(need(
                    "ended",
                    format!("ended:{id}@{who}"),
                    json!(id),
                    root_of(b),
                    sid.clone(),
                ));
            }
        } else if o.get("activity").and_then(Value::as_str) == Some("needs_you") {
            let at = b
                .get("at")
                .filter(|a| a.is_string())
                .cloned()
                .unwrap_or_else(|| root_of(b));
            out.push(need(
                ASK,
                format!("ask:{id}@{who}"),
                json!(id),
                at,
                sid.clone(),
            ));
        }
    }
    out
}

/// 照认可记录给每片的 `needs` 标 `acked`（`acked(片, 键)`）。`ask` 那一种不收认可。
pub(crate) fn mark_acked(doc: &mut Value, acked: &dyn Fn(&str, &str) -> bool) {
    let Some(slices) = doc.get_mut("slices").and_then(Value::as_array_mut) else {
        return;
    };
    for sl in slices {
        let name = sl
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let Some(n) = sl.get_mut("needs").and_then(Value::as_array_mut) else {
            continue;
        };
        for x in n {
            let yes = x.get("kind").and_then(Value::as_str) != Some(ASK)
                && x.get("key")
                    .and_then(Value::as_str)
                    .is_some_and(|k| acked(&name, k));
            x["acked"] = json!(yes);
        }
    }
}

/// 一片里此刻的键（认可只留这些，别的是过了版本的）。
pub(crate) fn keys_of(sl: &Value) -> Vec<String> {
    sl.get("needs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|x| x.get("key").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// 这个键此刻在不在、能不能认可：不在 ⇒ `None`；在、是问人那一种 ⇒ `Some(false)`。
pub(crate) fn ackable(sl: &Value, key: &str) -> Option<bool> {
    sl.get("needs")
        .and_then(Value::as_array)?
        .iter()
        .find(|x| x.get("key").and_then(Value::as_str) == Some(key))
        .map(|x| x.get("kind").and_then(Value::as_str) != Some(ASK))
}

/// 一片要你看的数：没认可的、不算问人那一种（那一条由会话那一侧数，只算一次）。
pub(crate) fn count_slice(sl: &Value) -> u64 {
    sl.get("needs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|x| x.get("kind").and_then(Value::as_str) != Some(ASK) && x["acked"] != true)
        .count() as u64
}

/// 一格正文的摘要（只用来分辨「改没改」）。
pub(crate) fn body_digest(cell: &Value) -> String {
    super::book::rev_of(
        cell.get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .as_bytes(),
    )
}

/// 一次退回此刻落没落地：那一格底下多出了退回时没有的子格（带它的标题）· 或它的正文改了。
/// 没落地 ⇒ `state` 照送的结局：`returned`（送到了）· `unsure`（送达未知）。`cells` 是那一片此刻的全部格（取新格的标题）。
pub(crate) fn landing(r: &Returned, cell: &Value, cells: &[Value]) -> Value {
    let state = if r.result == "unsure" {
        "unsure"
    } else {
        "returned"
    };
    let mut out = json!({"at": r.at, "to": r.to, "state": state, "by": null, "child": null});
    let now: Vec<&str> = cell
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if let Some(new) = now.iter().find(|c| !r.children.iter().any(|o| o == *c)) {
        let title = cells
            .iter()
            .find(|c| c.get("id").and_then(Value::as_str) == Some(new))
            .and_then(|c| c.get("title"))
            .cloned()
            .unwrap_or(Value::Null);
        out["state"] = json!("landed");
        out["by"] = json!("child");
        out["child"] = json!({"id": new, "title": title});
    } else if body_digest(cell) != r.body {
        out["state"] = json!("landed");
        out["by"] = json!("body");
    }
    out
}

/// 送进会话的那一行：只带事实（人 · 编号 标题：原话），收到人的话怎么办 pb 已经教过 agent。原话里的换行与连串空白并成一个空格（一行送出）。
pub(crate) fn line(id: &str, title: &str, text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    copy_text(
        "bePlan.return.line",
        &[("id", id), ("title", title), ("text", &text)],
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/needs_tests.rs"]
mod tests;
