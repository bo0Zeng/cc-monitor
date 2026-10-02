//! 文件种类：图标与「类型」那一列的字只在这一处判；按类型排序与那一列的字同源。

use super::*;
use crate::source::{Listed, Row, SortBy};

fn row(name: &str, is_dir: bool, link: bool) -> Listed {
    let mut l = Listed::plain(Row {
        name: name.into(),
        path: format!("/d/{name}"),
        is_dir,
        size: 1,
        lossy_name: false,
    });
    l.link = link;
    l
}

/// 一张手写的名字 → 种类表（链接先于目录；只有前导点的不算扩展名；无扩展名的 Makefile 认成代码）。
#[test]
fn names_map_to_the_kinds_written_by_hand() {
    let cases: [(&str, bool, bool, Kind); 11] = [
        ("src", true, false, Kind::Folder),
        ("lnk", true, true, Kind::Link),
        ("README.md", false, false, Kind::Text),
        ("main.rs", false, false, Kind::Code),
        ("Makefile", false, false, Kind::Code),
        ("a.PNG", false, false, Kind::Image),
        ("x.tar.gz", false, false, Kind::Archive),
        ("doc.pdf", false, false, Kind::Pdf),
        (".bashrc", false, false, Kind::Other),
        ("noext", false, false, Kind::Other),
        ("a.weird", false, false, Kind::Other),
    ];
    for (name, d, l, want) in cases {
        assert_eq!(kind_of(&row(name, d, l)), want, "{name}");
    }
    assert_eq!(ext_of(".bashrc"), None);
    assert_eq!(ext_of("a.PNG").as_deref(), Some("png"));
    assert!(is_hidden(".bashrc") && !is_hidden("a.txt"));
}

/// 每一种的图标与字各不相同（八种八个图标、八个词）；「类型」那一列的字带扩展名。
#[test]
fn every_kind_has_its_own_icon_and_word() {
    let icons: std::collections::BTreeSet<&str> = ALL.iter().map(|k| icon(*k)).collect();
    let words: std::collections::BTreeSet<String> = ALL.iter().map(|k| label(*k)).collect();
    assert_eq!((icons.len(), words.len()), (ALL.len(), ALL.len()));
    assert_eq!(
        type_text(&row("main.rs", false, false)),
        format!("{} · RS", label(Kind::Code))
    );
    assert_eq!(type_text(&row("src", true, false)), label(Kind::Folder));
}

/// 按「类型」排：目录在前，再按种类、再按扩展名（与那一列写的字同源）。
#[test]
fn sorting_by_type_groups_by_the_shown_kind() {
    let mut v = vec![
        row("z.png", false, false),
        row("b.rs", false, false),
        row("a.md", false, false),
        row("dir", true, false),
        row("c.ts", false, false),
    ];
    crate::source::sort_rows(&mut v, SortBy::Type);
    let names: Vec<&str> = v.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["dir", "a.md", "b.rs", "c.ts", "z.png"]);
}
