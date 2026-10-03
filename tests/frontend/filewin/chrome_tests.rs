//! 窗口的框：后退前进与历史 · 地址栏手输 · 导航键 · 状态栏那一行 · 隐藏文件开关 · 表头排序与列宽 · 左栏那几格。
//! 交互的几条都跑生产那条路（`Workspace::frame` 或同一套面板），喂合成事件、读这一帧画出来的字与窗口状态。

use super::testing::{click, frame, ws_click, ws_frame};
use super::*;
use crate::copy::testing::rects_of;
use crate::source::{Listed, Row, Source};
use crate::workspace::Workspace;

fn row(name: &str, is_dir: bool, size: u64, mtime: u64) -> Listed {
    let mut l = Listed::plain(Row {
        name: name.into(),
        path: format!("/m/{name}"),
        is_dir,
        size,
        lossy_name: false,
    });
    l.mtime_secs = Some(mtime);
    l
}

fn window(cwd: &str, rows: Vec<Listed>) -> FileWindow {
    let mut w = FileWindow::seeded(Source::remote("box".into()), cwd.into(), None, rows);
    *w.listing.error.lock().unwrap() = None;
    w
}

/// 后退 / 前进：走两步退一步再进一步回到原处；退了之后另走一步 ⇒ 前进那一摞清掉（浏览器 / 资源管理器的口径）。
#[test]
fn back_and_forward_walk_the_tabs_own_history() {
    let mut w = window("/a", Vec::new());
    assert!(!w.can_go_back() && !w.can_go_forward());
    w.navigate_to("/a/b".into());
    w.navigate_to("/a/b/c".into());
    assert!(w.go_back());
    assert_eq!(w.cwd, "/a/b");
    assert!(w.go_back());
    assert_eq!(w.cwd, "/a");
    assert!(!w.go_back(), "到头了");
    assert!(w.go_forward());
    assert_eq!(w.cwd, "/a/b");
    w.navigate_to("/z".into());
    assert!(!w.can_go_forward(), "另走一步之后前进那一摞该清掉");
    assert!(w.go_back());
    assert_eq!(w.cwd, "/a/b");
}

/// 地址栏手输：从 `/` 起头的去那儿（连续斜杠折一个、去尾斜杠）；不是 ⇒ 不动、出声。
#[test]
fn the_address_bar_goes_where_it_is_told_and_refuses_relative_paths() {
    let mut w = window("/a", Vec::new());
    w.begin_address_edit();
    assert_eq!(w.address_editing(), Some("/a"));
    w.addr_edit = Some("/x//y/".into());
    assert!(w.commit_address());
    assert_eq!(w.cwd, "/x/y");
    assert_eq!(w.address_editing(), None);
    w.addr_edit = Some("rel/dir".into());
    assert!(!w.commit_address());
    assert_eq!(w.cwd, "/x/y");
    assert_eq!(
        w.key_notice(),
        Some(copy_text("rsFilewinChrome.address.notAbsolute", &[]).as_str())
    );
}

/// 导航键的映射（纯）：Alt+← / Alt+→ / 鼠标侧键 / F5 / Ctrl+L / Ctrl+F；不带 Alt 的 ← → 不接（列表那张键位表刻意不接）。
#[test]
fn navigation_keys_map_as_in_a_file_manager() {
    let key = |k: egui::Key, m: egui::Modifiers| egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: m,
    };
    let side = |b: egui::PointerButton| egui::Event::PointerButton {
        pos: egui::Pos2::ZERO,
        button: b,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    };
    let got = nav_keys(&[
        key(egui::Key::ArrowLeft, egui::Modifiers::ALT),
        key(egui::Key::ArrowRight, egui::Modifiers::ALT),
        key(egui::Key::ArrowLeft, egui::Modifiers::NONE),
        key(egui::Key::F5, egui::Modifiers::NONE),
        key(egui::Key::L, egui::Modifiers::COMMAND),
        key(egui::Key::F, egui::Modifiers::COMMAND),
        side(egui::PointerButton::Extra1),
        side(egui::PointerButton::Extra2),
    ]);
    assert_eq!(
        got,
        [
            NavKey::Back,
            NavKey::Forward,
            NavKey::Refresh,
            NavKey::Address,
            NavKey::Search,
            NavKey::Back,
            NavKey::Forward
        ]
    );
}

/// 🔴 生产那条链：整扇窗一帧 —— 工具条上那颗「后退」真点得到（窗口框接在 `Workspace::frame` 上），Alt+→ 真前进。
#[test]
fn the_toolbar_on_the_window_really_goes_back_and_the_keys_really_go_forward() {
    let mut w = window("/a", Vec::new());
    w.navigate_to("/a/b".into());
    let mut ws = Workspace::new(w);
    let ctx = egui::Context::default();
    let _ = ws_frame(&ctx, &mut ws, Vec::new());
    let _ = ws_click(&ctx, &mut ws, egui_phosphor::regular::ARROW_LEFT);
    assert_eq!(ws.pane_on(0).cwd, "/a", "点了「后退」没退回去");
    let alt_right = egui::Event::Key {
        key: egui::Key::ArrowRight,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::ALT,
    };
    let _ = ws_frame(&ctx, &mut ws, vec![alt_right]);
    assert_eq!(ws.pane_on(0).cwd, "/a/b", "Alt+→ 没前进");
}

/// 状态栏那一行：项数 · 选中几项（文件大小合计，目录不算）；隐藏文件切成不显示 ⇒ 说另有几项没显示。
#[test]
fn the_status_line_counts_items_selection_and_hidden_files() {
    let mut w = window(
        "/m",
        vec![
            row("d", true, 4096, 1),
            row("a.txt", false, 1024, 2),
            row("b.txt", false, 2048, 3),
            row(".env", false, 10, 4),
        ],
    );
    assert_eq!(
        w.status_line(),
        copy_text("rsFilewinChrome.status.count", &[("n", "4")])
    );
    w.apply_intent(crate::select::Intent::SelectAll, 0.0, None);
    assert!(w.status_line().ends_with(&copy_text(
        "rsFilewinChrome.status.picked",
        &[
            ("n", "4"),
            ("size", &crate::rows::human_size(1024 + 2048 + 10))
        ]
    )));
    assert!(w.set_show_hidden(false));
    assert_eq!(w.listing.rows.lock().unwrap().len(), 3);
    assert_eq!(w.selection().len(), 3, "收起来的那一项该从选中里去掉");
    assert!(w.status_line().starts_with(&copy_text(
        "rsFilewinChrome.status.countHidden",
        &[("n", "3"), ("hidden", "1")]
    )));
    assert!(w.set_show_hidden(true));
    let names: Vec<String> = w
        .listing
        .rows
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(
        names,
        ["d", ".env", "a.txt", "b.txt"],
        "并回来要按当前那一列重排"
    );
}

/// 隐藏文件默认显示、淡一级（这一帧被画成淡色的恰好是名字以点开头的那几行）；命令栏那颗真点得到，点了就收起来。
#[test]
fn hidden_files_show_faded_and_the_toggle_hides_them() {
    let mut w = window(
        "/m",
        vec![
            row(".git", true, 0, 1),
            row("a", false, 1, 1),
            row(".rc", false, 1, 1),
        ],
    );
    let ctx = egui::Context::default();
    let _ = frame(&ctx, &mut w, Vec::new());
    let _ = frame(&ctx, &mut w, Vec::new());
    assert_eq!(w.tally.faded_rows, [0, 2]);
    let _ = click(&ctx, &mut w, HIDDEN_LABEL.as_str());
    assert!(!w.shows_hidden());
    let _ = frame(&ctx, &mut w, Vec::new());
    assert_eq!(w.tally.total_rows, 1);
    assert!(w.tally.faded_rows.is_empty());
}

/// 表头：点「修改时间」⇒ 按时间新的在前；再点 ⇒ 反过来（目录照旧在前）。拖分隔线 ⇒ 那一列变宽。
#[test]
fn clicking_a_header_sorts_and_clicking_again_reverses() {
    let mut w = window(
        "/m",
        vec![
            row("old", false, 1, 10),
            row("new", false, 1, 30),
            row("mid", false, 1, 20),
            row("dir", true, 0, 1),
        ],
    );
    let ctx = egui::Context::default();
    let _ = frame(&ctx, &mut w, Vec::new());
    let mtime = crate::source::SortBy::Mtime.label();
    let _ = click(&ctx, &mut w, &mtime);
    let names = |w: &FileWindow| -> Vec<String> {
        w.listing
            .rows
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.name.clone())
            .collect()
    };
    assert_eq!(names(&w), ["dir", "new", "mid", "old"]);
    // 再点：那一格的字带上了箭头，找带箭头的那一段。
    let painted = frame(&ctx, &mut w, Vec::new());
    let shown = format!("{mtime} {}", egui_phosphor::regular::CARET_DOWN);
    let at = rects_of(&painted, &shown);
    assert_eq!(at.len(), 1, "表头上该写着「{shown}」：{painted:?}");
    let _ = frame(&ctx, &mut w, crate::rows::testing::click_at(at[0].center()));
    assert_eq!(names(&w), ["dir", "old", "mid", "new"]);
    // 列宽：拖「修改时间|类型」那条线往右 40 ⇒ 修改时间那一列宽 40、类型那一列窄 40。
    let before = w.cols;
    w.cols.drag(1, 40.0);
    assert_eq!(
        (w.cols.mtime, w.cols.kind),
        (before.mtime + 40.0, before.kind - 40.0)
    );
    w.cols.drag(1, -10_000.0);
    assert_eq!(w.cols.mtime, crate::rows::MIN_COL, "拖到底停在最窄");
    // 一行放不下（双栏 · 窄窗口）：四列总宽恒等于那一行，名称列留最窄那一档，不画出界。
    for wide in [300.0_f32, 900.0] {
        let row = egui::Rect::from_min_size(egui::pos2(10.0, 0.0), egui::vec2(wide, 28.0));
        let r = w.cols.rects(row);
        assert_eq!(
            (r[0].left(), r[3].right()),
            (row.left(), row.right()),
            "宽 {wide}"
        );
        assert!(r[0].width() >= crate::rows::MIN_NAME - 0.01);
    }
}

/// 左栏「其他机器」：去掉这一台与本机；没有样子时点了出声、不发。
#[test]
fn other_machines_exclude_this_one_and_the_local_one() {
    let mut w = window("/m", Vec::new());
    w.machines = vec!["<local>".into(), "box".into(), "laptop".into(), "devbox".into()];
    let mut ws = Workspace::new(w);
    assert_eq!(ws.other_machines(), ["laptop", "devbox"]);
    assert!(!ws.open_other("laptop", None), "没通道不该发");
    assert_eq!(ws.notice(), Some(crate::shell::NO_LINE.as_str()));
}

/// 左栏「家目录」问不到：悬停看得见原因、点一下再问一次（此前灰着、提示看不见、再也不问）。
#[test]
fn a_home_that_could_not_be_asked_says_why_and_asks_again_on_click() {
    let ctx = egui::Context::default();
    let mut ws = Workspace::new(window("/a", Vec::new()));
    let _ = ws_frame(&ctx, &mut ws, Vec::new());
    *ws.home.0.lock().unwrap() = HomeState::Failed("原因甲".into());
    let _ = ws_click(&ctx, &mut ws, &copy_text("rsFilewinChrome.side.home", &[]));
    assert_eq!(
        ws.home_known(),
        Some(Err(crate::shell::NO_LINE.to_string())),
        "点了问不到的家目录，没有再问一次"
    );
}
