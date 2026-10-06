//! 窗口里的通用件（与网页那一侧 `ui/kit/` 同一套规范、同一份令牌）：条 · 状态行 · 加底色的字。
//! 窗口里的这几样只许在这里建，各处调它，不各画各的。

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Ui};

use copy_core::copy_text;

use super::theme::{metrics, palette};

/// 条的语气（规范 `C15`）：出错 · 警告 · 说明。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Error,
    Warn,
    Info,
}

impl Tone {
    fn color(self, ui: &Ui) -> Color32 {
        let p = palette(ui.ctx());
        match self {
            Self::Error => p.error,
            Self::Warn => p.warn,
            Self::Info => p.text2,
        }
    }

    fn icon(self) -> &'static str {
        use egui_phosphor::regular as ph;
        match self {
            Self::Error => ph::WARNING_CIRCLE,
            Self::Warn => ph::WARNING,
            Self::Info => ph::INFO,
        }
    }
}

/// 一条横贯的条（规范 `C15`）：左 3px 语气色 ＋ 图标 ＋ 一句话 ＋ 右端几个动作。回这一帧点了第几个动作。
pub fn banner(ui: &mut Ui, tone: Tone, text: &str, actions: &[String]) -> Option<usize> {
    let p = palette(ui.ctx());
    let m = metrics(ui.ctx());
    let c = tone.color(ui);
    let mut hit = None;
    let r = egui::Frame::new()
        .fill(c.gamma_multiply(0.12))
        .inner_margin(egui::Margin::symmetric(m.space[4] as i8, m.space[2] as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tone.icon()).color(c));
                // 动作贴右端先摆，那句话占剩下的宽、放不下截成「…」（悬停看全句）—— 窄窗里不把这一栏撑宽。
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (i, a) in actions.iter().enumerate().rev() {
                        if ui.button(a).clicked() {
                            hit = Some(i);
                        }
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(text).color(p.text)).truncate(),
                        )
                        .on_hover_text(text);
                    });
                });
            });
        });
    let rect = r.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
        0.0,
        c,
    );
    hit
}

/// 状态行（`--bg-2` 底、28 高）：左右两段内容由调用方摆。
pub fn strip<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = palette(ui.ctx());
    let m = metrics(ui.ctx());
    egui::Frame::new()
        .fill(p.bg2)
        .inner_margin(egui::Margin::symmetric(m.space[4] as i8, 0))
        .show(ui, |ui| {
            let w = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(w, STRIP_H),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_size(egui::vec2(w, STRIP_H));
                    add(ui)
                },
            )
            .inner
        })
        .inner
}

/// 状态行的高。
pub const STRIP_H: f32 = 28.0;

/// 一段字，`marks`（字节区间）那几段加命中底色（规范「搜索命中」那一格）。区间对不上字的边界 ⇒ 整段不加。
pub fn marked(ui: &Ui, text: &str, marks: &[(usize, usize)], color: Color32) -> LayoutJob {
    let p = palette(ui.ctx());
    let font = egui::TextStyle::Body.resolve(ui.style());
    let plain = TextFormat::simple(font.clone(), color);
    let hit = TextFormat {
        background: p.hit,
        ..TextFormat::simple(font, color)
    };
    let ok = marks.iter().all(|&(a, b)| {
        a < b && b <= text.len() && text.is_char_boundary(a) && text.is_char_boundary(b)
    });
    let mut job = LayoutJob::default();
    let mut at = 0;
    if ok {
        for &(a, b) in marks {
            if a < at {
                continue;
            }
            job.append(&text[at..a], 0.0, plain.clone());
            job.append(&text[a..b], 0.0, hit.clone());
            at = b;
        }
    }
    job.append(&text[at..], 0.0, plain);
    job
}

/// 开关按钮（规范 `C21`）：开着 ＝ `--state-active` 底 ＋ 图标强调色 ＋ 字主色；关着 ＝ 幽灵按钮。`label` 空 ⇒ 只画图标。
pub fn toggle(ui: &mut Ui, icon: &str, label: &str, on: bool) -> egui::Response {
    let p = palette(ui.ctx());
    let icon_c = if on { p.accent } else { p.text2 };
    let text_c = if on { p.text } else { p.text2 };
    let mut atoms = egui::Atoms::default();
    if !icon.is_empty() {
        atoms.push_right(egui::RichText::new(icon).color(icon_c));
    }
    if !label.is_empty() {
        atoms.push_right(egui::RichText::new(label).color(text_c));
    }
    let b = egui::Button::new(atoms)
        .frame_when_inactive(on)
        .fill(if on { p.hover } else { Color32::TRANSPARENT });
    ui.add(b)
}

/// 幽灵按钮（工具条 · 命令栏那种）：平时不画框，悬停一层淡底；`enabled` 假 ⇒ 灰，悬停说 `why`。
pub fn ghost(ui: &mut Ui, icon: &str, label: &str, enabled: bool, why: &str) -> egui::Response {
    let mut atoms = egui::Atoms::default();
    if !icon.is_empty() {
        atoms.push_right(icon);
    }
    if !label.is_empty() {
        atoms.push_right(label);
    }
    let r = ui.add_enabled(enabled, egui::Button::new(atoms).frame_when_inactive(false));
    if enabled || why.is_empty() {
        r
    } else {
        r.on_disabled_hover_text(why)
    }
}

/// 幽灵样子的下拉按钮（命令栏「新建 ▾」「⋯」· 地址栏「…」· 范围 ▾）：平时不画框，点开一层菜单。回那颗按钮的 `Response`。
pub fn menu<'a, R>(
    ui: &mut Ui,
    text: impl egui::IntoAtoms<'a>,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::Response {
    egui::containers::menu::MenuButton::from_button(
        egui::Button::new(text).frame_when_inactive(false),
    )
    .ui(ui, add)
    .0
}

/// 标签页（规范 `C5` 下划线式 ＋ 图标 ＋ ×（悬停与当前才显）＋ 6px 忙点）。回 `(点了, 点了 ×)`。
pub fn tab(
    ui: &mut Ui,
    icon: &str,
    label: &str,
    active: bool,
    busy: Option<Color32>,
) -> (egui::Response, bool) {
    let p = palette(ui.ctx());
    let font = egui::TextStyle::Button.resolve(ui.style());
    let color = if active { p.text } else { p.text2 };
    let g_icon = ui
        .painter()
        .layout_no_wrap(icon.to_string(), font.clone(), color);
    let mut job = LayoutJob::simple_singleline(label.to_string(), font.clone(), color);
    job.wrap = egui::text::TextWrapping {
        max_width: TAB_MAX - 60.0,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let g = ui.painter().layout_job(job);
    let dot = if busy.is_some() { 10.0 } else { 0.0 };
    let w = (12.0 + dot + g_icon.size().x + 6.0 + g.size().x + 6.0 + 16.0 + 8.0)
        .clamp(TAB_MIN, TAB_MAX);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, TAB_H), egui::Sense::click());
    let hovered = resp.hovered();
    if hovered && !active {
        ui.painter().rect_filled(rect, 0.0, p.hover);
    }
    let mut x = rect.left() + 12.0;
    let cy = rect.center().y;
    if let Some(c) = busy {
        ui.painter().circle_filled(egui::pos2(x + 3.0, cy), 3.0, c);
        x += dot;
    }
    ui.painter().galley(
        egui::pos2(x, cy - g_icon.size().y / 2.0),
        g_icon.clone(),
        color,
    );
    x += g_icon.size().x + 6.0;
    ui.painter()
        .galley(egui::pos2(x, cy - g.size().y / 2.0), g, color);
    if active {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - 2.0),
                rect.right_bottom(),
            ),
            0.0,
            p.accent,
        );
    }
    let mut closed = false;
    if active || hovered {
        let xr = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 14.0, cy),
            egui::vec2(16.0, 16.0),
        );
        let xresp = ui.interact(xr, resp.id.with("close"), egui::Sense::click());
        if xresp.hovered() {
            ui.painter().rect_filled(xr, 3.0, p.hover);
        }
        ui.painter().text(
            xr.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::X,
            egui::FontId::proportional(12.0),
            p.text2,
        );
        closed = xresp.clicked();
    }
    (resp, closed)
}

/// 标签页的高 · 最宽 · 最窄。
pub const TAB_H: f32 = 30.0;
pub const TAB_MAX: f32 = 180.0;
pub const TAB_MIN: f32 = 80.0;

/// 左栏的一项：当前所在 ⇒ 左边 2px 强调色条（不铺底）；右端可以挂一个小记号（转圈 / 警告）。
pub fn side_item(
    ui: &mut Ui,
    icon: &str,
    text: &str,
    here: bool,
    mark: SideMark,
) -> egui::Response {
    let p = palette(ui.ctx());
    let h = 28.0;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, p.hover);
    }
    if here {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                rect.left_top() + egui::vec2(0.0, 5.0),
                egui::vec2(2.0, h - 10.0),
            ),
            0.0,
            p.accent,
        );
    }
    let font = egui::TextStyle::Body.resolve(ui.style());
    let ic = ui
        .painter()
        .layout_no_wrap(icon.to_string(), font.clone(), p.text2);
    ui.painter().galley(
        egui::pos2(rect.left() + 10.0, rect.center().y - ic.size().y / 2.0),
        ic,
        p.text2,
    );
    let right_room = if matches!(mark, SideMark::None) {
        8.0
    } else {
        26.0
    };
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, p.text);
    job.wrap = egui::text::TextWrapping {
        max_width: (rect.width() - 34.0 - right_room).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let g = ui.painter().layout_job(job);
    ui.painter().galley(
        egui::pos2(rect.left() + 34.0, rect.center().y - g.size().y / 2.0),
        g,
        p.text,
    );
    let at = egui::pos2(rect.right() - 14.0, rect.center().y);
    match mark {
        SideMark::None => {}
        SideMark::Busy => {
            let r = egui::Rect::from_center_size(at, egui::vec2(12.0, 12.0));
            egui::Spinner::new().size(12.0).paint_at(ui, r);
        }
        SideMark::Warn => {
            ui.painter().text(
                at,
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::WARNING,
                egui::FontId::proportional(13.0),
                p.warn,
            );
        }
    }
    resp
}

/// 左栏一项右端挂的记号。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideMark {
    None,
    Busy,
    Warn,
}

/// 一条回执（规范 `C13`：右下角、8 秒、悬停停表；可带一个动作）。
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    pub action: Option<String>,
    /// 调用方给的记号（点了动作 ⇒ [`Toasts::show`] 交回它；`0` ＝ 不带）：撤销那一下据它找要做的那几件。
    pub tag: u64,
    /// 还剩几秒。
    pub left: f32,
}

/// 回执的一摞（每扇窗一份）。
#[derive(Clone, Debug, Default)]
pub struct Toasts {
    pub items: Vec<Toast>,
}

/// 回执摆多久。
pub const TOAST_SECS: f32 = 8.0;

impl Toasts {
    pub fn push(&mut self, text: String, action: Option<String>) {
        self.push_tagged(text, action, 0);
    }

    /// 同 [`Self::push`]，带一个记号（点了动作 ⇒ [`Self::show`] 交回它）。
    pub fn push_tagged(&mut self, text: String, action: Option<String>, tag: u64) {
        self.items.retain(|t| t.text != text);
        self.items.push(Toast {
            text,
            action,
            tag,
            left: TOAST_SECS,
        });
        if self.items.len() > 3 {
            self.items.remove(0);
        }
    }

    /// 画在右下角（压在最上层）。回这一帧点了动作的那一条的记号（[`Toast::tag`]）；点了动作那一条随即收掉。
    pub fn show(&mut self, ctx: &egui::Context, bottom_gap: f32) -> Option<u64> {
        let p = palette(ctx);
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let mut hit = None;
        let mut hovered_any = false;
        let screen = ctx.content_rect();
        let mut y = screen.bottom() - bottom_gap - 12.0;
        for (i, t) in self.items.iter().enumerate().rev() {
            let id = egui::Id::new(("filewin-toast", i, &t.text));
            let r = egui::Area::new(id)
                .order(egui::Order::Tooltip)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(screen.right() - 16.0, y))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(p.card)
                        .corner_radius(8.0)
                        .stroke(egui::Stroke::new(1.0, p.border_soft))
                        .shadow(ctx.global_style().visuals.popup_shadow)
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(egui_phosphor::regular::CHECK)
                                        .color(p.success),
                                );
                                ui.label(egui::RichText::new(&t.text).color(p.text));
                                if let Some(a) = &t.action {
                                    if ui.button(a).clicked() {
                                        hit = Some(i);
                                    }
                                }
                            });
                        });
                });
            hovered_any |= r.response.hovered() || r.response.contains_pointer();
            y -= r.response.rect.height() + 8.0;
        }
        if !hovered_any {
            for t in &mut self.items {
                t.left -= dt;
            }
        }
        let tag = hit.map(|i| {
            let tag = self.items[i].tag;
            self.items.remove(i);
            tag
        });
        self.items.retain(|t| t.left > 0.0);
        if !self.items.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        tag
    }
}

/// 列表加载那几行骨架（规范 `C19`）。
pub fn skeleton_rows(ui: &mut Ui, n: usize) {
    let p = palette(ui.ctx());
    let m = metrics(ui.ctx());
    for i in 0..n {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), m.row_h),
            egui::Sense::hover(),
        );
        let w = rect.width() * [0.42, 0.33, 0.5, 0.28, 0.38][i % 5];
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                rect.left_top() + egui::vec2(10.0, 8.0),
                egui::vec2(w, rect.height() - 16.0),
            ),
            4.0,
            p.hover,
        );
    }
}

/// 一块空 / 出错的中央态：图标 ＋ 一句 ＋ 几个动作。回点了第几个动作。
pub fn empty_state(ui: &mut Ui, icon: &str, text: &str, actions: &[String]) -> Option<usize> {
    let p = palette(ui.ctx());
    let mut hit = None;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.3).max(24.0));
        ui.label(egui::RichText::new(icon).size(32.0).color(p.faint));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(text).color(p.text2));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let w: f32 = actions.len() as f32 * 96.0;
            ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
            for (i, a) in actions.iter().enumerate() {
                if ui.button(a).clicked() {
                    hit = Some(i);
                }
            }
        });
    });
    hit
}

/// 进度条（规范 `C14`）：底 `--border-soft`、填 `--accent`；`None` ⇒ 不确定进度（来回走的一小段）。
pub fn progress(ui: &mut Ui, width: f32, frac: Option<f32>) {
    let p = palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 4.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, p.border_soft);
    let fill = match frac {
        Some(f) => egui::Rect::from_min_size(
            rect.min,
            egui::vec2(rect.width() * f.clamp(0.0, 1.0), rect.height()),
        ),
        None => {
            let t = ui.input(|i| i.time) as f32;
            let x = rect.left() + (t * 0.6).fract() * (rect.width() * 0.75);
            ui.ctx().request_repaint();
            egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(rect.width() * 0.25, rect.height()),
            )
        }
    };
    ui.painter().rect_filled(fill, 2.0, p.accent);
}

/// 一张表的头（32 高）：左标题 ＋ 右端一颗文字按钮与一颗收起（⌄）。
pub fn table_head(ui: &mut Ui, title: &str, action: &str, acted: &mut bool, collapse: &mut bool) {
    let p = palette(ui.ctx());
    let w = ui.available_width();
    let r = ui.allocate_ui_with_layout(
        egui::vec2(w, 32.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(egui::vec2(w, 32.0));
            ui.add_space(12.0);
            ui.label(egui::RichText::new(title).strong().color(p.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(egui_phosphor::regular::CARET_DOWN))
                            .frame_when_inactive(false),
                    )
                    .clicked()
                {
                    *collapse = true;
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(action).small().color(p.text2))
                            .frame_when_inactive(false),
                    )
                    .clicked()
                {
                    *acted = true;
                }
            });
        },
    );
    let rect = r.response.rect;
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, p.border_soft),
    );
}

/// 「进度」表一行底下那几行小字：失败的那一行摊开（每个没成的一行，稿 11「每个一句」）；别的态一行。
pub fn task_lines(v: &super::progress::View) -> Vec<String> {
    if v.detail.is_empty() {
        return Vec::new();
    }
    if v.state == super::progress::State::Failed {
        let sep = copy_text("rsFilewinProgress.detail.sep", &[]);
        return v.detail.split(sep.as_str()).map(str::to_string).collect();
    }
    vec![v.detail.clone()]
}

/// 「进度」表一行占多高：一行小字 36；失败摊开的每多一行 ＋16。
pub fn task_row_height(v: &super::progress::View) -> f32 {
    36.0 + 16.0 * (task_lines(v).len().max(1) - 1) as f32
}

/// 「进度」表的一行（稿 11：图标 22 ｜ 标题 ＋ 一行小字 ｜ 进度条 240 ｜ 读数 150 右对齐等宽 ｜ 按钮）。回这一帧按下的动作。
pub fn task_row(ui: &mut Ui, id: u64, v: &super::progress::View) -> Option<super::progress::Act> {
    use super::progress::State;
    let p = palette(ui.ctx());
    let w = ui.available_width();
    let lines = task_lines(v);
    let h = task_row_height(v);
    let icon_c = match v.state {
        State::Running | State::Stopped => p.text2,
        State::Done => p.success,
        State::Failed => p.error,
    };
    let mut hit = None;
    let r = ui.push_id(("filewin-task", id), |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(w, h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_size(egui::vec2(w, h));
                ui.add_space(12.0);
                ui.add_sized(
                    [22.0, h],
                    egui::Label::new(egui::RichText::new(v.icon).color(icon_c)),
                );
                ui.add_space(10.0);
                // 进度条 240 ＋ 读数 150 ＋ 按钮约 110 ＋ 间距：窄了先缩进度条，标题一栏至少 120。
                let bar = (ui.available_width() - 150.0 - 110.0 - 30.0 - 120.0).clamp(60.0, 240.0);
                let what_w = (ui.available_width() - bar - 150.0 - 110.0 - 30.0).max(80.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(what_w, h),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.set_width(what_w);
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(egui::RichText::new(&v.title).color(p.text))
                                .truncate(),
                        )
                        .on_hover_text(&v.title);
                        for l in &lines {
                            ui.add(
                                egui::Label::new(egui::RichText::new(l).small().color(p.text2))
                                    .truncate(),
                            )
                            .on_hover_text(l);
                        }
                    },
                );
                ui.add_space(10.0);
                match v.frac {
                    Some(f) => progress(ui, bar, f),
                    None => {
                        ui.add_space(bar);
                    }
                }
                ui.add_space(10.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(150.0, h),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        ui.set_width(150.0);
                        ui.label(egui::RichText::new(&v.nums).small().color(p.text2));
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    if let Some((label, a)) = &v.button {
                        let stop = matches!(a, Ok(super::progress::Act::Stop(_)) | Err(_));
                        let text = if stop {
                            format!("{} {label}", egui_phosphor::regular::STOP)
                        } else {
                            label.clone()
                        };
                        let r = ui.add_enabled(a.is_ok(), egui::Button::new(text));
                        let r = match a {
                            Err(why) => r.on_disabled_hover_text(why),
                            Ok(_) => r,
                        };
                        if r.clicked() {
                            if let Ok(a) = a {
                                hit = Some(a.clone());
                            }
                        }
                    }
                });
            },
        )
        .response
        .rect
    });
    let rect = r.inner;
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, p.border_soft.gamma_multiply(0.6)),
    );
    hit
}

/// 状态栏右端「进度」那一颗：有没看过的失败 ⇒ 左边一个红点；`frac` ⇒ 字后一小条合计进度；末尾 ⌃ / ⌄。回点了没有。
pub fn progress_chip(ui: &mut Ui, label: &str, frac: Option<f32>, red: bool, open: bool) -> bool {
    let p = palette(ui.ctx());
    let caret = if open {
        egui_phosphor::regular::CARET_DOWN
    } else {
        egui_phosphor::regular::CARET_UP
    };
    let font = egui::TextStyle::Small.resolve(ui.style());
    let g = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), p.text);
    let gc = ui
        .painter()
        .layout_no_wrap(caret.to_string(), font, p.text2);
    let dot = if red { 12.0 } else { 0.0 };
    let bar = if frac.is_some() { 46.0 } else { 0.0 };
    let w = 8.0 + dot + g.size().x + bar + 6.0 + gc.size().x + 8.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 22.0), egui::Sense::click());
    ui.painter().rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(1.0, p.border_soft),
        egui::StrokeKind::Inside,
    );
    if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    let cy = rect.center().y;
    let mut x = rect.left() + 8.0;
    if red {
        ui.painter()
            .circle_filled(egui::pos2(x + 3.5, cy), 3.5, p.error);
        x += dot;
    }
    let gw = g.size().x;
    ui.painter()
        .galley(egui::pos2(x, cy - g.size().y / 2.0), g, p.text);
    x += gw;
    if let Some(f) = frac {
        let b = egui::Rect::from_min_size(egui::pos2(x + 6.0, cy - 2.0), egui::vec2(40.0, 4.0));
        ui.painter().rect_filled(b, 2.0, p.border_soft);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(b.min, egui::vec2(40.0 * f.clamp(0.0, 1.0), 4.0)),
            2.0,
            p.accent,
        );
        x += bar;
    }
    ui.painter()
        .galley(egui::pos2(x + 6.0, cy - gc.size().y / 2.0), gc, p.text2);
    resp.clicked()
}

/// 一层挂在某颗控件下方的小清单（菜单那种浮层，`--card` 底）：每行等宽 12、悬停给第二格（全文）；末尾可多一行灰字。
/// Esc / 点别处收（`open` 置假）。回这一帧点了第几行。
pub fn path_list(
    ui: &Ui,
    id: egui::Id,
    below: egui::Rect,
    open: &mut bool,
    rows: &[(String, String)],
    tail: Option<&str>,
) -> Option<usize> {
    if !*open {
        return None;
    }
    let p = palette(ui.ctx());
    let mut hit = None;
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(below.left_bottom() + egui::vec2(0.0, 4.0))
        .show(ui.ctx(), |ui| {
            // 与菜单同一个浮层底（`Frame::popup`：不透明的浮层色 ＋ 浮层阴影）。
            egui::Frame::popup(&ui.ctx().global_style()).show(ui, |ui| {
                ui.set_max_width(420.0);
                let pad = ui.spacing().button_padding.x;
                for (i, (shown, full)) in rows.iter().enumerate() {
                    let r = ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(shown)
                                    .monospace()
                                    .size(12.0)
                                    .color(p.text),
                            )
                            .frame_when_inactive(false)
                            .truncate(),
                        )
                        .on_hover_text(full);
                    // 悬停那一行右端一颗复制图标：点一下复制的是那条路径。
                    if r.hovered() {
                        ui.painter().text(
                            r.rect.right_center() + egui::vec2(4.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            egui_phosphor::regular::COPY,
                            egui::FontId::proportional(12.0),
                            p.text2,
                        );
                    }
                    if r.clicked() {
                        hit = Some(i);
                    }
                }
                if let Some(t) = tail {
                    // 「另外 n 个」与上面每一行的字对齐（按钮字的左内边距）。
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        ui.label(egui::RichText::new(t).small().color(p.text2));
                    });
                }
            });
        });
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let at = ui.input(|i| i.pointer.interact_pos());
    let clicked_out = ui.input(|i| i.pointer.any_pressed())
        && !at.is_some_and(|a| area.response.rect.contains(a) || below.contains(a));
    if esc || clicked_out || hit.is_some() {
        *open = false;
    }
    hit
}

/// 对话框一颗按钮的样子（规范 `C10`）：普通 · 主按钮（强调色底）· 危险（错误色底）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Plain,
    Primary,
    Danger,
}

/// 对话框（规范 `C10`）：压暗的底 ＋ 居中一块（浮层底、圆角、最宽 480 且不超过窗宽 − 48）：
/// 标题（16 / 600）＋ 正文（调用方画）＋ 右下一排按钮。`focus` ＝ 打开时焦点在第几颗；Esc / 点压暗的底 ⇒ 当作点了第 `cancel` 颗。
/// 回这一帧点了第几颗。
pub fn dialog(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    body: impl FnOnce(&mut Ui),
    buttons: &[(String, Btn)],
    focus: usize,
    cancel: usize,
) -> Option<usize> {
    let p = palette(ctx);
    let mut hit = None;
    let width = (ctx.content_rect().width() - 48.0).clamp(240.0, 480.0);
    let frame = egui::Frame::popup(&ctx.global_style())
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(24));
    let shown = egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.label(egui::RichText::new(title).size(16.0).strong().color(p.text));
            ui.add_space(10.0);
            ui.scope(body);
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 按钮之间的间距是对话框自己的（规范 `C10` 一处）：正文改过 `item_spacing` 也漏不到这一排。
                ui.spacing_mut().item_spacing.x = metrics(ui.ctx()).space[2];
                let mut first_frame_focus = None;
                for (i, (label, look)) in buttons.iter().enumerate().rev() {
                    let text = egui::RichText::new(label).color(match look {
                        Btn::Plain => p.text,
                        Btn::Primary | Btn::Danger => p.text,
                    });
                    let b = match look {
                        Btn::Plain => egui::Button::new(text),
                        Btn::Primary => egui::Button::new(text).fill(p.accent_strong),
                        Btn::Danger => egui::Button::new(text).fill(p.error_strong),
                    };
                    let r = ui.add(b);
                    if i == focus {
                        first_frame_focus = Some(r.id);
                    }
                    if r.clicked() {
                        hit = Some(i);
                    }
                }
                // 打开那一刻焦点落在 `focus` 那一颗（撤不回的那一下要多按一次 Tab 才到）。
                if let Some(f) = first_frame_focus {
                    if ui.memory(|m| m.focused().is_none()) {
                        ui.memory_mut(|m| m.request_focus(f));
                    }
                }
            });
        });
    if hit.is_none() && shown.should_close() {
        hit = Some(cancel);
    }
    hit
}

/// 对话框里那一块清单的一行（稿 09：图标 ＋ 名字 ｜ 右端一格次级字）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListRow {
    pub icon: &'static str,
    pub name: String,
    pub meta: String,
}

/// 对话框里那一块清单（深一级的底、圆角；一行 28：图标 ＋ 名字左齐、右端一格次级字右齐等宽）；
/// 超过窗高一半在块里滚；`more` ⇒ 末尾一行灰字（「另外 n 项」）。
pub fn list_box(ui: &mut Ui, rows: &[ListRow], more: Option<&str>) {
    let p = palette(ui.ctx());
    let m = metrics(ui.ctx());
    let h = ui.ctx().content_rect().height() * 0.5;
    egui::Frame::new()
        .fill(p.bg)
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical().max_height(h).show(ui, |ui| {
                for r in rows {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), m.row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label(egui::RichText::new(r.icon).color(p.text2));
                            ui.add(
                                egui::Label::new(egui::RichText::new(&r.name).color(p.text))
                                    .truncate(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(&r.meta).size(12.0).color(p.text2),
                                    );
                                },
                            );
                        },
                    );
                }
                if let Some(t) = more {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), m.row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label(egui::RichText::new(t).color(p.text2));
                        },
                    );
                }
            });
        });
}

/// 同名覆盖那张表的一行（稿 12：☐ 名字 · 本机（大小 · 时间）· 那台（大小 · 时间））。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClashRow {
    pub name: String,
    pub here: String,
    pub there: String,
}

/// 同名覆盖那张表：表头（空 · 名字 · `here` · `there`）＋ 每行一个勾 ＋ 三格；勾上的那一行字是主色、没勾的次级色。
/// `ticks` 与 `rows` 同长；回这一帧有没有勾动。
pub fn clash_table(ui: &mut Ui, heads: [&str; 3], rows: &[ClashRow], ticks: &mut [bool]) -> bool {
    let p = palette(ui.ctx());
    let m = metrics(ui.ctx());
    let mut changed = false;
    let w = ui.available_width();
    let cols = [
        32.0,
        (w - 32.0) * 0.36,
        (w - 32.0) * 0.32,
        (w - 32.0) * 0.32,
    ];
    let cell = |ui: &mut Ui, width: f32, add: &mut dyn FnMut(&mut Ui)| {
        ui.allocate_ui_with_layout(
            egui::vec2(width, m.row_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(width);
                add(ui);
            },
        );
    };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.horizontal(|ui| {
            cell(ui, cols[0], &mut |_| {});
            for (k, h) in heads.iter().enumerate() {
                cell(ui, cols[k + 1], &mut |ui| {
                    ui.label(egui::RichText::new(*h).size(12.0).color(p.text2));
                });
            }
        });
        let h = ui.ctx().content_rect().height() * 0.4;
        egui::ScrollArea::vertical().max_height(h).show(ui, |ui| {
            for (i, r) in rows.iter().enumerate() {
                let on = ticks.get(i).copied().unwrap_or(false);
                let fg = if on { p.text } else { p.text2 };
                let line = ui.horizontal(|ui| {
                    cell(ui, cols[0], &mut |ui| {
                        let mut t = on;
                        if ui.checkbox(&mut t, "").changed() {
                            if let Some(x) = ticks.get_mut(i) {
                                *x = t;
                            }
                            changed = true;
                        }
                    });
                    cell(ui, cols[1], &mut |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&r.name).strong().color(fg))
                                .truncate(),
                        );
                    });
                    cell(ui, cols[2], &mut |ui| {
                        ui.label(egui::RichText::new(&r.here).color(fg));
                    });
                    cell(ui, cols[3], &mut |ui| {
                        ui.label(egui::RichText::new(&r.there).color(fg));
                    });
                });
                let y = line.response.rect.top();
                ui.painter().hline(
                    line.response.rect.x_range(),
                    y,
                    egui::Stroke::new(1.0, p.border_soft),
                );
            }
        });
    });
    changed
}

/// 就地输入那一格下面挂的出错句（稿 07：浮层底 ＋ 错误图标 ＋ 一句；画在最上层，不挤动列表）。
pub fn inline_error(ctx: &egui::Context, id: egui::Id, below: egui::Rect, text: &str) {
    let p = palette(ctx);
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(below.left_bottom() + egui::vec2(16.0, 2.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(&ctx.global_style())
                .inner_margin(egui::Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(egui_phosphor::regular::X_CIRCLE).color(p.error),
                        );
                        ui.label(egui::RichText::new(text).size(12.0).color(p.text));
                    });
                });
        });
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/kit_tests.rs"]
mod tests;
