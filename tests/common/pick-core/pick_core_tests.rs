//! 选中这几项 ⇒ 能做什么：两个出口各一张表、同一个判法。

use super::*;
use cmd_wire::files::{FileClass as C, PreviewGate};

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

fn ids(m: &[Vec<Entry>]) -> Vec<Vec<(&'static str, bool)>> {
    m.iter()
        .map(|g| g.iter().map(|e| (e.id, e.enabled)).collect())
        .collect()
}

/// ★ 选中的是哪一种：断链 · 指向目录的链接 · 目录 · 查看页开得了的图 · 文本（含开不了的图）· 压缩包 · 别的；
/// 名字寻址不到的单选 / 多选各一种。
#[test]
fn every_kind_comes_from_its_own_facts() {
    let g = gates(false);
    let gv = gates(true);
    let mut broken = file("l", C::Other, &g);
    broken.link = true;
    broken.link_broken = true;
    let mut dlink = file("dl", C::Folder, &g);
    dlink.link = true;
    dlink.link_dir = true;
    let mut nameless = file("x", C::Text, &g);
    nameless.lossy = true;
    let mut raw = nameless;
    raw.raw = true;
    let cases = [
        (vec![], Picked::Here),
        (vec![broken], Picked::BrokenLink),
        (vec![dlink], Picked::DirLink),
        (vec![dir("d")], Picked::Dir),
        (vec![file("p.png", C::Image, &gv)], Picked::Image),
        (vec![file("p.heic", C::Image, &g)], Picked::Text),
        (vec![file("a.zip", C::Archive, &g)], Picked::Archive),
        (vec![file("a.pdf", C::Pdf, &g)], Picked::File),
        (vec![nameless], Picked::Nameless),
        (vec![raw], Picked::Text),
        (vec![dir("d"), raw], Picked::Many),
        (vec![dir("d"), nameless], Picked::ManyNameless),
    ];
    for (picks, want) in cases {
        assert_eq!(picked_kind(&picks), want, "{picks:?}");
    }
}

/// ★ 手机那一份：顶栏下载 · 删除，⋯ 改名 · 复制到 · 改权限 · 全选；选中里有目录 ⇒ 下载灰着、说哪几项；多选改权限说项数；
/// 断链只能删、改名；名字寻址不到的只剩全选。
#[test]
fn the_phone_gets_its_bar_and_more_with_why_for_the_grey_ones() {
    let g = gates(false);
    let one = phone_menu(&[file("a.txt", C::Text, &g)]);
    assert_eq!(
        ids(&one),
        vec![
            vec![("download", true), ("delete", true)],
            vec![
                ("rename", true),
                ("copyTo", true),
                ("chmod", true),
                ("selectAll", true)
            ],
        ]
    );
    let d = phone_menu(&[dir("docs")]);
    assert_eq!(
        (d[0][0].enabled, d[0][0].why.clone()),
        (
            false,
            Some(copy_text(
                "rsFilewinSelect.why.dirDownload",
                &[("names", "docs")]
            ))
        )
    );
    let many = phone_menu(&[dir("docs"), file("a.txt", C::Text, &g), dir("src")]);
    let sep = copy_text("rsFilewinCopy.batch.listSep", &[]);
    assert_eq!(
        many[0][0].why,
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
        many[1][2].text,
        copy_text("rsFilewinSelect.label.chmodMany", &[("n", "3")])
    );
    let files = phone_menu(&[file("a", C::Text, &g), file("b", C::Text, &g)]);
    assert!(
        files[0][0].enabled && files[0][0].why.is_none(),
        "全是文件 ⇒ 下载可点"
    );
    let mut broken = file("l", C::Other, &g);
    broken.link = true;
    broken.link_broken = true;
    assert_eq!(
        ids(&phone_menu(&[broken])),
        vec![
            vec![("delete", true)],
            vec![("rename", true), ("selectAll", true)]
        ]
    );
    let mut nameless = file("x", C::Text, &g);
    nameless.lossy = true;
    assert_eq!(
        ids(&phone_menu(&[nameless])),
        vec![vec![], vec![("selectAll", true)]]
    );
    assert!(phone_menu(&[]).is_empty(), "什么都没选 ⇒ 不出");
}

/// ★ 桌面那一份：多选里有目录 ⇒ 不能下载；名字只有原始字节的不能复制到另一台；单选文本编辑不了照那台的门说；计划那一组插在删除之前。
#[test]
fn the_desk_drops_what_this_pick_cannot_do() {
    let g = gates(false);
    let acts = actions_for(Outlet::Desk, &[dir("d"), file("a", C::Text, &g)]);
    assert!(
        !acts.contains(&Action::Download) && acts.contains(&Action::Delete),
        "{acts:?}"
    );
    let mut raw = file("x", C::Text, &g);
    raw.lossy = true;
    raw.raw = true;
    let acts = actions_for(Outlet::Desk, &[raw, file("a", C::Text, &g)]);
    assert!(!acts.contains(&Action::CrossCopy), "{acts:?}");
    let no = Gates {
        edit: Gate::No("太大".into()),
        ..gates(false)
    };
    assert_eq!(
        blocked(Action::Edit, &[file("a", C::Text, &no)]),
        Some("太大".to_string())
    );
    let groups = menu_groups(Outlet::Desk, Picked::Text, true);
    assert_eq!(groups[groups.len() - 2], PLAN_GROUP.to_vec());
    assert_eq!(
        menu_groups(Outlet::Phone, Picked::Text, true),
        menu_groups(Outlet::Phone, Picked::Text, false),
        "手机没有计划那一组"
    );
}

/// 手机交进来的那一形照 camelCase 解（缺的开关当否）。
#[test]
fn the_phone_hands_in_camel_case_facts() {
    let p: PickIn = serde_json::from_value(serde_json::json!({
        "name": "docs", "isDir": true, "class": "folder"
    }))
    .unwrap();
    assert_eq!(picked_kind(&[p.pick()]), Picked::Dir);
}

/// ★ 一行的复制 · 下载两道门（桌面行上那颗按钮 · 复制到另一栏 · 下载问的都是它）：复制收名字寻址得到的文件与目录；
/// 下载只收文件（目录 · 指向目录的链接都不收）；有损名带着原始字节两道都过，不带就都不过。
#[test]
fn copy_and_download_gates_of_one_row() {
    let g = gates(false);
    let plain = file("a.txt", C::Text, &g);
    let d = dir("sub");
    let mut link_dir = file("to-dir", C::Folder, &g);
    link_dir.link = true;
    link_dir.link_dir = true;
    let mut lossy = file("\u{FFFD}odd", C::Other, &g);
    lossy.lossy = true;
    let mut lossy_raw = lossy;
    lossy_raw.raw = true;
    let got: Vec<(bool, bool)> = [plain, d, link_dir, lossy, lossy_raw]
        .iter()
        .map(|p| (p.copyable(), p.downloadable()))
        .collect();
    assert_eq!(
        got,
        vec![
            (true, true),
            (true, false),
            (true, false),
            (false, false),
            (true, true)
        ]
    );
}
