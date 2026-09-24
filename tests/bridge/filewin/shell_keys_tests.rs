//! 〔FW1+FW2 · 2026-09-24〕键盘 · 多选 · 右键菜单 **接到窗口上**的判据。
//!
//! 🔴 这一摞**每一条都真跑生产那个 `frame_body`**，喂的是 egui 的合成事件
//! （按键 / 带修饰键的点击 / 右键）—— 零件（`select.rs`）各自的判据住 `select_tests`；
//! 这里看的是**胶水**：`frame_body` → `show_file_rows` → `RenderTally` →
//! `apply_keys` / `apply_pick_click` / `apply_menu_click` → `perform` → 那几个 `begin_*`。
//! 零件全绿而这一跳断了的那一形（本仓第一刀栽过：判据钉的是副本），只有这一摞看得见。
//!
//! ⚠ 买不到：**真键盘 / 真鼠标**（本机无图形会话）。真 X 键盘事件那一格住本文件末尾，
//! 走 Xvfb 台架（XTEST 注进去的真 X 事件 → winit → egui → 这张键位表）。

use super::*;
use crate::filewin::copy::testing::{rects_of, text_in_frame, PaintedText};
use crate::filewin::select::Action;
use crate::filewin::source::Row;

const SCREEN: egui::Vec2 = egui::vec2(1280.0, 800.0);

fn synth_cfg(label: &str) -> crate::ssh_source::RemoteConfig {
    crate::ssh_source::RemoteConfig {
        host: "example.invalid".into(),
        label: label.into(),
        port: 22,
        user: "nobody".into(),
        key_path: None,
        backend_path: "/nonexistent/cc-monitor-backend".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

fn row(name: &str, is_dir: bool, size: u64, lossy: bool) -> Row {
    Row {
        name: name.to_string(),
        path: format!("/srv/data/{name}"),
        is_dir,
        size,
        lossy_name: lossy,
    }
}

fn file(name: &str) -> Row {
    row(name, false, 9, false)
}

fn dir(name: &str) -> Row {
    row(name, true, 0, false)
}

/// 一个看着 `/srv/data`、列表里有这几行、**不连任何东西**的窗口。
fn window(rows: Vec<Row>) -> FileWindow {
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("keys")),
        "/srv/data".to_string(),
        None,
        rows,
    );
    *w.listing.error.lock().unwrap() = None;
    w
}

/// 驱动器：一帧一帧喂**生产那个** `frame_body`。时钟每帧走一秒 ——
/// 两帧里各点一下**永远**不会被 egui 认成双击（双击窗是 0.3 秒）。
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

    /// 跑一帧，`mods` 是这一帧**按着的**修饰键。
    fn frame_mods(
        &mut self,
        w: &mut FileWindow,
        events: Vec<egui::Event>,
        mods: egui::Modifiers,
    ) -> Vec<PaintedText> {
        self.t += 1.0;
        // egui 的「这一帧按着哪几个修饰键」是跨帧的状态，由 `ModifiersChanged` 事件改
        // ⇒ 每帧开头都明说一次（不说就沿用上一帧的，Ctrl 会「粘」到下一次点击上）。
        let mut all = vec![egui::Event::ModifiersChanged(mods)];
        all.extend(events);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            time: Some(self.t),
            events: all,
            ..Default::default()
        };
        let out = self.ctx.run_ui(input, |ui| w.frame_body(ui));
        let painted = text_in_frame(&out);
        out.drop_without_applying_deltas();
        painted
    }

    fn frame(&mut self, w: &mut FileWindow, events: Vec<egui::Event>) -> Vec<PaintedText> {
        self.frame_mods(w, events, egui::Modifiers::NONE)
    }

    /// 按一个键（按下即可；`intents` 只认按下）。
    fn key(&mut self, w: &mut FileWindow, k: egui::Key, m: egui::Modifiers) -> Vec<PaintedText> {
        self.frame(
            w,
            vec![egui::Event::Key {
                key: k,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: m,
            }],
        )
    }

    /// 在 `name` 那一行的名字上点一下（`button` 左 / 右），按着 `mods`。
    /// 两帧：先移过去（命中测试按上一帧的 widget 表做），再按下松开。
    fn click_name(
        &mut self,
        w: &mut FileWindow,
        name: &str,
        button: egui::PointerButton,
        mods: egui::Modifiers,
    ) -> egui::Pos2 {
        let painted = self.frame(w, Vec::new());
        let at = rects_of(&painted, name);
        assert_eq!(
            at.len(),
            1,
            "这一帧上「{name}」出现了 {} 次 —— 落点算不出来，下面判的不是这一行",
            at.len()
        );
        let pos = at[0].center();
        self.frame(w, vec![egui::Event::PointerMoved(pos)]);
        self.frame_mods(
            w,
            vec![
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: mods,
                },
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: mods,
                },
            ],
            mods,
        );
        pos
    }

    fn pick(&mut self, w: &mut FileWindow, name: &str, mods: egui::Modifiers) {
        self.click_name(w, name, egui::PointerButton::Primary, mods);
    }
}

fn picked(w: &FileWindow) -> Vec<String> {
    w.selection().names()
}

fn set(names: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

const NONE: egui::Modifiers = egui::Modifiers::NONE;
const CTRL: egui::Modifiers = egui::Modifiers::COMMAND;
const SHIFT: egui::Modifiers = egui::Modifiers::SHIFT;

// ════════════════════════════════════════════════════════════════════════
// 多选：点一下 / Ctrl / Shift —— 经真事件、经生产那一帧
// ════════════════════════════════════════════════════════════════════════

/// 🔴 单击 · Ctrl+单击 · Shift+单击，**每一下都经 `frame_body`**，选中态逐步两向相等；
/// 而且下一帧**画出来的**选中色恰好落在那几行上（`RenderTally::picked_rows`）。
#[test]
fn clicks_with_modifiers_pick_exactly_those_rows_and_paint_exactly_those() {
    let mut w = window(vec![
        file("a.bin"),
        file("b.bin"),
        file("c.bin"),
        dir("sub"),
        file("z.bin"),
    ]);
    let mut d = Drive::new();
    d.frame(&mut w, Vec::new());
    assert!(picked(&w).is_empty(), "什么都没点就有选中");

    d.pick(&mut w, "b.bin", NONE);
    assert_eq!(picked(&w), set(&["b.bin"]));
    d.pick(&mut w, "sub", CTRL);
    assert_eq!(picked(&w), set(&["b.bin", "sub"]));
    d.pick(&mut w, "z.bin", SHIFT);
    assert_eq!(
        picked(&w),
        set(&["sub", "z.bin"]),
        "Shift 没从锚（sub）扩到 z.bin"
    );
    d.pick(&mut w, "a.bin", CTRL);
    assert_eq!(picked(&w), set(&["a.bin", "sub", "z.bin"]));

    // 下一帧：选中色画在哪几行（下标）—— 与选中态**逐项相等**。
    d.frame(&mut w, Vec::new());
    assert_eq!(
        w.tally.picked_rows,
        vec![0, 3, 4],
        "画出来的选中色与选中态对不上"
    );
    assert_eq!(
        w.tally.cursor_row,
        Some(0),
        "光标那一圈没画在最后点的那一行"
    );
    // 选中不止一项 ⇒ 工具栏上说几项。
    let painted = d.frame(&mut w, Vec::new());
    assert_eq!(rects_of(&painted, "已选 3 项").len(), 1, "没说选中了几项");
}

/// 单击**不动目录**（双击才动）—— 选中是单击的事，别让它顺手把人带走。
#[test]
fn a_single_click_on_a_directory_selects_it_and_stays_put() {
    let mut w = window(vec![dir("sub"), file("a.bin")]);
    let mut d = Drive::new();
    d.pick(&mut w, "sub", NONE);
    assert_eq!(picked(&w), set(&["sub"]));
    assert_eq!(w.cwd, "/srv/data", "单击一下目录就进去了");
}

// ════════════════════════════════════════════════════════════════════════
// FW1：每个键位一格
// ════════════════════════════════════════════════════════════════════════

/// ↓ / ↑：光标一行一行走，**只选光标那一行**；到头停住。
#[test]
fn arrow_down_and_up_walk_the_cursor_and_pick_only_it() {
    let mut w = window(vec![file("a.bin"), file("b.bin"), file("c.bin")]);
    let mut d = Drive::new();
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    assert_eq!(picked(&w), set(&["a.bin"]), "没有光标时 ↓ 该落第一行");
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    assert_eq!(picked(&w), set(&["c.bin"]));
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    assert_eq!(picked(&w), set(&["c.bin"]), "到底了没停住");
    d.key(&mut w, egui::Key::ArrowUp, NONE);
    assert_eq!(picked(&w), set(&["b.bin"]));
    assert_eq!(w.selection().cursor(), Some("b.bin"));
}

/// Shift+↓ / Shift+↑：从锚扩选。
#[test]
fn shift_arrows_extend_from_the_anchor() {
    let mut w = window(vec![
        file("a.bin"),
        file("b.bin"),
        file("c.bin"),
        file("d.bin"),
    ]);
    let mut d = Drive::new();
    d.pick(&mut w, "b.bin", NONE);
    d.key(&mut w, egui::Key::ArrowDown, SHIFT);
    d.key(&mut w, egui::Key::ArrowDown, SHIFT);
    assert_eq!(picked(&w), set(&["b.bin", "c.bin", "d.bin"]));
    d.key(&mut w, egui::Key::ArrowUp, SHIFT);
    assert_eq!(picked(&w), set(&["b.bin", "c.bin"]));
}

/// Home / End：跳到头 / 尾，而且**列表真的滚过去了**（光标那一行落在这一帧的物化区间里）。
#[test]
fn home_and_end_jump_and_scroll_the_row_into_view() {
    let rows: Vec<Row> = (0..400).map(|i| file(&format!("f{i:04}.bin"))).collect();
    let mut w = window(rows);
    let mut d = Drive::new();
    d.frame(&mut w, Vec::new());
    let (f0, l0) = (w.tally.first_row, w.tally.last_row);
    assert!(l0 < 399, "这一屏竟然放得下 400 行 —— 下面那一比不携带信息");
    d.key(&mut w, egui::Key::End, NONE);
    assert_eq!(picked(&w), set(&["f0399.bin"]));
    assert!(
        w.tally.first_row <= 399 && 399 < w.tally.last_row,
        "End 之后最后一行不在物化区间 [{}, {}) 里 —— 光标跑到屏幕外了",
        w.tally.first_row,
        w.tally.last_row
    );
    assert_ne!(w.tally.first_row, f0, "没滚");
    d.key(&mut w, egui::Key::Home, NONE);
    assert_eq!(picked(&w), set(&["f0000.bin"]));
    assert_eq!(w.tally.first_row, 0, "Home 之后没滚回顶上");
    // 🔴 在视野里挪一步 ⇒ **不滚**（每按一下都滚，就把用户自己的滚动按住了）。
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    assert_eq!(w.tally.first_row, 0, "光标还在视野里，列表却滚了");
    let _ = l0;
}

/// Alt+↑：上一级。
#[test]
fn alt_up_goes_to_the_parent_directory() {
    let mut w = window(vec![file("a.bin")]);
    let mut d = Drive::new();
    d.pick(&mut w, "a.bin", NONE);
    assert_eq!(
        picked(&w),
        set(&["a.bin"]),
        "前提：先选中一项（下面判「换目录清空」要它）"
    );
    d.key(&mut w, egui::Key::ArrowUp, egui::Modifiers::ALT);
    assert_eq!(w.cwd, "/srv", "Alt+↑ 没回上一级");
    // 换了目录 ⇒ 选中态清空（新目录里同名的不是同一样东西）。
    assert!(picked(&w).is_empty());
}

/// 回车：目录进去 · 文件走编辑（这个窗口没运行时 ⇒ 编辑那一支**出声**）· 多选时说一次只能开一项。
#[test]
fn enter_opens_a_directory_edits_a_file_and_refuses_a_bunch() {
    let mut w = window(vec![dir("sub"), file("a.txt"), file("b.txt")]);
    let mut d = Drive::new();
    // 文件：落到 `begin_edit`（它出声：没有运行时）。
    d.pick(&mut w, "a.txt", NONE);
    d.key(&mut w, egui::Key::Enter, NONE);
    let e = w.listing.error.lock().unwrap().clone().unwrap_or_default();
    assert!(e.contains("运行时"), "回车没落到编辑那一支：{e:?}");
    assert_eq!(w.cwd, "/srv/data");
    // 多选：出声，不动。
    d.pick(&mut w, "b.txt", CTRL);
    d.key(&mut w, egui::Key::Enter, NONE);
    assert_eq!(
        w.key_notice(),
        Some(crate::filewin::select::refusal(Action::Open, 2).as_str())
    );
    // 目录：进去。
    d.pick(&mut w, "sub", NONE);
    d.key(&mut w, egui::Key::Enter, NONE);
    assert_eq!(w.cwd, "/srv/data/sub", "回车没进目录");
}

/// F2：一项 ⇒ 改名框摆出来、预填那一项；两项 ⇒ 出声、不摆框。
#[test]
fn f2_renames_one_and_refuses_two() {
    let mut w = window(vec![file("a.bin"), file("b.bin")]);
    let mut d = Drive::new();
    d.pick(&mut w, "a.bin", NONE);
    d.pick(&mut w, "b.bin", CTRL);
    d.key(&mut w, egui::Key::F2, NONE);
    assert!(w.write_prompt().is_none(), "选了两项按 F2 竟然摆出了改名框");
    assert_eq!(
        w.key_notice(),
        Some(crate::filewin::select::refusal(Action::Rename, 2).as_str())
    );
    d.pick(&mut w, "b.bin", NONE);
    d.key(&mut w, egui::Key::F2, NONE);
    let p = w.write_prompt().expect("F2 没摆出改名框").clone();
    assert_eq!(p.src_name, "b.bin");
    assert_eq!(p.text, "b.bin");
    assert!(
        w.key_notice().is_none(),
        "改名框摆出来了，上一句「做不了」还挂着"
    );
}

/// Ctrl+A：全选（两向相等），而且那一帧**画出来的**选中色盖满了视野里每一行。
#[test]
fn ctrl_a_picks_everything() {
    let names = ["a.bin", "b.bin", "c.bin", "sub"];
    let mut w = window(vec![
        file("a.bin"),
        file("b.bin"),
        file("c.bin"),
        dir("sub"),
    ]);
    let mut d = Drive::new();
    d.key(&mut w, egui::Key::A, CTRL);
    assert_eq!(picked(&w), set(&names));
    assert_eq!(w.tally.picked_rows, vec![0, 1, 2, 3]);
}

/// 打字跳转：敲「ze」跳到 zeta（不分大小写）；停一秒以上再敲一个不存在的开头 ⇒ **出声**。
#[test]
fn typing_jumps_to_the_first_name_with_that_prefix() {
    let mut w = window(vec![file("alpha"), file("Zeta.txt"), file("zulu")]);
    let mut d = Drive::new();
    d.frame(&mut w, vec![egui::Event::Text("z".into())]);
    assert_eq!(picked(&w), set(&["Zeta.txt"]));
    // ⚠ 驱动器每帧走一秒 ⇒ 下一个字**正好**隔一秒（不算停顿，接着攒）。
    d.frame(&mut w, vec![egui::Event::Text("u".into())]);
    assert_eq!(picked(&w), set(&["zulu"]), "「zu」没接着攒");
    // 停两秒 ⇒ 重来；「q」谁都不是 ⇒ 出声，选中不动。
    d.t += 2.0;
    let _ = d.frame(&mut w, vec![egui::Event::Text("q".into())]);
    let painted = d.frame(&mut w, Vec::new());
    assert_eq!(picked(&w), set(&["zulu"]));
    assert!(
        painted.iter().any(|(t, _)| t == "没有以「q」开头的项"),
        "没找到却一句话都没画"
    );
}

/// 🔴 键盘**不抢**：搜索框里正在打字 / 有模态框摆着 / 画的是命中那一摞 —— 三形各按一下 ↓，选中态都不动。
/// 反空真：同一个窗口、同一个键，闸都撤掉之后 ↓ 真的动了。
#[test]
fn keys_do_not_leak_past_a_focused_field_a_modal_or_the_hit_list() {
    let mut w = window(vec![file("a.bin"), file("b.bin")]);
    let mut d = Drive::new();
    // ① 模态框（新建目录那个框）。
    assert!(w.begin_mkdir());
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    d.key(&mut w, egui::Key::Delete, NONE);
    assert!(picked(&w).is_empty(), "模态框摆着，↓ 却动了列表");
    w.cancel_write();
    // ①′〔合并 F7b〕「新建空文件」那个框也是模态的 —— `modal_up()` 里要有它。
    assert!(w.begin_new_file());
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    d.key(&mut w, egui::Key::Delete, NONE);
    assert!(picked(&w).is_empty(), "新建空文件那个框摆着，↓ 却动了列表");
    w.cancel_new_file();
    // ② 搜索框里有字 ⇒ 画的是命中那一摞，而且搜索框拿着焦点。
    crate::filewin::find::testing::type_into_search(&d.ctx, &mut w, "a");
    assert!(w.showing_hits());
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    assert!(picked(&w).is_empty(), "搜索框里正在打字，↓ 却动了列表");
    // ③ 清空搜索框（还拿着焦点）⇒ 画回目录列表，但字仍归搜索框。
    w.query.clear();
    assert!(!w.showing_hits());
    assert!(d.ctx.egui_wants_keyboard_input(), "前提：搜索框还拿着焦点");
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    assert!(picked(&w).is_empty(), "搜索框拿着焦点，↓ 却动了列表");
    // 反空真：点一下列表里的行 ⇒ 焦点交回列表 ⇒ ↓ 真的动了。
    d.pick(&mut w, "a.bin", NONE);
    assert!(
        !d.ctx.egui_wants_keyboard_input(),
        "点了行，焦点却没交回列表"
    );
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    assert_eq!(picked(&w), set(&["b.bin"]));
}

/// 🔴 点一下行 ⇒ **键盘交回列表**，哪怕此前拿着焦点的是一颗按钮（Tab 过去的）。
///
/// ⚠ 这一跳是 egui 的缺省行为（点别处即交出焦点），不是本仓的代码 —— 本条钉的是
/// `keys_blocked` 第 4 道闸**依赖**的那个前提：egui 哪天换了缺省，键盘会在点过行之后
/// 仍然归那颗按钮，而用户看到的是「↓ 没反应」。
#[test]
fn clicking_a_row_takes_the_keyboard_back_from_a_focused_button() {
    let mut w = window(vec![file("a.bin"), file("b.bin")]);
    let mut d = Drive::new();
    d.frame(&mut w, Vec::new());
    d.key(&mut w, egui::Key::Tab, NONE);
    d.frame(&mut w, Vec::new());
    assert!(
        d.ctx.egui_wants_keyboard_input(),
        "前提：Tab 之后该有一个控件拿着焦点"
    );
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    assert!(picked(&w).is_empty(), "一颗按钮拿着焦点，↓ 却动了列表");
    d.pick(&mut w, "a.bin", NONE);
    assert!(
        !d.ctx.egui_wants_keyboard_input(),
        "点了行，焦点还在那颗按钮上"
    );
    d.key(&mut w, egui::Key::ArrowDown, NONE);
    assert_eq!(picked(&w), set(&["b.bin"]), "点了行之后 ↓ 没归列表");
}

/// 🔴 **执行口自己也问那张表**：表里没有的动作，`perform` 出声、不做 ——
/// 哪怕底下那个 `begin_*` 自己会静默拒掉（那一形在屏幕上就是「点了没反应」）。
#[test]
fn perform_refuses_what_the_table_refuses_and_says_so() {
    use Action::*;
    let lossy = "\u{FFFD}x";
    // (行, 选中谁, 表里没有的那几件)
    let cases: Vec<(Vec<Row>, Vec<&str>, Vec<Action>)> = vec![
        (vec![dir("sub")], vec!["sub"], vec![Edit, Copy, Download]),
        (
            vec![row(lossy, false, 3, true)],
            vec![lossy],
            vec![Open, Edit, Copy, Download, Rename, Chmod, Delete],
        ),
        (
            vec![file("a.bin"), file("b.bin")],
            vec!["a.bin", "b.bin"],
            // 〔FW5〕多项的「权限」放开了（批量改权限）⇒ 不在拒绝表里。
            vec![Open, Edit, Copy, Download, Rename],
        ),
        (
            vec![file("a.bin")],
            vec![],
            vec![Open, Edit, Copy, Download, Rename, Chmod, Delete],
        ),
    ];
    for (rows, pick, refused) in cases {
        for a in refused {
            let mut w = window(rows.clone());
            let mut d = Drive::new();
            for (k, n) in pick.iter().enumerate() {
                d.pick(&mut w, n, if k == 0 { NONE } else { CTRL });
            }
            assert!(!w.perform(a, None), "{pick:?} 上 {a:?} 竟然做了");
            assert_eq!(
                w.key_notice(),
                Some(crate::filewin::select::refusal(a, pick.len()).as_str()),
                "{pick:?} 上 {a:?} 做不了却没出声"
            );
            assert!(
                w.write_prompt().is_none() && w.copy_prompt().is_none() && w.pull_ask().is_none()
            );
            assert_eq!(w.cwd, "/srv/data");
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
// 🔴 选中态 == 批量那一摞（Delete 键 → 一次问完 → 线上）
// ════════════════════════════════════════════════════════════════════════

/// 🔴🔴 **选中哪几项，删的就是哪几项 —— 逐项相等，两向。**
///
/// 走的是生产那一整条：真合成 Ctrl+点击选中三项 → 按 Delete → `perform` →
/// `start_writes` → `run_writes`（围栏 → **一次问完**）→ 看板上摆着的那一问 →
/// 答「做」→ 通道 → 合成后端收到的那几行 `files-delete`。
///
/// 两侧**异源**：期望是手写的三个名字；实得一侧是「问的那一摞」与「线上那几行」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_batch_that_reaches_the_wire_is_exactly_the_selection() {
    use crate::filewin::find::testing::{wire_up, Declared, FakeBackend};
    let wired = wire_up(
        "keys-batch",
        FakeBackend::new(&["files-delete", "files-ls"], Declared::default()),
    )
    .await;
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("keys-batch")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        vec![
            file("a.bin"),
            file("b.bin"),
            file("c.bin"),
            dir("sub"),
            file("d.bin"),
        ],
    );
    w.attach_line(wired.line.clone());
    let mut d = Drive::new();
    d.pick(&mut w, "a.bin", NONE);
    d.pick(&mut w, "c.bin", CTRL);
    d.pick(&mut w, "sub", CTRL);
    let want = set(&["a.bin", "c.bin", "sub"]);
    assert_eq!(picked(&w), want, "前提：选中态没选成那三项");

    d.key(&mut w, egui::Key::Delete, NONE);
    for _ in 0..400 {
        if w.write_board.is_asking() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let asking = w.write_board.asking();
    let asked: Vec<String> = {
        let mut v: Vec<String> = asking
            .iter()
            .map(|op| match op {
                crate::filewin::writeops::WriteOp::Delete { path, .. } => {
                    crate::filewin::source::remote_basename(path).to_string()
                }
                other => panic!("批量那一摞里混进了别的操作：{other:?}"),
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(asked, want, "问的那一摞与选中态不等（两向）");
    // 目录那一项带着「是目录」走（删目录与删文件是两条不同的远端调用）。
    assert!(asking.iter().any(|op| matches!(
        op,
        crate::filewin::writeops::WriteOp::Delete { path, is_dir: true, raw: None } if path == "/srv/data/sub"
    )));
    assert!(
        wired.cmds().is_empty(),
        "还没答就上了线：{:?}",
        wired.cmds()
    );

    assert!(w.write_board.settle(true));
    for _ in 0..400 {
        if w.write_board.rounds() > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let out = w.write_board.last().expect("那一摞没跑完");
    assert_eq!(out.ok, 3, "实得 {out:?}");
    let on_wire: Vec<String> = {
        let mut v: Vec<String> = wired
            .log
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["cmd"] == "files-delete")
            .map(|r| r["args"]["rel"].as_str().unwrap_or("").to_string())
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        on_wire, want,
        "线上那几行 files-delete 与选中态不等（两向）"
    );
    // 跑完 ⇒ 选中清掉（那几个名字已经不在了），下一帧重列。
    d.frame(&mut w, Vec::new());
    assert!(picked(&w).is_empty(), "删完了选中还挂着那几个名字");
}

// ════════════════════════════════════════════════════════════════════════
// FW2：右键菜单 == 当前选中能做的动作（两向）
// ════════════════════════════════════════════════════════════════════════

/// 摆着的菜单上**画出来的**那几段字（只收整个落在菜单那一块里的，免得收到底下的行）。
fn menu_texts(d: &Drive, w: &FileWindow, painted: &[PaintedText]) -> Vec<String> {
    let m = w.menu().expect("菜单没摆出来");
    let area = d
        .ctx
        .memory(|mem| mem.area_rect(FileWindow::menu_id(m.serial)))
        .expect("菜单那一块没有矩形 —— 它没被画出来");
    let mut v: Vec<String> = painted
        .iter()
        .filter(|(_, r)| area.contains_rect(*r))
        .map(|(t, _)| t.clone())
        .collect();
    v.sort();
    v
}

/// 右键点 `target`（先按 `pre` 那几下选好），跑到菜单真画出来那一帧，回菜单上的字。
fn open_menu(
    rows: Vec<Row>,
    pre: &[(&str, egui::Modifiers)],
    target: &str,
) -> (Drive, FileWindow, Vec<String>) {
    let mut w = window(rows);
    let mut d = Drive::new();
    for (n, m) in pre {
        d.pick(&mut w, n, *m);
    }
    d.click_name(&mut w, target, egui::PointerButton::Secondary, NONE);
    assert!(w.menu().is_some(), "右键点了「{target}」，菜单没摆出来");
    // 第一帧是 egui 弹层的「量尺寸」那一趟（不画）⇒ 再跑一帧才是真画。
    let painted = d.frame(&mut w, Vec::new());
    let texts = menu_texts(&d, &w, &painted);
    (d, w, texts)
}

/// 🔴🔴 **菜单项 == 当前选中能做的动作 —— 逐格手写，两向相等。**
///
/// 期望一侧是**手写**的字（不调 `actions_for`，否则两侧同源恒真）；
/// 实得一侧是 egui 这一帧**真画在菜单那一块里**的字。
#[test]
fn the_menu_lists_exactly_what_the_selection_allows() {
    let big = crate::filewin::editor::MAX_EDIT_BYTES as u64 + 1;
    let lossy = "\u{FFFD}x";
    // (情形, 行, 先选, 右键点谁, 菜单上该有的字)
    let cases: Vec<(
        &str,
        Vec<Row>,
        Vec<(&str, egui::Modifiers)>,
        &str,
        Vec<&str>,
    )> = vec![
        (
            "一个普通文件",
            vec![file("a.bin"), file("f.txt")],
            vec![],
            "f.txt",
            vec!["编辑", "复制", "下载", "改名", "权限", "删除"],
        ),
        (
            "一个目录",
            vec![file("a.bin"), dir("sub")],
            vec![],
            "sub",
            vec!["打开", "改名", "权限", "删除"],
        ),
        (
            "一个超编辑上限的文件",
            vec![file("a.bin"), row("h.bin", false, big, false)],
            vec![],
            "h.bin",
            vec!["复制", "下载", "改名", "权限", "删除"],
        ),
        (
            "一个有损名文件",
            vec![file("a.bin"), row(lossy, false, 3, true)],
            vec![],
            lossy,
            vec![MENU_EMPTY],
        ),
        (
            "右键落在选中里：两项",
            vec![file("a.bin"), file("b.bin"), file("c.bin")],
            vec![("a.bin", NONE), ("c.bin", CTRL)],
            "c.bin",
            vec!["改这 2 项的权限", "删除这 2 项"],
        ),
        (
            "右键落在选中外：换成只选它",
            vec![file("a.bin"), file("b.bin"), dir("c")],
            vec![("a.bin", NONE), ("b.bin", CTRL)],
            "c",
            vec!["打开", "改名", "权限", "删除"],
        ),
        (
            "两项混着有损名",
            vec![row(lossy, false, 3, true), file("b.bin"), file("c.bin")],
            vec![(lossy, NONE), ("c.bin", CTRL)],
            "c.bin",
            vec![MENU_EMPTY],
        ),
    ];
    for (what, rows, pre, target, want) in cases {
        let (_, _, got) = open_menu(rows, &pre, target);
        let mut want: Vec<String> = want.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(
            got, want,
            "「{what}」那一格：菜单上画的与该有的不等（两向）"
        );
    }
}

/// 🔴 菜单上**每一项点下去都落到了对的地方**（对一个普通文件、一个目录）。
///
/// 一项一格：点完之后窗口状态里那件事**恰好**发生在右键点的那一行上。
#[test]
fn every_menu_item_lands_on_the_row_it_was_opened_for() {
    type Check = fn(&FileWindow) -> Result<(), String>;
    let on_file: Vec<(&str, Check)> = vec![
        ("编辑", |w| {
            let e = w.listing.error.lock().unwrap().clone().unwrap_or_default();
            e.contains("运行时").then_some(()).ok_or(e)
        }),
        ("复制", |w| match w.copy_prompt() {
            Some(p) if p.src_name == "f.txt" => Ok(()),
            other => Err(format!("{other:?}")),
        }),
        ("下载", |w| match w.pull_ask() {
            Some(a) if a.src_name() == "f.txt" => Ok(()),
            other => Err(format!("{other:?}")),
        }),
        ("改名", |w| match w.write_prompt() {
            Some(p) if p.src_name == "f.txt" && p.text == "f.txt" => Ok(()),
            other => Err(format!("{other:?}")),
        }),
        ("权限", |w| match w.write_prompt() {
            Some(p) if p.src_name == "f.txt" && p.text.is_empty() => Ok(()),
            other => Err(format!("{other:?}")),
        }),
        ("删除", |w| {
            // 没有运行时 ⇒ 那一摞起不来，而它**出声**（不静默吞掉一次删除）。
            let e = w.listing.error.lock().unwrap().clone().unwrap_or_default();
            e.contains("运行时").then_some(()).ok_or(e)
        }),
    ];
    for (label, check) in on_file {
        let (mut d, mut w, texts) = open_menu(vec![file("a.bin"), file("f.txt")], &[], "f.txt");
        assert!(
            texts.iter().any(|t| t == label),
            "菜单上没有「{label}」：{texts:?}"
        );
        click_menu_item(&mut d, &mut w, label);
        assert!(w.menu().is_none(), "点了「{label}」菜单还摆着");
        check(&w).unwrap_or_else(|e| panic!("点了「{label}」之后状态不对：{e}"));
    }
    // 目录：「打开」⇒ 进去。
    let (mut d, mut w, _) = open_menu(vec![file("a.bin"), dir("sub")], &[], "sub");
    click_menu_item(&mut d, &mut w, "打开");
    assert_eq!(w.cwd, "/srv/data/sub");
}

/// 在菜单上点 `label` 那一项（两帧：移过去 · 按下松开）。
fn click_menu_item(d: &mut Drive, w: &mut FileWindow, label: &str) {
    let painted = d.frame(w, Vec::new());
    let area = d
        .ctx
        .memory(|mem| mem.area_rect(FileWindow::menu_id(w.menu().unwrap().serial)))
        .unwrap();
    let hits: Vec<egui::Rect> = rects_of(&painted, label)
        .into_iter()
        .filter(|r| area.contains_rect(*r))
        .collect();
    assert_eq!(hits.len(), 1, "菜单上「{label}」不是恰好一处");
    let pos = hits[0].center();
    d.frame(w, vec![egui::Event::PointerMoved(pos)]);
    d.frame(w, crate::filewin::rows::testing::click_at(pos));
}

/// 点菜单外面 ⇒ 菜单收掉，**什么都不做**；右键点另一行 ⇒ 换成那一行的菜单（不是只关不开）。
#[test]
fn clicking_elsewhere_closes_the_menu_and_another_right_click_reopens_it() {
    let (mut d, mut w, _) = open_menu(vec![file("a.bin"), dir("sub"), file("f.txt")], &[], "f.txt");
    let first = w.menu().unwrap().serial;
    // 在另一行上右键 ⇒ 新菜单（对它说话）。
    d.click_name(&mut w, "sub", egui::PointerButton::Secondary, NONE);
    let m = w.menu().expect("第二次右键只把菜单关了，没开新的");
    assert_ne!(m.serial, first);
    assert_eq!(picked(&w), set(&["sub"]));
    assert!(m.actions.contains(&Action::Open));
    // 点空白处（列表下面远处）⇒ 收掉、什么都不做。
    let far = egui::pos2(600.0, 760.0);
    d.frame(&mut w, vec![egui::Event::PointerMoved(far)]);
    d.frame(&mut w, crate::filewin::rows::testing::click_at(far));
    assert!(w.menu().is_none(), "点了别处菜单还摆着");
    assert_eq!(w.cwd, "/srv/data", "点别处关菜单，竟然顺手做了一件事");
    assert!(w.write_prompt().is_none() && w.copy_prompt().is_none());
}

// ════════════════════════════════════════════════════════════════════════
// 🔴 真 X 键盘：Xvfb 台架（XTEST 注进去的真键 → winit → egui → `frame_body` → 这张键位表）
// ════════════════════════════════════════════════════════════════════════
//
// # 它买的是上面那一摞买不到的那一段
//
// 上面每一格喂的都是**合成的** `egui::Event::Key`。而「真键盘按下 F2」要先过
// X 的键码 → winit 的逻辑键 → egui-winit 的 `egui::Key` 这三道翻译，
// 其中任何一道把 `F2` / `Delete` / `Home` 翻成了别的（或把 Ctrl+A 连带发成了 `Text("a")`），
// 合成事件那一摞**一条都不会红**。⇒ 这一格在真 X 服务器上把每个键真按一遍，
// 从**生产那个** `FileWindow` 里读回选中态。
//
// # 被测对象只有一份
//
// 托管它的那个 `eframe::App` 只做两件事：调生产那个 `frame_body`（与 `App::ui` 那一句委派逐字同形）·
// 把选中态抄进一个共享格子让驱动线程读。键位表、胶水、四道闸一个字节都不在这儿。
//
// # ⚠ 买不到（逐条，别读宽）
//
// - 真物理键盘（按键抖动、输入法、非 US 键盘布局 —— Xvfb 的键盘布局是默认那一份）；
// - 真窗口管理器下的焦点（Xvfb 没有 WM；这一格靠「指针在哪个窗口、键就给哪个窗口」加一次显式 `windowfocus`）；
// - Windows / macOS 一趟没跑过（本族整条 `cfg(not(windows))`）。

/// 真键那一趟从窗口里带出来的读数（驱动线程隔着线程读）。
#[cfg(not(windows))]
#[derive(Default)]
struct KeyProbe {
    frames: u64,
    picked: Vec<String>,
    cwd: String,
    prompt: Option<String>,
    error: Option<String>,
    /// 驱动线程要求：把 F2 摆出来的那个框收掉（它是模态的，摆着的时候键盘不归列表）。
    cancel_prompt: bool,
    close_now: bool,
}

#[cfg(not(windows))]
struct KeyProbeApp {
    w: FileWindow,
    shared: std::sync::Arc<std::sync::Mutex<KeyProbe>>,
}

#[cfg(not(windows))]
impl eframe::App for KeyProbeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 🔴 与 `impl eframe::App for FileWindow` 那一句委派逐字同形 —— 被测对象是同一个。
        self.w.frame_body(ui);
        let ctx = ui.ctx().clone();
        let mut p = self.shared.lock().unwrap();
        p.frames += 1;
        p.picked = self.w.selection().names();
        p.cwd = self.w.cwd.clone();
        p.prompt = self.w.write_prompt().map(|q| q.src_name.clone());
        p.error = self.w.listing.error.lock().unwrap().clone();
        if p.cancel_prompt {
            p.cancel_prompt = false;
            self.w.cancel_write();
        }
        let close = p.close_now;
        drop(p);
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // 台架自己把帧推起来（驱动线程隔着线程读）；⚠ 这是台架的选择，不是生产的行为。
        ctx.request_repaint();
    }
}

/// 窗口标题 —— **全 ASCII**（`xdotool search --name` 按 C 区域设置编译正则，中文标题一个都找不着；
/// `rows_tests` 那一格现打栽过）。
#[cfg(not(windows))]
const KEY_PROBE_TITLE: &str = "ccm-filewin-real-key-probe";

/// 真键那一趟的步骤：`(读数名, xdotool 参数, 等到什么为止)`。
#[cfg(not(windows))]
type KeyStep = (&'static str, &'static [&'static str], fn(&KeyProbe) -> bool);

#[cfg(not(windows))]
fn key_steps() -> Vec<KeyStep> {
    fn p(k: &KeyProbe, names: &[&str]) -> bool {
        let mut want: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        want.sort();
        k.picked == want
    }
    vec![
        ("k.down1", &["key", "Down"], |k| p(k, &["alpha.bin"])),
        ("k.down2", &["key", "Down"], |k| p(k, &["beta.bin"])),
        ("k.shift_down", &["key", "shift+Down"], |k| {
            p(k, &["beta.bin", "gamma.bin"])
        }),
        ("k.end", &["key", "End"], |k| p(k, &["zeta.bin"])),
        ("k.home", &["key", "Home"], |k| p(k, &["alpha.bin"])),
        ("k.ctrl_a", &["key", "ctrl+a"], |k| k.picked.len() == 5),
        ("k.type_ze", &["type", "ze"], |k| p(k, &["zeta.bin"])),
        ("k.delete", &["key", "Delete"], |k| {
            k.error.as_deref().is_some_and(|e| e.contains("运行时"))
        }),
        ("k.type_su", &["type", "su"], |k| p(k, &["sub"])),
        ("k.f2", &["key", "F2"], |k| {
            k.prompt.as_deref() == Some("sub")
        }),
        ("k.return", &["key", "Return"], |k| k.cwd == "/srv/data/sub"),
        ("k.alt_up", &["key", "alt+Up"], |k| k.cwd == "/srv/data"),
    ]
}

/// **实景工作面**：真 X 键盘打在生产那个窗口上。
#[cfg(not(windows))]
#[test]
#[ignore = "实景工作面：由 a_real_x_keyboard_drives_the_list 在它自己的进程里点起来"]
fn xvfb_worker_real_keys_on_the_window() {
    use crate::filewin::rows::testing::xvfb;
    use std::sync::{Arc, Mutex};
    let display = xvfb::child_display();
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("xvfb-keys")),
        "/srv/data".to_string(),
        None,
        vec![
            file("alpha.bin"),
            file("beta.bin"),
            file("gamma.bin"),
            dir("sub"),
            file("zeta.bin"),
        ],
    );
    *w.listing.error.lock().unwrap() = None;
    let shared = Arc::new(Mutex::new(KeyProbe::default()));
    let app = KeyProbeApp {
        w,
        shared: Arc::clone(&shared),
    };
    let drv_shared = Arc::clone(&shared);
    let drv_display = display.clone();
    let driver = std::thread::spawn(move || drive_real_keys(&drv_display, &drv_shared));
    let opts = eframe::NativeOptions {
        event_loop_builder: Some(Box::new(crate::filewin::shell::any_thread_hook)),
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_title(KEY_PROBE_TITLE),
        ..Default::default()
    };
    let ran = eframe::run_native(
        KEY_PROBE_TITLE,
        opts,
        Box::new(move |_cc| Ok(Box::new(app))),
    );
    xvfb::emit(
        "k.run_native",
        match &ran {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("err:{e}"),
        },
    );
    for line in driver
        .join()
        .unwrap_or_else(|_| vec!["驱动线程炸了".into()])
    {
        println!("  驱动线程：{line}");
    }
    xvfb::emit("k.frames", shared.lock().unwrap().frames);
}

/// 驱动那一侧：等窗口 → 把指针与焦点放进窗口 → 逐步按键、每步等到窗口状态变过来（带上限）→ 关窗。
#[cfg(not(windows))]
fn drive_real_keys(
    display: &str,
    shared: &std::sync::Arc<std::sync::Mutex<KeyProbe>>,
) -> Vec<String> {
    use crate::filewin::rows::testing::xvfb;
    let mut log = Vec::new();
    let sleep = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));
    let ids = xvfb::wait_for_windows(display, KEY_PROBE_TITLE, 20_000);
    let Some(id) = ids.first().cloned() else {
        shared.lock().unwrap().close_now = true;
        log.push("一个窗口都没等到".into());
        return log;
    };
    for _ in 0..100 {
        if shared.lock().unwrap().frames >= 5 {
            break;
        }
        sleep(50);
    }
    // 指针放进窗口下半截的空白处（不压在任何行、任何输入框上），再显式给一次焦点。
    if let Ok(g) = xvfb::geometry(display, &id) {
        let x = (g.x + g.w as i32 / 2).to_string();
        let y = (g.y + g.h as i32 - 40).to_string();
        if let Err(e) = xvfb::xdotool_on(display, &["mousemove", "--sync", &x, &y]) {
            log.push(format!("移不过去：{e}"));
        }
    }
    if let Err(e) = xvfb::xdotool_on(display, &["windowfocus", "--sync", &id]) {
        log.push(format!("windowfocus 没成（没有 WM 时可能如此）：{e}"));
    }
    sleep(200);
    for (name, args, done) in key_steps() {
        // 两段打字之间要隔过「停一秒重来」那个窗（`TYPE_AHEAD_RESET_SECS`）。
        if args[0] == "type" {
            sleep(1_300);
        }
        if let Err(e) = xvfb::xdotool_on(display, args) {
            log.push(format!("{name}：xdotool 没成：{e}"));
        }
        let mut ok = false;
        for _ in 0..60 {
            if done(&shared.lock().unwrap()) {
                ok = true;
                break;
            }
            sleep(50);
        }
        xvfb::emit(name, if ok { "ok" } else { "timeout" });
        if !ok {
            let p = shared.lock().unwrap();
            log.push(format!(
                "{name} 没等到：picked={:?} cwd={} prompt={:?} error={:?}",
                p.picked, p.cwd, p.prompt, p.error
            ));
        }
        // F2 摆出来的框是模态的（它摆着的时候键盘不归列表 —— 那正是第一道闸）⇒ 收掉再往下。
        if name == "k.f2" {
            shared.lock().unwrap().cancel_prompt = true;
            sleep(150);
        }
    }
    sleep(150);
    shared.lock().unwrap().close_now = true;
    log
}

/// 🔴 **真 X 键盘下，键位表的每一格都按得动** —— 逐格相等（每一格恰好 `ok`）。
///
/// 反空真：每一步等的都是**窗口状态真变成了那一格要的样子**（不是「按过了」），
/// 而相邻两步要的状态互不相同 ⇒ 一个键被吞掉，它自己那一格与下一格都会 `timeout`。
#[cfg(not(windows))]
#[test]
fn a_real_x_keyboard_drives_the_list() {
    use crate::filewin::rows::testing::xvfb;
    let run = {
        let _guard = xvfb::exclusive();
        xvfb::require_toolbox("「真键盘按得动文件窗口」");
        let screen = xvfb::Screen::start()
            .unwrap_or_else(|e| panic!("起不了 Xvfb ⇒ 这一格判不了，不是过了：{e}"));
        xvfb::run_scenario(
            screen.display(),
            "filewin::shell::keys_tests::xvfb_worker_real_keys_on_the_window",
        )
    };
    run.must_have_passed("「真键盘按得动文件窗口」");
    let frames: u64 = run.reading("k.frames").parse().unwrap_or(0);
    assert!(
        frames >= 5,
        "窗口只画了 {frames} 帧 —— 下面那几格量的不是一个活窗口"
    );
    let got: Vec<(String, String)> = key_steps()
        .iter()
        .map(|(n, _, _)| (n.to_string(), run.reading(n)))
        .collect();
    let want: Vec<(String, String)> = key_steps()
        .iter()
        .map(|(n, _, _)| (n.to_string(), "ok".to_string()))
        .collect();
    assert_eq!(
        got, want,
        "真键盘下有几格没按动（`timeout` 那几格）。驱动线程的日记在子进程 stdout 里：\n{}",
        run.stdout
    );
    assert_eq!(run.reading("k.run_native"), "ok", "窗口没干净收场");
}

// ════════════════════════════════════════════════════════════════════════
// 〔FW5 · 第四波〕批量改权限 · 删目录连同内容 · 乱码名 —— 接到窗口上的那几跳
// ════════════════════════════════════════════════════════════════════════

/// 🔴 选中三项（普通文件 · 目录 · 带字节的乱码名）→ 菜单那一项「权限」→ 一个框 → 敲 `640` →
/// 一次问完 → 线上恰好三行 `files-chmod`，乱码名那一行的 `rel` 是 b16。
/// 再选中目录与乱码名 → Delete → 线上两行 `files-delete`：目录带 `recursive: true`，乱码名走 b16。
///
/// 两侧异源：期望手写；实得读合成后端真收到的那几行。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn batch_chmod_and_a_recursive_delete_reach_the_wire_with_the_right_shapes() {
    use crate::filewin::find::testing::{wire_up, Declared, FakeBackend};
    use crate::filewin::source::Listed;
    let wired = wire_up(
        "keys-fw5",
        FakeBackend::new(
            &["files-chmod", "files-delete", "files-ls"],
            Declared::default(),
        ),
    )
    .await;
    let odd = Listed {
        raw_name: Some(b"caf\xe9".to_vec()),
        ..Listed::plain(row("caf\u{FFFD}", false, 3, true))
    };
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("keys-fw5")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        vec![
            Listed::plain(file("a.bin")),
            odd.clone(),
            Listed::plain(dir("sub")),
        ],
    );
    w.attach_line(wired.line.clone());
    // ⚠ 选中直接走选中态本体（不经 `Drive::pick`）：那个驱动按**画出来的字**找行，
    //   而 U+FFFD 在无头字体链上画不出来（落点算不出）。点击 → 选中那一跳另有判据看着。
    fn pick_rows(w: &mut FileWindow, idx: &[usize]) {
        let rows = w.listing.rows.lock().unwrap().clone();
        w.selection.clear();
        for (k, &i) in idx.iter().enumerate() {
            w.selection
                .click(&rows, i, if k == 0 { NONE } else { CTRL });
        }
    }
    pick_rows(&mut w, &[0, 1, 2]);
    assert_eq!(w.selection().len(), 3, "前提：三项没选上");
    assert!(w.perform(Action::Chmod, None), "批量改权限没摆出框");
    {
        let p = w.write_prompt.as_mut().expect("框没摆出来");
        assert_eq!(p.heading(), "把这 3 项的权限改成（八进制）：");
        p.text = "640".into();
    }
    assert!(w.confirm_write(None), "框里的字没变成一摞");
    async fn settle(w: &mut FileWindow, round: u64) {
        for _ in 0..400 {
            if w.write_board.is_asking() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(
            w.write_board.asking().len(),
            if round == 0 { 3 } else { 2 },
            "一次问完那一摞件数不对"
        );
        assert!(w.write_board.settle(true));
        for _ in 0..400 {
            if w.write_board.rounds() > round {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    settle(&mut w, 0).await;
    let b16 = serde_json::json!({ "b16": "636166e9" });
    let chmods: Vec<serde_json::Value> = wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-chmod")
        .map(|r| r["args"]["rel"].clone())
        .collect();
    assert_eq!(
        chmods,
        vec![
            serde_json::json!("a.bin"),
            b16.clone(),
            serde_json::json!("sub")
        ],
        "线上那几行 files-chmod 与选中态不等"
    );

    // 删：目录 ＋ 乱码名（不跑帧 ⇒ 列表不重列，下标还是那三行）。
    pick_rows(&mut w, &[1, 2]);
    assert!(w.perform(Action::Delete, None), "批量删没起来");
    settle(&mut w, 1).await;
    let dels: Vec<serde_json::Value> = wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-delete")
        .map(|r| r["args"].clone())
        .collect();
    assert_eq!(
        dels,
        vec![
            serde_json::json!({ "root": "/srv/data", "rel": b16 }),
            serde_json::json!({ "root": "/srv/data", "rel": "sub", "recursive": true }),
        ],
        "线上那几行 files-delete 不对（目录要带 recursive，乱码名要走 b16）"
    );
}
