//! 桌面那一份菜单成品（`files-menu`）：哪一种选中给哪几组 · 每项的字 · 灰着的为什么 · 十四件每件此刻的样子。期望照文案表的键现取。

use super::*;
use cmd_wire::files::{FileClass as C, FileKind, Gate, Gates, PreviewGate, WirePath};
use copy_core::copy_text;

fn gates() -> Gates {
    Gates {
        preview: PreviewGate::No(String::new()),
        view: None,
        edit: Gate::Cap(1),
    }
}

/// `files-ls` 的一行（菜单只读种类 · 链接 · 名字 · 门那几格；别的格照真行的形填）。
fn file(name: &str, class: C) -> LsEntry {
    let at = WirePath(format!("/x/{name}").into_bytes());
    LsEntry {
        path: at.clone(),
        name: WirePath(name.as_bytes().to_vec()),
        kind: FileKind::File,
        size: Some(1),
        size_text: None,
        mtime_secs: None,
        mtime_text: None,
        mtime_full: None,
        link_dir: None,
        link_to: None,
        class,
        type_text: String::new(),
        gates: Some(gates()),
        shown: at,
    }
}

fn dir(name: &str) -> LsEntry {
    LsEntry {
        kind: FileKind::Dir,
        size: None,
        gates: None,
        ..file(name, C::Folder)
    }
}

fn ask(picked: Vec<LsEntry>) -> DeskMenu {
    desk_menu(&FilesMenuReq {
        picked,
        here: Here::default(),
        plan: None,
    })
}

fn items(m: &DeskMenu) -> Vec<Vec<Item>> {
    m.groups
        .iter()
        .map(|g| g.iter().map(|e| e.item).collect())
        .collect()
}

fn why(m: &DeskMenu, a: Action) -> Option<String> {
    m.act(a).expect("十四件每件一行").why.clone()
}

/// ★ 三个文件多选：三组（下载 · 复制到另一台 · 复制到另一栏｜算大小 · 改权限｜删除），字带「3 项」；改名不在表里 ⇒ 说一次只改一个。
#[test]
fn three_files_get_the_many_menu_with_counted_words() {
    let m = ask(vec![
        file("a.txt", C::Text),
        file("b.txt", C::Text),
        file("c.txt", C::Text),
    ]);
    use Action as A;
    assert_eq!(
        items(&m),
        vec![
            vec![
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Across)
            ],
            vec![Item::Act(A::Size), Item::Act(A::Chmod)],
            vec![Item::Act(A::Delete)],
        ]
    );
    let n = [("n", "3")];
    let texts: Vec<Vec<String>> = m
        .groups
        .iter()
        .map(|g| g.iter().map(|e| e.text.clone()).collect())
        .collect();
    assert_eq!(
        texts,
        vec![
            vec![
                copy_text("rsFilewinSelect.label.downloadMany", &n),
                copy_text("rsFilewinSelect.label.crossMany", &n),
                copy_text("rsFilewinWorkspace.drag.hint", &n),
            ],
            vec![
                copy_text("rsFilewinSize.label.size", &[]),
                copy_text("rsFilewinSelect.menu.chmod", &[]),
            ],
            vec![copy_text("rsFilewinSelect.menu.deleteMany", &n)],
        ]
    );
    assert!(m.groups.iter().flatten().all(|e| e.why.is_none()));
    assert_eq!(
        why(&m, A::Rename),
        Some(copy_text("rsFilewinSelect.refusal.renameOne", &n))
    );
    assert_eq!(why(&m, A::Download), None);
    let order: Vec<Action> = m.acts.iter().map(|s| s.action).collect();
    assert_eq!(order, Action::ALL.to_vec());
}

/// ★ 单选的那一份归某一格：删除那组前多一组，只有一项「在计划里看」，字带那一格的标题。
#[test]
fn a_planned_pick_gets_the_plan_group_before_delete() {
    let m = desk_menu(&FilesMenuReq {
        picked: vec![file("a.txt", C::Text)],
        here: Here::default(),
        plan: Some("读数那一格".into()),
    });
    let g = &m.groups[m.groups.len() - 2];
    assert_eq!(
        g,
        &vec![DeskEntry {
            item: Item::InPlan,
            text: copy_text("rsFilewinPlan.menu.open", &[("title", "读数那一格")]),
            why: None,
        }]
    );
    assert_eq!(m.groups.last().unwrap()[0].item, Item::Act(Action::Delete));
    assert_eq!(
        m.groups.last().unwrap()[0].text,
        copy_text("history.menu.delete", &[])
    );
}

/// ★ 做不了的为什么：选中里有目录 ⇒ 下载灰着说哪几项（菜单那一项 · acts 同一句）；什么都没选 ⇒ 每件都说先选中一项，
/// 菜单是对这个目录的那几项。
#[test]
fn the_why_of_each_grey_one_is_written_once() {
    let m = ask(vec![dir("docs"), file("a.txt", C::Text)]);
    let dl = copy_text("rsFilewinSelect.why.dirDownload", &[("names", "docs")]);
    let entry = m
        .groups
        .iter()
        .flatten()
        .find(|e| e.item == Item::Act(Action::Download))
        .unwrap();
    assert_eq!(entry.why, Some(dl.clone()));
    assert_eq!(why(&m, Action::Download), Some(dl));

    let none = ask(vec![]);
    let first = copy_text("rsFilewinChrome.command.pickFirst", &[]);
    assert!(none
        .acts
        .iter()
        .all(|s| s.why.as_deref() == Some(first.as_str())));
    assert_eq!(
        items(&none),
        vec![
            vec![Item::Mkdir, Item::NewFile, Item::Upload],
            vec![Item::Term]
        ]
    );
    assert_eq!(
        none.groups[0][0].text,
        copy_text("rsFilewinWriteops.inline.newDir", &[])
    );
}

/// ★ 此刻做不到的：那台断着 ⇒ 除了进目录、要碰那台的都灰着说离线（对这个目录的那几项也是）；那台说做不到改权限 ⇒ 改权限说这台不支持。
#[test]
fn what_the_machine_cannot_do_now_is_greyed_with_its_reason() {
    let off = Here {
        offline: Some(Cut::Machine),
        unavailable: vec![],
    };
    let ro = Some(copy_text("link.machine.readOnly", &[]));
    let m = desk_menu(&FilesMenuReq {
        picked: vec![dir("d")],
        here: off.clone(),
        plan: None,
    });
    assert_eq!(why(&m, Action::Open), None);
    assert_eq!(why(&m, Action::Rename), ro);
    let here = desk_menu(&FilesMenuReq {
        picked: vec![],
        here: off,
        plan: None,
    });
    assert!(here.groups.iter().flatten().all(|e| e.why == ro));
    let no_chmod = desk_menu(&FilesMenuReq {
        picked: vec![file("a", C::Text)],
        here: Here {
            offline: None,
            unavailable: vec!["files-chmod".into()],
        },
        plan: None,
    });
    assert_eq!(
        why(&no_chmod, Action::Chmod),
        Some(copy_text("rsFilewinShell.menu.unavailableHere", &[]))
    );
}

/// 表里没有这一件、而这一份本来就做不了它 ⇒ 说这一份做不了的那一句（具体的），不说「表里没有」那一句：
/// 一份二进制按回车（编辑）说编辑门那一句；目录按下载说「文件夹下载不了」那一句。
#[test]
fn a_specific_reason_wins_over_not_in_the_table() {
    let mut bin = file("a.bin", C::Archive);
    bin.gates = Some(Gates {
        edit: Gate::No("not text".into()),
        ..gates()
    });
    let m = ask(vec![bin]);
    assert!(
        !items(&m)
            .iter()
            .flatten()
            .any(|i| *i == Item::Act(Action::Edit)),
        "前提：压缩包那一种的表里没有编辑"
    );
    assert_eq!(why(&m, Action::Edit).as_deref(), Some("not text"));
    let m = ask(vec![dir("sub")]);
    assert_eq!(
        why(&m, Action::Download),
        Some(copy_text(
            "rsFilewinSelect.why.dirDownload",
            &[("names", "sub")]
        ))
    );
}

/// 「复制到另一栏」：右键那几组只给多选，命令栏那一颗对单项照样做（寻址得到就行）⇒ 单选一份文件、一个目录，它都不灰。
#[test]
fn across_is_open_for_a_single_pick_too() {
    assert!(!items(&ask(vec![file("a.txt", C::Text)]))
        .iter()
        .flatten()
        .any(|i| *i == Item::Act(Action::Across)));
    assert_eq!(
        why(&ask(vec![file("a.txt", C::Text)]), Action::Across),
        None
    );
    assert_eq!(why(&ask(vec![dir("sub")]), Action::Across), None);
}
