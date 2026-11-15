//! **桌面文件窗口那一份菜单成品**（`files-menu`，寻址本机）：选中这几项 ⇒ 右键菜单那几组 ＋ 十四件每件此刻的样子（[`desk_menu`]）。
//! 种类 · 做不了的为什么 · 此刻做不到的 · 一件事的字照 `pick-core`（手机核心同一份判法）；这里只有桌面那张表（[`DESK`]）、
//! 菜单项的字与「这一件不适用」那几句。窗口照回包画、照 `acts` 灰命令栏 · 拦键盘，不另判。

use copy_core::{copy_text, write_refused};
use pick_core::{
    blocked, groups_of, label, picked_kind, refused_here, Pick, PickIn, Picked, Table,
};

pub use cmd_wire::files::{
    ActState, Action, Cut, DeskEntry, DeskMenu, FilesMenuReq, Here, Item, LsEntry,
};

use Action as A;

/// 🔴 **桌面那张表**：一种选中 ⇒ 几组项，组与组之间一道线，危险的那一组在最后。算大小只给目录与多选 · 解压只给压缩包 ·
/// 查看给图片 · 编辑给文本。
const DESK: Table = &[
    (
        Picked::Here,
        &[&[Item::Mkdir, Item::NewFile, Item::Upload], &[Item::Term]],
    ),
    (
        Picked::Dir,
        &[
            &[Item::Act(A::Open), Item::Act(A::Copy)],
            &[
                Item::Act(A::Size),
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::DirLink,
        &[
            &[Item::Act(A::Open), Item::Act(A::Copy)],
            &[
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::Text,
        &[
            &[
                Item::Act(A::Edit),
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Copy),
            ],
            &[
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::Image,
        &[
            &[
                Item::Act(A::View),
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Copy),
            ],
            &[
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::Archive,
        &[
            &[
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Copy),
            ],
            &[
                Item::Act(A::Extract),
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::File,
        &[
            &[
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Copy),
            ],
            &[
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::BrokenLink,
        &[
            &[Item::Act(A::Rename), Item::Act(A::Properties)],
            &[Item::Act(A::Delete)],
        ],
    ),
    (Picked::NamelessDir, &[&[Item::Act(A::Open)]]),
    (Picked::Nameless, &[]),
    (
        Picked::Many,
        &[
            &[
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Across),
            ],
            &[Item::Act(A::Size), Item::Act(A::Chmod)],
            &[Item::Act(A::Delete)],
        ],
    ),
    (Picked::ManyNameless, &[]),
];

/// 单选的那一份归某一格 ⇒ 多这一组，插在删除那一组之前。
const PLAN_GROUP: &[Item] = &[Item::InPlan];

/// 菜单上那一项的字。`n` ＝ 选中了几项；`plan` ＝ 那一格的标题。要先问一句才做的那几件（改权限 · 删除）字后带「…」。
fn item_text(i: Item, n: usize, plan: Option<&str>) -> String {
    match i {
        Item::Act(A::Delete) if n > 1 => {
            copy_text("rsFilewinSelect.menu.deleteMany", &[("n", &n.to_string())])
        }
        Item::Act(A::Delete) => copy_text("history.menu.delete", &[]),
        Item::Act(A::Chmod) => copy_text("rsFilewinSelect.menu.chmod", &[]),
        Item::Act(a) => label(a, n),
        Item::InPlan => copy_text(
            "rsFilewinPlan.menu.open",
            &[("title", plan.unwrap_or_default())],
        ),
        // 空白处那个菜单上没有「新建 ▾」那一层，字写全。
        Item::Mkdir => copy_text("rsFilewinWriteops.inline.newDir", &[]),
        Item::NewFile => copy_text("rsFilewinSelect.menu.newFile", &[]),
        Item::Upload => copy_text("rsFilewinUpload.label.upload", &[]),
        Item::Term => copy_text("mobDrawer.item.terminal", &[]),
        Item::SelectAll => copy_text("mobFiles.menu.selectAll", &[]),
    }
}

/// 表里没有这一件（这一种选中做不了）⇒ 说的那一句（不许静默：按了 F2 没反应与「改完了」同形）。
fn not_in_table(a: Action, n: usize) -> String {
    let n_s = n.to_string();
    match a {
        A::Delete => copy_text("rsFilewinSelect.refusal.deleteBadNames", &[]),
        A::Chmod if n > 1 => copy_text("rsFilewinSelect.refusal.chmodBadNames", &[]),
        A::Open | A::Edit | A::View => copy_text("rsFilewinSelect.refusal.openOne", &[("n", &n_s)]),
        A::Rename if n > 1 => copy_text("rsFilewinSelect.refusal.renameOne", &[("n", &n_s)]),
        _ => copy_text("rsFilewinSelect.refusal.cannot", &[("label", &label(a, n))]),
    }
}

/// 菜单上一项此刻做不了的为什么：这一份做不了 · 这台做不到 · 断着（对这个目录的那几项要碰那台）。
fn item_why(i: Item, picked: &[Pick], here: &Here) -> Option<String> {
    match i {
        Item::Act(a) => blocked(a, picked).or_else(|| refused_here(a, here)),
        Item::Mkdir | Item::NewFile | Item::Upload | Item::Term => write_refused(here.offline),
        Item::InPlan | Item::SelectAll => None,
    }
}

/// ★ 选中这几项（此刻那一台 [`Here`] · 归哪一格）⇒ [`DeskMenu`]。
pub fn desk_menu(req: &FilesMenuReq) -> DeskMenu {
    let rows: Vec<PickIn> = req.picked.iter().map(PickIn::of_ls).collect();
    let picked: Vec<Pick> = rows.iter().map(PickIn::pick).collect();
    let n = picked.len();
    let kind = picked_kind(&picked);
    let mut table = groups_of(DESK, kind);
    let in_table: Vec<Action> = table
        .iter()
        .flatten()
        .filter_map(|i| match i {
            Item::Act(a) => Some(*a),
            _ => None,
        })
        .collect();
    if req.plan.is_some() {
        let at = table
            .iter()
            .position(|g| g.contains(&Item::Act(A::Delete)))
            .unwrap_or(table.len());
        table.insert(at, PLAN_GROUP.to_vec());
    }
    let groups = table
        .into_iter()
        .map(|g| {
            g.into_iter()
                .map(|item| DeskEntry {
                    item,
                    text: item_text(item, n, req.plan.as_deref()),
                    why: item_why(item, &picked, &req.here),
                })
                .collect()
        })
        .collect();
    let acts = Action::ALL
        .iter()
        .map(|&a| ActState {
            action: a,
            text: item_text(Item::Act(a), n.max(1), None),
            // 这一份做不了的那一句比「表里没有这一件」具体（编辑一份二进制 ⇒ 说编辑门那一句，不说「只能打开一项」）⇒ 先说它。
            why: if n == 0 {
                Some(copy_text("rsFilewinChrome.command.pickFirst", &[]))
            } else if let Some(w) = blocked(a, &picked) {
                Some(w)
            } else if !in_table.contains(&a)
                // 「复制到另一栏」右键菜单只给多选（单项那一种表里没有它），命令栏那一颗对单项照样做：寻址得到就行。
                && !(a == A::Across && picked.iter().all(|p| p.copyable()))
            {
                Some(not_in_table(a, n))
            } else {
                refused_here(a, &req.here)
            },
        })
        .collect();
    DeskMenu { groups, acts }
}

#[cfg(test)]
#[path = "../../../tests/backend/files/menu_tests.rs"]
mod tests;
