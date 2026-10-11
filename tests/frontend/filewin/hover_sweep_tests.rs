//! ★ **指针扫全窗，画面不逐帧翻**（「鼠标一动就有概率闪」那一类）。
//!
//! egui 的命中按**上一帧**的控件位置判：哪颗控件只在「悬停」时才登记点击区，它一登记就可能盖住让它出来的那一颗
//! ⇒ 下一帧不悬停 ⇒ 它不登记 ⇒ 再下一帧又悬停 ⇒ … 每帧翻一次。鼠标一动就落到这种点上，看上去就是一闪一闪。
//! 这一类不靠一处一处找：无头跑生产那个 [`Workspace::frame`]，几种常见状态（标签多开 · 双栏 · 右键菜单 · 预览 · 搜索结果）各扫一遍，
//! 指针摆在每个点上连跑几帧，判后两帧画出来的东西（图形 ＋ 文字 ＋ 登记的控件）一模一样。
//!
//! 点从哪来：一张粗网格 ＋ 每颗登记过的控件的四角 · 四边中点 · 中心 · 右端 14px（× 常摆在那）。
//! 扫的过程中新冒出来的控件（悬停才露的那种）也把它的点加进来，再扫。
//!
//! ⚠ 买不到：渲染那一层（显卡 · 合成器 · 重画节奏）的闪 —— 要是这里一处都不翻，那一类得靠帧日志（[`crate::frame_log`]）。

use super::*;
use crate::find::testing::{window_on, wire_up, Declared, FakeBackend};
use std::hash::{Hash, Hasher};

const SCREEN: egui::Vec2 = egui::vec2(1280.0, 800.0);
const GRID: f32 = 24.0;

/// 一帧的指纹：所有图形（层 · 裁剪框 · 外框 · 填色 · 字）＋ 这一帧登记的每颗控件（id · 点击区）。
fn fingerprint(ctx: &egui::Context, out: &egui::FullOutput) -> (u64, Vec<String>) {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let q = |v: f32| (v * 4.0).round() as i64;
    let mut texts = Vec::new();
    fn walk(s: &egui::Shape, h: &mut impl Hasher, texts: &mut Vec<String>, q: &dyn Fn(f32) -> i64) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, h, texts, q)),
            egui::Shape::Text(t) => {
                (q(t.pos.x), q(t.pos.y), t.galley.text()).hash(h);
                texts.push(t.galley.text().to_string());
            }
            egui::Shape::Rect(r) => {
                (
                    q(r.rect.min.x),
                    q(r.rect.min.y),
                    q(r.rect.max.x),
                    q(r.rect.max.y),
                )
                    .hash(h);
                r.fill.to_array().hash(h);
                r.stroke.color.to_array().hash(h);
            }
            other => {
                let b = other.visual_bounding_rect();
                (q(b.min.x), q(b.min.y), q(b.max.x), q(b.max.y)).hash(h);
            }
        }
    }
    for cs in &out.shapes {
        let c = cs.clip_rect;
        (q(c.min.x), q(c.min.y), q(c.max.x), q(c.max.y)).hash(&mut h);
        walk(&cs.shape, &mut h, &mut texts, &q);
    }
    let mut ws: Vec<(egui::Id, egui::Rect)> = ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .layers()
            .flat_map(|(_, rs)| rs.iter().map(|w| (w.id, w.interact_rect)))
            .collect()
    });
    ws.sort_by_key(|(id, _)| id.value());
    for (id, r) in &ws {
        (id.value(), q(r.min.x), q(r.min.y), q(r.max.x), q(r.max.y)).hash(&mut h);
    }
    (h.finish(), texts)
}

/// 这一帧登记了的控件的点击区（只要落在屏内、有面积的）。
fn widget_rects(ctx: &egui::Context) -> Vec<(egui::Id, egui::Rect)> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN);
    ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .layers()
            .flat_map(|(_, rs)| rs.iter().map(|w| (w.id, w.interact_rect)))
            .filter(|(_, r)| r.is_positive() && r.intersects(screen))
            .collect()
    })
}

fn probe_points(r: egui::Rect) -> Vec<egui::Pos2> {
    let r = r.shrink(1.5);
    if !r.is_positive() {
        return vec![r.center()];
    }
    vec![
        r.left_top(),
        r.right_top(),
        r.left_bottom(),
        r.right_bottom(),
        r.center_top(),
        r.center_bottom(),
        r.left_center(),
        r.right_center(),
        r.center(),
        egui::pos2(r.right() - 12.5, r.center().y),
    ]
}

struct Sweep {
    ctx: egui::Context,
    t: f64,
}

impl Sweep {
    fn new() -> Self {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, &crate::theme::testing::default_theme());
        Self { ctx, t: 0.0 }
    }

    fn frame(&mut self, ws: &mut Workspace, events: Vec<egui::Event>) -> (u64, Vec<String>) {
        self.t += 1.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            time: Some(self.t),
            events,
            ..Default::default()
        };
        let out = self.ctx.run_ui(input, |ui| ws.frame(ui));
        let fp = fingerprint(&self.ctx, &out);
        out.drop_without_applying_deltas();
        fp
    }

    /// 扫一遍：回翻了的那几个点（点 ＋ 两帧各多出来的字）。
    fn run(&mut self, scene: &str, ws: &mut Workspace) -> Vec<String> {
        for _ in 0..3 {
            self.frame(ws, vec![egui::Event::PointerGone]);
        }
        let mut queue: Vec<egui::Pos2> = Vec::new();
        let mut y = GRID / 2.0;
        while y < SCREEN.y {
            let mut x = GRID / 2.0;
            while x < SCREEN.x {
                queue.push(egui::pos2(x, y));
                x += GRID;
            }
            y += GRID;
        }
        let mut seen_ids = std::collections::HashSet::new();
        let mut seen_pts = std::collections::HashSet::new();
        let mut add = |rects: Vec<(egui::Id, egui::Rect)>, queue: &mut Vec<egui::Pos2>| {
            for (id, r) in rects {
                if seen_ids.insert((id, (r.min.x as i32, r.min.y as i32, r.max.x as i32))) {
                    queue.extend(probe_points(r));
                }
            }
        };
        add(widget_rects(&self.ctx), &mut queue);
        let mut flips = Vec::new();
        let mut i = 0;
        while i < queue.len() && i < 8000 {
            let p = queue[i];
            i += 1;
            if !seen_pts.insert((p.x as i32 * 2, p.y as i32 * 2))
                || !(0.0..SCREEN.x).contains(&p.x)
                || !(0.0..SCREEN.y).contains(&p.y)
            {
                continue;
            }
            self.frame(ws, vec![egui::Event::PointerMoved(p)]);
            self.frame(ws, Vec::new());
            let a = self.frame(ws, Vec::new());
            let b = self.frame(ws, Vec::new());
            if a.0 != b.0 {
                let only = |x: &Vec<String>, y: &Vec<String>| -> Vec<String> {
                    x.iter().filter(|t| !y.contains(t)).cloned().collect()
                };
                flips.push(format!(
                    "[{scene}] ({:.1},{:.1}) 帧甲独有字 {:?} · 帧乙独有字 {:?}",
                    p.x,
                    p.y,
                    only(&a.1, &b.1),
                    only(&b.1, &a.1)
                ));
            }
            add(widget_rects(&self.ctx), &mut queue);
        }
        assert!(
            i > 500,
            "[{scene}] 只扫了 {i} 个点：量具没摆开（屏是空的？）"
        );
        flips
    }
}

fn row(name: &str, dir: &str, is_dir: bool) -> crate::source::Row {
    crate::source::Row {
        name: name.into(),
        path: format!("{dir}/{name}"),
        is_dir,
        size: 10,
        lossy_name: false,
    }
}

/// 一个不联网的目录视图（行直接喂进去），带两条书签（左栏那一行的 × 也扫到）。
fn pane(dir: &str) -> FileWindow {
    let mut w = FileWindow::seeded(
        crate::source::Source::remote(String::from("sweep")),
        dir.to_string(),
        None,
        vec![
            row("src", dir, true),
            row("Cargo.toml", dir, false),
            row("README.md", dir, false),
            row("notes.txt", dir, false),
        ],
    );
    *w.listing.error.lock().unwrap() = None;
    let file = std::env::temp_dir().join(format!(
        "ccm-filewin-sweep-bm-{}-{}.json",
        std::process::id(),
        dir.replace('/', "_")
    ));
    let _ = std::fs::remove_file(&file);
    let shelf = crate::bookmarks::Shelf::open(Some(file), &crate::source::Origin("sweep".into()));
    assert!(
        shelf.toggle(dir) && shelf.toggle("/etc"),
        "量具：书签没记上 {:?}",
        shelf.notice()
    );
    w.shelf = Some(shelf);
    w
}

/// 一棵真目录 ＋ 合成后端：预览与搜索那两种状态要它答。
async fn wired_pane(tag: &str) -> (crate::find::testing::Wired, FileWindow, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!("ccm-filewin-sweep-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let dir = base.join("work");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("main.rs"),
        "fn main() {\n    println!(\"retry\");\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("retry.rs"), "pub fn retry() {}\n").unwrap();
    std::fs::write(dir.join("src").join("retry_policy.rs"), "// retry\n").unwrap();
    let mut offered = vec!["files-ls", "files-read-text", "files-stat", "files-home"];
    offered.extend_from_slice(crate::find::COMMANDS);
    let be = FakeBackend::new(&offered, Declared::default())
        .homed(&base)
        .preindexed(&base);
    let wired = wire_up(&format!("sweep-{tag}"), be).await;
    let d = dir.to_string_lossy().to_string();
    let mut w = window_on(&wired, &d);
    w.navigate_to(dir.join("src").to_string_lossy().to_string());
    while w.listing.is_loading() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    w.go_back();
    while w.listing.is_loading() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    (wired, w, base)
}

fn no_flips(flips: Vec<String>) {
    assert!(
        flips.is_empty(),
        "指针停着不动，画面却逐帧翻（{} 处；列前 30 处）：\n{}",
        flips.len(),
        flips
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// ★ 单栏、两个标签页（第二个选中）· 左栏两条书签。
#[test]
fn sweeping_two_tabs_never_flips_a_frame() {
    let mut ws = Workspace::new(pane("/srv/data"));
    assert!(ws.add_tab(0, pane("/srv/logs")));
    no_flips(Sweep::new().run("两个标签", &mut ws));
}

/// ★ 双栏，各两个标签页。
#[test]
fn sweeping_two_sides_never_flips_a_frame() {
    let mut ws = Workspace::new(pane("/srv/a"));
    assert!(ws.add_tab(0, pane("/srv/a2")));
    assert!(ws.add_side(pane("/srv/b")));
    assert!(ws.add_tab(1, pane("/srv/b2")));
    no_flips(Sweep::new().run("双栏各两个标签", &mut ws));
}

/// ★ 右键菜单开着（照真的点一下右键）；扫完菜单还开着。
#[test]
fn sweeping_with_the_context_menu_open_never_flips_a_frame() {
    let mut ws = Workspace::new(pane("/srv/m"));
    let mut s = Sweep::new();
    s.frame(&mut ws, Vec::new());
    let at = {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            time: Some(0.5),
            ..Default::default()
        };
        let out = s.ctx.run_ui(input, |ui| ws.frame(ui));
        let t = crate::copy::testing::text_in_frame(&out);
        out.drop_without_applying_deltas();
        crate::copy::testing::rects_of(&t, "README.md")[0].center()
    };
    s.frame(&mut ws, vec![egui::Event::PointerMoved(at)]);
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    s.frame(&mut ws, vec![press(true), press(false)]);
    assert!(ws.pane_on(0).menu().is_some(), "量具：右键没把菜单摆出来");
    let flips = s.run("右键菜单", &mut ws);
    assert!(
        ws.pane_on(0).menu().is_some(),
        "量具：扫的途中菜单收了（扫的不是开着菜单那一形）"
    );
    no_flips(flips);
}

/// ★ 预览开着、选中 main.rs（正文到货之后再扫）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sweeping_with_the_preview_open_never_flips_a_frame() {
    let (_wired, mut w, base) = wired_pane("preview").await;
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
    let mut ws = Workspace::new(w);
    ws.set_preview(true);
    let mut s = Sweep::new();
    for _ in 0..400 {
        s.frame(&mut ws, Vec::new());
        if matches!(
            ws.preview.as_ref().map(|p| p.view()),
            Some(crate::preview::View::Text { .. })
        ) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        matches!(
            ws.preview.as_ref().map(|p| p.view()),
            Some(crate::preview::View::Text { .. })
        ),
        "量具：预览没等到正文 {:?}",
        ws.preview.as_ref().map(|p| p.view())
    );
    let flips = s.run("预览开着", &mut ws);
    let _ = std::fs::remove_dir_all(base);
    no_flips(flips);
}

/// ★ 搜索结果摆着。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sweeping_over_search_results_never_flips_a_frame() {
    let (_wired, mut w, base) = wired_pane("search").await;
    let ctx = egui::Context::default();
    ctx.run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
    let before = w.search.rounds();
    crate::find::testing::type_into_search(&ctx, &mut w, "retry");
    crate::find::testing::settle(&w.search, before, "扫全窗的搜索那一形").await;
    assert!(!w.search.hit_rows().is_empty(), "量具：搜索没出结果");
    let mut ws = Workspace::new(w);
    let flips = Sweep::new().run("搜索结果", &mut ws);
    let _ = std::fs::remove_dir_all(base);
    no_flips(flips);
}

/// ★ 查看页摆着一张图（同目录还有一张 ⇒ 右边那颗「下一张」圆钮悬停才露）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sweeping_over_the_picture_page_never_flips_a_frame() {
    let base = std::env::temp_dir().join(format!("ccm-filewin-sweep-view-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let png = |w: u32, h: u32| {
        let mut buf = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(w, h, image::Rgba([200, 90, 60, 255]))
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    std::fs::write(base.join("a.png"), png(1600, 1000)).unwrap();
    std::fs::write(base.join("b.png"), png(40, 30)).unwrap();
    let be = FakeBackend::new(
        &["files-ls", "files-read-chunk", "files-stat"],
        Declared::default(),
    );
    let wired = wire_up("sweep-view", be).await;
    let d = base.to_string_lossy().to_string();
    let mut w = window_on(&wired, &d);
    w.reload();
    for _ in 0..600 {
        if !w.listing.is_loading() && !w.listing.rows.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let i = w
        .listing
        .rows
        .lock()
        .unwrap()
        .iter()
        .position(|r| r.name == "a.png")
        .expect("量具：列表里没有 a.png");
    assert!(w.begin_view(i), "量具：查看页没开");
    let mut ws = Workspace::new(w);
    let mut s = Sweep::new();
    let ready = |ws: &Workspace| {
        ws.pane_on(ws.focus()).viewing().is_some_and(|v| {
            matches!(
                ws.pictures.look(&v.key(), None),
                Some(crate::picture::Look::Ready(_))
            )
        })
    };
    for _ in 0..600 {
        s.frame(&mut ws, Vec::new());
        if ready(&ws) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(ready(&ws), "量具：查看页没等到图");
    let flips = s.run("查看页", &mut ws);
    let _ = std::fs::remove_dir_all(base);
    no_flips(flips);
}

/// 真窗口台架（不是判据）：开一扇真文件窗口（生产那个 `Workspace` 当 eframe 的 App，开窗参数同 `shell::run`），
/// 帧日志写到 `CCM_FILEWIN_FRAME_LOG`，跑 `CCM_FILEWIN_RIG_SECS` 秒自己关。场景走 `CCM_FILEWIN_RIG_SCENE`
/// （tabs · split · preview · menu）。后端是合成后端（目录是这一格自己铺的占位），不连任何机器。
/// 渲染那一层的读数（OpenGL 的厂商 / 渲染器 / 版本 · 开窗参数里的 vsync）另写一份 `<帧日志>.gl.txt`。
/// 用法：外面模拟鼠标在窗口上乱动，回头读帧日志看鼠标一动时每帧画面变没变、谁请求的重画。拷到别的机器上跑时
/// 带上 `tokens.css` 并设 `CCM_FILEWIN_TOKENS_CSS`（判据用的主题从它解）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn real_window_rig_for_the_frame_log() {
    let need = |k: &str| {
        std::env::var(k).unwrap_or_else(|_| panic!("这一格只给真窗口台架跑：缺环境变量 {k}"))
    };
    let log = std::path::PathBuf::from(need(crate::frame_log::ENV));
    let scene = need("CCM_FILEWIN_RIG_SCENE");
    let secs: u64 = need("CCM_FILEWIN_RIG_SECS").parse().unwrap();
    let (_wired, mut w, base) = wired_pane(&format!("rig-{scene}")).await;
    for n in ["Cargo.toml", "README.md", "notes.txt", "build.log"] {
        std::fs::write(base.join("work").join(n), format!("{n}\n")).unwrap();
    }
    w.reload();
    while w.listing.is_loading() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
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
    if scene == "menu" {
        w.tally.menu_clicked = Some(i);
        w.apply_menu_click(egui::pos2(520.0, 300.0));
    }
    let gl_out = log.with_extension("gl.txt");
    let h = std::thread::spawn(move || {
        struct Rig {
            ws: Workspace,
            gl_out: std::path::PathBuf,
            said: bool,
        }
        impl eframe::App for Rig {
            fn logic(&mut self, ctx: &egui::Context, f: &mut eframe::Frame) {
                eframe::App::logic(&mut self.ws, ctx, f);
            }
            fn ui(&mut self, ui: &mut egui::Ui, f: &mut eframe::Frame) {
                if !self.said {
                    self.said = true;
                    let gl = f.gl().map(|gl| {
                        use eframe::glow::HasContext as _;
                        unsafe {
                            format!(
                                "vendor={} renderer={} version={}",
                                gl.get_parameter_string(eframe::glow::VENDOR),
                                gl.get_parameter_string(eframe::glow::RENDERER),
                                gl.get_parameter_string(eframe::glow::VERSION)
                            )
                        }
                    });
                    let _ = std::fs::write(
                        &self.gl_out,
                        format!(
                            "backend=glow {}\nvsync(glow_options 缺省)={}\nppp={}\n",
                            gl.unwrap_or_else(|| "no-gl".into()),
                            eframe::NativeOptions::default().glow_options.vsync,
                            ui.ctx().pixels_per_point()
                        ),
                    );
                }
                eframe::App::ui(&mut self.ws, ui, f);
            }
            fn clear_color(&self, v: &egui::Visuals) -> [f32; 4] {
                eframe::App::clear_color(&self.ws, v)
            }
        }
        let theme = crate::theme::testing::default_theme();
        // 开窗参数照 `shell::run` 那一处（其余取缺省，与生产一样）。
        let opts = eframe::NativeOptions {
            event_loop_builder: Some(Box::new(crate::platform::any_thread_hook)),
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size(crate::shell::MIN_WINDOW)
                .with_title("filewin-rig"),
            ..Default::default()
        };
        eframe::run_native(
            "filewin-rig",
            opts,
            Box::new(move |cc| {
                crate::theme::install(&cc.egui_ctx, &theme);
                w.font = crate::fonts::FontState::Pending(crate::fonts::install(
                    &cc.egui_ctx,
                    Some(&theme),
                ));
                let mut ws = Workspace::new(w);
                ws.theme = Some(theme);
                match scene.as_str() {
                    "tabs" => {
                        let mut more = pane("/srv/logs");
                        more.shelf = None;
                        assert!(ws.add_tab(0, more));
                        ws.select_tab(0, 0);
                    }
                    "split" => {
                        assert!(ws.set_split(true));
                    }
                    "preview" => ws.set_preview(true),
                    _ => {}
                }
                ws.frame_log = crate::frame_log::FrameLog::to_file(&log).unwrap();
                let ctx = cc.egui_ctx.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(secs));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    ctx.request_repaint();
                });
                Ok(Box::new(Rig {
                    ws,
                    gl_out,
                    said: false,
                }))
            }),
        )
        .unwrap();
    });
    tokio::task::spawn_blocking(move || h.join().unwrap())
        .await
        .unwrap();
    let _ = std::fs::remove_dir_all(base);
}

/// 🔴 **划过一排带提示的行，提示框不一行一行地冒**：指针在第一行的修改时间上停够、它的提示出来之后，
/// 往下一行一行划过（每行 16 毫秒，正常挪鼠标的速度）—— 划的途中别的行的提示一个都不出（出了又收就是一闪一闪）；
/// 停下来够久，停在的那一行的提示照出（正控）。缺省的 egui 有 0.2 秒「刚出过提示 ⇒ 下一个立刻出」的宽限，正是这一形。
#[test]
fn sliding_down_rows_after_a_tooltip_does_not_pop_one_per_row() {
    let mut w = pane("/srv/tips");
    let names: Vec<String> = (0..8).map(|k| format!("f{k}.txt")).collect();
    *w.listing.rows.lock().unwrap() = names
        .iter()
        .enumerate()
        .map(|(k, n)| {
            let mut l: crate::source::Listed = row(n, "/srv/tips", false).into();
            l.mtime_secs = Some(1_000 + k as u64);
            l.mtime_text = Some(format!("10:0{k}"));
            l.mtime_full = Some(format!("全时刻-{k}"));
            l
        })
        .collect();
    let mut ws = Workspace::new(w);
    let ctx = egui::Context::default();
    crate::theme::install(&ctx, &crate::theme::testing::default_theme());
    let mut t = 0.0;
    let mut frame = |ws: &mut Workspace, at: Option<egui::Pos2>, dt: f64| {
        t += dt;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            time: Some(t),
            events: at
                .map(|p| vec![egui::Event::PointerMoved(p)])
                .unwrap_or_default(),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| ws.frame(ui));
        let painted = crate::copy::testing::text_in_frame(&out);
        out.drop_without_applying_deltas();
        painted
    };
    let painted = frame(&mut ws, None, 0.016);
    let painted = if painted.is_empty() {
        frame(&mut ws, None, 0.016)
    } else {
        painted
    };
    let cells: Vec<egui::Rect> = (0..8)
        .map(|k| crate::copy::testing::rects_of(&painted, &format!("10:0{k}"))[0])
        .collect();
    let tip_up = |p: &[crate::copy::testing::PaintedText], k: usize| {
        p.iter().any(|(s, _)| s == &format!("全时刻-{k}"))
    };
    // 停在第一行的修改时间上，等它的提示出来（量具自检）。
    let mut shown = false;
    frame(&mut ws, Some(cells[0].center()), 0.016);
    for _ in 0..80 {
        if tip_up(&frame(&mut ws, None, 0.016), 0) {
            shown = true;
            break;
        }
    }
    assert!(shown, "量具：停够了第一行的提示也没出来");
    // 往下划：每行一帧，16 毫秒一步。
    let mut popped = Vec::new();
    for k in 1..7 {
        // 一行里挪三步（每步 16 毫秒），指针一直在动。
        for dx in [-6.0, 0.0, 6.0] {
            let p = frame(
                &mut ws,
                Some(cells[k].center() + egui::vec2(dx, 0.0)),
                0.016,
            );
            if (1..8).any(|j| tip_up(&p, j)) {
                popped.push(k);
            }
        }
    }
    assert!(
        popped.is_empty(),
        "划过的那几行一行冒一个提示（出了又收，看着就是闪）：第 {popped:?} 行"
    );
    // 正控：停在第 6 行够久 ⇒ 它的提示照出。
    let mut later = false;
    for _ in 0..80 {
        if tip_up(&frame(&mut ws, None, 0.016), 6) {
            later = true;
            break;
        }
    }
    assert!(later, "停下来够久，提示却不出了");
}

/// 🔴 **等提示出来的那半秒不白转满帧**：指针挪到一格带提示的修改时间上、停住，提示出来之前那几帧
/// 要的下一次重画是「到点那一下」（几百毫秒后），不是「马上再画」（那就是按显示器刷新率空转）。
#[test]
fn waiting_for_a_tooltip_does_not_spin_frames() {
    let mut w = pane("/srv/spin");
    *w.listing.rows.lock().unwrap() = vec![{
        let mut l: crate::source::Listed = row("a.txt", "/srv/spin", false).into();
        l.mtime_secs = Some(1_000);
        l.mtime_text = Some("10:00".into());
        l.mtime_full = Some("全时刻".into());
        l
    }];
    let mut ws = Workspace::new(w);
    let ctx = egui::Context::default();
    crate::theme::install(&ctx, &crate::theme::testing::default_theme());
    let mut t = 0.0;
    let mut frame = |ws: &mut Workspace, at: Option<egui::Pos2>| {
        t += 0.016;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            time: Some(t),
            events: at
                .map(|p| vec![egui::Event::PointerMoved(p)])
                .unwrap_or_default(),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| ws.frame(ui));
        let painted = crate::copy::testing::text_in_frame(&out);
        let delay = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        out.drop_without_applying_deltas();
        (painted, delay)
    };
    frame(&mut ws, None);
    let (painted, _) = frame(&mut ws, None);
    let cell = crate::copy::testing::rects_of(&painted, "10:00")[0];
    frame(&mut ws, Some(cell.center() - egui::vec2(4.0, 0.0)));
    frame(&mut ws, Some(cell.center()));
    // 停住之后的几帧（提示还没到点）：下一次重画至少隔 100 毫秒。
    let mut delays = Vec::new();
    for _ in 0..6 {
        let (p, d) = frame(&mut ws, None);
        assert!(
            !p.iter().any(|(s, _)| s == "全时刻"),
            "量具：提示不该这么快出来"
        );
        delays.push(d);
    }
    assert!(
        delays
            .iter()
            .skip(2)
            .all(|d| *d >= std::time::Duration::from_millis(100)),
        "等提示的那几帧每帧都要马上再画（空转满帧）：{delays:?}"
    );
}
