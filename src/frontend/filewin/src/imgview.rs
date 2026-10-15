//! **图片查看页**：图片开成当前这一栏里的一个标签（同编辑页），看法有适应窗口 · 原尺寸 · 缩放 · 拖动 · 翻同目录的上一张下一张。
//!
//! 这里只留落地要知道的。
//!
//! # 一、形状：查看页就是一个 `FileWindow` 标签（同编辑页）
//!
//! 目录页上双击 / 回车 / 右键「查看」/ 预览头「查看」⇒ [`super::shell::FileWindow::begin_view`]：
//! 挂在窗口里 ⇒ 交给窗口那一级（`want_view`），同一栏里已有一页正摆着这一张 ⇒ 切过去，否则开一页；
//! 单独建的目录视图（判据那一形）⇒ 就地摆。翻页在同一个标签里换图。
//!
//! # 二、读：与预览同一份（[`super::picture`]）
//!
//! 上限照列表那一行上后端给的查看那道门（列表那一行就超了 ⇒ 不读、摆出后端那一句）；解出来的那一张与预览共用。
//!
//! # 三、几何：以「原图像素」算
//!
//! `scale` ＝ 屏上**物理**像素 / 原图像素：1 ＝ 原尺寸（100%）。画出来多大（点）＝ 原图 × scale ÷ `pixels_per_point`。
//! `offset` ＝ 图的中心离画面中心多远（点）。比画面大的那个方向能拖、拖不出空白；比画面小的方向始终居中。
//!
//! # ⚠ 买不到什么
//!
//! - 动图只摆第一帧；EXIF 方向不转；不能旋转 · 裁 · 存回去。
//! - 同目录那一摞是开页那一刻取的，之后目录变了不跟。

use copy_core::copy_text;

use super::picture::{Look, PicKey};
use super::source::{Listed, RemotePath};

/// 缩放的下限 · 上限（`scale`，1 ＝ 原尺寸）。
pub const SCALE_MIN: f32 = 0.05;
pub const SCALE_MAX: f32 = 16.0;
/// 一步放大多少（滚轮一格 · + −）。
pub const STEP: f32 = 1.25;

/// 怎么看。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// 整张放进画面（小图不放大）。
    Fit,
    /// 图上一个像素 ＝ 屏上一个物理像素。
    Full,
    /// 缩放过（两颗开关都不亮）。
    Free(f32),
}

/// 同目录那一摞里的一张。
#[derive(Clone, Debug)]
pub struct Shot {
    pub listed: Listed,
    /// 发出去的路径（当前目录有损时按字节）。
    pub at: RemotePath,
}

/// 一个查看页的状态（**UI 线程自己的**）。
#[derive(Clone, Debug)]
pub struct Viewer {
    list: Vec<Shot>,
    index: usize,
    pub zoom: Zoom,
    /// 图的中心离画面中心多远（点）。
    pub offset: egui::Vec2,
}

/// 能开查看页的那一行：后端给了查看那道门（解得开的图）· 名字是合法 UTF-8。
pub fn viewable(r: &Listed) -> bool {
    !r.opens_as_dir() && !r.lossy_name && view_gate(r).is_some()
}

/// 那一行的查看那道门（后端判的；不是解得开的图 ⇒ `None`）。
fn view_gate(r: &Listed) -> Option<&cmd_wire::files::Gate> {
    r.gates.as_ref()?.view.as_ref()
}

impl Viewer {
    /// 开在 `list` 的第 `index` 张（越界 ⇒ 最后一张）。`list` 空 ⇒ `None`。
    pub fn open(list: Vec<Shot>, index: usize) -> Option<Self> {
        if list.is_empty() {
            return None;
        }
        let index = index.min(list.len() - 1);
        Some(Self {
            list,
            index,
            zoom: Zoom::Fit,
            offset: egui::Vec2::ZERO,
        })
    }

    /// 摆着的那一张。
    pub fn shot(&self) -> &Shot {
        &self.list[self.index]
    }

    /// 摆着的那一张的路径。
    pub fn path(&self) -> &str {
        &self.shot().listed.row.path
    }

    /// 摆着的那一张的名字（标签名）。
    pub fn name(&self) -> &str {
        &self.shot().listed.row.name
    }

    /// 那一张的钥匙（与预览共用那一份）。
    pub fn key(&self) -> PicKey {
        PicKey::of(&self.shot().listed)
    }

    /// 第几张（从 1 起）· 一共几张。
    pub fn place(&self) -> (usize, usize) {
        (self.index + 1, self.list.len())
    }

    pub fn has_prev(&self) -> bool {
        self.index > 0
    }

    pub fn has_next(&self) -> bool {
        self.index + 1 < self.list.len()
    }

    /// 翻一张（`by` ＝ -1 上一张 · 1 下一张）；到头不绕回。翻了 ⇒ 回到适应窗口。回值 ＝ 真的翻了。
    pub fn step(&mut self, by: isize) -> bool {
        let to = self.index as isize + by;
        if to < 0 || to as usize >= self.list.len() {
            return false;
        }
        self.index = to as usize;
        self.zoom = Zoom::Fit;
        self.offset = egui::Vec2::ZERO;
        true
    }

    /// 列表那一行就超了上限 ⇒ 后端写好的那一句（不读）。
    pub fn refused(&self) -> Option<String> {
        view_gate(&self.shot().listed)?.refused().map(str::to_string)
    }

    /// 读这一张许读多少（后端给的）。
    fn cap(&self) -> u64 {
        view_gate(&self.shot().listed)
            .and_then(cmd_wire::files::Gate::cap)
            .unwrap_or(0)
    }

    /// 此刻的 `scale`（画面 `view` 点 · `ppp` 每点几个物理像素）。
    pub fn scale(&self, native: [u32; 2], view: egui::Vec2, ppp: f32) -> f32 {
        match self.zoom {
            Zoom::Fit => fit_scale(native, view, ppp),
            Zoom::Full => 1.0,
            Zoom::Free(s) => s,
        }
    }

    /// 以画面里 `at`（离画面中心多远，点）为不动点，缩放 `steps` 步（正 ＝ 放大）。
    pub fn zoom_by(
        &mut self,
        steps: f32,
        at: egui::Vec2,
        native: [u32; 2],
        view: egui::Vec2,
        ppp: f32,
    ) {
        let old = self.scale(native, view, ppp);
        let new = (old * STEP.powf(steps)).clamp(SCALE_MIN, SCALE_MAX);
        self.offset = zoom_about(self.offset, at, old, new);
        self.zoom = Zoom::Free(new);
        self.offset = clamp_offset(self.offset, drawn(native, new, ppp), view);
    }

    /// 换一种看法（适应窗口 / 原尺寸）：从中心开始。
    pub fn set_zoom(&mut self, z: Zoom) {
        self.zoom = z;
        self.offset = egui::Vec2::ZERO;
    }

    /// 拖了 `delta`（点）。
    pub fn pan(&mut self, delta: egui::Vec2, native: [u32; 2], view: egui::Vec2, ppp: f32) {
        let s = self.scale(native, view, ppp);
        self.offset = clamp_offset(self.offset + delta, drawn(native, s, ppp), view);
    }
}

/// 适应窗口的 `scale`：整张放进画面，小图不放大（≤ 1）。
pub fn fit_scale(native: [u32; 2], view: egui::Vec2, ppp: f32) -> f32 {
    let (w, h) = (native[0].max(1) as f32, native[1].max(1) as f32);
    (view.x * ppp / w)
        .min(view.y * ppp / h)
        .clamp(SCALE_MIN, 1.0)
}

/// 按 `scale` 画出来多大（点）。
pub fn drawn(native: [u32; 2], scale: f32, ppp: f32) -> egui::Vec2 {
    egui::vec2(native[0] as f32, native[1] as f32) * scale / ppp.max(0.01)
}

/// 缩放之后的 `offset`：`at`（离画面中心，点）底下那一点不动。
pub fn zoom_about(offset: egui::Vec2, at: egui::Vec2, old: f32, new: f32) -> egui::Vec2 {
    at - (at - offset) * (new / old.max(f32::MIN_POSITIVE))
}

/// 拖不出空白：比画面大的方向，边到画面边为止；比画面小的方向居中。
pub fn clamp_offset(offset: egui::Vec2, drawn: egui::Vec2, view: egui::Vec2) -> egui::Vec2 {
    let axis = |o: f32, d: f32, v: f32| {
        let room = ((d - v) / 2.0).max(0.0);
        o.clamp(-room, room)
    };
    egui::vec2(
        axis(offset.x, drawn.x, view.x),
        axis(offset.y, drawn.y, view.y),
    )
}

/// 百分比（原尺寸 ＝ 100）。
pub fn percent(scale: f32) -> u32 {
    (scale * 100.0).round() as u32
}

/// 查看页头条与画面上点了什么（交回那一页做）。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Acts {
    pub step: isize,
    pub zoom: Option<Zoom>,
    /// 缩放几步（以画面中心为不动点；滚轮另算）。
    pub steps: f32,
    pub download: bool,
    pub retry: bool,
}

/// 「查看」那一项的字（菜单 · 预览头）。
pub static VIEW_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinImgview.label.view", &[]));

/// 查看页这一页的键（这一栏有焦点、没在输入框里）：← → 翻页 · + − 缩放 · 0 回到适应窗口。
/// Ctrl 组合键是整窗缩放（不抢）。
pub fn keys(events: &[egui::Event]) -> Acts {
    let mut a = Acts::default();
    for e in events {
        if let egui::Event::Key {
            key,
            pressed: true,
            modifiers: m,
            ..
        } = e
        {
            if m.command || m.alt || m.ctrl {
                continue;
            }
            match key {
                egui::Key::ArrowLeft => a.step -= 1,
                egui::Key::ArrowRight => a.step += 1,
                egui::Key::Plus | egui::Key::Equals => a.steps += 1.0,
                egui::Key::Minus => a.steps -= 1.0,
                egui::Key::Num0 => a.zoom = Some(Zoom::Fit),
                _ => {}
            }
        }
    }
    a
}

/// 这一帧滚轮滚了几格（向上 ＝ 放大）：一行一格、50 点一格、一页四格；带 Ctrl 的是整窗缩放，不算。
pub fn wheel_steps(events: &[egui::Event]) -> f32 {
    let mut n = 0.0;
    for e in events {
        if let egui::Event::MouseWheel {
            unit,
            delta,
            modifiers,
            ..
        } = e
        {
            if modifiers.command || modifiers.ctrl {
                continue;
            }
            n += match unit {
                egui::MouseWheelUnit::Line => delta.y,
                egui::MouseWheelUnit::Point => delta.y / 50.0,
                egui::MouseWheelUnit::Page => delta.y * 4.0,
            };
        }
    }
    n.clamp(-4.0, 4.0)
}

/// 头条右半（从右往左摆）：下载 ｜ + 百分比 − ｜ 原尺寸 适应窗口 ｜ › i / n ‹。
pub fn head_acts(ui: &mut egui::Ui, v: &Viewer, pct: Option<u32>, acts: &mut Acts) {
    use egui_phosphor::regular as ph;
    let p = super::theme::palette(ui.ctx());
    if super::kit::ghost(
        ui,
        ph::DOWNLOAD_SIMPLE,
        &super::download::DOWNLOAD_LABEL,
        true,
        "",
    )
    .clicked()
    {
        acts.download = true;
    }
    ui.separator();
    let ready = pct.is_some();
    if super::kit::ghost(ui, ph::PLUS, "", ready, "")
        .on_hover_text(copy_text("rsFilewinImgview.head.zoomIn", &[]))
        .clicked()
    {
        acts.steps += 1.0;
    }
    let shown = pct.map_or_else(String::new, |n| {
        copy_text("rsFilewinImgview.head.percent", &[("p", &n.to_string())])
    });
    ui.add_sized(
        [44.0, ui.available_height()],
        egui::Label::new(egui::RichText::new(shown).color(p.text).small()),
    );
    if super::kit::ghost(ui, ph::MINUS, "", ready, "")
        .on_hover_text(copy_text("rsFilewinImgview.head.zoomOut", &[]))
        .clicked()
    {
        acts.steps -= 1.0;
    }
    ui.separator();
    if super::kit::toggle(
        ui,
        "",
        &copy_text("rsFilewinImgview.head.full", &[]),
        v.zoom == Zoom::Full,
    )
    .clicked()
    {
        acts.zoom = Some(Zoom::Full);
    }
    if super::kit::toggle(
        ui,
        "",
        &copy_text("rsFilewinImgview.head.fit", &[]),
        v.zoom == Zoom::Fit,
    )
    .clicked()
    {
        acts.zoom = Some(Zoom::Fit);
    }
    let (i, n) = v.place();
    if n > 1 {
        ui.separator();
        if super::kit::ghost(
            ui,
            ph::ARROW_RIGHT,
            "",
            v.has_next(),
            &copy_text("rsFilewinImgview.head.last", &[]),
        )
        .on_hover_text(copy_text("rsFilewinImgview.head.next", &[]))
        .clicked()
        {
            acts.step += 1;
        }
        ui.label(
            egui::RichText::new(copy_text(
                "rsFilewinImgview.head.count",
                &[("i", &i.to_string()), ("n", &n.to_string())],
            ))
            .color(p.text2)
            .small(),
        );
        if super::kit::ghost(
            ui,
            ph::ARROW_LEFT,
            "",
            v.has_prev(),
            &copy_text("rsFilewinImgview.head.first", &[]),
        )
        .on_hover_text(copy_text("rsFilewinImgview.head.prev", &[]))
        .clicked()
        {
            acts.step -= 1;
        }
    }
}

/// 状态栏那一行：`1024 × 768 · 23.4 KB · 09-21`（尺寸要等图解好）；缩过 ⇒ 第二段「原图 … · 按 … 显示」。
pub fn status(v: &Viewer, pic: Option<&super::picture::Pic>) -> (String, Option<String>) {
    let r = &v.shot().listed;
    let size = copy_core::size_text(r.row.size);
    let time = r.mtime_text.clone();
    let line = match (pic, &time) {
        (Some(p), Some(t)) => copy_text(
            "rsFilewinImgview.status.meta",
            &[
                ("w", &p.native[0].to_string()),
                ("h", &p.native[1].to_string()),
                ("size", &size),
                ("time", t),
            ],
        ),
        (Some(p), None) => copy_text(
            "rsFilewinImgview.status.metaNoTime",
            &[
                ("w", &p.native[0].to_string()),
                ("h", &p.native[1].to_string()),
                ("size", &size),
            ],
        ),
        (None, Some(t)) => copy_text("rsFilewinPreview.ui.meta", &[("size", &size), ("time", t)]),
        (None, None) => size,
    };
    let shrunk = pic
        .filter(|p| p.shown[0] as u32 != p.native[0] || p.shown[1] as u32 != p.native[1])
        .map(|p| {
            copy_text(
                "rsFilewinImgview.status.shrunk",
                &[
                    ("w", &p.native[0].to_string()),
                    ("h", &p.native[1].to_string()),
                    ("dw", &p.shown[0].to_string()),
                    ("dh", &p.shown[1].to_string()),
                ],
            )
        });
    (line, shrunk)
}

/// 透明处的棋盘格：一张 2 × 2 的小贴图（`--bg` 与 `--card` 两色）铺着画（一格 8 点），整扇窗只上传一次。
fn checker(ctx: &egui::Context) -> egui::TextureId {
    let id = egui::Id::new("filewin-imgview-checker");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t.id();
    }
    let p = super::theme::palette(ctx);
    let img = egui::ColorImage::new([2, 2], vec![p.bg, p.card, p.card, p.bg]);
    let t = ctx.load_texture(
        "filewin-imgview-checker",
        img,
        egui::TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: egui::TextureFilter::Nearest,
            wrap_mode: egui::TextureWrapMode::Repeat,
            mipmap_mode: None,
        },
    );
    let tid = t.id();
    ctx.data_mut(|d| d.insert_temp(id, t));
    tid
}

/// 画面：底 `--bg-2`；按此刻那一张怎么样摆（在读 · 没成 · 超上限 · 图）。图比画面大能拖、滚轮以指针为中心缩放；
/// 左右两颗圆钮指针在画面上才露（[`super::kit::hover_reveal`]）。
pub fn canvas(
    ui: &mut egui::Ui,
    v: &mut Viewer,
    look: Option<&Look>,
    refused: Option<&str>,
    acts: &mut Acts,
) {
    use egui_phosphor::regular as ph;
    let p = super::theme::palette(ui.ctx());
    let rect = ui.available_rect_before_wrap();
    let resp = ui.interact(
        rect,
        ui.id().with("filewin-imgview-canvas"),
        egui::Sense::click_and_drag(),
    );
    ui.painter().rect_filled(rect, 0.0, p.bg2);
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    match (refused, look) {
        (Some(why), _) => {
            if super::kit::empty_state(
                &mut body,
                ph::IMAGE,
                why,
                &[super::download::DOWNLOAD_LABEL.to_string()],
            )
            .is_some()
            {
                acts.download = true;
            }
        }
        (None, Some(Look::Failed(why))) => {
            let said = copy_text(
                "rsFilewinImgview.state.failed",
                &[("name", v.name()), ("why", why)],
            );
            match super::kit::empty_state(
                &mut body,
                ph::IMAGE_BROKEN,
                &said,
                &[
                    super::download::DOWNLOAD_LABEL.to_string(),
                    copy_text("rsFilewinEditPage.action.retry", &[]),
                ],
            ) {
                Some(0) => acts.download = true,
                Some(1) => acts.retry = true,
                _ => {}
            }
        }
        (None, Some(Look::Ready(pic))) => {
            let ppp = ui.ctx().pixels_per_point();
            let view = rect.size();
            let native = pic.native;
            // 滚轮（不带 Ctrl：Ctrl + 滚轮是整窗缩放）以指针为中心。
            if resp.hovered() {
                let steps = ui.input(|i| wheel_steps(&i.events));
                if steps != 0.0 {
                    let at = ui
                        .input(|i| i.pointer.hover_pos())
                        .map_or(egui::Vec2::ZERO, |h| h - rect.center());
                    v.zoom_by(steps, at, native, view, ppp);
                }
            }
            let s = v.scale(native, view, ppp);
            let size = drawn(native, s, ppp);
            let movable = size.x > view.x + 0.5 || size.y > view.y + 0.5;
            if resp.dragged() && movable {
                v.pan(resp.drag_delta(), native, view, ppp);
            }
            v.offset = clamp_offset(v.offset, size, view);
            let at = egui::Rect::from_center_size(rect.center() + v.offset, size);
            // 只画在画面里（拖出去的那一截裁掉）：棋盘格铺在图底下（透明处露出来），再画图。
            let mut clip = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            clip.set_clip_rect(rect);
            let cells = at.size() / 16.0;
            egui::Image::new((checker(ui.ctx()), at.size()))
                .uv(egui::Rect::from_min_size(egui::Pos2::ZERO, cells))
                .paint_at(&clip, at);
            egui::Image::new((pic.tex.id(), at.size())).paint_at(&clip, at);
            if movable {
                if resp.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                } else if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
        }
        (None, _) => {
            let (got, total) = match look {
                Some(Look::Reading { got, total }) => (*got, *total),
                _ => (0, v.shot().listed.row.size),
            };
            body.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.4).max(24.0));
                ui.spinner();
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(copy_text(
                        "rsFilewinImgview.state.reading",
                        &[
                            ("name", v.name()),
                            ("got", &copy_core::size_text(got)),
                            ("total", &copy_core::size_text(total)),
                        ],
                    ))
                    .color(p.text2),
                );
                ui.add_space(6.0);
                super::kit::progress(ui, 240.0, (total > 0).then(|| got as f32 / total as f32));
            });
        }
    }
    // 左右两颗圆钮：点击区每帧都登记，指针在画面上才画（hover_reveal）；那一边没有就不登记。
    for (by, on, icon, side) in [
        (
            -1isize,
            v.has_prev(),
            ph::ARROW_LEFT,
            rect.left() + 12.0 + 16.0,
        ),
        (1, v.has_next(), ph::ARROW_RIGHT, rect.right() - 12.0 - 16.0),
    ] {
        if !on {
            continue;
        }
        let r =
            egui::Rect::from_center_size(egui::pos2(side, rect.center().y), egui::vec2(32.0, 32.0));
        if let Some(b) = super::kit::hover_reveal(ui, &resp, r, resp.id.with(("nav", by)), false) {
            let fill = if b.hovered() {
                p.card
            } else {
                egui::Color32::from_black_alpha(140)
            };
            ui.painter().circle_filled(r.center(), 16.0, fill);
            ui.painter()
                .circle_stroke(r.center(), 16.0, egui::Stroke::new(1.0, p.border));
            ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                icon,
                egui::FontId::proportional(16.0),
                p.text,
            );
            if b.clicked() {
                acts.step += by;
            }
        }
    }
    ui.advance_cursor_after_rect(rect);
}

impl super::shell::FileWindow {
    /// 这一页正文是查看页（查看页标签，或单独建的视图正摆着一张）。
    pub fn showing_picture(&self) -> bool {
        self.view_tab || self.viewing.is_some()
    }

    /// 摆着的那一张（判据与界面看同一个值）。
    pub fn viewing(&self) -> Option<&Viewer> {
        self.viewing.as_ref()
    }

    /// 起一个查看页看第 `i` 行（那一行得是 [`viewable`]）。同目录那一摞按列表此刻的顺序取（只取能看的那几张）。
    /// 挂在窗口里的目录页 ⇒ 交给窗口那一级（开一页 / 切到正摆着它的那一页）；单独的视图 ⇒ 就地摆。回值 ＝ 真的要开。
    pub fn begin_view(&mut self, i: usize) -> bool {
        let (list, at) = {
            let rows = self.listing.rows.lock().unwrap();
            if !rows.get(i).is_some_and(viewable) {
                return false;
            }
            let mut at = 0;
            let mut list = Vec::new();
            for (k, r) in rows.iter().enumerate() {
                if !viewable(r) {
                    continue;
                }
                if k == i {
                    at = list.len();
                }
                list.push(Shot {
                    listed: r.clone(),
                    at: self.row_path(r),
                });
            }
            (list, at)
        };
        let Some(v) = Viewer::open(list, at) else {
            return false;
        };
        if self.in_workspace && !self.view_tab {
            self.want_view = Some(v);
        } else {
            self.viewing = Some(v);
        }
        true
    }

    /// 此刻还摆着的那一张的钥匙（窗口每帧报给 [`super::picture::Pictures::keep_only`]）。
    pub fn picture_key(&self) -> Option<PicKey> {
        self.viewing.as_ref().map(Viewer::key)
    }

    /// 查看页在前台时状态栏那一行（尺寸 · 大小 · 修改时间；缩过 ⇒ 第二段）。
    pub fn view_status(&self) -> Option<(String, Option<String>)> {
        let v = self.viewing.as_ref()?;
        let pic = match self.pictures.look(&v.key(), None) {
            Some(Look::Ready(p)) => Some(p),
            _ => None,
        };
        Some(status(v, pic.as_ref()))
    }

    /// 没读过 / 被扔了 ⇒ 起一趟读（与预览共用那一份）。
    fn want_picture(&self, ctx: &egui::Context) {
        let Some(v) = self.viewing.as_ref() else {
            return;
        };
        if v.refused().is_some() || self.pictures.look(&v.key(), None).is_some() {
            return;
        }
        let (Some(rt), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            return;
        };
        let side = ctx
            .input(|i| i.max_texture_side)
            .min(super::picture::MAX_SIDE as usize) as u32;
        self.pictures.load(
            &v.key(),
            super::picture::Load {
                rt,
                line,
                origin: self.source.origin(),
                wire: v.shot().at.path(),
                cap: v.cap(),
                side,
                ctx: Some(ctx.clone()),
            },
        );
    }

    /// 查看页这一帧：键 → 头条（面包屑 · 翻页 · 看法 · 缩放 · 下载）→ 画面。
    pub(super) fn view_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.viewing.is_none() {
            return;
        }
        self.want_picture(&ctx);
        let typing = ctx.egui_wants_keyboard_input();
        let mut acts = if self.focused && !typing {
            keys(&ctx.input(|i| i.events.clone()))
        } else {
            Acts::default()
        };
        let Some(v) = self.viewing.clone() else {
            return;
        };
        let look = self.pictures.look(&v.key(), Some(&ctx));
        let native = match &look {
            Some(Look::Ready(p)) => Some(p.native),
            _ => None,
        };
        let canvas_size = {
            let r = ui.available_rect_before_wrap();
            egui::vec2(r.width(), (r.height() - super::kit::STRIP_H).max(1.0))
        };
        let ppp = ctx.pixels_per_point();
        let pct = native.map(|n| percent(v.scale(n, canvas_size, ppp)));
        let mut go = None;
        super::kit::strip(ui, |ui| {
            go = self.path_crumbs(ui, v.path());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                head_acts(ui, &v, pct, &mut acts);
            });
        });
        if let Some(d) = go {
            self.want_dir = Some(d);
        }
        let refused = v.refused();
        if let Some(vm) = self.viewing.as_mut() {
            canvas(ui, vm, look.as_ref(), refused.as_deref(), &mut acts);
            if let Some(n) = native {
                if acts.steps != 0.0 {
                    vm.zoom_by(acts.steps, egui::Vec2::ZERO, n, canvas_size, ppp);
                }
            }
            if let Some(z) = acts.zoom {
                vm.set_zoom(z);
            }
            if acts.step != 0 {
                vm.step(acts.step.signum());
            }
        }
        if acts.retry {
            self.pictures.forget(&v.key());
        }
        if acts.download {
            self.begin_pull_row(v.shot().listed.clone());
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/imgview_tests.rs"]
mod tests;
