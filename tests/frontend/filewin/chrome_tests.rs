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

/// 地址栏手输：那一串（只去首尾空白）原样去列，不归一、不判绝对（相对路径由那台拒、照它那一句说）；
/// 列成了 ⇒ 当前目录换成那台写好的那一级（回包面包屑最后一段），不多记一步历史。
#[test]
fn the_address_bar_sends_what_was_typed_and_lands_where_the_backend_says() {
    let mut w = window("/a", Vec::new());
    w.begin_address_edit();
    assert_eq!(w.address_editing(), Some("/a"));
    w.addr_edit = Some(" ~/x//y/ ".into());
    assert!(w.commit_address());
    assert_eq!(w.cwd, "~/x//y/", "手输那一串该原样交出去");
    assert_eq!(w.address_editing(), None);
    *w.listing.landed_dir.lock().unwrap() = Some("/home/u/x/y".into());
    w.adopt_landed_dir();
    assert_eq!(w.cwd, "/home/u/x/y", "列成了没换成那台写好的那一级");
    assert!(
        w.listing.landed_dir.lock().unwrap().is_none(),
        "收过一次就该清掉"
    );
    assert!(w.go_back());
    assert_eq!(w.cwd, "/a", "换成那台写好的形不该多记一步历史");
    w.addr_edit = Some("rel/dir".into());
    assert!(w.commit_address());
    assert_eq!(w.cwd, "rel/dir", "相对路径也原样交出去（由那台拒）");
    w.addr_edit = Some("   ".into());
    assert!(!w.commit_address(), "空白一串不去列");
    // 上一个目录列成的那一级没收走就换了目录 ⇒ 作废：不拿它把当前目录拨回去，「家」「根」也不按它亮。
    *w.listing.landed_dir.lock().unwrap() = Some("/home/u/x/y".into());
    w.listing
        .at_home
        .store(true, std::sync::atomic::Ordering::SeqCst);
    *w.listing.caret.lock().unwrap() = Some(crate::source::Caret {
        by: crate::source::SortBy::Size,
        descending: true,
    });
    w.navigate_to("/z".into());
    w.adopt_landed_dir();
    assert_eq!(w.cwd, "/z", "换了目录还被上一个目录的回包拨回去");
    assert!(
        !w.listing.at_home.load(std::sync::atomic::Ordering::SeqCst),
        "换了目录「家」还按上一个亮着"
    );
    assert_eq!(
        *w.listing.caret.lock().unwrap(),
        None,
        "换了目录表头箭头还照上一个目录的回包画"
    );
}

/// 同一目录重问、问不成（没运行时 · 有运行时没通道 · 那台拒了）⇒ 错误那一屏不挂上一趟的头：
/// 表头箭头 · 「家」「根」· 截断那一条一起作废。
#[test]
fn a_failed_reask_drops_what_the_last_head_said() {
    let side = crate::find::testing::SideRt::new();
    let ways: [(&str, fn(&mut FileWindow, &crate::find::testing::SideRt)); 3] = [
        ("没运行时", |w, _| w.reload()),
        ("没通道", |w, side| {
            w.rt = Some(side.handle());
            w.reload();
        }),
        ("那台拒了", |w, _| {
            let mine = w.listing.start();
            assert!(crate::shell::listing::store_listed_if_current(
                &w.listing,
                mine,
                Err("拒了".into()),
            ));
        }),
    ];
    for (how, ask) in ways {
        let mut w = window("/m", Vec::new());
        let mine = w.listing.start();
        assert!(crate::shell::listing::store_listed_if_current(
            &w.listing,
            mine,
            Ok(crate::find::testing::reply_of_rows(
                &[],
                cmd_wire::files::LsHead {
                    by: crate::source::SortBy::Size,
                    descending: true,
                    truncated: true,
                    at_home: true,
                    ..crate::find::testing::plain_head(9)
                },
            )),
        ));
        let l = &w.listing;
        let at = |c: &std::sync::atomic::AtomicBool| c.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            l.caret.lock().unwrap().is_some()
                && at(&l.truncated)
                && at(&l.at_home)
                && at(&l.at_root)
        );
        ask(&mut w, &side);
        let l = &w.listing;
        assert!(l.error.lock().unwrap().is_some(), "{how}：没落成错误那一屏");
        assert_eq!(
            *l.caret.lock().unwrap(),
            None,
            "{how}：错误那一屏还挂着上一趟的箭头"
        );
        assert!(!at(&l.truncated), "{how}：错误那一屏还说上一趟被截断");
        assert!(
            !at(&l.at_home) && !at(&l.at_root),
            "{how}：「家」「根」还按上一趟亮"
        );
    }
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

/// 状态栏那一行：项数 · 已选几项（都是文件时带合计大小；有目录就只写几项）；隐藏文件切成不显示 ⇒ 说另有几项没显示。
#[test]
fn the_status_line_counts_items_selection_and_hidden_files() {
    let mut w = window(
        "/m",
        vec![
            row("d", true, 4096, 1),
            row(".env", false, 10, 4),
            row("a.txt", false, 1024, 2),
            row("b.txt", false, 2048, 3),
        ],
    );
    assert_eq!(
        w.status_line(),
        copy_text("rsFilewinChrome.status.count", &[("n", "4")])
    );
    w.apply_intent(crate::select::Intent::SelectAll, 0.0, None);
    // 选中里有目录 ⇒ 只写几项（目录要算大小才知道多大）。
    assert!(
        w.status_line()
            .ends_with(&copy_text("rsFilewinChrome.status.pickedN", &[("n", "4")])),
        "{}",
        w.status_line()
    );
    // 都是文件 ⇒ 几项 · 合计大小。
    let mut files = window(
        "/m",
        vec![row("a.txt", false, 1024, 2), row("b.txt", false, 2048, 3)],
    );
    files.apply_intent(crate::select::Intent::SelectAll, 0.0, None);
    assert!(
        files.status_line().ends_with(&copy_text(
            "rsFilewinChrome.status.picked",
            &[("n", "2"), ("size", &copy_core::size_text(1024 + 2048))]
        )),
        "{}",
        files.status_line()
    );
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
        "并回来要回到它在那一屏里的位置"
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
    // 点表头会重列一趟：接一台收下就不答的 ⇒ 每一趟都停在「回包未到」。
    let side = crate::find::testing::SideRt::new();
    crate::find::testing::hold_silent(&mut w, &side);
    let ctx = egui::Context::default();
    let _ = frame(&ctx, &mut w, Vec::new());
    let mtime = crate::rows::column_label(crate::source::SortBy::Mtime);
    let size = crate::rows::column_label(crate::source::SortBy::Size);
    let (down, up) = (
        egui_phosphor::regular::CARET_DOWN,
        egui_phosphor::regular::CARET_UP,
    );
    // 表头上带箭头的那几段（四列 × 两个朝向里画出来的）。
    let carets = |painted: &[crate::copy::testing::PaintedText]| {
        crate::rows::ORDER
            .into_iter()
            .map(crate::rows::column_label)
            .flat_map(|l| [format!("{l} {down}"), format!("{l} {up}")])
            .filter(|t| !rects_of(painted, t).is_empty())
            .collect::<Vec<_>>()
    };
    // 回包头还没到过 ⇒ 哪一格都不带箭头。
    assert_eq!(
        carets(&frame(&ctx, &mut w, Vec::new())),
        Vec::<String>::new()
    );
    let _ = click(&ctx, &mut w, &mtime);
    // 先后由后端排：点下去换的是这一档（重列一趟），时间第一下新的在前。
    let by = |by, reverse| crate::source::Sort { by, reverse };
    assert_eq!(w.sort, by(crate::source::SortBy::Mtime, false));
    // 箭头画在哪一列、朝哪都照那台答的（回包头的 `by` · `descending`，落进 listing），窗口不自己算：
    // 那台答了「按修改时间 · 从大到小」⇒ 修改时间那一格朝下。
    *w.listing.caret.lock().unwrap() = Some(crate::source::Caret {
        by: crate::source::SortBy::Mtime,
        descending: true,
    });
    let shown = format!("{mtime} {down}");
    let painted = frame(&ctx, &mut w, Vec::new());
    assert_eq!(carets(&painted), [shown.clone()], "{painted:?}");
    // 点「大小」、回包未到 ⇒ 请求换成大小，箭头仍在那台答的修改时间那一格、朝下。
    let _ = click(&ctx, &mut w, &size);
    assert_eq!(w.sort, by(crate::source::SortBy::Size, false));
    let painted = frame(&ctx, &mut w, Vec::new());
    assert_eq!(carets(&painted), [shown.clone()], "{painted:?}");
    // 点带箭头的那一段 ⇒ 切回修改时间（第一下）；再点 ⇒ 反过来。
    let _ = click(&ctx, &mut w, &shown);
    assert_eq!(w.sort, by(crate::source::SortBy::Mtime, false));
    let _ = click(&ctx, &mut w, &shown);
    assert_eq!(w.sort, by(crate::source::SortBy::Mtime, true));
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

/// 左栏「其他机器」：本机打头（本机也开文件窗口）、去掉这一台；看着本机的那扇窗不再列本机；没有样子时点了出声、不发。
#[test]
fn other_machines_lead_with_the_local_one_and_leave_out_this_one() {
    let mut w = window("/m", Vec::new());
    w.machines = vec![
        "<local>".into(),
        "box".into(),
        "laptop".into(),
        "devbox".into(),
    ];
    let mut ws = Workspace::new(w);
    assert_eq!(ws.other_machines(), [cmd_wire::LOCAL, "laptop", "devbox"]);
    let mut here = window("/m", Vec::new());
    here.source = crate::source::Source::remote(cmd_wire::LOCAL.to_string());
    here.machines = vec!["box".into(), "laptop".into()];
    assert_eq!(Workspace::new(here).other_machines(), ["box", "laptop"]);
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
