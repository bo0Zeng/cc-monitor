//! **补足一组会话**（帧命令 `set-plan`）：一组 ＝ 几行（目录 ＋ 别名 ＋ 个数），每行数「这台上 · 工作目录就是这一行的目录 ·
//! 用这一行的别名起的 · agent 还在」的，只起差的几个。判定只住这里（[`plan_rows`]，纯函数）；事实由入口交
//! （`faces/session_set_face.rs`：活会话表 · 启动台账里的 `aliases` · 配置文件）。
//!
//! - 「用哪条别名起的」只认启动台账记下的（`ccm @名` 那一发留的便条，`control::launch_account`）；不经别名起的不算进任何一行。
//! - 目录照这台的家目录展开 `~` 再比，末尾的 `/` 不算。
//! - 起不了的那一行（别名不存在 · 别名合不下来 · 目录不存在）照数已有的、不算进「再起」，别的行照起。
//! - 组名：行里写了 `tab-group` 就是它；否则取别名的名字，这组里同一条别名出现在不止一个目录 ⇒「别名 · 目录末一段」。

use crate::assets::aliases::profile::{self, Book, RowEdit};
use copy_core::copy_text;
use serde::Serialize;

/// 这台此刻活着的一条会话：会话 id · 工作目录 · 经哪条别名起的（台账里没有 ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Live {
    pub(crate) sid: String,
    pub(crate) cwd: String,
    pub(crate) alias: Option<String>,
}

/// 一行补足的样子。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowPlan {
    pub(crate) cwd: String,
    pub(crate) alias: String,
    /// 要补足到几个。
    pub(crate) want: u8,
    /// 此刻已有几个。
    pub(crate) have: u32,
    /// 这一下再起几个（起不了 ⇒ 0）。
    pub(crate) more: u32,
    /// 起出来的进哪个标签页组。
    pub(crate) group: String,
    /// 这一行起不了的原因（能起 ⇒ `null`）。
    pub(crate) blocked: Option<String>,
}

/// 整组补足的样子：每行 ＋ 合计再起几个。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetPlan {
    pub(crate) rows: Vec<RowPlan>,
    pub(crate) more: u32,
}

/// 比目录用的那一形：展开 `~`、去掉末尾的分隔符（根目录本身留着）。
fn dir_key(d: &str, home: Option<&std::path::Path>) -> String {
    let full = crate::platform::paths::expand_tilde_from(d.trim(), home);
    let t = full.trim_end_matches(['/', '\\']);
    if t.is_empty() {
        full
    } else {
        t.to_string()
    }
}

/// 目录末一段（组名缺省用它分开同一条别名的几行）。
fn last_seg(d: &str) -> &str {
    d.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(d)
}

/// 这一行进哪个组（规则见头注）。
fn group_of(r: &RowEdit, rows: &[RowEdit]) -> String {
    if let Some(g) = &r.tab_group {
        return g.clone();
    }
    let base = r.alias.clone();
    if rows.iter().filter(|o| o.alias == r.alias).count() > 1 {
        copy_text(
            "beSessionSet.group.withDir",
            &[("group", &base), ("dir", last_seg(&r.cwd))],
        )
    } else {
        base
    }
}

/// 这一行起不了的原因：别名不存在 · 别名合不下来 · 目录不存在。
fn blocked_of(
    r: &RowEdit,
    book: &Book,
    home: Option<&std::path::Path>,
    is_dir: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if book.find(&r.alias).is_none() {
        return Some(copy_text(
            "beSessionSet.alias.missing",
            &[("alias", &r.alias)],
        ));
    }
    if let Err(e) = profile::resolve(book, &r.alias, &[]) {
        return Some(e);
    }
    let d = crate::platform::paths::expand_tilde_from(r.cwd.trim(), home);
    if !is_dir(&d) {
        return Some(copy_text("beSessionSet.dir.missing", &[("cwd", &r.cwd)]));
    }
    None
}

/// **补足的唯一判法**：每行数已有、算再起几个、定组名、说起不了的原因。只读，一个字节不写。
pub(crate) fn plan_rows(
    rows: &[RowEdit],
    book: &Book,
    live: &[Live],
    home: Option<&std::path::Path>,
    is_dir: &dyn Fn(&str) -> bool,
) -> SetPlan {
    let out: Vec<RowPlan> = rows
        .iter()
        .map(|r| {
            let key = dir_key(&r.cwd, home);
            let have = live
                .iter()
                .filter(|l| l.alias.as_deref() == Some(r.alias.as_str()))
                .filter(|l| dir_key(&l.cwd, home) == key)
                .count() as u32;
            let blocked = blocked_of(r, book, home, is_dir);
            let more = match blocked {
                Some(_) => 0,
                None => u32::from(r.n).saturating_sub(have),
            };
            RowPlan {
                cwd: r.cwd.clone(),
                alias: r.alias.clone(),
                want: r.n,
                have,
                more,
                group: group_of(r, rows),
                blocked,
            }
        })
        .collect();
    SetPlan {
        more: out.iter().map(|r| r.more).sum(),
        rows: out,
    }
}

/// `set-plan` 的入参：存好的哪一组（`name`），或框里还没存的那几行（`rows`），两格给一格。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SetPlanReq {
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) name: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) rows: Option<Vec<RowEdit>>,
}

crate::wire::typed::codes! {
    /// `set-plan` 的码（入参不对是通用的 `bad_args`）：没有那一组 · 那一组写错或配置文件读不了（那一句说清哪一样）。
    pub(crate) enum SetPlanErr {
        NoSuchSet = "no_such_set",
        Refused = "refused",
    }
}

/// `set-plan` 的本体（配置文件 · 活着的会话 · 目录在不在由入口交）。
pub(crate) fn answer(
    req: &SetPlanReq,
    book: &Book,
    live: &[Live],
    home: Option<&std::path::Path>,
    is_dir: &dyn Fn(&str) -> bool,
) -> Result<SetPlan, (&'static str, String)> {
    let rows = match (&req.name, &req.rows) {
        (Some(name), None) => {
            let s = book.find_set(name).ok_or_else(|| {
                (
                    "no_such_set",
                    copy_text("beSessionSet.set.unknown", &[("name", name)]),
                )
            })?;
            if let Some(p) = s.problems.first() {
                return Err((
                    "refused",
                    copy_text(
                        "beSessionSet.set.broken",
                        &[("name", name), ("why", &p.message)],
                    ),
                ));
            }
            profile::edit_of_set(s).rows
        }
        (None, Some(rows)) => rows.clone(),
        _ => {
            return Err((
                "bad_args",
                crate::common::contract::malformed(
                    "`set-plan` takes exactly one of `name` and `rows`",
                ),
            ))
        }
    };
    Ok(plan_rows(&rows, book, live, home, is_dir))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_set_tests.rs"]
mod tests;
