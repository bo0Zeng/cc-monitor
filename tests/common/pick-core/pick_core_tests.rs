//! 选中这几项 ⇒ 能做什么：同一个判法、同一份字（手机那张表与它的判据住手机核心 `menu.rs`）。

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

/// ★ 一件事的字只有一份（[`label`]）：多项时下载 · 删除 · 改权限 · 复制到另一台带「n 项」，一项不带；桌面菜单与手机顶栏都照它。
#[test]
fn the_word_of_an_action_carries_the_count_when_many() {
    assert_eq!(
        label(Action::Download, 3),
        copy_text("rsFilewinSelect.label.downloadMany", &[("n", "3")])
    );
    assert_eq!(
        label(Action::Download, 1),
        copy_text("rsFilewinDownload.label.download", &[])
    );
    assert_eq!(
        label(Action::Delete, 2),
        copy_text("rsFilewinSelect.label.deleteMany", &[("n", "2")])
    );
    assert_eq!(
        label(Action::Chmod, 4),
        copy_text("rsFilewinSelect.label.chmodMany", &[("n", "4")])
    );
    assert_eq!(
        label(Action::CrossCopy, 2),
        copy_text("rsFilewinSelect.label.crossMany", &[("n", "2")])
    );
    assert_eq!(
        label(Action::Rename, 3),
        label(Action::Rename, 1),
        "改名不带件数"
    );
}

/// ★ 这一份做不了的：多选里有目录 ⇒ 不能下载（说是哪几项）；名字只有原始字节的不能复制到另一台；单选文本编辑不了照那台的门说。
#[test]
fn blocked_says_what_this_pick_cannot_do() {
    let g = gates(false);
    let mixed = [dir("d"), file("a", C::Text, &g)];
    assert_eq!(
        blocked(Action::Download, &mixed),
        Some(copy_text(
            "rsFilewinSelect.why.dirDownload",
            &[("names", "d")]
        ))
    );
    assert_eq!(blocked(Action::Delete, &mixed), None);
    let mut raw = file("x", C::Text, &g);
    raw.lossy = true;
    raw.raw = true;
    assert_eq!(
        blocked(Action::CrossCopy, &[raw, file("a", C::Text, &g)]),
        Some(copy_text(
            "rsFilewinSelect.why.lossyCross",
            &[("names", "x")]
        ))
    );
    let no = Gates {
        edit: Gate::No("太大".into()),
        ..gates(false)
    };
    assert_eq!(
        blocked(Action::Edit, &[file("a", C::Text, &no)]),
        Some("太大".to_string())
    );
}

/// ★ 一行的事实从 `files-ls` 那一行认：指向目录的链接 · 断链 · 有损名（带着字节）· 名字那一段读成字。
#[test]
fn the_facts_come_from_the_ls_row() {
    let row = |v: serde_json::Value| -> PickIn {
        let e: cmd_wire::files::LsEntry = serde_json::from_value(v).unwrap();
        PickIn::of_ls(&e)
    };
    let base = |name: serde_json::Value, kind: &str, class: &str| serde_json::json!({ "path": "/x", "shown": "/x", "name": name, "kind": kind, "class": class, "type_text": "" });
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

/// ★ 菜单那几样上线的形：一件事 `snake_case`、带一件事的项写成 `{"act": …}`、别的项是一个词；回包写出再读回逐格相等。
#[test]
fn the_menu_reply_goes_over_the_wire_as_is() {
    use cmd_wire::files::{ActState, DeskEntry, DeskMenu};
    assert_eq!(
        serde_json::to_value(Item::Act(Action::CrossCopy)).unwrap(),
        serde_json::json!({ "act": "cross_copy" })
    );
    assert_eq!(
        serde_json::to_value(Item::InPlan).unwrap(),
        serde_json::json!("in_plan")
    );
    let m = DeskMenu {
        groups: vec![vec![DeskEntry {
            item: Item::Act(Action::Open),
            text: "项甲".into(),
            why: None,
        }]],
        acts: vec![ActState {
            action: Action::Rename,
            text: "项乙".into(),
            why: Some("一次只改一个".into()),
        }],
    };
    let back: DeskMenu = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
    assert_eq!(back, m);
    assert_eq!(Action::ALL.len(), 14);
}

/// ★ 一行复制得了没有（本机后端「复制到另一栏」那一件问它）：收名字寻址得到的文件与目录（连指向目录的链接）；
/// 有损名带着原始字节就过，不带就不过。
#[test]
fn the_copy_gate_of_one_row() {
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
    let got: Vec<bool> = [plain, d, link_dir, lossy, lossy_raw]
        .iter()
        .map(Pick::copyable)
        .collect();
    assert_eq!(got, vec![true, true, true, false, true]);
}

/// 此刻做不到的：断在哪一段，写类就说哪一段（那一句住 `copy_core::write_refused`）；进目录照常；连着 ⇒ 不拦。期望手写。
#[test]
fn a_cut_to_the_monitor_is_said_as_such_on_every_write() {
    let at = |offline| Here {
        offline,
        unavailable: vec![],
    };
    let monitor = Some(copy_text("rsFilewinChrome.link.severedReadOnly", &[]));
    assert_eq!(
        refused_here(Action::Rename, &at(Some(Cut::Monitor))),
        monitor
    );
    assert_eq!(
        refused_here(Action::Rename, &at(Some(Cut::Machine))),
        Some(copy_text("link.machine.readOnly", &[]))
    );
    assert_eq!(refused_here(Action::Open, &at(Some(Cut::Monitor))), None);
    assert_eq!(refused_here(Action::Rename, &at(None)), None);
}
