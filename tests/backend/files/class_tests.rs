//! 种类与几道门：按名字判的那一种、「类型」那一列的字、预览 / 看图 / 编辑在上限两侧各一格。

use super::*;
use cmd_wire::files::{FileClass as C, Gate, PreviewGate};

const EDIT_MAX_BYTES: u64 = super::super::READ_TEXT_MAX_BYTES as u64;

#[test]
fn the_class_comes_from_the_name_and_links_draw_what_they_point_at() {
    let cases: &[(&str, bool, bool, bool, C)] = &[
        ("a.rs", false, false, false, C::Code),
        ("Makefile", false, false, false, C::Code),
        ("notes.MD", false, false, false, C::Text),
        ("p.PNG", false, false, false, C::Image),
        ("x.tar", false, false, false, C::Archive),
        ("doc.pdf", false, false, false, C::Pdf),
        (".bashrc", false, false, false, C::Other),
        ("noext", false, false, false, C::Other),
        ("src.rs", true, false, false, C::Folder),
        ("to-dir", false, true, true, C::Folder),
        ("to.png", false, true, false, C::Image),
    ];
    for (name, dir, link, link_dir, want) in cases {
        assert_eq!(
            class_of_entry(name.as_bytes(), *dir, *link, *link_dir),
            *want,
            "{name}"
        );
    }
    assert_eq!(
        type_key(b"to.png", false, true).0,
        C::Link,
        "按类型排时链接自成一种"
    );
}

#[test]
fn the_type_column_text_is_written_here() {
    let t = |n: &str, dir, link, ld, broken| type_text(n.as_bytes(), dir, link, ld, broken);
    assert_eq!(
        t("a.rs", false, false, false, false),
        copy_text(
            "beFilesKind.type.withExt",
            &[
                ("kind", &copy_text("beFilesKind.label.code", &[])),
                ("ext", "RS")
            ]
        )
    );
    assert_eq!(
        t("d", true, false, false, false),
        copy_text("beFilesKind.label.folder", &[])
    );
    assert_eq!(
        t("x.pdf", false, false, false, false),
        copy_text("beFilesKind.label.pdf", &[])
    );
    assert_eq!(
        t("l", false, true, true, false),
        copy_text(
            "beFilesKind.type.linkTo",
            &[("kind", &copy_text("beFilesKind.label.folder", &[]))]
        )
    );
    assert_eq!(
        t("l", false, true, false, true),
        copy_text("beFilesKind.type.linkBroken", &[])
    );
}

#[test]
fn text_preview_is_allowed_up_to_its_cap_and_carries_the_syntax() {
    assert_eq!(
        gates_of(b"a.rs", PREVIEW_TEXT_MAX_BYTES).preview,
        PreviewGate::Text(cmd_wire::files::TextPreview {
            cap: PREVIEW_TEXT_MAX_BYTES,
            lang: Some("rs".into())
        })
    );
    assert_eq!(
        gates_of(b"a.txt", 1).preview,
        PreviewGate::Text(cmd_wire::files::TextPreview {
            cap: PREVIEW_TEXT_MAX_BYTES,
            lang: None
        }),
        "纯文本不上色"
    );
    assert!(matches!(
        gates_of(b"a.svg", 1).preview,
        PreviewGate::Text(ref t) if t.lang.as_deref() == Some("svg")
    ));
    let over = PREVIEW_TEXT_MAX_BYTES + 1;
    assert_eq!(
        gates_of(b"a.rs", over).preview,
        PreviewGate::No(copy_text(
            "beFilesOpen.preview.tooBig",
            &[
                ("size", &size_text(over)),
                ("limit", &size_text(PREVIEW_TEXT_MAX_BYTES))
            ]
        ))
    );
}

#[test]
fn images_preview_and_view_each_up_to_their_own_cap() {
    let at = |n: u64| gates_of(b"p.png", n);
    assert_eq!(
        at(PREVIEW_IMAGE_MAX_BYTES).preview,
        PreviewGate::Image(PREVIEW_IMAGE_MAX_BYTES)
    );
    assert!(matches!(
        at(PREVIEW_IMAGE_MAX_BYTES + 1).preview,
        PreviewGate::No(_)
    ));
    assert_eq!(
        at(VIEW_IMAGE_MAX_BYTES).view,
        Some(Gate::Cap(VIEW_IMAGE_MAX_BYTES))
    );
    assert_eq!(
        at(VIEW_IMAGE_MAX_BYTES + 1).view,
        Some(Gate::No(copy_text(
            "beFilesOpen.view.tooBig",
            &[
                ("size", &size_text(VIEW_IMAGE_MAX_BYTES + 1)),
                ("limit", &size_text(VIEW_IMAGE_MAX_BYTES))
            ]
        )))
    );
    // 解不开的图：不预览、没有查看页；svg 当文本。
    assert_eq!(
        gates_of(b"p.heic", 1).preview,
        PreviewGate::No(copy_text("beFilesOpen.preview.imageKind", &[]))
    );
    assert_eq!(gates_of(b"p.heic", 1).view, None);
    assert_eq!(gates_of(b"p.svg", 1).view, None);
    assert_eq!(
        gates_of(b"x.zip", 1).preview,
        PreviewGate::No(copy_text("beFilesOpen.preview.binaryKind", &[]))
    );
}

#[test]
fn edit_is_allowed_up_to_the_read_ceiling_and_says_how_much_over() {
    assert_eq!(
        gates_of(b"a.log", EDIT_MAX_BYTES).edit,
        Gate::Cap(EDIT_MAX_BYTES)
    );
    let over = EDIT_MAX_BYTES + 3;
    assert_eq!(
        gates_of(b"a.log", over).edit,
        Gate::No(copy_text(
            "beFilesOpen.edit.tooBig",
            &[
                ("size", &size_text(over)),
                ("bytes", &over.to_string()),
                ("limit", &size_text(EDIT_MAX_BYTES)),
                ("over", "3")
            ]
        ))
    );
}

/// `files-ls` 每行带上种类 · 那一列的字 · 门（目录与指向目录的链接没有门）；按类型排：目录在前，链接自成一种，再扩展名。
#[cfg(unix)]
#[test]
fn ls_rows_carry_class_type_text_and_gates_and_sort_by_type() {
    let d = std::env::temp_dir().join(format!("class-ls-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("sub")).unwrap();
    std::fs::write(d.join("b.rs"), b"fn main(){}").unwrap();
    std::fs::write(d.join("a.txt"), b"hi").unwrap();
    std::fs::write(d.join("c.png"), b"x").unwrap();
    std::os::unix::fs::symlink(d.join("sub"), d.join("ldir")).unwrap();
    std::os::unix::fs::symlink(d.join("a.txt"), d.join("lfile.txt")).unwrap();
    let ls = |sort: Option<cmd_wire::files::LsSort>| {
        super::super::answer(
            cmd_wire::files::LsReq {
                path: cmd_wire::files::WirePath(d.as_os_str().as_encoded_bytes().to_vec()),
                limit: None,
                dirs_only: None,
                sort,
            },
            &Default::default(),
        )
        .expect("ls")
    };
    let names = |r: &cmd_wire::files::LsReply| -> Vec<String> {
        r.entries
            .iter()
            .map(|e| String::from_utf8_lossy(&e.name.0).into_owned())
            .collect()
    };
    let by_type = ls(Some(cmd_wire::files::LsSort {
        by: cmd_wire::files::LsSortBy::Type,
        reverse: false,
    }));
    // 目录那一摞：sub（文件夹）在链接 ldir 之前；文件：文本 · 代码 · 图片 · 链接。
    assert_eq!(
        names(&by_type),
        ["sub", "ldir", "a.txt", "b.rs", "c.png", "lfile.txt"]
    );
    let row = |n: &str| {
        by_type
            .entries
            .iter()
            .find(|e| e.name.0 == n.as_bytes())
            .unwrap()
            .clone()
    };
    assert_eq!(row("sub").gates, None);
    assert_eq!(row("ldir").gates, None, "指向目录的链接点得进去，不读");
    assert_eq!(row("ldir").class, C::Folder);
    assert_eq!(row("lfile.txt").class, C::Text);
    assert_eq!(
        row("b.rs").type_text,
        type_text(b"b.rs", false, false, false, false)
    );
    assert_eq!(row("b.rs").gates, Some(gates_of(b"b.rs", 11)));
    assert!(row("c.png").gates.unwrap().view.is_some());
    let reversed = ls(Some(cmd_wire::files::LsSort {
        by: cmd_wire::files::LsSortBy::Type,
        reverse: true,
    }));
    assert_eq!(
        names(&reversed),
        ["ldir", "sub", "lfile.txt", "c.png", "b.rs", "a.txt"]
    );
    let _ = std::fs::remove_dir_all(&d);
}
