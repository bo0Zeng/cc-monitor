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
    let plus = d.find(&mut ws, NEW_TAB_LABEL.as_str());
    assert_eq!(plus.len(), 1, "一栏时「＋」该恰好一颗");
    d.click(&mut ws, plus[0].center(), egui::PointerButton::Primary);
    assert_eq!(ws.tabs_on(0), 2, "点了「＋」没开出标签页");
    let close = d.find(&mut ws, CLOSE_TAB_LABEL.as_str());
    assert_eq!(close.len(), 2, "两个标签各有一颗「×」");
    d.click(&mut ws, close[1].center(), egui::PointerButton::Primary);
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
    let painted = d.frame(&mut ws, Vec::new());
    assert!(
        painted.iter().any(|(t, _)| *t == said),
        "那句话没画出来：{said}"
    );
    assert!(
        painted
            .iter()
            .any(|(t, _)| t.starts_with(BUSY_MARK.as_str())),
        "后台那个有事的标签，名字前没有「●」"
    );
    // 收掉那一问 ⇒ 关得掉（阴性对照）。
    assert!(ws.select_tab(0, 0));
    ws.pane_on_mut(0).cancel_new_file();
    assert!(ws.close_tab(0, 0));
    assert_eq!(ws.tabs_on(0), 1);
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
        said.contains("复制完成：2 项，4 个文件、2 个目录、84 字节"),
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
        painted.iter().any(|(t, _)| t == "复制 1 项到另一栏"),
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
    let d = dir.to_string_lossy().to_string();
    let mut offered = vec!["files-ls", "files-read-text", "files-stat", "files-home"];
    offered.extend_from_slice(crate::find::COMMANDS);
    let be = FakeBackend::new(&offered, Declared::default())
        .homed(&base)
        .preindexed(&base);
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
    shelf.toggle(&dir.join("src").to_string_lossy());
    shelf.toggle("/etc");
    w.shelf = Some(shelf);
    let target = match scene.as_str() {
        "empty" => dir.join("empty-dir").to_string_lossy().to_string(),
        "missing" => dir.join("gone").to_string_lossy().to_string(),
        _ => d.clone(),
    };
    // 窗口开在 d 上：先走开一步再回来，列表才真去问一趟（导航到当前目录是空操作）。
    w.navigate_to(dir.join("src").to_string_lossy().to_string());
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
        r.mtime_secs = std::fs::metadata(&r.path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|t| t.as_secs());
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
    if scene == "search" {
        // 在工具条的搜索框里打字（合成事件，同 Ctrl+F 那个口），等那块板子落下一份答案。
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        let before = w.search.rounds();
        crate::find::testing::type_into_search(&ctx, &mut w, "retry");
        crate::find::testing::settle(&w.search, before, "截图的搜索那一张").await;
    }
    let theme = crate::theme::testing::default_theme();
    let preview = scene == "main";
    let split = scene == "split";
    let h = std::thread::spawn(move || {
        struct Shot {
            ws: Workspace,
            n: u32,
            out: String,
            preview: bool,
        }
        impl eframe::App for Shot {
            fn ui(&mut self, ui: &mut egui::Ui, _f: &mut eframe::Frame) {
                self.n += 1;
                if self.n == 5 && self.preview {
                    self.ws.set_preview(true);
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
                .with_inner_size([1280.0, 800.0])
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
                }))
            }),
        )
        .unwrap();
    });
    tokio::task::spawn_blocking(move || h.join().unwrap())
        .await
        .unwrap();
}
