//! **信任在各号之间同步** —— 纯：一份配置里信任过哪些目录 · 把几个目录标成信任过（只换那一格）。
//! 一个字节都不写；读盘与落盘在 [`super::trust_share_exec`]。
//!
//! 信任记在哪一格由适配层说（`agents::AccountsFace::trust` 那一格的 [`TrustCells`]）；这里只按那几格读写，不认任何一家的键名。
//! 只增不减：任何一处信任过的目录，并集标进每个号；从某个号里去掉一格不会传开（下一趟又从别处补回）。

use super::json_key;
use crate::agents::TrustCells;
use copy_core::copy_text;
use serde_json::Value;
use std::collections::BTreeSet;

/// 一份配置（已解开）里信任过的目录（表不在 / 不是对象 ⇒ 空）。
pub(crate) fn trusted(cells: &TrustCells, root: &Value) -> BTreeSet<String> {
    let Some(table) = root.get(cells.table).and_then(Value::as_object) else {
        return BTreeSet::new();
    };
    table
        .iter()
        .filter(|(_, e)| is_on(cells, e))
        .map(|(d, _)| d.clone())
        .collect()
}

fn is_on(cells: &TrustCells, entry: &Value) -> bool {
    let cell = match cells.flag {
        None => Some(entry),
        Some(f) => entry.get(f),
    };
    cell == Some(&Value::Bool(true))
}

/// 一个目录那一格的键路。
fn path_of(cells: &TrustCells, dir: &str) -> Vec<String> {
    let mut p = vec![cells.table.to_string(), dir.to_string()];
    p.extend(cells.flag.map(str::to_string));
    p
}

/// 原文里把 `dirs` 都标成信任过：只换那几格，别的字节不动；都已经是 ⇒ `Ok(None)`。
/// 原文解不开 / 不是对象 / 表那一层不是对象 ⇒ `Err`、不出新原文。
pub(crate) fn mark(
    cells: &TrustCells,
    text: &str,
    dirs: &BTreeSet<String>,
) -> Result<Option<String>, String> {
    let root: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| {
        copy_text(
            "beAcctMcpShare.read.badJson",
            &[("line", &e.line().to_string())],
        )
    })?;
    if !root.is_object() {
        return Err(copy_text("beAcctMcpShare.read.notObject", &[]));
    }
    let have = trusted(cells, &root);
    let edits: Vec<(Vec<String>, Value)> = dirs
        .iter()
        .filter(|d| !have.contains(*d))
        .map(|d| (path_of(cells, d), Value::Bool(true)))
        .collect();
    if edits.is_empty() {
        return Ok(None);
    }
    json_key::set_paths(text, &edits).map(Some)
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/trust_share_tests.rs"]
mod tests;
