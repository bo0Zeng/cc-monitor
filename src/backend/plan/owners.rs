//! 文件窗口反查（设计稿 planned-build 06）：一个目录落在哪一片的仓库里、那里每份文件归哪一格。纯函数，吃标过的成品。
//!
//! - 一片的仓库 ＝ `<工作区>/<片名>/`（pb 的盘面：做出来的东西一片一个目录；格的文件声明是相对它的路径）。
//!   pb 还没在 dump 里给每片的仓库目录 ⇒ 照这个盘面拼；请求单第 8 条请 pb 给 `repo_dir`，给了就换成它给的。
//! - 一份文件被几格声明（pb 判据红「两个节点声明同一个文件」）⇒ 排期上先出现的那一格作主，其余进 `dup`（文件那一侧也看得出来）。
//! - 那一格住哪一块：从它往上找第一个块根格（`blocks[].cells` 里的那一格），写块根格的标题；住顶块 ⇒ `null`。
//! - 无主（没有一格声明）要 pb 给 `unowned`（请求单 3）；没给之前 `unowned` 恒 `null`，文件窗口不出「无主」两个字。
//! - 那一片读不成（没读好过 ⇒ `error`；读好过 ⇒ `stale`）⇒ `unreadable` 是那一句、条目空（文件窗口整列写「—」）。

use super::needs::TOP_BLOCK;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// 目录相对一片仓库的那一段（`""` ＝ 仓库根）；不在 ⇒ `None`。按段比，不按字符前缀比。
fn rel_in<'a>(dir: &'a str, repo: &str) -> Option<&'a str> {
    let d = dir.trim_end_matches('/');
    let r = repo.trim_end_matches('/');
    match d.strip_prefix(r)? {
        "" => Some(""),
        rest => rest.strip_prefix('/'),
    }
}

/// 那一格住哪一块：往上找第一个块根格；住顶块 ⇒ `None`。回块根格的标题。
fn block_title(sl: &Value, id: &str) -> Option<String> {
    let cells: BTreeMap<&str, &Value> = sl
        .get("cells")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|c| Some((str_of(c, "id")?, c)))
        .collect();
    let roots: Vec<&str> = sl
        .get("blocks")
        .and_then(Value::as_array)?
        .iter()
        .filter(|b| str_of(b, "id") != Some(TOP_BLOCK))
        .flat_map(|b| {
            b.get("cells")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(Value::as_str)
        .collect();
    let mut at = Some(id);
    while let Some(i) = at {
        if roots.contains(&i) {
            return cells
                .get(i)
                .and_then(|c| str_of(c, "title"))
                .map(str::to_string);
        }
        at = cells.get(i).and_then(|c| str_of(c, "parent"));
    }
    None
}

/// `plan-files {dir}` 的本体：`doc` 是一个工作区标过的成品；`dir` 是那台机器上的绝对路径。
pub(crate) fn of_dir(doc: &Value, dir: &str) -> Value {
    let ws = str_of(doc, "workspace").unwrap_or_default();
    let none =
        json!({"workspace": ws, "slice": null, "unreadable": null, "entries": [], "unowned": null});
    let Some(slices) = doc.get("slices").and_then(Value::as_array) else {
        return none;
    };
    for sl in slices {
        let Some(name) = str_of(sl, "name").filter(|n| !n.is_empty() && !n.starts_with('.')) else {
            continue;
        };
        let repo = format!("{}/{name}", ws.trim_end_matches('/'));
        let Some(rel) = rel_in(dir, &repo) else {
            continue;
        };
        let unreadable = sl
            .get("stale")
            .and_then(|s| str_of(s, "said"))
            .or_else(|| str_of(sl, "error"));
        if let Some(said) = unreadable {
            return json!({"workspace": ws, "slice": name, "unreadable": said, "entries": [], "unowned": null});
        }
        // 文件名 ⇒ 作主的那一条（先声明的格）· 其余声明它的格。
        let mut by_name: BTreeMap<String, (Value, Vec<Value>)> = BTreeMap::new();
        for c in sl
            .get("cells")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = str_of(c, "id").unwrap_or_default();
            for f in c
                .get("files")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(p) = str_of(f, "path") else { continue };
                let (parent, file) = p.rsplit_once('/').unwrap_or(("", p));
                if parent != rel || file.is_empty() {
                    continue;
                }
                let me = json!({"id": id, "title": c.get("title").cloned().unwrap_or(Value::Null)});
                match by_name.get_mut(file) {
                    Some((_, dup)) => dup.push(me),
                    None => {
                        let sign = c
                            .get("signs")
                            .and_then(Value::as_array)
                            .and_then(|a| a.last())
                            .and_then(|g| g.get("atText"))
                            .cloned()
                            .unwrap_or(Value::Null);
                        let head = json!({
                            "name": file,
                            "id": id,
                            "title": c.get("title").cloned().unwrap_or(Value::Null),
                            "statusCode": c.get("statusCode").cloned().unwrap_or(Value::Null),
                            "status": c.get("status").cloned().unwrap_or(Value::Null),
                            "block": block_title(sl, id),
                            "signAtText": sign,
                            "fileState": f.get("stateCode").cloned().unwrap_or(Value::Null),
                            "fileNote": f.get("note").cloned().unwrap_or(Value::Null),
                        });
                        by_name.insert(file.to_string(), (head, Vec::new()));
                    }
                }
            }
        }
        let entries: Vec<Value> = by_name
            .into_values()
            .map(|(mut head, dup)| {
                head["dup"] = json!(dup);
                head
            })
            .collect();
        return json!({"workspace": ws, "slice": name, "unreadable": null, "entries": entries, "unowned": null});
    }
    none
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/owners_tests.rs"]
mod tests;
