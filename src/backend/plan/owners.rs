//! 文件窗口反查：一个目录落在哪一片的仓库里、那里每份文件归哪一格。纯函数，吃标过的成品。
//!
//! - 一片的仓库 ＝ `<工作区>/<path>/`：`path` 是成品里那一片的仓库位置（pb `pb repo <路径>` 登记的、相对工作区根，
//!   如 `repos/app`；照 pb 给的用，没给 ⇒ 不认这一片的仓库）。格的文件声明是相对它的路径。拼这个绝对路径只在 [`repo_dir`]（盯盘也用它）。
//! - 一份文件被几格声明（pb 判据红「两个节点声明同一个文件」）⇒ 排期上先出现的那一格作主，其余进 `dup`（文件那一侧也看得出来）。
//! - 每一格（作主的 · `dup` 里的）都带 `label`「编号 标题」（[`super::product::label`] 那一处拼），文件窗口照它写、不自己拼。
//! - 那一格住哪个权力范围：从它往上找第一个划出的根节点（`blocks[].cells` 里的那一格），写它的标题；住 project ⇒ `null`。
//! - 无主（没有一格声明）照 pb 的 `unowned`：这个目录里的那几份进 `unowned`（文件名）；仓库读不出（pb 给 `paths: null`）⇒ `unowned` 为 null、`unownedWhy` 是 pb 那一句。
//! - 那一片读不成（没读好过 ⇒ `error`；读好过 ⇒ `stale`）⇒ `unreadable` 是那一句、条目空（文件窗口整列写「—」）。

use super::needs::TOP_BLOCK;
use cmd_wire::plan::{PlanFileDup, PlanFileOwner};
use cmd_wire::plan::{PlanFiles, PlanRead, PlanSlice};
use std::collections::BTreeMap;

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
fn block_title(sl: &PlanSlice, id: &str) -> Option<String> {
    let cells: BTreeMap<&str, _> = sl.cells.iter().map(|c| (c.id.as_str(), c)).collect();
    let roots: Vec<&str> = sl
        .blocks
        .iter()
        .filter(|b| b.id != TOP_BLOCK)
        .flat_map(|b| b.cells.iter().map(String::as_str))
        .collect();
    let mut at = Some(id);
    while let Some(i) = at {
        if roots.contains(&i) {
            return cells.get(i).and_then(|c| c.title.clone());
        }
        at = cells.get(i).and_then(|c| c.parent.as_deref());
    }
    None
}

/// 一片的仓库目录（绝对路径，`/` 分隔）：`<工作区>/<path>`。成品里没有 `path` 或是片名不像一片（空 · `.` 起头）⇒ `None`。
pub(crate) fn repo_dir(ws: &str, sl: &PlanSlice) -> Option<String> {
    if sl.name.is_empty() || sl.name.starts_with('.') {
        return None;
    }
    let path = sl.path.as_deref().filter(|p| !p.is_empty())?;
    Some(format!(
        "{}/{}",
        ws.trim_end_matches('/'),
        path.trim_matches('/')
    ))
}

/// 不在任何一片的仓库里（`workspace` 照给的）。
pub(crate) fn nowhere(workspace: Option<String>) -> PlanFiles {
    PlanFiles {
        workspace,
        slice: None,
        unreadable: None,
        entries: Vec::new(),
        unowned: None,
        unowned_why: None,
    }
}

/// `plan-files {dir}` 的本体：`doc` 是一个工作区标过的成品；`dir` 是那台机器上的绝对路径。
pub(crate) fn of_dir(doc: &PlanRead, dir: &str) -> PlanFiles {
    let ws = doc.workspace.as_str();
    for sl in &doc.slices {
        let Some(repo) = repo_dir(ws, sl) else {
            continue;
        };
        let Some(rel) = rel_in(dir, &repo) else {
            continue;
        };
        let mut out = nowhere(Some(ws.to_string()));
        out.slice = Some(sl.name.clone());
        let unreadable = sl
            .stale
            .as_ref()
            .and_then(|s| s.said.clone())
            .or_else(|| sl.error.clone());
        if unreadable.is_some() {
            out.unreadable = unreadable;
            return out;
        }
        // 文件名 ⇒ 作主的那一条（先声明的格）· 其余声明它的格。
        let mut by_name: BTreeMap<String, PlanFileOwner> = BTreeMap::new();
        for c in &sl.cells {
            for f in &c.files {
                let p = f.path.as_str();
                let (parent, file) = p.rsplit_once('/').unwrap_or(("", p));
                if parent != rel || file.is_empty() {
                    continue;
                }
                let label = super::product::label(&c.id, c.title.as_deref());
                match by_name.get_mut(file) {
                    Some(head) => head.dup.push(PlanFileDup {
                        id: c.id.clone(),
                        label,
                        title: c.title.clone(),
                    }),
                    None => {
                        let head = PlanFileOwner {
                            name: file.to_string(),
                            id: c.id.clone(),
                            label,
                            title: c.title.clone(),
                            status_code: c.status_code,
                            status: c.state_line.clone(),
                            block: block_title(sl, &c.id),
                            file_state: f.state_code,
                            file_note: f.note.clone(),
                            dup: Vec::new(),
                        };
                        by_name.insert(file.to_string(), head);
                    }
                }
            }
        }
        out.entries = by_name.into_values().collect();
        // 无主：pb 给的是整片（相对仓库根）；只留落在这个目录里的那几份的文件名。
        out.unowned = sl.unowned.paths.as_ref().map(|ps| {
            ps.iter()
                .filter_map(|p| {
                    let (parent, file) = p.rsplit_once('/').unwrap_or(("", p));
                    (parent == rel && !file.is_empty()).then(|| file.to_string())
                })
                .collect()
        });
        if out.unowned.is_none() {
            out.unowned_why = sl.unowned.why.clone();
        }
        return out;
    }
    nowhere(Some(ws.to_string()))
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/owners_tests.rs"]
mod tests;
