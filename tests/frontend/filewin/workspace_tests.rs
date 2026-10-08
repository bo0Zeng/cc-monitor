//! 焦点闸（只有焦点那一栏接键盘与拖入）· 新标签 / 右栏沿用同一条通道 · 有活的标签关不掉并说出是哪件。
//! 「复制到另一栏」走后端 `files-copy`、不下载再上传：在那台机器上能就地完成的事，不该让字节跑一趟网络。
//!
//! [`super`] 的判据 —— **标签页 ＋ 双栏 ＋ 复制到另一栏**（每一条都真跑生产那个 [`Workspace::frame`]）。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`only_the_focused_side_takes_the_keyboard`] | 两栏开着按 ↓：焦点那栏的光标动、另一栏不动（相等）；点一下另一栏再按 ↓，反过来 | 光标读的是各栏自己的选中态 |
//! | [`only_the_focused_side_takes_dropped_files`] | 拖一个本机文件进来：只有焦点那一栏接（相等） | 读的是两栏各自那一格错误 |
//! | [`a_new_tab_and_the_right_side_inherit_the_line_and_the_shelf`] | 新标签 / 右栏接着同一条通道（线上真多两趟 `files-ls`）与同一份书签 | 实得是合成后端的那本账 |
//! | [`tabs_open_switch_and_close`] | 开 / 切 / 关标签页之后，标签数与每个标签的目录逐格相等；最后一个关不掉 | 期望手写 |
//! | [`a_busy_tab_refuses_to_close_and_says_why`] | 有一问摆着的标签关不掉、那句话画出来；收掉那一问就关得掉 | 那句话从这一帧的 galley 里读回 |
//! | [`splitting_and_unsplitting`] | 开双栏 ⇒ 右栏落在焦点那栏的目录上、焦点给它；收 ⇒ 回一栏 | 期望手写 |
//! | [`right_click_menus_open_on_both_sides`] | 两栏各右键一次，两次菜单**都真的摆出来**（序号是进程级的，两栏不撞） | 菜单那一块的矩形从 egui 的内存里读 |
//! | [`copy_across_speaks_files_copy_with_a_common_root`] | 线上那一行 `files-copy` 的 `root` / `from` / `to` 逐格相等（合成后端） | 期望手写；实得是合成后端真收到的那一行 |
//! | [`copy_across_refuses_what_it_cannot_do_and_sends_nothing`] | 没开双栏 / 没选 / 选中里有有损名 / 两栏同目录 ⇒ 出声、线上零条（带正控：合法那一摞恰好两条） | 零命中读的是线上那本账 |
//! | [`copy_across_takes_the_whole_selection_and_a_directory_goes_recursive`] | 一摞（文件 ＋ 目录）⇒ 线上两行逐格相等，目录那行带 `recursive: true` | 期望手写；实得是合成后端真收到的 |
//! | [`across_args_cuts_paths_at_the_common_directory`] | 公共前缀那一刀逐格相等（含只在根下相交的那一形） | 期望手写 |
//! | [`dragging_rows_onto_the_other_side_copies_them_and_dropping_back_does_nothing`] | 〔「行拖到另一栏的手势」〕合成指针拖起左栏一行 ⇒ 拖着时画「复制 1 项到另一栏」、松在右栏 ⇒ 线上恰一条 `files-copy`（逐格相等）；阴性：拖回本栏松手 ⇒ 零条 | 实得是合成后端真收到的；期望手写 |
//! | [`ctrl_t_and_ctrl_w_open_and_close_tabs_on_the_focused_side`] | 〔标签页快捷键 ＋ `§6.3` 键盘四道闸〕Ctrl+T / Ctrl+W 只作用于焦点那一栏（两栏各自的标签数逐格相等）；有一问摆着 ⇒ 零作用；不按 Ctrl 的 T ⇒ 零作用 | 标签数读的是各栏自己的表；期望手写 |
//!
//! ⚠ 买不到：真窗口真画在屏幕上；真的拖一行过去（手势没做，理由住 `super` 头注）。

use super::*;
use crate::copy::testing::{rects_of, text_in_frame, PaintedText};
use crate::find::testing::{window_on, wire_up, Declared, FakeBackend};
use crate::source::{Row, Source};

const SCREEN: egui::Vec2 = egui::vec2(1600.0, 900.0);

fn cfg(label: &str) -> String {
    String::from(label)
}

fn file(name: &str, dir: &str) -> Row {
    Row {
        name: name.into(),
        path: format!("{dir}/{name}"),
        is_dir: false,
        size: 10,
        lossy_name: false,
    }
}

fn subdir(name: &str, dir: &str) -> Row {
    Row {
        is_dir: true,
        ..file(name, dir)
    }
}

/// 一个不联网的目录视图（行直接喂进去）。
fn pane(dir: &str, names: &[&str]) -> FileWindow {
    let mut w = FileWindow::seeded(
        Source::remote(cfg("ws")),
        dir.to_string(),
        None,
        names.iter().map(|n| file(n, dir)).collect::<Vec<_>>(),
    );
    *w.listing.error.lock().unwrap() = None;
    w
}

/// 驱动器：一帧一帧喂**生产那个** `Workspace::frame`（时钟每帧走一秒，永不凑成双击）。
struct Drive {
    ctx: egui::Context,
    t: f64,
}

impl Drive {
    fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            t: 0.0,
        }
    }

    fn frame(&mut self, ws: &mut Workspace, events: Vec<egui::Event>) -> Vec<PaintedText> {
        self.t += 1.0;
        let mut all = vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)];
        all.extend(events);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            time: Some(self.t),
            events: all,
            ..Default::default()
        };
        let out = self.ctx.run_ui(input, |ui| ws.frame(ui));
        let painted = text_in_frame(&out);
        out.drop_without_applying_deltas();
        painted
    }

    fn key(&mut self, ws: &mut Workspace, k: egui::Key) {
        self.frame(
            ws,
            vec![egui::Event::Key {
                key: k,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }

    /// 在 `pos` 上按一下 `button`（先移过去一帧：命中测试按上一帧的 widget 表做）。
    fn click(&mut self, ws: &mut Workspace, pos: egui::Pos2, button: egui::PointerButton) {
        self.frame(ws, vec![egui::Event::PointerMoved(pos)]);
        self.frame(
            ws,
            vec![
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }

    /// 这一帧上内容正好是 `label` 的那几处，按横坐标从左到右。
    fn find(&mut self, ws: &mut Workspace, label: &str) -> Vec<egui::Rect> {
        let painted = self.frame(ws, Vec::new());
        let mut at = rects_of(&painted, label);
        at.sort_by(|a, b| a.left().total_cmp(&b.left()));
        at
    }
}

fn two_sides(left: FileWindow, right: FileWindow) -> Workspace {
    let mut ws = Workspace::new(left);
    assert!(ws.add_side(right));
    ws
}

fn cursors(ws: &Workspace) -> Vec<Option<String>> {
    (0..ws.sides())
        .map(|k| ws.pane_on(k).selection().cursor().map(str::to_string))
        .collect()
}

#[test]
fn only_the_focused_side_takes_the_keyboard() {
    let mut ws = two_sides(pane("/l", &["a", "b"]), pane("/r", &["c", "d"]));
    let mut d = Drive::new();
    assert_eq!(ws.focus(), 1, "开第二栏之后焦点在它身上");
    d.frame(&mut ws, Vec::new());
    d.key(&mut ws, egui::Key::ArrowDown);
    assert_eq!(
        cursors(&ws),
        vec![None, Some("c".to_string())],
        "焦点那栏之外也动了"
    );
    // 点一下左栏里一个空处（左栏第一行的名字上）⇒ 焦点换过去；再按 ↓ ⇒ 只有左栏动。
    let at = d.find(&mut ws, "a");
    assert_eq!(at.len(), 1);
    d.click(&mut ws, at[0].center(), egui::PointerButton::Primary);
    assert_eq!(ws.focus(), 0, "在左栏里按下鼠标，焦点没过去");
    d.key(&mut ws, egui::Key::ArrowDown);
    assert_eq!(
        cursors(&ws),
        vec![Some("b".to_string()), Some("c".to_string())],
        "焦点换到左栏之后，右栏的光标还在动 / 左栏没动"
    );
}

/// 一个合成的「拖进来的本机文件」（egui 只要它的路径；内容一个字节都不读）。
#[derive(Debug)]
struct FakeDrop(std::path::PathBuf);

impl egui::DroppedFile for FakeDrop {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        Err("判据不读内容".into())
    }
}

/// 🔴 拖进来的本机文件**只交给焦点那一栏**（两栏都接的话，拖一个文件进来两栏各传一次）。
///
/// 两栏都没有运行时 ⇒ 接了的那一栏会当场出声「上传要一个 tokio 运行时…」（`start_drop` 那一支）；
/// 没接的那一栏一个字都不说。读的是两栏各自的那一格错误。
#[test]
fn only_the_focused_side_takes_dropped_files() {
    let mut ws = two_sides(pane("/l", &["a"]), pane("/r", &["b"]));
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
        dropped_files: vec![std::sync::Arc::new(FakeDrop(std::path::PathBuf::from(
            "/tmp/拖进来的.txt",
        )))],
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| ws.frame(ui));
    out.drop_without_applying_deltas();
    let said: Vec<bool> = (0..2)
        .map(|k| ws.pane_on(k).listing.error.lock().unwrap().is_some())
        .collect();
    assert_eq!(
        said,
        vec![false, true],
        "拖入没有只落在焦点那一栏（焦点在第 {} 栏）",
        ws.focus()
    );
}

/// 新标签页 / 右栏**照着焦点那个目录视图起**：同一条通道（真的去列了一趟）、同一份书签、落在同一个目录。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_tab_and_the_right_side_inherit_the_line_and_the_shelf() {
    let wired = wire_up(
        "fw34-ws-inherit",
        FakeBackend::new(&["files-ls"], Declared::default()),
    )
    .await;
    let mut first = window_on(&wired, "/");
    first.shelf = Some(crate::bookmarks::Shelf::open(
        None,
        &crate::source::Origin("fw34-ws-inherit".into()),
    ));
    let mut ws = Workspace::new(first);
    assert!(ws.open_tab(0));
    assert!(ws.set_split(true));
    for _ in 0..600 {
        if wired.count("files-ls") >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        wired.count("files-ls"),
        2,
        "新标签 ＋ 右栏各该经那条通道列一趟"
    );
    for (side, i) in [(0usize, 1usize), (1, 0)] {
        let p = ws.tab(side, i);
        assert!(
            p.line.is_some() && p.rt.is_some(),
            "第 {side} 栏第 {i} 个没接上通道"
        );
        assert!(p.shelf.is_some(), "第 {side} 栏第 {i} 个没接上书签");
        assert_eq!(p.cwd, "/");
    }
}

fn cwds_on(ws: &Workspace, side: usize) -> Vec<String> {
    (0..ws.tabs_on(side))
        .map(|i| ws.tab(side, i).cwd.clone())
        .collect()
}

#[test]
fn tabs_open_switch_and_close() {
    let mut ws = Workspace::new(pane("/srv", &["a"]));
    assert!(!ws.close_tab(0, 0), "最后一个标签页不许关");
    assert!(ws.notice().is_some());
    assert!(ws.open_tab(0));
    assert_eq!(
        cwds_on(&ws, 0),
        vec!["/srv", "/srv"],
        "新标签落在这一栏当前的目录上"
    );
    assert_eq!(ws.active_on(0), 1, "开完就切过去");
    ws.pane_on_mut(0).navigate_to("/srv/x".into());
    assert!(ws.select_tab(0, 0));
    assert_eq!(ws.pane_on(0).cwd, "/srv");
    assert!(ws.close_tab(0, 1));
    assert_eq!(cwds_on(&ws, 0), vec!["/srv"]);
    assert_eq!(ws.active_on(0), 0);
    // 真点「＋」：从这一帧画出来的字里找那颗按钮。
    let mut d = Drive::new();
    let plus = d.find(&mut ws, egui_phosphor::regular::PLUS);
    let plus: Vec<_> = plus
        .into_iter()
        .filter(|r| r.top() < 140.0 && r.left() > 220.0)
        .collect();
    assert_eq!(plus.len(), 1, "一栏时标签栏末尾的「＋」该恰好一颗");
    d.click(&mut ws, plus[0].center(), egui::PointerButton::Primary);
    assert_eq!(ws.tabs_on(0), 2, "点了「＋」没开出标签页");
    // × 只在当前那个标签（与悬停的那个）上出 ⇒ 两个标签时恰好一颗，点它关掉当前那个。
    let close: Vec<_> = d
        .find(&mut ws, egui_phosphor::regular::X)
        .into_iter()
        .filter(|r| r.top() < 140.0 && r.left() > 220.0)
        .collect();
    assert_eq!(close.len(), 1, "当前那个标签该有一颗「×」");
    d.click(&mut ws, close[0].center(), egui::PointerButton::Primary);
    assert_eq!(ws.tabs_on(0), 1, "点了「×」没关掉");
}

#[test]
fn a_busy_tab_refuses_to_close_and_says_why() {
    let mut ws = Workspace::new(pane("/srv", &["a"]));
    assert!(ws.open_tab(0));
    // 后台那个标签（第 0 个）摆着一问：新建空文件那个框。
    assert!(ws.select_tab(0, 0));
    assert!(ws.pane_on_mut(0).begin_new_file());
    assert!(ws.select_tab(0, 1));
    assert!(ws.tab(0, 0).busy_reason().is_some());
    assert!(!ws.close_tab(0, 0), "有一问摆着的标签被关掉了");
    assert_eq!(ws.tabs_on(0), 2);
    let said = ws.notice().expect("关不掉却没说为什么").to_string();
    let mut d = Drive::new();
    // 那句话收成右下角的回执（浮层第一帧只量大小、第二帧才画）。
    let _ = d.frame(&mut ws, Vec::new());
    let painted = d.frame(&mut ws, Vec::new());
    assert!(
        painted.iter().any(|(t, _)| *t == said),
        "那句话没画出来：{said}"
    );

    // 收掉那一问 ⇒ 关得掉（阴性对照）。
    assert!(ws.select_tab(0, 0));
    ws.pane_on_mut(0).cancel_new_file();
    assert!(ws.close_tab(0, 0));
    assert_eq!(ws.tabs_on(0), 1);
}

/// 稿：关一个还有传输在跑的目录标签页 ⇒ **直接关**，传输照跑（它是「进度」表里的一行，不挂在标签页上）；
/// 那一趟落地 ⇒ 正开在那个目录的标签页由窗口这一级重列（开它的那个标签页已经不在了）。
#[test]
fn closing_a_tab_with_a_transfer_running_closes_it_and_the_transfer_keeps_going() {
    let mut ws = Workspace::new(pane("/srv", &["a"]));
    assert!(ws.open_tab(0));
    // 第 1 个标签页上有一摞上传在飞（表里一行；看板还没落结局）。
    let board = crate::transfer::DropBoard::default();
    let id = ws.progress.add(
        crate::progress::Trip::Upload {
            board: board.clone(),
            items: Vec::new(),
            dir: "srv".into(),
        },
        Some("/srv".into()),
        None,
    );
    assert!(
        ws.tab(0, 1).progress.same(&ws.progress),
        "新标签页拿的不是窗口那一份「进度」表"
    );
    assert!(
        ws.tab(0, 1).busy_reason().is_none(),
        "在传的那一摞挡住了关标签页"
    );
    assert!(ws.close_tab(0, 1), "有传输在跑的标签页没关掉");
    assert_eq!(ws.tabs_on(0), 1);
    assert_eq!(ws.progress.running(), 1, "关了标签页，那一趟从表里没了");
    // 关窗时它还算（关窗就没了）。
    assert!(
        ws.pane_on(0).work_reason().is_some(),
        "关窗那一问没算上还在跑的那一趟"
    );
    // 那一趟的号要由窗口这一级接管（开它的标签页不在了）：落地 ⇒ 开在 /srv 的那个标签页重列一次。
    ws.progress.orphan(id);
    let before = ws
        .pane_on(0)
        .listing
        .epoch
        .load(std::sync::atomic::Ordering::SeqCst);
    board.finish(crate::transfer::DropOutcome {
        ok: 1,
        ..Default::default()
    });
    ws.settle_progress();
    assert!(
        ws.pane_on(0)
            .listing
            .epoch
            .load(std::sync::atomic::Ordering::SeqCst)
            > before,
        "那一趟落地了，开在那个目录的标签页没重列"
    );
    assert_eq!(ws.progress.running(), 0);
}

/// 稿 18：和那台断了 ⇒ 工具条下一条警告条（重连中 ⇒「devbox 断开 · 重新连接中…」，没按钮；一轮没连上 ⇒
/// 「devbox 离线 · 采样 HH:MM」＋［重新连接］，时刻 ＝ 摆着那一屏列到的时刻）；列表照常摆着、F5 不把它换成一句「连不上」；
/// 写类那几件说「离线 · 只读」做不了；又连上 ⇒ 条没了、每个标签页重列一遍。
#[test]
fn a_dropped_link_shows_the_strip_keeps_the_stale_listing_and_blocks_writes() {
    use crate::source::LinkSeen;
    let mut ws = Workspace::new(pane("/srv", &["a.txt", "b.txt"]));
    let at = 1_700_000_000u64;
    ws.pane_on(0)
        .listing
        .landed
        .store(at, std::sync::atomic::Ordering::SeqCst);
    let machine = ws.pane_on(0).source.label();
    let screen = egui::vec2(1280.0, 800.0);
    // 连着（流说了 Up）⇒ 没有条。
    ws.link.set(LinkSeen::Up);
    let painted = frames_at(&mut ws, screen, 3);
    let retry = copy_text("rsFilewinChrome.link.retry", &[]);
    assert!(!painted.iter().any(|(t, _)| *t == retry), "连着却有断线条");
    // 刚断：在重连。
    ws.link.set(LinkSeen::Reconnecting);
    let painted = frames_at(&mut ws, screen, 3);
    let said = copy_text(
        "rsFilewinChrome.link.reconnecting",
        &[("machine", &machine)],
    );
    assert!(
        painted.iter().any(|(t, _)| *t == said),
        "在重连却没说：{said}"
    );
    assert!(
        !painted.iter().any(|(t, _)| *t == retry),
        "在重连时就给了「重新连接」"
    );
    // 一轮没连上：离线 · 采样那一刻 ＋［重新连接］。
    ws.link.set(LinkSeen::Down);
    let painted = frames_at(&mut ws, screen, 3);
    let said = copy_text(
        "rsFilewinChrome.link.offline",
        &[
            ("machine", &machine),
            ("time", &crate::source::mtime_text(at).short),
        ],
    );
    assert!(
        painted.iter().any(|(t, _)| *t == said),
        "离线那一句没画：{said}"
    );
    assert!(
        painted.iter().any(|(t, _)| *t == retry),
        "离线却没有「重新连接」"
    );
    // 列表照常摆着；F5 不去问（问了只会把这一屏换成一句「连不上」）。
    let before = ws
        .pane_on(0)
        .listing
        .epoch
        .load(std::sync::atomic::Ordering::SeqCst);
    ws.pane_on(0).reload();
    assert_eq!(
        ws.pane_on(0)
            .listing
            .epoch
            .load(std::sync::atomic::Ordering::SeqCst),
        before,
        "离线时 F5 还是去问了"
    );
    assert_eq!(ws.pane_on(0).listing.rows.lock().unwrap().len(), 2);
    // 写类那几件做不了，说同一句。
    let read_only = copy_text("rsFilewinChrome.link.readOnly", &[]);
    assert_eq!(
        ws.pane_on(0).offline_refusal(crate::select::Action::Rename),
        Some(read_only.clone())
    );
    // 下载也要那台（读也得连着）⇒ 同样灰着（「要不要那台机器」一处判，`offline_cmd`）。
    assert_eq!(
        ws.pane_on(0)
            .offline_refusal(crate::select::Action::Download),
        Some(read_only.clone()),
        "下载要那台机器，离线时却没挡"
    );
    // 又连上：条没了，标签页重列一遍。
    ws.link.set(LinkSeen::Up);
    let painted = frames_at(&mut ws, screen, 3);
    assert!(
        !painted.iter().any(|(t, _)| *t == retry),
        "连上了断线条还在"
    );
    assert!(
        ws.pane_on(0)
            .listing
            .epoch
            .load(std::sync::atomic::Ordering::SeqCst)
            > before,
        "又连上了却没重列那一屏"
    );
    assert_eq!(
        ws.pane_on(0).offline_refusal(crate::select::Action::Rename),
        None
    );
}

#[test]
fn splitting_and_unsplitting() {
    let mut ws = Workspace::new(pane("/srv/data", &["a"]));
    assert!(ws.set_split(true));
    assert_eq!(ws.sides(), 2);
    assert_eq!(ws.pane_on(1).cwd, "/srv/data", "右栏没落在焦点那栏的目录上");
    assert_eq!(ws.focus(), 1);
    assert!(!ws.set_split(true), "已经两栏了再开一次不该变");
    // 右栏有一问摆着 ⇒ 收不掉。
    assert!(ws.pane_on_mut(1).begin_new_file());
    assert!(!ws.set_split(false));
    assert_eq!(ws.sides(), 2);
    ws.pane_on_mut(1).cancel_new_file();
    assert!(ws.set_split(false));
    assert_eq!((ws.sides(), ws.focus()), (1, 0));
}

/// 🔴 两栏**各**右键一次，两个菜单都真的摆出来。
///
/// 菜单那一层的 egui id 按「第几次开菜单」取号；那个号若是每个目录视图各数各的，
/// 两栏各自的第一个菜单会撞同一个 id ⇒ egui 当成「开着时有人点了别处」当场关掉 ⇒ 第二栏右键只关不开。
#[test]
fn right_click_menus_open_on_both_sides() {
    let mut ws = two_sides(pane("/l", &["a"]), pane("/r", &["b"]));
    let mut d = Drive::new();
    for (name, side) in [("b", 1usize), ("a", 0usize)] {
        let at = d.find(&mut ws, name);
        assert_eq!(at.len(), 1);
        d.click(&mut ws, at[0].center(), egui::PointerButton::Secondary);
        let m = ws
            .pane_on(side)
            .menu()
            .unwrap_or_else(|| panic!("在第 {side} 栏右键，菜单没摆出来"))
            .clone();
        // 第一帧是量尺寸那一趟 ⇒ 再跑一帧才是真画。
        d.frame(&mut ws, Vec::new());
        let area = d
            .ctx
            .memory(|mem| mem.area_rect(FileWindow::menu_id(m.serial)));
        assert!(area.is_some(), "第 {side} 栏的菜单没真画出来（id 撞了？）");
    }
}

/// 一台合成后端 ＋ 两栏（左 `/srv/a`、右 `/srv/b`），左栏选中 `x.txt`。
async fn across_rig(tag: &str) -> (crate::find::testing::Wired, Workspace) {
    let wired = wire_up(
        tag,
        FakeBackend::new(&["files-copy", "files-stat"], Declared::default()),
    )
    .await;
    let mut left = window_on(&wired, "/srv/a");
    *left.listing.rows.lock().unwrap() =
        vec![file("x.txt", "/srv/a").into(), subdir("d", "/srv/a").into()];
    let right = window_on(&wired, "/srv/b");
    let ws = two_sides(left, right);
    (wired, ws)
}

async fn settle_copy(w: &FileWindow) {
    for _ in 0..600 {
        if w.copy_board.rounds() > 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("等了 3 秒复制那一趟还没回话");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copy_across_speaks_files_copy_with_a_common_root() {
    let (wired, mut ws) = across_rig("fw34-across").await;
    ws.focus_side(0);
    ws.pane_on_mut(0).apply_intent(
        crate::select::Intent::Step {
            by: 1,
            extend: false,
        },
        0.0,
        None,
    );
    assert_eq!(ws.pane_on(0).picked_name(), Ok("x.txt".to_string()));
    assert!(ws.copy_to_other(None), "起不来：{:?}", ws.notice());
    settle_copy(ws.pane_on(1)).await;
    let copies: Vec<serde_json::Value> = wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-copy")
        .map(|r| r["args"].clone())
        .collect();
    assert_eq!(
        copies,
        vec![serde_json::json!({
            "root": "/srv",
            "from": "a/x.txt",
            "to": "b/x.txt",
            "overwrite": false,
        })],
        "线上那一行与期望不等"
    );
    // 结局落在**目标那一栏**（问与结局画在那一侧），源那一栏一趟都没跑。
    assert_eq!(ws.pane_on(1).copy_board.rounds(), 1);
    assert_eq!(ws.pane_on(0).copy_board.rounds(), 0);
}

/// 做不了的那几形：没开双栏 / 一项都没选 / 选了有损名 / 两栏同目录 ⇒ 出声、线上零条（带正控）。
/// 〔FW34 那一版〕「选了目录」「选了两项」也在这里 —— 今天那两形**做得了**（见下一条），从阴性挪成了正控。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copy_across_refuses_what_it_cannot_do_and_sends_nothing() {
    use crate::select::Intent;
    // ① 没开双栏。
    let (wired, mut ws) = across_rig("fw34-across-no").await;
    let mut one = Workspace::new(window_on(&wired, "/srv/a"));
    assert!(!one.copy_to_other(None));
    assert!(one.notice().is_some());
    // ② 一项都没选。
    ws.focus_side(0);
    assert!(!ws.copy_to_other(None));
    assert!(ws.notice().is_some());
    // ③ 选中的一摞里有一个有损名 ⇒ 整摞不做。
    {
        let mut rows = ws.pane_on(0).listing.rows.lock().unwrap();
        rows.push(
            Row {
                lossy_name: true,
                ..file("\u{FFFD}odd", "/srv/a")
            }
            .into(),
        );
    }
    ws.pane_on_mut(0).apply_intent(Intent::SelectAll, 0.0, None);
    assert!(!ws.copy_to_other(None));
    assert!(
        ws.notice().unwrap_or("").contains("\u{FFFD}odd"),
        "没点名是哪一项：{:?}",
        ws.notice()
    );
    ws.pane_on(0).listing.rows.lock().unwrap().pop();
    // ④ 两栏同一个目录。
    ws.pane_on_mut(1).navigate_to("/srv/a".into());
    ws.pane_on_mut(0).apply_intent(Intent::SelectAll, 0.0, None);
    assert!(!ws.copy_to_other(None));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        wired.count("files-copy"),
        0,
        "做不了的那几形上了线：{:?}",
        wired.cmds()
    );
    // 正控：换回别的目录 ⇒ 两项（一个文件、一个目录）各恰好一条。
    ws.pane_on_mut(1).navigate_to("/srv/b".into());
    assert!(ws.copy_to_other(None), "起不来：{:?}", ws.notice());
    settle_copy(ws.pane_on(1)).await;
    assert_eq!(wired.count("files-copy"), 2);
}

/// 要求：「复制目录 · 批量复制」＋ `§6.3`「多选时要每一项都能…才给」。
///
/// 左栏选中一个文件 ＋ 一个目录 ⇒ 线上两行 `files-copy` 逐格相等：文件那一行与 FW34 那一形逐字同（不多一个键），
/// 目录那一行多 `recursive: true`、`overwrite: false`；结局一句把两件的条数加起来（合成后端：文件 1/0、目录 3/2）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copy_across_takes_the_whole_selection_and_a_directory_goes_recursive() {
    let (wired, mut ws) = across_rig("w5-across-batch").await;
    ws.focus_side(0);
    ws.pane_on_mut(0)
        .apply_intent(crate::select::Intent::SelectAll, 0.0, None);
    assert!(ws.copy_to_other(None), "起不来：{:?}", ws.notice());
    settle_copy(ws.pane_on(1)).await;
    let copies: Vec<serde_json::Value> = wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-copy")
        .map(|r| r["args"].clone())
        .collect();
    assert_eq!(
        copies,
        vec![
            serde_json::json!({"root": "/srv", "from": "a/x.txt", "to": "b/x.txt", "overwrite": false}),
            serde_json::json!({"root": "/srv", "from": "a/d", "to": "b/d", "overwrite": false, "recursive": true}),
        ],
        "线上那两行与期望不等"
    );
    let last = ws.pane_on(1).copy_board.last().expect("没有结局");
    let said = crate::copy::outcome_notice(&last).text;
    assert!(
        said.contains(&copy_core::copy_text(
            "rsFilewinCopy.outcome.batchDone",
            &[("n", "2"), ("files", "4"), ("dirs", "2"), ("bytes", "84")]
        )),
        "结局那句没把两件加起来：{said}"
    );
}

#[test]
fn across_args_cuts_paths_at_the_common_directory() {
    let job = |from: &str, to: &str| CopyJob {
        from: from.into(),
        to: to.into(),
        name: crate::source::remote_basename(to).into(),
        is_dir: false,
        from_raw: None,
        to_raw: None,
    };
    let cases: &[(&str, &str, (&str, &str, &str))] = &[
        ("/srv/a/x", "/srv/b/x", ("/srv", "a/x", "b/x")),
        ("/a/x", "/b/x", ("/", "a/x", "b/x")),
        ("/srv/a/x", "/srv/a/sub/x", ("/srv/a", "x", "sub/x")),
        ("/srv/a/sub/x", "/srv/a/x", ("/srv/a", "sub/x", "x")),
        ("/x", "/y/x", ("/", "x", "y/x")),
    ];
    for (from, to, (root, f, t)) in cases {
        let v = across_args(&job(from, to), true).unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "root": root, "from": f, "to": t, "overwrite": true }),
            "{from} → {to}"
        );
    }
    assert_eq!(
        common_dir("/srv/ab", "/srv/a"),
        "/srv",
        "按段比，不按字符前缀比"
    );
}

fn ctrl(k: egui::Key) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    }
}

fn tab_counts(ws: &Workspace) -> Vec<usize> {
    (0..ws.sides()).map(|k| ws.tabs_on(k)).collect()
}

/// 要求：「标签页快捷键（Ctrl+T / Ctrl+W）」＋ `§6.3`「键盘归谁，四道闸」。
#[test]
fn ctrl_t_and_ctrl_w_open_and_close_tabs_on_the_focused_side() {
    let mut ws = two_sides(pane("/l", &["a"]), pane("/r", &["b"]));
    let mut d = Drive::new();
    assert_eq!(ws.focus(), 1);
    d.frame(&mut ws, vec![ctrl(egui::Key::T)]);
    assert_eq!(tab_counts(&ws), vec![1, 2], "Ctrl+T 没落在焦点那一栏");
    assert_eq!(ws.active_on(1), 1, "开完没切过去");
    assert_eq!(ws.pane_on(1).cwd, "/r", "新标签没落在这一栏当前的目录上");
    d.frame(&mut ws, vec![ctrl(egui::Key::W)]);
    assert_eq!(
        tab_counts(&ws),
        vec![1, 1],
        "Ctrl+W 没关掉焦点那一栏的当前标签"
    );
    // 最后一个关不掉，并且出声。
    d.frame(&mut ws, vec![ctrl(egui::Key::W)]);
    assert_eq!(tab_counts(&ws), vec![1, 1]);
    assert!(ws.notice().is_some(), "最后一个标签关不掉却没说为什么");
    // 阴性一：不按 Ctrl 的 T（打字跳转那一路）⇒ 不开标签。
    d.frame(
        &mut ws,
        vec![egui::Event::Key {
            key: egui::Key::T,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(tab_counts(&ws), vec![1, 1], "不带 Ctrl 的 T 开了标签");
    // 阴性二：焦点那一栏有一问摆着（新建空文件那个框）⇒ 键归那个框，Ctrl+T 零作用。
    assert!(ws.pane_on_mut(1).begin_new_file());
    d.frame(&mut ws, vec![ctrl(egui::Key::T)]);
    assert_eq!(tab_counts(&ws), vec![1, 1], "框开着时 Ctrl+T 还是开了标签");
    ws.pane_on_mut(1).cancel_new_file();
    // 框收掉之后的那一帧：框里那个输入框不再画，它手上的键盘焦点跟着放掉。
    d.frame(&mut ws, Vec::new());
    // 正控：框收掉之后同一个键开得出来；焦点换到左栏之后落在左栏。
    ws.focus_side(0);
    d.frame(&mut ws, vec![ctrl(egui::Key::T)]);
    assert_eq!(
        tab_counts(&ws),
        vec![2, 1],
        "焦点换到左栏之后 Ctrl+T 没落在左栏"
    );
}

/// 要求：「行拖到另一栏的手势 —— 今天只有『复制到另一栏』按钮；行上命中矩形是 `Sense::click()`，
/// 要换成可拖并带出『从哪一行拖起』」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dragging_rows_onto_the_other_side_copies_them_and_dropping_back_does_nothing() {
    let (wired, mut ws) = across_rig("w5-drag").await;
    let mut d = Drive::new();
    let name_at = d.find(&mut ws, "x.txt");
    assert_eq!(name_at.len(), 1, "左栏那一行的名字没画出来");
    let start = name_at[0].center();
    let press = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let right_mid = egui::pos2(SCREEN.x * 0.75, SCREEN.y * 0.5);
    // ── 阴性：拖起来又松回本栏 ⇒ 什么都不做。
    d.frame(&mut ws, vec![egui::Event::PointerMoved(start)]);
    d.frame(&mut ws, vec![press(start, true)]);
    let back = start + egui::vec2(0.0, 40.0);
    d.frame(&mut ws, vec![egui::Event::PointerMoved(back)]);
    assert!(ws.pane_on(0).dragging, "挪过拖动阈值之后没认成「在拖」");
    d.frame(&mut ws, vec![press(back, false)]);
    assert!(!ws.pane_on(0).dragging, "松手之后「在拖」没收掉");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        wired.count("files-copy"),
        0,
        "松在本栏却复制了：{:?}",
        wired.cmds()
    );
    // ── 正题：拖到右栏松手 ⇒ 复制过去（拖的是按下那一行）。
    let name_at = d.find(&mut ws, "x.txt");
    let start = name_at[0].center();
    d.frame(&mut ws, vec![egui::Event::PointerMoved(start)]);
    d.frame(&mut ws, vec![press(start, true)]);
    d.frame(
        &mut ws,
        vec![egui::Event::PointerMoved(start + egui::vec2(40.0, 0.0))],
    );
    let painted = d.frame(&mut ws, vec![egui::Event::PointerMoved(right_mid)]);
    assert!(
        painted.iter().any(|(t, _)| t == &copy_core::copy_text("rsFilewinWorkspace.drag.hint", &[("n", "1")])),
        "拖到另一栏上方时没说「复制 1 项到另一栏」：{:?}",
        painted.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
    );
    d.frame(&mut ws, vec![press(right_mid, false)]);
    settle_copy(ws.pane_on(1)).await;
    let copies: Vec<serde_json::Value> = wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-copy")
        .map(|r| r["args"].clone())
        .collect();
    assert_eq!(
        copies,
        vec![
            serde_json::json!({"root": "/srv", "from": "a/x.txt", "to": "b/x.txt", "overwrite": false})
        ],
        "拖过去那一趟线上那一行与期望不等"
    );
}

/// 〔有损名全寻址（）〕跨目录那一形按**字节**切：源在一个有损名目录里 ⇒ 根照样是按段比的公共前缀，
/// 有损那一段发 `{"b16": …}`、合法 UTF-8 那一段照旧是字符串（期望手写）。
#[test]
fn across_args_cut_lossy_paths_by_their_bytes() {
    let job = CopyJob {
        from: "/srv/a\u{FFFD}/x".into(),
        to: "/srv/b/x".into(),
        name: "x".into(),
        is_dir: false,
        from_raw: Some(b"/srv/a\xff/x".to_vec()),
        to_raw: None,
    };
    assert_eq!(
        across_args(&job, false).unwrap(),
        serde_json::json!({ "root": "/srv", "from": { "b16": "61ff2f78" }, "to": "b/x", "overwrite": false })
    );
}

/// 今天本机时区的 `h:m` 那一刻（UNIX 秒；截图里「采样 13:40」那个时刻）。
fn chrono_like_today_at(h: i64, m: i64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let off = crate::source::local_offset_at(now);
    let day = (now + off).div_euclid(86_400) * 86_400;
    (day + h * 3600 + m * 60 - off) as u64
}

/// 截图工具（`npm run shots`）那一格：不是判据。在私有 Xvfb 上开真窗口、画几帧、窗口内截屏存 PNG。
/// 一个进程只许建一个事件循环 ⇒ 每张图一个进程，场景走环境变量：
/// `CCM_SHOTS_FILEWIN_SCENE`（main · split · empty · missing · search）· `CCM_SHOTS_FILEWIN_OUT`（PNG）· `CCM_SHOTS_FILEWIN_DIR`（合成的家目录）·
/// `CCM_SHOTS_FILEWIN_PREVIEW`（预览里那份 main.rs 的正文）。
/// 目录与文件是截图工具事先铺好的占位；后端是合成后端（`find::testing::FakeBackend`），不连任何机器。
#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn screenshot_for_the_shots_tool() {
    let need = |k: &str| {
        std::env::var(k).unwrap_or_else(|_| panic!("这一格只给截图工具跑：缺环境变量 {k}"))
    };
    let scene = need("CCM_SHOTS_FILEWIN_SCENE");
    let out = need("CCM_SHOTS_FILEWIN_OUT");
    let base = std::path::PathBuf::from(need("CCM_SHOTS_FILEWIN_DIR"));
    // 合成目录由截图工具事先铺好（这一格只读盘，不写）。
    let dir = base.join("work").join("orders-service");
    // 预览那一栏的正文：合成后端按路径从内存里答（不读盘），工具把 main.rs 那一份一并给过来。
    let code = need("CCM_SHOTS_FILEWIN_PREVIEW");
    // 窗口里看到的是一台真机器那样的路径（`/home/user/…`）：合成后端把它换成盘上的沙箱路径再答（[`FakeBackend::aliased`]），
    //   面包屑里就没有沙箱那几级（场景名之类）。`dir` 是盘上那一份（读盘用），`vdir` / `d` 是窗口那一份。
    let vhome = std::path::PathBuf::from("/home/user");
    let vdir = vhome.join("work").join("orders-service");
    let d = vdir.to_string_lossy().to_string();
    let mut offered = vec![
        "files-ls",
        "files-read-text",
        "files-stat",
        "files-home",
        "files-rename",
        "files-mkdir",
    ];
    offered.extend_from_slice(crate::find::COMMANDS);
    // 「n 个目录无权限［查看］」那一张：后端报 3 个、交 2 个名单。
    let declared = if scene == "unreadable" {
        Declared {
            unreadable_dirs: 3,
            unreadable_paths: vec![
                dir.join("secrets").to_string_lossy().to_string(),
                dir.join("deploy/keys").to_string_lossy().to_string(),
            ],
            ..Declared::default()
        }
    } else {
        Declared::default()
    };
    let be = FakeBackend::new(&offered, declared)
        .homed(&base)
        .preindexed(&base)
        .aliased("/home/user", &base);
    be.disk
        .lock()
        .unwrap()
        .insert(dir.join("main.rs").to_string_lossy().to_string(), code);
    let wired = wire_up("devbox", be).await;
    let mut w = window_on(&wired, &d);
    w.machines = vec![
        "<local>".into(),
        "devbox".into(),
        "gpu-01".into(),
        "win-laptop".into(),
    ];
    let shelf = crate::bookmarks::Shelf::open(
        Some(base.join("bm.json")),
        &crate::source::Origin("devbox".into()),
    );
    shelf.toggle(&d);
    shelf.toggle(&vdir.join("src").to_string_lossy());
    shelf.toggle("/etc");
    w.shelf = Some(shelf);
    let target = match scene.as_str() {
        "empty" => vdir.join("empty-dir").to_string_lossy().to_string(),
        "missing" => vdir.join("gone").to_string_lossy().to_string(),
        _ => d.clone(),
    };
    // 窗口开在 d 上：先走开一步再回来，列表才真去问一趟（导航到当前目录是空操作）。
    w.navigate_to(vdir.join("src").to_string_lossy().to_string());
    while w.listing.is_loading() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    if target == d {
        w.go_back();
    } else {
        w.navigate_to(target.clone());
    }
    while w.listing.is_loading() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    // 合成后端的 files-ls 不送修改时间（真后端送）：照盘上补上。
    for r in w.listing.rows.lock().unwrap().iter_mut() {
        let real = r.path.replacen("/home/user", &base.to_string_lossy(), 1);
        r.mtime_secs = std::fs::metadata(&real)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|t| t.as_secs());
        // 真后端连写好的两格一起送（合成场景替它按这台的钟写）。
        let texts = r.mtime_secs.map(crate::source::mtime_text);
        r.mtime_text = texts.as_ref().map(|t| t.short.clone());
        r.mtime_full = texts.map(|t| t.full);
    }
    if target == d {
        let i = w
            .listing
            .rows
            .lock()
            .unwrap()
            .iter()
            .position(|r| r.name == "main.rs")
            .unwrap();
        w.apply_intent(
            crate::select::Intent::Edge {
                end: false,
                extend: false,
            },
            0.0,
            None,
        );
        for _ in 0..i {
            w.apply_intent(
                crate::select::Intent::Step {
                    by: 1,
                    extend: false,
                },
                0.0,
                None,
            );
        }
    }
    if scene == "search" || scene == "unreadable" {
        // 在工具条的搜索框里打字（合成事件，同 Ctrl+F 那个口），等那块板子落下一份答案。
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        let before = w.search.rounds();
        crate::find::testing::type_into_search(&ctx, &mut w, "retry");
        crate::find::testing::settle(&w.search, before, "截图的搜索那一张").await;
    }
    // 一趟下载在路上（进度那一行）。
    if scene == "pull" {
        w.pull.begin("release.tar.gz");
        w.pull.progress(1_200_000, 3_400_000);
        w.progress.add(
            crate::progress::Trip::Download {
                board: w.pull.clone(),
                name: "release.tar.gz".into(),
                src: vdir.join("release.tar.gz").to_string_lossy().to_string(),
                dest: "~/下载/release.tar.gz".into(),
            },
            None,
            None,
        );
    }
    // 就地新建文件夹那一格把名字清空了就回车：原因挂在那一格下面。
    if scene == "mkdir-error" {
        w.begin_mkdir();
        w.write_prompt_mut().unwrap().text.clear();
        w.confirm_write(None);
    }
    // 「进度」表（稿 11）：两趟在跑（上传一摞 · 下载）· 删除一个文件夹（那台撤不动 ⇒「停」灰着）· 上传失败 · 复制到另一台完成。
    if scene == "progress" {
        use crate::progress::Trip;
        let items: Vec<crate::transfer::Pending> =
            ["release-1.tar.gz", "release-2.tar.gz", "release-3.tar.gz"]
                .iter()
                .filter_map(|n| crate::transfer::Pending::into_remote_dir(&format!("/tmp/{n}"), &d))
                .collect();
        let up = crate::transfer::DropBoard::default();
        w.progress.add(
            Trip::Upload {
                board: up.clone(),
                items: items.clone(),
                dir: "orders-service".into(),
            },
            Some(d.clone()),
            None,
        );
        up.cancels().mint("release-2.tar.gz");
        up.progress("release-1.tar.gz", 20_000_000, 20_000_000);
        up.progress("release-2.tar.gz", 4_400_000, 20_000_000);
        let down = crate::download::DownloadBoard::default();
        w.progress.add(
            Trip::Download {
                board: down.clone(),
                name: "logs-0930.zip".into(),
                src: vdir.join("logs-0930.zip").to_string_lossy().to_string(),
                dest: "~/下载/logs-0930.zip".into(),
            },
            None,
            None,
        );
        down.begin("logs-0930.zip");
        down.progress(58_000_000, 480_000_000);
        let rm = crate::writeops::WriteBoard::default();
        w.progress.add(
            Trip::Delete {
                board: rm,
                base: 0,
                name: "node_modules".into(),
                n: 1,
            },
            Some(d.clone()),
            Some(copy_text(
                "rsFilewinProgress.stop.uncancellable",
                &[("machine", "devbox")],
            )),
        );
        let failed = crate::transfer::DropBoard::default();
        w.progress.add(
            Trip::Upload {
                board: failed.clone(),
                items: items.clone(),
                dir: "orders-service".into(),
            },
            Some(d.clone()),
            None,
        );
        failed.finish(crate::transfer::DropOutcome {
            ok: 1,
            failed: vec![
                ("release-2.tar.gz".into(), "磁盘满".into()),
                ("release-3.tar.gz".into(), "磁盘满".into()),
            ],
            ..Default::default()
        });
        let cross = crate::cross_copy::CrossBoard::default();
        w.progress.add(
            Trip::Cross {
                board: cross.clone(),
                name: "report.pdf".into(),
                machine: "gpu-01".into(),
            },
            None,
            None,
        );
        cross.finish(crate::cross_copy::Outcome::Done {
            name: "report.pdf".into(),
            machine: "gpu-01".into(),
            path: "/home/user/inbox/report.pdf".into(),
            bytes: 182_000,
        });
        w.progress.settle(chrono_like_today_at(13, 41));
    }
    // 断线 · 过期（稿 18）：那条流说「离线」；摆着的那一屏是 13:40 列到的。
    if scene == "stale" {
        w.link.set(crate::source::LinkSeen::Down);
        let at = chrono_like_today_at(13, 40);
        w.listing
            .landed
            .store(at, std::sync::atomic::Ordering::SeqCst);
    }
    let row_of = |w: &crate::shell::FileWindow, name: &str| {
        w.listing
            .rows
            .lock()
            .unwrap()
            .iter()
            .position(|r| r.name == name)
            .unwrap()
    };
    // 就地改名填错（稿 07）：main.rs 改成 Cargo.toml、回车 ⇒ 后端回「已存在」（合成后端照盘上真文件判）。
    if scene == "rename-error" {
        let i = row_of(&w, "main.rs");
        w.begin_rename(i);
        w.write_prompt_mut().unwrap().text = "Cargo.toml".into();
        w.confirm_write(None);
    }
    // 就地新建文件夹 ＋ 上一次改名的回执（稿 08）。
    if scene == "new-folder" {
        w.begin_mkdir();
        w.receipt_undo = Some((
            copy_core::copy_text("rsFilewinWriteops.inline.renamed", &[("name", "retry.rs")]),
            vec![crate::writeops::WriteOp::Rename {
                from: format!("{d}/retry.rs"),
                to: format!("{d}/main.rs"),
                raw: None,
            }],
        ));
    }
    // 改权限（稿 10）：main.rs 上「⋯ → 权限」；现值答回来之后（帧里）勾上三个执行位。
    if scene == "chmod" {
        let i = row_of(&w, "main.rs");
        w.begin_chmod(i);
    }
    // 上传遇到同名（稿 12）：本机三份、两份同名。
    if scene == "upload-clash" {
        let local = base.join("..").join("..").join("local");
        let items: Vec<crate::transfer::Pending> = ["main.rs", "README.md"]
            .iter()
            .filter_map(|n| {
                crate::transfer::Pending::into_remote_dir(&local.join(n).to_string_lossy(), &d)
            })
            .collect();
        for n in ["main.rs", "README.md"] {
            let md = std::fs::metadata(dir.join(n)).unwrap();
            w.board.note_there(
                n,
                Some(md.len()),
                // 「那台」那一格是后端写好的短写法（合成场景替它按这台的钟写）。
                md.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|t| crate::source::mtime_text(t.as_secs()).short),
            );
        }
        w.board.set_total(3);
        std::mem::forget(w.board.ask(items));
    }
    // 复制到另一台（稿 13）：report.pdf ⇒ 那一问；选择器去那台的主目录、选中 inbox（帧里）。
    if scene == "cross-copy" {
        let i = row_of(&w, "report.pdf");
        w.begin_cross(i);
    }
    // 删除那一问（稿 09：一个文件夹 ＋ 两个文件 ⇒ 「删除 3 项」）。
    if scene == "delete-ask" {
        let i = w
            .listing
            .rows
            .lock()
            .unwrap()
            .iter()
            .position(|r| r.name == "docs")
            .unwrap();
        w.apply_intent(
            crate::select::Intent::Edge {
                end: false,
                extend: false,
            },
            0.0,
            None,
        );
        for _ in 0..i {
            w.apply_intent(
                crate::select::Intent::Step {
                    by: 1,
                    extend: false,
                },
                0.0,
                None,
            );
        }
        // 再 Ctrl 点上 main.rs 与 notes.txt（选中态本体那一口，同键盘 / 点击走的那一个）。
        w.pick_also(&["main.rs", "notes.txt"]);
        w.perform(crate::select::Action::Delete, None);
    }
    let open_unreadable = scene == "unreadable";
    let shot_w: f32 = std::env::var("CCM_SHOTS_FILEWIN_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1280.0);
    let scene_name = scene.clone();
    let theme = crate::theme::testing::default_theme();
    let preview = scene == "main";
    let split = scene == "split";
    let h = std::thread::spawn(move || {
        struct Shot {
            ws: Workspace,
            n: u32,
            out: String,
            preview: bool,
            open_unreadable: bool,
            scene: String,
        }
        impl eframe::App for Shot {
            fn ui(&mut self, ui: &mut egui::Ui, _f: &mut eframe::Frame) {
                self.n += 1;
                if self.n == 5 && self.preview {
                    self.ws.set_preview(true);
                }
                // 编辑页那几张（稿 03 · 05 · 06 · 17）：目录页上开 main.rs ⇒ 同一栏开出编辑页；之后改一行（未保存）。
                if self.n == 3 && self.scene.starts_with("edit")
                    || self.n == 3 && self.scene == "close-window"
                {
                    let i = self
                        .ws
                        .pane_on(0)
                        .listing
                        .rows
                        .lock()
                        .unwrap()
                        .iter()
                        .position(|r| r.name == "main.rs");
                    if let Some(i) = i {
                        self.ws.pane_on_mut(0).begin_edit(i, Some(ui.ctx().clone()));
                    }
                }
                if self.n == 15 && self.ws.pane_on(0).edit_tab {
                    let pane = self.ws.pane_on_mut(0);
                    if let Some(t) = pane.editing_text_mut() {
                        t.insert_str(0, "// 重试那一段改成指数退避\n");
                    }
                    match self.scene.as_str() {
                        "edit-page" => {
                            pane.find_open = true;
                            if let Some(f) = pane.find_bar_mut() {
                                f.needle = "Retry".into();
                            }
                        }
                        "edit-stale" => {
                            if let Some(e) = pane.editing_mut() {
                                e.mark_stale("main.rs 在你打开之后被改过".into());
                            }
                        }
                        _ => {}
                    }
                }
                if self.n == 20 && self.scene == "edit-close" {
                    let a = self.ws.active_on(0);
                    self.ws.close_tab(0, a);
                }
                if self.n == 20 && self.scene == "close-window" {
                    let down = crate::download::DownloadBoard::default();
                    self.ws.progress.add(
                        crate::progress::Trip::Download {
                            board: down.clone(),
                            name: "logs-0930.zip".into(),
                            src: "/x/logs-0930.zip".into(),
                            dest: "~/下载/logs-0930.zip".into(),
                        },
                        None,
                        None,
                    );
                    down.begin("logs-0930.zip");
                    down.progress(12, 100);
                    self.ws.begin_close_ask();
                }
                // 改权限：现值（644）答回来、预填过之后勾上三个执行位 ⇒ 755。
                if self.n == 12 && self.scene == "chmod" {
                    let pane = self.ws.pane_on_mut(0);
                    if let Some(p) = pane.write_prompt_mut() {
                        for k in [2, 5, 8] {
                            p.toggle_bit(k);
                        }
                    }
                }
                // 复制到另一台：选择器读出那台主目录之后选中 inbox。
                // 第 4 批那几张：拖着两个文件经过 · 窄档抽屉开着 · 空格「看一眼」· 键盘走到搜索框。
                if self.scene == "drag-in" && self.n >= 10 {
                    ui.ctx().input_mut(|i| {
                        for n in ["release-4.tar.gz", "notes-2.txt"] {
                            i.raw.hovered_files.push(egui::HoveredFile {
                                path: Some(n.into()),
                                ..Default::default()
                            });
                        }
                    });
                }
                if self.n == 8 && self.scene == "narrow-drawer" {
                    self.ws.drawer_open = true;
                }
                if self.n == 8 && self.scene == "peek" {
                    self.ws.pane_on_mut(0).want_peek = true;
                }
                if self.n >= 8 && self.scene == "focus" {
                    self.ws.kb_nav = true;
                    ui.ctx().memory_mut(|m| {
                        m.request_focus(egui::Id::new(crate::shell::SEARCH_BOX_ID))
                    });
                }
                if self.n == 20 && self.scene == "cross-copy" {
                    self.ws.pane_on_mut(0).cross_pick.pick("inbox");
                }
                if self.n == 10 && self.open_unreadable {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("filewin-unreadable-list"), true)
                    });
                }
                if self.n == 20 && self.ws.sides() == 2 && !self.ws.pane_on(1).listing.is_loading()
                {
                    let rows = self.ws.pane_on(0).listing.rows.lock().unwrap().clone();
                    *self.ws.pane_on(1).listing.rows.lock().unwrap() = rows;
                }
                self.ws.frame(ui);
                let ctx = ui.ctx().clone();
                if self.n == 40 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                }
                let shot = ctx.input(|i| {
                    i.raw.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(img) = shot {
                    let [w, h] = img.size;
                    let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                    image::save_buffer(
                        &self.out,
                        &raw,
                        w as u32,
                        h as u32,
                        image::ExtendedColorType::Rgba8,
                    )
                    .unwrap();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ctx.request_repaint();
            }
        }
        let opts = eframe::NativeOptions {
            event_loop_builder: Some(Box::new(crate::platform::any_thread_hook)),
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([shot_w, 800.0])
                .with_title("filewin-shot"),
            ..Default::default()
        };
        eframe::run_native(
            "filewin-shot",
            opts,
            Box::new(move |cc| {
                crate::theme::install(&cc.egui_ctx, &theme);
                w.font = crate::fonts::FontState::Pending(crate::fonts::install(
                    &cc.egui_ctx,
                    Some(&theme),
                ));
                let mut ws = Workspace::new(w);
                ws.theme = Some(theme);
                ws.ask_home(Some(cc.egui_ctx.clone()));
                if split {
                    ws.set_split(true);
                }
                Ok(Box::new(Shot {
                    ws,
                    n: 0,
                    out,
                    preview,
                    open_unreadable,
                    scene: scene_name,
                }))
            }),
        )
        .unwrap();
    });
    tokio::task::spawn_blocking(move || h.join().unwrap())
        .await
        .unwrap();
}

/// 🔴 **双栏时编辑面只占它那一栏**：右栏开一份长文件 ⇒ 编辑面那一块落在右栏里、在窗口里，
/// 左栏的行照样画在它外面（没被盖住、没被挤走）。
#[test]
fn an_editor_on_the_right_side_stays_in_the_right_side() {
    let mut ws = two_sides(pane("/l", &["left-only.txt"]), pane("/r", &["right.txt"]));
    let long: String = (0..3000).map(|i| format!("line {i}\n")).collect();
    let right = ws.pane_on_mut(1);
    right.edits.deliver(crate::editor::Arrived::Text {
        path: "/r/right.txt".into(),
        name: "right.txt".into(),
        text: long,
        sha256: crate::find::testing::fake_sha256(""),
    });
    assert!(right.settle_opened_edits());
    let mut d = Drive::new();
    let mut painted = Vec::new();
    for _ in 0..4 {
        painted = d.frame(&mut ws, Vec::new());
    }
    // 编辑页头条上那颗「保存」只画在右栏里。
    let save = rects_of(&painted, &copy_text("rsFilewinShell.editor.save", &[]));
    assert_eq!(save.len(), 1, "编辑页没立起来");
    let area = save[0];
    let whole = egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN);
    assert!(whole.contains_rect(area), "编辑页 {area:?} 出了窗口");
    // 左栏那一行最右那一格（大小）：编辑面的左沿在它右边 ⇒ 没越过两栏的分界。
    let mut sizes = rects_of(&painted, &crate::rows::human_size(10));
    sizes.sort_by(|a, b| a.left().total_cmp(&b.left()));
    assert_eq!(
        sizes.len(),
        1,
        "左栏那一行的大小格该画一处（右栏是编辑页，不画列表）"
    );
    assert_eq!(
        rects_of(&painted, "left-only.txt").len(),
        1,
        "左栏那一行没画"
    );
    assert!(
        area.left() > sizes[0].right(),
        "右栏的编辑面 {area:?} 越过了左栏（左栏那一行的大小格在 {:?}）",
        sizes[0]
    );
}

/// 跑一帧：这一帧的输入里带不带「点了关窗」；回这一帧窗口收到的命令。
fn close_frame(d: &mut Drive, ws: &mut Workspace, close: bool) -> Vec<egui::ViewportCommand> {
    d.t += 1.0;
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
        time: Some(d.t),
        ..Default::default()
    };
    if close {
        input.viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                events: vec![egui::ViewportEvent::Close],
                ..Default::default()
            },
        );
    }
    let out = d.ctx.run_ui(input, |ui| {
        ws.guard_close(ui.ctx());
        ws.frame(ui);
    });
    let cmds = out
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map(|v| v.commands.clone())
        .unwrap_or_default();
    out.drop_without_applying_deltas();
    cmds
}

fn open_text(p: &mut FileWindow, text: &str) {
    p.edits.deliver(crate::editor::Arrived::Text {
        path: format!("{}/t.txt", p.cwd),
        name: "t.txt".into(),
        text: text.into(),
        sha256: crate::find::testing::fake_sha256(""),
    });
    assert!(p.settle_opened_edits());
}

/// 🔴 稿 17：点 × 关窗 —— 什么都不会丢 ⇒ 直接关；开着一份没改过的文本照样直接关；
/// 有没保存的编辑页 / 还在跑的那几趟 ⇒ 拦下、摆「关闭文件窗口 · devbox」按族列出（未保存 · 传输中），
/// 按钮「取消」·「全部保存后关闭」（只在有没保存的时出现）·「仍然关闭」；「取消」⇒ 作罢；那一问摆着时再点一次 × ⇒ 照关；「仍然关闭」⇒ 关。
#[test]
fn closing_the_window_asks_once_listing_unsaved_pages_and_running_trips() {
    let cancel = egui::ViewportCommand::CancelClose;
    let mut d = Drive::new();
    let mut ws = two_sides(pane("/l", &["a"]), pane("/r", &["b"]));
    assert!(
        !close_frame(&mut d, &mut ws, true).contains(&cancel),
        "什么都没有也拦了"
    );
    open_text(ws.pane_on_mut(1), "x\n");
    assert!(
        !close_frame(&mut d, &mut ws, true).contains(&cancel),
        "没改过的文本也拦了关窗"
    );
    // 一页改了没存 ＋ 一趟下载在跑。
    *ws.pane_on_mut(1).editing_text_mut().unwrap() = "x2\n".into();
    let down = crate::download::DownloadBoard::default();
    ws.progress.add(
        crate::progress::Trip::Download {
            board: down.clone(),
            name: "logs.zip".into(),
            src: "/r/logs.zip".into(),
            dest: "/tmp/logs.zip".into(),
        },
        None,
        None,
    );
    down.begin("logs.zip");
    down.progress(12, 100);
    assert!(
        close_frame(&mut d, &mut ws, true).contains(&cancel),
        "有东西会丢，关窗却没拦"
    );
    assert!(ws.closing_ask(), "拦下了却没问");
    let painted = d.frame(&mut ws, Vec::new());
    let has = |t: &str| painted.iter().any(|(p, _)| p == t);
    for t in [
        copy_text("rsFilewinWorkspace.closeAsk.title", &[("machine", "ws")]),
        copy_text("rsFilewinWorkspace.closeAsk.unsaved", &[]),
        "t.txt".to_string(),
        copy_text("rsFilewinWorkspace.closeAsk.transfers", &[]),
        "logs.zip".to_string(),
        copy_text("rsFilewinWorkspace.closeAsk.down", &[("pct", "12")]),
        copy_text("rsFilewinWorkspace.closeAsk.cancel", &[]),
        copy_text("rsFilewinWorkspace.closeAsk.saveAll", &[]),
        copy_text("rsFilewinWorkspace.closeAsk.force", &[]),
    ] {
        assert!(
            has(&t),
            "关窗那一问上没有「{t}」：{:?}",
            painted.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }
    // 那一问摆着时再点一次 × ⇒ 照关（不再拦）。
    assert!(
        !close_frame(&mut d, &mut ws, true).contains(&cancel),
        "那一问摆着时再点 × 还是拦着"
    );
    // 只剩一趟在跑（没有没保存的）⇒ 那一问上没有「全部保存后关闭」。
    let mut ws = two_sides(pane("/l", &["a"]), pane("/r", &["b"]));
    let up = crate::transfer::DropBoard::default();
    ws.progress.add(
        crate::progress::Trip::Upload {
            board: up,
            items: Vec::new(),
            dir: "l".into(),
        },
        Some("/l".into()),
        None,
    );
    let mut d = Drive::new();
    assert!(close_frame(&mut d, &mut ws, true).contains(&cancel));
    let painted = d.frame(&mut ws, Vec::new());
    let save_all = copy_text("rsFilewinWorkspace.closeAsk.saveAll", &[]);
    assert!(
        !painted.iter().any(|(p, _)| *p == save_all),
        "没有没保存的也给了「全部保存后关闭」"
    );
    // 「仍然关闭」⇒ 关。
    let force = rects_of(
        &painted,
        &copy_text("rsFilewinWorkspace.closeAsk.force", &[]),
    );
    assert_eq!(force.len(), 1);
    d.frame(&mut ws, vec![egui::Event::PointerMoved(force[0].center())]);
    d.t += 1.0;
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
        time: Some(d.t),
        events: crate::rows::testing::click_at(force[0].center()),
        ..Default::default()
    };
    input
        .viewports
        .insert(egui::ViewportId::ROOT, Default::default());
    let out = d.ctx.run_ui(input, |ui| ws.frame(ui));
    let cmds = out
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map(|v| v.commands.clone())
        .unwrap_or_default();
    out.drop_without_applying_deltas();
    assert!(
        cmds.contains(&egui::ViewportCommand::Close),
        "点了「仍然关闭」没关"
    );
}

/// 跑几帧 `Workspace::frame`（产品那一套样子），屏幕 `screen`；回最后一帧画出来的字。
fn frames_at(ws: &mut Workspace, screen: egui::Vec2, n: usize) -> Vec<PaintedText> {
    let ctx = egui::Context::default();
    crate::theme::install(&ctx, &crate::theme::testing::default_theme());
    let mut painted = Vec::new();
    for k in 0..n {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            time: Some(k as f64),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| ws.frame(ui));
        painted = text_in_frame(&out);
        out.drop_without_applying_deltas();
    }
    painted
}

/// 🔴 **窗口窄了不截断**：最小窗口（单栏）与 640×480 双栏、外加一句长提示，
/// 这一帧画出来的每一段字都在窗口里（放不下的截成「…」、收进「⋯」，不是画到窗外被裁掉）；
/// 搜索框、「⋯」、当前目录、列表头那几样都还在。
#[test]
fn a_narrow_window_cuts_nothing_off() {
    let [w0, h0] = crate::shell::MIN_WINDOW;
    for (screen, split) in [
        (egui::vec2(w0, h0), false),
        (egui::vec2(640.0, 480.0), true),
    ] {
        let dir = "/home/user/work/a-rather-long-project-directory-name";
        let mut ws = Workspace::new(pane(dir, &["Cargo.toml", "README.md"]));
        if split {
            assert!(ws.add_side(pane(dir, &["Cargo.toml", "README.md"])));
        }
        ws.set_notice(
            copy_core::copy_text(
                "rsFilewinWorkspace.closeTab.busy",
                &[(
                    "why",
                    &*copy_core::copy_text(
                        "rsFilewinShell.busy.io",
                        &[("path", "/home/user/work/一个很长很长的路径/文件.txt")],
                    ),
                )],
            )
            .into(),
        );
        let painted = frames_at(&mut ws, screen, 4);
        let whole = egui::Rect::from_min_size(egui::Pos2::ZERO, screen).expand(0.5);
        let out: Vec<&PaintedText> = painted
            .iter()
            .filter(|(t, r)| !t.trim().is_empty() && !whole.contains_rect(*r))
            .collect();
        assert!(out.is_empty(), "{screen:?}：这几段字画到了窗口外：{out:?}");
        let has = |pred: &dyn Fn(&str) -> bool| painted.iter().any(|(t, _)| pred(t));
        assert!(
            has(&|t| t.contains(copy_core::copy_static!("rsFilewinShell.search.hint"))),
            "{screen:?}：搜索框不见了"
        );
        assert!(
            has(&|t| t == egui_phosphor::regular::DOTS_THREE),
            "{screen:?}：没有「⋯」"
        );
        assert!(
            has(&|t| t.starts_with("a-rather")),
            "{screen:?}：地址栏里看不到当前目录"
        );
        assert!(
            has(&|t| t.starts_with("Cargo")),
            "{screen:?}：列表那一行不见了"
        );
    }
}

/// 这一帧画出来的每一块实心矩形（递归进 `Shape::Vec`），按画的先后。
fn rect_shapes(out: &egui::FullOutput) -> Vec<egui::epaint::RectShape> {
    fn walk(s: &egui::Shape, acc: &mut Vec<egui::epaint::RectShape>) {
        match s {
            egui::Shape::Rect(r) => acc.push(r.clone()),
            egui::Shape::Vec(v) => v.iter().for_each(|x| walk(x, acc)),
            _ => {}
        }
    }
    let mut acc = Vec::new();
    for cs in &out.shapes {
        walk(&cs.shape, &mut acc);
    }
    acc
}

/// 🔴 **窄档左栏抽屉照规范 `C11` 画**（720 宽开着抽屉）：浮层底（`--card`）· 外侧两角圆、贴边两角方 ·
/// 对话框那一档投影（`--shadow-modal`）· 底下的内容区盖一层 `--overlay-dim`，比抽屉宽（盖到抽屉以外），且先于抽屉画（压在下面）。
#[test]
fn the_narrow_drawer_floats_with_a_shadow_over_a_dimmed_list() {
    let t = crate::theme::testing::default_theme();
    let p = crate::theme::Palette::of_theme(&t);
    let mut ws = Workspace::new(pane("/home/user/work", &["Cargo.toml", "README.md"]));
    ws.drawer_open = true;
    let ctx = egui::Context::default();
    crate::theme::install(&ctx, &t);
    let screen = egui::vec2(720.0, 800.0);
    let mut rects = Vec::new();
    for k in 0..4 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            time: Some(k as f64),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| ws.frame(ui));
        rects = rect_shapes(&out);
        out.drop_without_applying_deltas();
    }
    assert!(ws.drawer_open, "抽屉自己收了");
    let card = p.card.to_opaque();
    let drawer = rects
        .iter()
        .position(|r| r.fill == card && r.corner_radius.nw == 0 && r.corner_radius.ne > 0)
        .expect("没有一块 --card 底、外侧圆角的抽屉");
    let d = &rects[drawer];
    let xl = t.radius_xl.round() as u8;
    assert_eq!(
        (
            d.corner_radius.nw,
            d.corner_radius.sw,
            d.corner_radius.ne,
            d.corner_radius.se
        ),
        (0, 0, xl, xl),
        "抽屉的角：贴边两角方、外侧两角是对话框那一档"
    );
    let blur = t.shadow_modal.blur.round();
    assert!(
        rects
            .iter()
            .any(|r| r.blur_width > 0.0 && (r.blur_width - blur).abs() < 1.0),
        "抽屉没有对话框那一档投影（blur {blur}）"
    );
    let dim = rects
        .iter()
        .position(|r| r.fill == p.dim && r.rect.width() > d.rect.width() + 100.0)
        .expect("抽屉底下的内容区没盖那层淡暗");
    assert!(dim < drawer, "淡暗画在抽屉之后 ⇒ 盖到抽屉上了");
}

/// 当前目录的名字不是合法 UTF-8：新标签页 / 双栏照样落在那个目录（带着原始字节），不报「目录不存在」。
#[test]
fn a_new_tab_or_side_keeps_a_non_utf8_directory_by_its_bytes() {
    let raw = b"/srv/caf\xe9".to_vec();
    let mut first = pane("/srv/caf\u{FFFD}", &["a.txt"]);
    first.cwd_raw = Some(raw.clone());
    let mut ws = Workspace::new(first);
    assert!(ws.open_tab(0));
    assert_eq!(
        ws.pane_on(0).cwd_raw.as_deref(),
        Some(raw.as_slice()),
        "新标签页丢了目录的字节"
    );
    assert!(ws.set_split(true));
    assert_eq!(
        ws.pane_on(1).cwd_raw.as_deref(),
        Some(raw.as_slice()),
        "右栏丢了目录的字节"
    );
}

/// 稿 ⑤：目录页上打开一份文本 ⇒ **在同一栏开一个编辑页**（标签上是那个文件名，目录页还是目录页）；
/// 同一份再开一次 ⇒ 切过去、不开第二份；另一份 ⇒ 另一页；另一栏照样能开自己的编辑页（左边改、右边翻目录）。
#[test]
fn opening_a_text_opens_an_edit_tab_beside_the_folder_and_again_switches_to_it() {
    let mut ws = two_sides(
        pane("/srv/a", &["a.txt", "b.txt"]),
        pane("/srv/b", &["c.txt"]),
    );
    let mut d = Drive::new();
    assert!(ws.pane_on_mut(0).begin_edit(0, None));
    d.frame(&mut ws, Vec::new());
    assert_eq!(ws.tabs_on(0), 2, "没在左栏开出编辑页");
    assert_eq!(ws.active_on(0), 1, "开完没切过去");
    assert!(ws.tab(0, 1).edit_tab && !ws.tab(0, 0).edit_tab);
    assert_eq!(ws.tab(0, 0).cwd, "/srv/a", "目录页被换了");
    assert_eq!(Workspace::tab_title(ws.tab(0, 1)), "a.txt");
    assert_eq!(ws.tabs_on(1), 1, "开到右栏去了");
    // 同一份再开一次 ⇒ 切过去。
    assert!(ws.select_tab(0, 0));
    assert!(ws.pane_on_mut(0).begin_edit(0, None));
    d.frame(&mut ws, Vec::new());
    assert_eq!(ws.tabs_on(0), 2, "同一份开了第二页");
    assert_eq!(ws.active_on(0), 1, "同一份没切过去");
    // 另一份 ⇒ 另一页。
    assert!(ws.select_tab(0, 0));
    assert!(ws.pane_on_mut(0).begin_edit(1, None));
    d.frame(&mut ws, Vec::new());
    assert_eq!(ws.tabs_on(0), 3);
    assert_eq!(Workspace::tab_title(ws.tab(0, 2)), "b.txt");
    // 右栏也开一份（左边那几页不碍事）。
    ws.focus_side(1);
    assert!(ws.pane_on_mut(1).begin_edit(0, None));
    d.frame(&mut ws, Vec::new());
    assert_eq!(ws.tabs_on(1), 2);
    assert!(ws.pane_on(1).edit_tab);
    // 没改过的编辑页点 × 直接关；改了没存的先问，答「不保存」才关。
    open_text(ws.pane_on_mut(1), "x\n");
    assert!(ws.close_tab(1, 1), "没改过的编辑页关不掉");
    assert_eq!(ws.tabs_on(1), 1);
    ws.select_tab(0, 2);
    open_text(ws.pane_on_mut(0), "y\n");
    *ws.pane_on_mut(0).editing_text_mut().unwrap() = "y2\n".into();
    assert!(!ws.close_tab(0, 2), "改了没存的编辑页直接关了");
    assert!(ws.tab(0, 2).asking_discard(), "没问「关闭 x · 未保存」");
    ws.pane_on_mut(0).discard_edit();
    ws.pane_on_mut(0).want_close = true;
    d.frame(&mut ws, Vec::new());
    assert_eq!(ws.tabs_on(0), 2, "答了「不保存」那一页没关");
}

/// 「进度」表照行数长高：四趟在跑 ＋ 一趟上传失败（两个没成，自动摊开）⇒ 失败那一行与它底下每一句原因都整个画在表里，
/// 不被状态栏压住、不在表的可见区外面。改值验：失败那一行不摊开（原因挤成一行截断）⇒ 本条红。
/// ⚠ 无头帧里面板高度钉回旧的 214 本条照样绿（这一形只在真窗口的截图里看得出，`filewin-progress` 那一张）。
#[test]
fn the_progress_table_grows_to_show_an_expanded_failure_whole() {
    use crate::progress::Trip;
    let dir = "/srv/data";
    let mut ws = Workspace::new(pane(dir, &["a.bin"]));
    for k in 0..4 {
        let down = crate::download::DownloadBoard::default();
        ws.progress.add(
            Trip::Download {
                board: down.clone(),
                name: format!("big-{k}.iso"),
                src: format!("{dir}/big-{k}.iso"),
                dest: format!("/tmp/big-{k}.iso"),
            },
            None,
            None,
        );
        down.begin(&format!("big-{k}.iso"));
        down.progress(1, 10);
    }
    let items: Vec<crate::transfer::Pending> = ["r-2.tar.gz", "r-3.tar.gz"]
        .iter()
        .filter_map(|n| crate::transfer::Pending::into_remote_dir(&format!("/tmp/{n}"), dir))
        .collect();
    let failed = crate::transfer::DropBoard::default();
    ws.progress.add(
        Trip::Upload {
            board: failed.clone(),
            items,
            dir: "data".into(),
        },
        Some(dir.into()),
        None,
    );
    failed.finish(crate::transfer::DropOutcome {
        failed: vec![
            ("r-2.tar.gz".into(), "磁盘满".into()),
            ("r-3.tar.gz".into(), "没有权限".into()),
        ],
        ..Default::default()
    });
    let screen = egui::vec2(1280.0, 800.0);
    let painted = frames_at(&mut ws, screen, 6);
    assert!(ws.progress.is_open(), "有失败却没自动摊开「进度」表");
    // 前提：这几行摆下来比旧的缺省表高（214）高 —— 不然本条量不出「表长高了」。
    let want = crate::progress::table_height(&ws.progress, false);
    assert!(want > 214.0, "前提不成立：表只要 {want} 高");
    // 表头「进度」那一行之下才是表的可见区：每一趟（含最上面那几趟在跑的）都整个在它下面 —— 表长高了，不是滚走了。
    let head = painted
        .iter()
        .find(|(t, _)| *t == copy_text("rsFilewinProgress.head.title", &[]))
        .map(|(_, r)| *r)
        .expect("表头没画出来");
    let rows: Vec<&egui::Rect> = painted
        .iter()
        .filter(|(t, _)| t.contains("big-") || t.contains("r-2") || t.contains("r-3"))
        .map(|(_, r)| r)
        .collect();
    assert!(rows.len() >= 6, "表里的行没画全：{painted:?}");
    for r in &rows {
        assert!(
            r.top() >= head.bottom(),
            "有一行滚到了表头上面（表没长高）：{r:?} · 表头 {head:?}"
        );
    }
    // 状态栏那一条住窗口最底下 28 高：失败那几句的底边都要在它上面。
    let floor = screen.y - 28.0;
    for want in ["r-2.tar.gz：磁盘满", "r-3.tar.gz：没有权限"] {
        let at: Vec<&egui::Rect> = painted
            .iter()
            .filter(|(t, _)| t == want)
            .map(|(_, r)| r)
            .collect();
        assert_eq!(at.len(), 1, "「{want}」没画成单独一行：{painted:?}");
        assert!(
            at[0].bottom() <= floor && at[0].top() >= 0.0,
            "「{want}」被状态栏压住 / 在表外：{:?}（状态栏顶 {floor}）",
            at[0]
        );
    }
}

/// 断线时哪几件做不了 —— 唯一一处（`offline_cmd`）：要碰那台的一律「离线 · 只读」（含「终端」），只改看法的照常；连着时全都照常。
#[test]
fn offline_greys_exactly_what_needs_the_machine() {
    use crate::chrome::Cmd;
    use crate::select::Action as A;
    let mut ws = Workspace::new(pane("/srv/data", &["a.bin"]));
    let all = [
        (Cmd::Mkdir, true),
        (Cmd::NewFile, true),
        (Cmd::Upload, true),
        (Cmd::Term, true),
        (Cmd::Across, true),
        (Cmd::Act(A::Download), true),
        (Cmd::Act(A::Edit), true),
        (Cmd::Act(A::Delete), true),
        (Cmd::Act(A::Properties), true),
        (Cmd::Act(A::Open), false),
        (Cmd::Sidebar, false),
        (Cmd::Hidden, false),
        (Cmd::Split, false),
        (Cmd::Preview, false),
    ];
    for (c, _) in all {
        assert_eq!(ws.pane_on(0).offline_cmd(c), None, "连着时 {c:?} 却灰了");
    }
    ws.link.set(crate::source::LinkSeen::Down);
    let pane = ws.pane_on(0);
    for (c, greyed) in all {
        assert_eq!(
            pane.offline_cmd(c).is_some(),
            greyed,
            "断线时 {c:?} 该{}灰",
            if greyed { "" } else { "不" }
        );
    }
}

/// 标签页那一组键（甲5 键盘表）：Ctrl+Tab / Ctrl+Shift+Tab 绕着走 · Ctrl+n 第 n 个 · F6 换栏 · Ctrl+B 左栏 · Ctrl+H 隐藏文件 · Alt+P 预览。
#[test]
fn the_tab_and_view_keys_do_what_the_key_table_says() {
    use egui::{Key, Modifiers as M};
    let k = |key, m| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: m,
    };
    assert_eq!(
        tab_keys(&[
            k(Key::Tab, M::COMMAND),
            k(Key::Tab, M::COMMAND | M::SHIFT),
            k(Key::Num3, M::COMMAND),
            k(Key::F6, M::NONE),
            k(Key::B, M::COMMAND),
            k(Key::H, M::COMMAND),
            k(Key::P, M::ALT),
        ]),
        vec![
            TabKey::Next,
            TabKey::Prev,
            TabKey::Nth(2),
            TabKey::OtherSide,
            TabKey::Sidebar,
            TabKey::Hidden,
            TabKey::Preview,
        ]
    );
    let mut ws = Workspace::new(pane("/l", &["a"]));
    assert!(ws.add_side(pane("/r", &["b"])));
    ws.open_tab(0);
    ws.focus_side(0);
    let ctx = egui::Context::default();
    let run = |ws: &mut Workspace, ev: egui::Event| {
        let input = egui::RawInput {
            events: vec![ev],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            ws.apply_tab_keys(ui.ctx());
        })
        .drop_without_applying_deltas();
    };
    run(&mut ws, k(Key::Tab, M::COMMAND));
    assert_eq!(ws.active_on(0), 0, "最后一个之后绕回第一个");
    run(&mut ws, k(Key::Num2, M::COMMAND));
    assert_eq!(ws.active_on(0), 1);
    run(&mut ws, k(Key::F6, M::NONE));
    assert_eq!(ws.focus, 1, "F6 没换到另一栏");
    run(&mut ws, k(Key::P, M::ALT));
    assert!(ws.preview.is_some(), "Alt+P 没开预览");
}

/// 空格（列表上）⇒ 窗口那一级开「看一眼」浮层；再按收起。预览栏开着且是宽档 ⇒ 不做事。
#[test]
fn space_toggles_the_peek_overlay_unless_the_preview_is_open() {
    let mut ws = Workspace::new(pane("/srv/data", &["a.txt"]));
    let screen = egui::vec2(1280.0, 800.0);
    ws.pane_on_mut(0).want_peek = true;
    let _ = frames_at(&mut ws, screen, 1);
    assert!(ws.peek.is_some(), "空格没开「看一眼」");
    ws.pane_on_mut(0).want_peek = true;
    let _ = frames_at(&mut ws, screen, 1);
    assert!(ws.peek.is_none(), "再按空格没收起");
    ws.set_preview(true);
    ws.pane_on_mut(0).want_peek = true;
    let _ = frames_at(&mut ws, screen, 1);
    assert!(ws.peek.is_none(), "预览栏开着（宽档）空格却开了浮层");
}

/// 🔴 复现 4.1.3 的崩溃：两栏、焦点在右栏，点命令栏「两栏」收掉右栏 ⇒ 同一帧往后画状态栏时
/// 还拿着帧初记下的「焦点 ＝ 1」去取右栏 ⇒ `sides[1]` 越界、整个窗口进程退出（0xc0000409）。
#[test]
fn unsplitting_from_the_focused_right_side_does_not_crash() {
    let mut ws = two_sides(pane("/l", &["a"]), pane("/r", &["b"]));
    let mut d = Drive::new();
    assert_eq!(ws.focus(), 1, "前提：焦点在右栏");
    let at = d.find(&mut ws, SPLIT_LABEL.as_str());
    assert_eq!(at.len(), 1, "命令栏上的「两栏」该恰好一颗");
    d.click(&mut ws, at[0].center(), egui::PointerButton::Primary);
    assert_eq!((ws.sides(), ws.focus()), (1, 0), "点了没收掉右栏");
    // 收完再画几帧也稳；兜底没被用上（真修在调用方，不是靠兜底吞掉）。
    d.frame(&mut ws, Vec::new());
    d.frame(&mut ws, Vec::new());
    assert_eq!(ws.slips(), 0, "有一帧拿着过时的栏号取了目录视图");
}

/// 工作区的不变式：一或两栏；焦点落在某一栏；每栏至少一个标签、`active` 在表里；
/// 恰好一个标签拿着焦点、就是焦点那一栏当前那个。
fn assert_shape(ws: &Workspace, step: usize, op: &str) {
    let n = ws.sides.len();
    assert!((1..=2).contains(&n), "第 {step} 步（{op}）后栏数 {n}");
    assert!(
        ws.focus < n,
        "第 {step} 步（{op}）后焦点 {} 不在 {n} 栏里",
        ws.focus
    );
    for (k, s) in ws.sides.iter().enumerate() {
        assert!(!s.tabs.is_empty(), "第 {step} 步（{op}）后第 {k} 栏空了");
        assert!(
            s.active < s.tabs.len(),
            "第 {step} 步（{op}）后第 {k} 栏 active {} ≥ 标签数 {}",
            s.active,
            s.tabs.len()
        );
    }
    let focused: Vec<(usize, usize)> = ws
        .sides
        .iter()
        .enumerate()
        .flat_map(|(k, s)| {
            s.tabs
                .iter()
                .enumerate()
                .filter(|(_, t)| t.pane.focused)
                .map(move |(i, _)| (k, i))
        })
        .collect();
    assert_eq!(
        focused,
        vec![(ws.focus, ws.sides[ws.focus].active)],
        "第 {step} 步（{op}）后拿焦点的标签不对"
    );
    assert_eq!(
        ws.slips(),
        0,
        "第 {step} 步（{op}）里有人拿着过时的栏号 / 标签号取目录视图（退回兜底了，但那是缺陷）"
    );
}

/// 🔴 随机操作序列：开 · 关 · 切 · 换栏 · 开收两栏（走接口，也走真点命令栏那颗）· 快捷键 · 标签自己要关，
/// 几千步，每步之后核 [`assert_shape`]，并且每一步都真跑一帧生产的 `Workspace::frame`（崩在帧里也算红）。
/// 种子固定（失败可重放）；几颗种子各跑一趟。
#[test]
fn random_tab_and_split_sequences_keep_the_shape() {
    const OPS: [&str; 11] = [
        "开标签",
        "关标签",
        "切标签",
        "换栏",
        "开收两栏",
        "点「两栏」",
        "Ctrl+T",
        "Ctrl+W",
        "Ctrl+Tab",
        "F6",
        "标签要关自己",
    ];
    for seed in [0x9E37_79B9_7F4A_7C15u64, 7, 2024, 31337] {
        let mut x = seed;
        let mut rnd = move |m: usize| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % m as u64) as usize
        };
        let mut ws = Workspace::new(pane("/srv", &["a", "b"]));
        let mut d = Drive::new();
        let mut split_at: Option<egui::Pos2> = None;
        for step in 0..600 {
            let op = rnd(OPS.len());
            let side = rnd(2);
            let i = rnd(4);
            let mut events = Vec::new();
            match op {
                0 => {
                    ws.open_tab(side);
                }
                1 => {
                    ws.close_tab(side, i);
                }
                2 => {
                    ws.select_tab(side, i);
                }
                3 => ws.focus_side(side),
                4 => {
                    let two = ws.sides() == 2;
                    ws.set_split(!two);
                }
                5 => {
                    let at = match split_at {
                        Some(p) => p,
                        None => {
                            let r = d.find(&mut ws, SPLIT_LABEL.as_str());
                            assert_eq!(r.len(), 1, "命令栏上的「两栏」该恰好一颗");
                            split_at = Some(r[0].center());
                            r[0].center()
                        }
                    };
                    d.click(&mut ws, at, egui::PointerButton::Primary);
                }
                6 => events.push(ctrl(egui::Key::T)),
                7 => events.push(ctrl(egui::Key::W)),
                8 => events.push(egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: if rnd(2) == 0 {
                        egui::Modifiers::COMMAND
                    } else {
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
                    },
                }),
                9 => events.push(egui::Event::Key {
                    key: egui::Key::F6,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }),
                _ => {
                    let k = side.min(ws.sides.len() - 1);
                    let t = i.min(ws.sides[k].tabs.len() - 1);
                    ws.sides[k].tabs[t].pane.want_close = true;
                }
            }
            assert_shape(&ws, step, OPS[op]);
            d.frame(&mut ws, events);
            assert_shape(&ws, step, OPS[op]);
        }
    }
}

/// 兜底那一层：拿着过时的栏号 / 被弄坏的 `active` 取目录视图 ⇒ 退回最近那一个、记一笔，不 panic。
#[test]
fn a_stale_side_or_tab_index_falls_back_instead_of_panicking() {
    let mut ws = Workspace::new(pane("/srv", &["a"]));
    assert_eq!(ws.pane_on(1).cwd, "/srv", "过时的栏号该退回最后一栏");
    assert_eq!(ws.slips(), 1);
    ws.sides[0].active = 3;
    assert_eq!(
        ws.pane_on_mut(0).cwd,
        "/srv",
        "越界的 active 该退回最后一个标签"
    );
    assert_eq!(ws.slips(), 2);
    // 这一形下整帧也画得完。
    Drive::new().frame(&mut ws, Vec::new());
}

/// 🔴 锁中毒不崩：本包产品代码里取 `std::sync::Mutex` 一律经 `crate::Held::held`，不许 `lock().unwrap()` / `lock().expect(..)` 回潮
/// （后台任务拿着锁 panic 一次，往后每帧都在 unwrap 上 panic、整扇窗退出）。正控：`held` 在真中毒的锁上照样拿得到里面那份。
#[test]
fn lock_guard_window_takes_locks_via_held() {
    use crate::Held;
    let m = std::sync::Arc::new(std::sync::Mutex::new(7));
    let m2 = m.clone();
    let _ = std::thread::spawn(move || {
        let _g = m2.lock();
        panic!("poison on purpose");
    })
    .join();
    assert!(m.is_poisoned(), "正控：锁该已中毒");
    assert_eq!(*m.held(), 7, "中毒的锁 held 没拿到里面那份");

    let src = crate::guard_support::crate_src_root();
    let mut hits = Vec::new();
    let mut files = 0;
    for (p, text) in guard_core::scan_tree!(&src, &["rs"]) {
        files += 1;
        let flat: String = text.split_whitespace().collect();
        for pat in [".lock().unwrap()", ".lock().expect("] {
            let n = flat.matches(pat).count();
            if n > 0 {
                hits.push(format!("{} × {n} {pat}", p.display()));
            }
        }
    }
    assert!(files > 20, "量具：只扫到 {files} 个源文件，住址不对");
    assert!(
        hits.is_empty(),
        "又有 lock().unwrap() 回潮：\n{}",
        hits.join("\n")
    );
}
