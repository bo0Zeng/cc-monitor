//! **手机文件页那一张表**：选中这几项 ⇒ 顶栏那几颗 ＋ ⋯ 里那几项 ＋ 点一下做什么（[`phone_menu`]：每项 `id · text · enabled · why`）。
//! 种类 · 做不了的为什么 · 此刻做不到的 · 一件事的字都照 `pick-core`（桌面文件窗口同一份判法与字），这里只有手机那张表与交回的词。

use copy_core::copy_text;
use pick_core::{
    blocked, groups_of, label, picked_kind, refused_here, Action, Here, Item, Pick, Picked, Table,
};

/// 手机文件页选中这几项时交回的那一份（线上形住契约 `cmd_wire::phone`，Kotlin 读同一份生成的类）。
pub use cmd_wire::phone::{MenuEntry, PhoneMenu};

use Action as A;

/// 顶栏那一组：下载 · 删除（下载只收文件：选中里有目录 ⇒ 灰着、说哪几项）。
const BAR: &[Item] = &[Item::Act(A::Download), Item::Act(A::Delete)];
/// 选一个文件 ⋯ 里那几项。
const ONE: &[Item] = &[
    Item::Act(A::Rename),
    Item::Act(A::CopyTo),
    Item::Act(A::Chmod),
    Item::SelectAll,
];

/// 选一份文本 ⋯ 里那几项（打头是预览 · 编辑）。
const TEXT: &[Item] = &[
    Item::Act(A::View),
    Item::Act(A::Edit),
    Item::Act(A::Rename),
    Item::Act(A::CopyTo),
    Item::Act(A::Chmod),
    Item::SelectAll,
];

/// 🔴 **手机那张表**：第一组顶栏、第二组 ⋯。查看在手机上是文本预览（手机不开看图页）；没有解压 · 算大小 · 另一台 · 另一栏
/// （手机直连每一台，同一台换个目录就是另一栏那件事）；没有计划那一组。
const PHONE: Table = &[
    (Picked::Here, &[]),
    (Picked::Dir, &[BAR, ONE]),
    (Picked::DirLink, &[BAR, ONE]),
    (Picked::Text, &[BAR, TEXT]),
    (Picked::Image, &[BAR, ONE]),
    (Picked::Archive, &[BAR, ONE]),
    (Picked::File, &[BAR, ONE]),
    (
        Picked::BrokenLink,
        &[
            &[Item::Act(A::Delete)],
            &[Item::Act(A::Rename), Item::SelectAll],
        ],
    ),
    (Picked::NamelessDir, &[&[], &[Item::SelectAll]]),
    (Picked::Nameless, &[&[], &[Item::SelectAll]]),
    (
        Picked::Many,
        &[
            BAR,
            &[Item::SelectAll, Item::Act(A::CopyTo), Item::Act(A::Chmod)],
        ],
    ),
    (Picked::ManyNameless, &[&[], &[Item::SelectAll]]),
];

/// 🔴 **点一下那一行做什么**（同桌面双击）：目录 ⇒ 进去 · 文本 ⇒ 预览 · 别的文件 ⇒ 下载；断链 · 名字读不出的文件 ⇒ 不做。
const TAP: &[(Picked, Action)] = &[
    (Picked::Dir, A::Open),
    (Picked::DirLink, A::Open),
    (Picked::NamelessDir, A::Open),
    (Picked::Text, A::View),
    (Picked::Image, A::Download),
    (Picked::Archive, A::Download),
    (Picked::File, A::Download),
];

/// 那一项交回 Kotlin 的词。手机表里没有的那几项 ⇒ `None`。
fn id_of(i: Item) -> Option<&'static str> {
    Some(match i {
        Item::Act(A::Open) => "open",
        Item::Act(A::View) => "preview",
        Item::Act(A::Edit) => "edit",
        Item::Act(A::Download) => "download",
        Item::Act(A::Delete) => "delete",
        Item::Act(A::Rename) => "rename",
        Item::Act(A::CopyTo) => "copyTo",
        Item::Act(A::Chmod) => "chmod",
        Item::SelectAll => "selectAll",
        _ => return None,
    })
}

/// 那一项的字：一件事照 [`label`]（`n` ＝ 选中了几项）；全选是手机那一项。
fn text_of(i: Item, n: usize) -> String {
    match i {
        Item::Act(a) => label(a, n),
        _ => copy_text("mobFiles.menu.selectAll", &[]),
    }
}

/// ★ 文件页选中这几项（此刻那一台 [`Here`]）⇒ [`PhoneMenu`]。
pub fn phone_menu(picked: &[Pick], here: &Here) -> PhoneMenu {
    let kind = picked_kind(picked);
    let mut groups = groups_of(PHONE, kind).into_iter().map(|g| {
        g.into_iter()
            .filter_map(|i| {
                let id = id_of(i)?;
                let why = match i {
                    Item::Act(a) => blocked(a, picked).or_else(|| refused_here(a, here)),
                    _ => None,
                };
                Some(MenuEntry {
                    id,
                    text: text_of(i, picked.len()),
                    enabled: why.is_none(),
                    why,
                })
            })
            .collect::<Vec<_>>()
    });
    let bar = groups.next().unwrap_or_default();
    let more = groups.next().unwrap_or_default();
    let tap = TAP
        .iter()
        .find(|(k, _)| *k == kind)
        .and_then(|(_, a)| id_of(Item::Act(*a)));
    PhoneMenu { bar, more, tap }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmd_wire::files::{FileClass as C, Gate, Gates, PreviewGate};
    use pick_core::Cut;

    fn gates(view: bool) -> Gates {
        Gates {
            preview: PreviewGate::No(String::new()),
            view: view.then_some(Gate::Cap(1)),
            edit: Gate::Cap(1),
        }
    }

    fn file<'a>(name: &'a str, class: C, g: &'a Gates) -> Pick<'a> {
        Pick {
            name,
            is_dir: false,
            link: false,
            link_dir: false,
            link_broken: false,
            lossy: false,
            raw: false,
            class,
            gates: Some(g),
        }
    }

    fn dir(name: &str) -> Pick<'_> {
        Pick {
            name,
            is_dir: true,
            link: false,
            link_dir: false,
            link_broken: false,
            lossy: false,
            raw: false,
            class: C::Folder,
            gates: None,
        }
    }

    fn ids(m: &PhoneMenu) -> Vec<Vec<(&'static str, bool)>> {
        [&m.bar, &m.more]
            .iter()
            .map(|g| g.iter().map(|e| (e.id, e.enabled)).collect())
            .collect()
    }

    fn menu(picked: &[Pick]) -> PhoneMenu {
        phone_menu(picked, &Here::default())
    }

    /// ★ 手机那一份：顶栏下载 · 删除，⋯ 改名 · 复制到 · 改权限 · 全选（单选文本打头多预览 · 编辑）；选中里有目录 ⇒ 下载灰着、说哪几项；
    /// 多选改权限说项数；断链只能删、改名；名字寻址不到的只剩全选。
    #[test]
    fn the_phone_gets_its_bar_and_more_with_why_for_the_grey_ones() {
        let g = gates(false);
        let one = menu(&[file("a.txt", C::Text, &g)]);
        assert_eq!(
            ids(&one),
            vec![
                vec![("download", true), ("delete", true)],
                vec![
                    ("preview", false),
                    ("edit", true),
                    ("rename", true),
                    ("copyTo", true),
                    ("chmod", true),
                    ("selectAll", true)
                ],
            ]
        );
        let d = menu(&[dir("docs")]);
        assert_eq!(
            (d.bar[0].enabled, d.bar[0].why.clone()),
            (
                false,
                Some(copy_text(
                    "rsFilewinSelect.why.dirDownload",
                    &[("names", "docs")]
                ))
            )
        );
        let many = menu(&[dir("docs"), file("a.txt", C::Text, &g), dir("src")]);
        let sep = copy_text("kit.text.sep", &[]);
        assert_eq!(
            many.bar[0].why,
            Some(copy_text(
                "rsFilewinSelect.why.dirDownload",
                &[("names", &format!("docs{sep}src"))]
            ))
        );
        assert_eq!(
            ids(&many)[1],
            vec![("selectAll", true), ("copyTo", true), ("chmod", true)]
        );
        assert_eq!(
            many.more[2].text,
            copy_text("rsFilewinSelect.label.chmodMany", &[("n", "3")])
        );
        let three = menu(&[
            file("a", C::Text, &g),
            file("b", C::Text, &g),
            file("c", C::Text, &g),
        ]);
        assert_eq!(
            (three.bar[0].text.clone(), three.bar[1].text.clone()),
            (
                copy_text("rsFilewinSelect.label.downloadMany", &[("n", "3")]),
                copy_text("rsFilewinSelect.label.deleteMany", &[("n", "3")])
            ),
            "多选三项：顶栏下载 · 删除与桌面命令栏那一项同字（带「3 项」）"
        );
        let files = menu(&[file("a", C::Text, &g), file("b", C::Text, &g)]);
        assert!(
            files.bar[0].enabled && files.bar[0].why.is_none(),
            "全是文件 ⇒ 下载可点"
        );
        let mut broken = file("l", C::Other, &g);
        broken.link = true;
        broken.link_broken = true;
        assert_eq!(
            ids(&menu(&[broken])),
            vec![
                vec![("delete", true)],
                vec![("rename", true), ("selectAll", true)]
            ]
        );
        let mut nameless = file("x", C::Text, &g);
        nameless.lossy = true;
        assert_eq!(
            ids(&menu(&[nameless])),
            vec![vec![], vec![("selectAll", true)]]
        );
        assert_eq!(ids(&menu(&[])), vec![vec![], vec![]], "什么都没选 ⇒ 不出");
    }

    /// ★ 手机单选文本的预览 · 编辑照那台的两道门：读得了 ⇒ 可点；预览门说不 ⇒ 灰着说那一句；编辑门说不 ⇒ 灰着说那一句。
    #[test]
    fn the_phone_previews_and_edits_by_the_gates() {
        let open = Gates {
            preview: PreviewGate::Text(cmd_wire::files::TextPreview { cap: 9, lang: None }),
            view: None,
            edit: Gate::Cap(9),
        };
        let m = menu(&[file("a.txt", C::Text, &open)]);
        assert_eq!(ids(&m)[1][..2], [("preview", true), ("edit", true)]);
        let shut = Gates {
            preview: PreviewGate::No("二进制".into()),
            view: None,
            edit: Gate::No("太大".into()),
        };
        let m = menu(&[file("a.bin", C::Text, &shut)]);
        assert_eq!(
            (m.more[0].why.as_deref(), m.more[1].why.as_deref()),
            (Some("二进制"), Some("太大"))
        );
        assert_eq!(
            m.more[0].text,
            copy_text("tabBatch.result.view", &[]),
            "预览那一项的字同桌面查看那一项"
        );
    }

    /// ★ 点一下做什么由表给：目录 · 指向目录的链接 ⇒ 进去；文本 ⇒ 预览；别的文件 ⇒ 下载；断链 · 名字读不出的文件 ⇒ 什么都不做；多选 ⇒ 不给。
    #[test]
    fn a_tap_does_what_the_table_says() {
        let g = gates(false);
        let mut dlink = file("dl", C::Folder, &g);
        dlink.link = true;
        dlink.link_dir = true;
        let mut broken = file("l", C::Other, &g);
        broken.link = true;
        broken.link_broken = true;
        let mut nameless = file("x", C::Text, &g);
        nameless.lossy = true;
        let taps: Vec<Option<&str>> = [
            vec![dir("d")],
            vec![dlink],
            vec![file("a.txt", C::Text, &g)],
            vec![file("a.zip", C::Archive, &g)],
            vec![file("a.pdf", C::Pdf, &g)],
            vec![broken],
            vec![nameless],
            vec![dir("d"), dir("e")],
        ]
        .iter()
        .map(|p| menu(p).tap)
        .collect();
        assert_eq!(
            taps,
            vec![
                Some("open"),
                Some("open"),
                Some("preview"),
                Some("download"),
                Some("download"),
                None,
                None,
                None
            ]
        );
    }

    /// ★ 此刻做不到的两种：那台断着 ⇒ 除了进目录全灰、说「离线 · 只读」；那台说做不到那条命令（改权限）⇒ 那一项灰着说这台不支持。
    #[test]
    fn the_phone_greys_what_the_machine_cannot_do_now() {
        let g = gates(false);
        let off = Here {
            offline: Some(Cut::Machine),
            unavailable: vec![],
        };
        let m = phone_menu(&[file("a", C::Text, &g), file("b", C::Text, &g)], &off);
        let ro = copy_text("link.machine.readOnly", &[]);
        for e in m.bar.iter().chain(&m.more) {
            let want = (e.id != "selectAll").then(|| ro.clone());
            assert_eq!((e.id, e.why.clone()), (e.id, want));
        }
        assert_eq!(
            phone_menu(&[dir("d")], &off).tap,
            Some("open"),
            "进目录照常"
        );
        let no_chmod = Here {
            offline: None,
            unavailable: vec!["files-chmod".into()],
        };
        let m = phone_menu(&[file("a", C::Text, &g)], &no_chmod);
        let chmod = m.more.iter().find(|e| e.id == "chmod").unwrap();
        assert_eq!(
            chmod.why,
            Some(copy_text("rsFilewinShell.menu.unavailableHere", &[]))
        );
        assert!(m
            .more
            .iter()
            .filter(|e| e.id != "chmod" && e.id != "preview")
            .all(|e| e.enabled));
        assert_eq!(refused_here(Action::Chmod, &no_chmod), chmod.why);
        assert_eq!(refused_here(Action::Open, &off), None);
    }
}
