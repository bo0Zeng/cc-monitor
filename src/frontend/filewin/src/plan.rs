//! 文件窗口反查（设计稿 planned-build 06）：目录落在某一片的仓库里 ⇒ 列表多一列「格」· 预览头多一块「归属」· 右键多一项「在计划里看」。
//!
//! 判定全在那台的后端（`plan-files`：这个目录归哪一片、每份文件归哪一格、谁也声明了它）；这里只把回包摊成按名字查的一张表、排版。
//! 别处的目录（不在任何一片里）回包 `slice: null` ⇒ 什么都不多。无主要 pb 给 `unowned`，没给之前「无主」两个字不出。
//! 「在计划里看」经通道交给 monitor（[`filewin_contract::PLAN_OPEN_OP`]）：把主窗口拉到前面、切到计划页、选中那一格。

use copy_core::copy_text;
use std::collections::BTreeMap;

/// 那条线上命令的名字（后端 `plan` 族）。
pub const CMD_PLAN_FILES: &str = "plan-files";

/// 那一格的状态（后端的 `statusCode`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Done,
    Open,
    Dropped,
}

/// 一份文件归的那一格。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    pub id: String,
    /// 标题（pb 没给 ⇒ 编号）。
    pub title: String,
    pub status: Status,
    /// 住的那一块（块根格的标题）；住顶块 ⇒ `None`。
    pub block: Option<String>,
    /// 作数那一条签收的时刻（那台写好的字）。
    pub signed: Option<String>,
    /// 对账「坏」时 pb 给的原因（在 · 缺 · 空 不写）。
    pub broken: Option<String>,
    /// 也声明它的那几格的标题。
    pub dup: Vec<String>,
}

/// 一个目录的反查结果（落在某一片的仓库里才有）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanDir {
    pub workspace: String,
    pub slice: String,
    /// 那一片此刻读不成的那一句 ⇒ 「格」一列整列写「—」。
    pub unreadable: Option<String>,
    /// 文件名 ⇒ 归的那一格。
    pub owners: BTreeMap<String, Owner>,
}

impl PlanDir {
    /// 这一项归哪一格（文件夹 · 没人声明的 ⇒ `None`）。
    pub fn owner(&self, name: &str) -> Option<&Owner> {
        self.owners.get(name)
    }
}

fn s(v: &serde_json::Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// `plan-files` 的回包 → 一张按名字查的表；不在任何一片里（`slice: null`）⇒ `None`。
pub fn from_reply(v: &serde_json::Value) -> Option<PlanDir> {
    let slice = s(v, "slice")?;
    let owners = v
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let id = s(e, "id")?;
            let status = match e.get("statusCode").and_then(serde_json::Value::as_str) {
                Some("done") => Status::Done,
                Some("dropped") => Status::Dropped,
                _ => Status::Open,
            };
            let broken = (e.get("fileState").and_then(serde_json::Value::as_str) == Some("broken"))
                .then(|| s(e, "fileNote").unwrap_or_default());
            let dup = e
                .get("dup")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|d| s(d, "title").or_else(|| s(d, "id")))
                .collect();
            Some((
                s(e, "name")?,
                Owner {
                    title: s(e, "title").unwrap_or_else(|| id.clone()),
                    id,
                    status,
                    block: s(e, "block"),
                    signed: s(e, "signAtText"),
                    broken,
                    dup,
                },
            ))
        })
        .collect();
    Some(PlanDir {
        workspace: s(v, "workspace").unwrap_or_default(),
        slice,
        unreadable: s(v, "unreadable"),
        owners,
    })
}

/// 状态的图标（同计划页：做完了 ✓ · 没做完 ○ · 不做了 ⊘）。
pub fn status_icon(st: Status) -> &'static str {
    match st {
        Status::Done => egui_phosphor::regular::CHECK_CIRCLE,
        Status::Open => egui_phosphor::regular::CIRCLE,
        Status::Dropped => egui_phosphor::regular::PROHIBIT,
    }
}

/// 列名。
pub fn column_label() -> String {
    copy_text("rsFilewinPlan.col.title", &[])
}

/// 读不成那一片时「格」一列每一格写的字。
pub fn unreadable_mark() -> String {
    copy_text("rsFilewinPlan.cell.unreadable", &[])
}

/// 也声明它的那一句（悬停 · 预览头那一行红字）。
pub fn dup_text(o: &Owner) -> String {
    copy_text(
        "rsFilewinPlan.cell.dup",
        &[(
            "title",
            &o.dup.join(&copy_text("rsFilewinPlan.cell.listSep", &[])),
        )],
    )
}

/// 预览头「归属」那一块的第二行：片 · 块「…」· 签 …（没有的那一段不写）。
pub fn owner_line(slice: &str, o: &Owner) -> String {
    let mut parts = vec![slice.to_string()];
    if let Some(b) = &o.block {
        parts.push(copy_text("rsFilewinPlan.owner.block", &[("block", b)]));
    }
    if let Some(t) = &o.signed {
        parts.push(copy_text("rsFilewinPlan.owner.signed", &[("time", t)]));
    }
    parts.join(&copy_text("rsFilewinRows.tip.sep", &[]))
}

/// 右键菜单那一项的字。
pub fn menu_label(o: &Owner) -> String {
    copy_text("rsFilewinPlan.menu.open", &[("title", &o.title)])
}

/// 预览头那颗按钮的字。
pub fn open_label() -> String {
    copy_text("rsFilewinPlan.owner.open", &[])
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/plan_tests.rs"]
mod tests;
