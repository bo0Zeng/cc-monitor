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

fn ids(m: &PhoneMenu) -> Vec<Vec<(&'static str, bool)>> {
    [&m.bar, &m.more]
        .iter()
        .map(|g| g.iter().map(|e| (e.id, e.enabled)).collect())
        .collect()
}

fn menu(picked: &[Pick]) -> PhoneMenu {
    phone_menu(picked, &Here::default())
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
    let sep = copy_text("rsFilewinCopy.batch.listSep", &[]);
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
        copy_text("rsFilewinWorkspace.label.preview", &[])
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
    let ro = copy_text("rsFilewinChrome.link.readOnly", &[]);
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

/// ★ 一行的事实从 `files-ls` 那一行认：指向目录的链接 · 断链 · 有损名（带着字节）· 名字那一段读成字。
#[test]
fn the_facts_come_from_the_ls_row() {
    let row = |v: serde_json::Value| -> PickIn {
        let e: cmd_wire::files::LsEntry = serde_json::from_value(v).unwrap();
        PickIn::of_ls(&e)
    };
    let base = |name: serde_json::Value, kind: &str, class: &str| serde_json::json!({ "path": "/x", "name": name, "kind": kind, "class": class, "type_text": "" });
    let mut dl = base("dl".into(), "symlink", "folder");
    dl["link_dir"] = true.into();
    dl["link_to"] = "dir".into();
    let mut gone = base("gone".into(), "symlink", "other");
    gone["link_to"] = "missing".into();
    let odd = base(serde_json::json!({ "b16": "6f64ff" }), "file", "other");
    let kinds: Vec<Picked> = [
        base("d".into(), "dir", "folder"),
        dl,
        gone,
        odd.clone(),
        base("a.txt".into(), "file", "text"),
    ]
    .into_iter()
    .map(|v| picked_kind(&[row(v).pick()]))
    .collect();
    assert_eq!(
        kinds,
        vec![
            Picked::Dir,
            Picked::DirLink,
            Picked::BrokenLink,
            Picked::Text,
            Picked::Text
        ]
    );
    let o = row(odd);
    assert_eq!(
        (o.lossy, o.raw, o.name.as_str()),
        (true, true, "od\u{FFFD}")
    );
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

/// 断在哪一段决定写类说什么：那台断着 ⇒「离线 · 只读」；窗口与 cc-monitor 之间断了 ⇒ 说与 cc-monitor 断开，不说那台离线。
/// 进目录两种都照常；连着 ⇒ 写类不拦。期望手写。
#[test]
fn a_cut_to_the_monitor_is_said_as_such_on_every_write() {
    assert_eq!(write_refused(None), None);
    assert_eq!(
        write_refused(Some(Cut::Machine)),
        Some(copy_text("rsFilewinChrome.link.readOnly", &[]))
    );
    let monitor = Some(copy_text("rsFilewinChrome.link.severedReadOnly", &[]));
    assert_eq!(write_refused(Some(Cut::Monitor)), monitor);
    let here = Here {
        offline: Some(Cut::Monitor),
        unavailable: vec![],
    };
    assert_eq!(refused_here(Action::Rename, &here), monitor);
    assert_eq!(refused_here(Action::Open, &here), None);
}
