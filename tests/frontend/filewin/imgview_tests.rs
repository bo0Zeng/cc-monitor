//! [`super`] 的判据 —— **图片查看页**。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`fit_never_enlarges_and_fills_the_tighter_side`] | 适应窗口：大图按紧的那一边放进画面、小图 100%；高 DPI 折回物理像素 | 期望手写 |
//! | [`zooming_keeps_the_point_under_the_pointer_still`] | 以指针为中心缩放：指针底下那一点缩放前后在屏上同一处 | 期望由几何现算 |
//! | [`dragging_never_shows_empty_space_and_a_small_side_stays_centred`] | 拖不出空白；比画面小的方向居中 | 期望手写 |
//! | [`keys_on_the_page_and_ctrl_keys_left_to_the_window`] | ← → + − 0 各做各的；带 Ctrl 的不认（整窗缩放） | 期望手写 |
//! | [`an_image_offers_view_and_not_edit_and_a_text_the_other_way`] | 右键菜单那张表：图 ⇒「查看」无「编辑」；文本反过来；svg 照旧「编辑」 | 期望手写 |
//! | [`enter_and_double_click_open_a_view_tab_and_again_switch_to_it`] | 回车 / 真双击一张图 ⇒ 同一栏开一个查看页；再开同一张 ⇒ 切过去；双击文本 ⇒ 编辑页 | 指针事件合成，命中按画出来的那一行 |
//! | [`left_and_right_walk_the_images_of_that_folder_and_stop_at_the_ends`] | 同目录只翻图（文本跳过）、到头不绕回；标签名跟着换 | 期望手写 |
//! | [`the_page_draws_the_picture_and_shares_one_decode_with_the_preview`] | 预览先摆出这张图，再开查看页：线上从头读只一趟、两处同一张贴图；头条 · 状态栏的字真画出来 | 实得是合成后端那本账；字从这一帧读回 |
//! | [`zoom_keys_the_wheel_and_dragging_move_the_picture`] | + ⇒ 放大一步；0 ⇒ 回适应窗口；滚轮 ⇒ 缩放；原尺寸下拖动 ⇒ 平移 | 读 `Viewer` 的看法与偏移 |
//! | [`over_the_cap_says_why_and_reads_nothing`] | 列表那一行超上限 ⇒ 那一句 ＋「下载」画出来，线上零条 `files-read-chunk` | 线上那本账 |

use super::*;
use crate::copy::testing::{rects_of, text_in_frame, PaintedText};
use crate::find::testing::{window_on, wire_up, Declared, FakeBackend, Wired};
use crate::shell::FileWindow;
use crate::workspace::Workspace;

const SCREEN: egui::Vec2 = egui::vec2(1280.0, 800.0);

#[test]
fn fit_never_enlarges_and_fills_the_tighter_side() {
    let view = egui::vec2(1280.0, 600.0);
    assert!((fit_scale([1600, 1000], view, 1.0) - 0.6).abs() < 1e-6);
    assert_eq!(fit_scale([100, 100], view, 1.0), 1.0, "小图被放大了");
    // 2× 屏：画面 600 点 ＝ 1200 物理像素 ⇒ 1000 高的图放得下原尺寸。
    assert_eq!(fit_scale([1600, 1000], view, 2.0), 1.0);
    assert_eq!(percent(0.6), 60);
}

#[test]
fn zooming_keeps_the_point_under_the_pointer_still() {
    let native = [4000, 3000];
    let view = egui::vec2(800.0, 600.0);
    let mut v = Viewer::open(vec![shot("/d/a.png", 10)], 0).unwrap();
    v.set_zoom(Zoom::Full);
    v.offset = egui::vec2(100.0, -50.0);
    let at = egui::vec2(150.0, 80.0);
    // 指针底下那一点在原图上的坐标（像素）。
    let under = |v: &Viewer| (at - v.offset) / v.scale(native, view, 1.0);
    let before = under(&v);
    v.zoom_by(2.0, at, native, view, 1.0);
    assert!(
        matches!(v.zoom, Zoom::Free(s) if (s - 1.5625).abs() < 1e-4),
        "{:?}",
        v.zoom
    );
    let after = under(&v);
    assert!(
        (before - after).length() < 1e-2,
        "指针底下那一点挪了：{before:?} → {after:?}"
    );
    // 缩到下限就停。
    v.zoom_by(-100.0, at, native, view, 1.0);
    assert!(matches!(v.zoom, Zoom::Free(s) if s == SCALE_MIN));
}

#[test]
fn dragging_never_shows_empty_space_and_a_small_side_stays_centred() {
    let view = egui::vec2(800.0, 600.0);
    // 宽过画面 400、矮于画面：横向最多拖 ±200，纵向居中。
    let c = clamp_offset(egui::vec2(999.0, 40.0), egui::vec2(1200.0, 300.0), view);
    assert_eq!(c, egui::vec2(200.0, 0.0));
    let c = clamp_offset(egui::vec2(-50.0, 0.0), egui::vec2(1200.0, 300.0), view);
    assert_eq!(c, egui::vec2(-50.0, 0.0));
}

fn key(k: egui::Key, m: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: m,
    }
}

#[test]
fn keys_on_the_page_and_ctrl_keys_left_to_the_window() {
    use egui::{Key, Modifiers as M};
    let a = keys(&[key(Key::ArrowRight, M::NONE), key(Key::Plus, M::NONE)]);
    assert_eq!((a.step, a.steps, a.zoom), (1, 1.0, None));
    let a = keys(&[
        key(Key::ArrowLeft, M::NONE),
        key(Key::Minus, M::NONE),
        key(Key::Num0, M::NONE),
    ]);
    assert_eq!((a.step, a.steps, a.zoom), (-1, -1.0, Some(Zoom::Fit)));
    let a = keys(&[
        key(Key::Plus, M::COMMAND),
        key(Key::Num0, M::COMMAND),
        key(Key::ArrowRight, M::ALT),
    ]);
    assert_eq!(a, Acts::default(), "Ctrl / Alt 组合键被查看页抢了");
}

fn listed(name: &str, size: u64) -> Listed {
    crate::find::testing::gated(Listed::plain(crate::source::Row {
        name: name.into(),
        path: format!("/d/{name}"),
        is_dir: false,
        size,
        lossy_name: false,
    }))
}

fn shot(path: &str, size: u64) -> Shot {
    let name = crate::source::remote_basename(path).to_string();
    Shot {
        listed: listed(&name, size),
        at: RemotePath::plain(path),
    }
}

#[test]
fn an_image_offers_view_and_not_edit_and_a_text_the_other_way() {
    use crate::select::{actions_for, Action};
    let img = listed("logo.png", 1000);
    let a = actions_for(&[&img]);
    assert_eq!(
        a.first(),
        Some(&Action::View),
        "图片第一项不是「查看」：{a:?}"
    );
    assert!(!a.contains(&Action::Edit), "图片还给「编辑」：{a:?}");
    let txt = listed("notes.txt", 1000);
    let a = actions_for(&[&txt]);
    assert!(
        a.contains(&Action::Edit) && !a.contains(&Action::View),
        "{a:?}"
    );
    let svg = listed("icon.svg", 1000);
    let a = actions_for(&[&svg]);
    assert!(
        a.contains(&Action::Edit) && !a.contains(&Action::View),
        "svg 不照旧当文本：{a:?}"
    );
    assert_eq!(Action::View.label(1), VIEW_LABEL.as_str());
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x % 255) as u8, (y % 255) as u8, 90, 255])
    })
    .write_to(&mut buf, image::ImageFormat::Png)
    .unwrap();
    buf.into_inner()
}

/// 一个真目录（a.png · b.txt · c.png）＋ 合成后端；窗口列好了它。
async fn rig(tag: &str) -> (Wired, FileWindow, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ccm-fw-imgview-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.png"), png(1600, 1000)).unwrap();
    std::fs::write(dir.join("b.txt"), "hello\n").unwrap();
    std::fs::write(dir.join("c.png"), png(30, 20)).unwrap();
    let be = FakeBackend::new(
        &[
            "files-ls",
            "files-read-text",
            "files-read-chunk",
            "files-stat",
        ],
        Declared::default(),
    );
    let wired = wire_up(tag, be).await;
    let w = window_on(&wired, &dir.to_string_lossy());
    w.reload();
    for _ in guard_core::wall::rounds() {
        if !w.listing.is_loading() && !w.listing.rows.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    *w.listing.error.lock().unwrap() = None;
    (wired, w, dir)
}

fn row_of(w: &FileWindow, name: &str) -> usize {
    let names: Vec<String> = w
        .listing
        .rows
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    names.iter().position(|n| n == name).unwrap_or_else(|| {
        panic!(
            "列表里没有 {name}：{names:?} · {:?}",
            w.listing.error.lock().unwrap()
        )
    })
}

fn chunk_reads(wired: &Wired, from_zero: bool) -> usize {
    wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-read-chunk" && (!from_zero || r["args"]["offset"] == 0))
        .count()
}

struct Drive {
    ctx: egui::Context,
    t: f64,
}

impl Drive {
    fn new() -> Self {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, &crate::theme::testing::default_theme());
        Self { ctx, t: 0.0 }
    }

    fn frame_at(
        &mut self,
        ws: &mut Workspace,
        dt: f64,
        events: Vec<egui::Event>,
    ) -> Vec<PaintedText> {
        self.t += dt;
        let mut all = vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)];
        all.extend(events);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            time: Some(self.t),
            events: all,
            max_texture_side: Some(8192),
            ..Default::default()
        };
        let out = self.ctx.run_ui(input, |ui| ws.frame(ui));
        let painted = text_in_frame(&out);
        out.drop_without_applying_deltas();
        painted
    }

    fn frame(&mut self, ws: &mut Workspace, events: Vec<egui::Event>) -> Vec<PaintedText> {
        self.frame_at(ws, 1.0, events)
    }

    /// 跑到查看页那一张解好为止（带上限）。
    async fn until_ready(&mut self, ws: &mut Workspace) -> Vec<PaintedText> {
        for _ in guard_core::wall::rounds() {
            let painted = self.frame(ws, Vec::new());
            let ready = ws.pane_on(ws.focus()).viewing().is_some_and(|v| {
                matches!(
                    ws.pictures.look(&v.key(), None),
                    Some(crate::picture::Look::Ready(_))
                )
            });
            if ready {
                return self.frame(ws, Vec::new());
            }
            let _ = painted;
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("查看页到兜底上限图还没好");
    }

    /// 真双击：按下松开两次（时间挨着、同一处）。
    fn double_click(&mut self, ws: &mut Workspace, at: egui::Pos2) {
        let b = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame_at(ws, 1.0, vec![egui::Event::PointerMoved(at)]);
        self.frame_at(ws, 0.05, vec![b(true)]);
        self.frame_at(ws, 0.05, vec![b(false)]);
        self.frame_at(ws, 0.05, vec![b(true)]);
        self.frame_at(ws, 0.05, vec![b(false)]);
        self.frame_at(ws, 0.05, Vec::new());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enter_and_double_click_open_a_view_tab_and_again_switch_to_it() {
    let (_wired, w, dir) = rig("fw-iv-open").await;
    let a = row_of(&w, "a.png");
    let mut ws = Workspace::new(w);
    let mut d = Drive::new();
    d.frame(&mut ws, Vec::new());
    // 回车（键盘那一口：光标在 a.png 上）。
    let p = ws.pane_on_mut(0);
    p.apply_intent(
        crate::select::Intent::Edge {
            end: false,
            extend: false,
        },
        0.0,
        None,
    );
    for _ in 0..a {
        p.apply_intent(
            crate::select::Intent::Step {
                by: 1,
                extend: false,
            },
            0.0,
            None,
        );
    }
    d.frame(&mut ws, vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    d.frame(&mut ws, Vec::new());
    assert_eq!(ws.tabs_on(0), 2, "回车没开出查看页");
    assert!(ws.tab(0, 1).view_tab && !ws.tab(0, 0).view_tab);
    assert_eq!(Workspace::tab_title(ws.tab(0, 1)), "a.png");
    assert_eq!(ws.active_on(0), 1, "开完没切过去");
    // 回到目录页，真双击那一行 ⇒ 切回已开着的那一页，不开第二份。
    assert!(ws.select_tab(0, 0));
    let painted = d.frame(&mut ws, Vec::new());
    let at = rects_of(&painted, "a.png")
        .into_iter()
        .find(|r| r.top() > 120.0)
        .expect("列表里没画 a.png")
        .center();
    d.double_click(&mut ws, at);
    assert_eq!(ws.tabs_on(0), 2, "同一张开了第二页");
    assert_eq!(ws.active_on(0), 1, "双击同一张没切过去");
    // 双击文本 ⇒ 编辑页。
    assert!(ws.select_tab(0, 0));
    let painted = d.frame(&mut ws, Vec::new());
    let at = rects_of(&painted, "b.txt")
        .into_iter()
        .find(|r| r.top() > 120.0)
        .expect("列表里没画 b.txt")
        .center();
    d.double_click(&mut ws, at);
    assert_eq!(ws.tabs_on(0), 3, "双击文本没开编辑页");
    assert!(ws.tab(0, 2).edit_tab, "双击文本开的不是编辑页");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn left_and_right_walk_the_images_of_that_folder_and_stop_at_the_ends() {
    let mut w = FileWindow::seeded(
        crate::source::Source::remote(String::from("iv")),
        "/d".into(),
        None,
        vec![
            listed("a.png", 10),
            listed("b.txt", 10),
            listed("c.jpg", 10),
            listed("d.tiff", 10),
        ],
    );
    assert!(w.begin_view(0), "图片开不了");
    assert!(!w.begin_view(1), "文本开成了查看页");
    let v = w.viewing().unwrap().clone();
    assert_eq!(v.place(), (1, 2), "同目录那一摞不是只有那两张图");
    let mut v = v;
    assert!(!v.step(-1), "第一张还能往前翻");
    assert!(v.step(1));
    assert_eq!(v.name(), "c.jpg");
    assert!(!v.step(1), "最后一张绕回去了");
    assert_eq!(v.name(), "c.jpg");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_page_draws_the_picture_and_shares_one_decode_with_the_preview() {
    let (wired, mut w, dir) = rig("fw-iv-share").await;
    let a = row_of(&w, "a.png");
    w.apply_intent(
        crate::select::Intent::Edge {
            end: false,
            extend: false,
        },
        0.0,
        None,
    );
    for _ in 0..a {
        w.apply_intent(
            crate::select::Intent::Step {
                by: 1,
                extend: false,
            },
            0.0,
            None,
        );
    }
    let mut ws = Workspace::new(w);
    ws.set_preview(true);
    let mut d = Drive::new();
    let mut shown = None;
    for _ in guard_core::wall::rounds() {
        d.frame(&mut ws, Vec::new());
        if let Some(crate::preview::View::Image { tex, size, .. }) =
            ws.preview.as_ref().map(|p| p.view().clone())
        {
            shown = Some((tex.id(), size));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let (pv_tex, pv_size) = shown.expect("预览没摆出这张图");
    assert_eq!(pv_size, [1600, 1000], "预览写的不是原图多大");
    // 预览头上点「查看」（同一个执行口）。
    ws.pane_on_mut(0).perform(crate::select::Action::View, None);
    let painted = d.until_ready(&mut ws).await;
    assert_eq!(
        chunk_reads(&wired, true),
        1,
        "同一张图从头读了不止一趟（预览与查看页没共用）"
    );
    let v = ws.pane_on(0).viewing().unwrap().clone();
    let tex = match ws.pictures.look(&v.key(), None) {
        Some(crate::picture::Look::Ready(p)) => p.tex.id(),
        other => panic!("{other:?}"),
    };
    assert_eq!(tex, pv_tex, "查看页与预览摆的不是同一张贴图");
    let texts: Vec<&str> = painted.iter().map(|(t, _)| t.as_str()).collect();
    for want in [
        copy_text("rsFilewinImgview.head.fit", &[]),
        copy_text("rsFilewinImgview.head.full", &[]),
        copy_text("rsFilewinImgview.head.count", &[("i", "1"), ("n", "2")]),
        crate::download::DOWNLOAD_LABEL.to_string(),
        copy_text(
            "rsFilewinImgview.status.metaNoTime",
            &[
                ("w", "1600"),
                ("h", "1000"),
                ("size", &copy_core::size_text(v.shot().listed.row.size)),
            ],
        ),
    ] {
        assert!(
            texts.contains(&want.as_str()),
            "这一帧没画「{want}」：{texts:?}"
        );
    }
    // 适应窗口的百分比：画面比图小 ⇒ 不到 100%。
    assert!(
        texts.iter().any(|t| t.ends_with('%')
            && t.trim_end_matches('%')
                .parse::<u32>()
                .is_ok_and(|n| n < 100)),
        "百分比没画 / 不对：{texts:?}"
    );
    // 关掉预览、关掉查看页 ⇒ 那一张没人摆了，扔掉。
    ws.set_preview(false);
    assert!(ws.close_tab(0, 1), "查看页关不掉");
    d.frame(&mut ws, Vec::new());
    assert!(ws.pictures.is_empty(), "没人摆的图还留着");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zoom_keys_the_wheel_and_dragging_move_the_picture() {
    let (_wired, mut w, dir) = rig("fw-iv-zoom").await;
    let a = row_of(&w, "a.png");
    assert!(w.begin_view(a));
    let mut ws = Workspace::new(w);
    let mut d = Drive::new();
    d.until_ready(&mut ws).await;
    let zoom = |ws: &Workspace| ws.pane_on(0).viewing().unwrap().zoom;
    assert_eq!(zoom(&ws), Zoom::Fit);
    d.frame(&mut ws, vec![key(egui::Key::Plus, egui::Modifiers::NONE)]);
    let Zoom::Free(s) = zoom(&ws) else {
        panic!("+ 没放大：{:?}", zoom(&ws));
    };
    d.frame(&mut ws, vec![key(egui::Key::Num0, egui::Modifiers::NONE)]);
    assert_eq!(zoom(&ws), Zoom::Fit, "0 没回到适应窗口");
    // 滚轮（指针在画面中间）。
    let mid = egui::pos2(640.0, 450.0);
    d.frame(&mut ws, vec![egui::Event::PointerMoved(mid)]);
    d.frame(
        &mut ws,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 50.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        }],
    );
    assert!(
        matches!(zoom(&ws), Zoom::Free(x) if (x - s).abs() < 0.02),
        "滚轮没缩放：{:?}",
        zoom(&ws)
    );
    // 原尺寸（1600 宽 > 画面）下按住拖 ⇒ 平移。
    ws.pane_on_mut(0)
        .viewing
        .as_mut()
        .unwrap()
        .set_zoom(Zoom::Full);
    d.frame(&mut ws, Vec::new());
    let b = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    d.frame_at(&mut ws, 0.05, vec![b(mid, true)]);
    for k in 1..=6 {
        d.frame_at(
            &mut ws,
            0.05,
            vec![egui::Event::PointerMoved(
                mid + egui::vec2(-20.0 * k as f32, 0.0),
            )],
        );
    }
    d.frame_at(&mut ws, 0.05, vec![b(mid + egui::vec2(-120.0, 0.0), false)]);
    let off = ws.pane_on(0).viewing().unwrap().offset;
    assert!(off.x < -50.0, "拖了没平移：{off:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn over_the_cap_says_why_and_reads_nothing() {
    let (wired, mut w, dir) = rig("fw-iv-cap").await;
    let a = row_of(&w, "a.png");
    // 后端那道查看门说不许 ⇒ 摆出那一句、不读。
    let big = 32 * 1024 * 1024 + 1;
    let gated = crate::find::testing::fake_gates("a.png", false, big).2;
    let Some(cmd_wire::files::Gate::No(said)) = gated.as_ref().and_then(|g| g.view.clone()) else {
        panic!("合成后端对 32 M 以上的图该给不许");
    };
    {
        let mut rows = w.listing.rows.lock().unwrap();
        rows[a].row.size = big;
        rows[a].gates = gated;
    }
    assert!(w.begin_view(a));
    let mut ws = Workspace::new(w);
    let mut d = Drive::new();
    let mut painted = Vec::new();
    for _ in 0..5 {
        painted = d.frame(&mut ws, Vec::new());
    }
    assert!(
        !rects_of(&painted, &said).is_empty(),
        "超上限没说为什么：{painted:?}"
    );
    assert!(
        rects_of(&painted, &crate::download::DOWNLOAD_LABEL).len() >= 2,
        "画面上没给「下载」（头条一颗 ＋ 画面一颗）"
    );
    assert_eq!(chunk_reads(&wired, false), 0, "说了不读却读了");
    let _ = std::fs::remove_dir_all(dir);
}
