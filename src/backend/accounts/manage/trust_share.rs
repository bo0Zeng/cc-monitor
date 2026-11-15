//! **信任在各号之间同步** —— 纯：一份配置里信任过哪些目录 · 把几个目录标成信任过（只换那一格）。
//! 一个字节都不写；读盘与落盘在 [`super::trust_share_exec`]。
//!
//! 信任记在哪一格由适配层说（`agents::AccountsFace::trust` 那一格的 [`TrustCells`]）；这里只按那几格读写（JSON 那一形），
//! 或交给那一家自己声明的改法与读法（[`TrustCells::edit`]）；不认任何一家的键名。
//! 只增不减：任何一处信任过的目录，并集标进每个号；从某个号里去掉一格不会传开（下一趟又从别处补回）。

use crate::agents::TrustCells;
use copy_core::copy_text;
use serde_json::Value;
use std::collections::BTreeSet;

/// 一份配置（已解开）里信任过的目录（表不在 / 不是对象 ⇒ 空）。
pub fn trusted(cells: &TrustCells, root: &Value) -> BTreeSet<String> {
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
fn path_of<'a>(cells: &TrustCells, dir: &'a str) -> Vec<&'a str> {
    let mut p = vec![cells.table, dir];
    p.extend(cells.flag);
    p
}

/// 那份配置还不在时当它是什么原文：那一家自己的改法 ⇒ 空串；JSON 那一形 ⇒ 空对象。
pub(crate) fn blank(cells: &TrustCells) -> &'static str {
    if cells.edit.is_some() {
        ""
    } else {
        "{}\n"
    }
}

/// 一份配置的原文里信任过的目录：那一家声明了自己的读法（[`TrustCells::edit`]）⇒ 只经它；没声明 ⇒ 照 JSON 那一形解开再按那几格读。
/// 读不懂 ⇒ `Err(一句原因)`。
pub(crate) fn trusted_in(cells: &TrustCells, text: &str) -> Result<BTreeSet<String>, String> {
    if let Some(edit) = cells.edit {
        return (edit.trusted)(text);
    }
    let v: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    Ok(trusted(cells, &v))
}

/// 原文里把 `dirs` 都标成信任过：只换那几格，别的字节不动；都已经是 ⇒ `Ok(None)`。
/// 那一家声明了自己的改法 ⇒ 只经它；没声明 ⇒ JSON 那一形：原文解不开 / 不是对象 / 表那一层不是对象 ⇒ `Err`、不出新原文。
pub(crate) fn mark(
    cells: &TrustCells,
    text: &str,
    dirs: &BTreeSet<String>,
) -> Result<Option<String>, String> {
    if let Some(edit) = cells.edit {
        return (edit.mark)(text, dirs);
    }
    let root: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| {
        copy_text(
            "beAcctMcpShare.read.unreadable",
            &[("line", &e.line().to_string())],
        )
    })?;
    if !root.is_object() {
        return Err(copy_text("reason.content.unparsable", &[]));
    }
    let have = trusted(cells, &root);
    let paths: Vec<Vec<&str>> = dirs
        .iter()
        .filter(|d| !have.contains(*d))
        .map(|d| path_of(cells, d))
        .collect();
    if paths.is_empty() {
        return Ok(None);
    }
    let edits: Vec<(&[&str], Value)> = paths
        .iter()
        .map(|p| (p.as_slice(), Value::Bool(true)))
        .collect();
    crate::common::json_patch::set_checked(text, &edits)
        .map(Some)
        .ok_or_else(|| copy_text("reason.content.unparsable", &[]))
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/trust_share_tests.rs"]
mod tests;
