//! 窗口里的通用件（与网页那一侧 `ui/kit/` 同一套规范、同一份令牌）：条 · 状态行 · 加底色的字。
//! 窗口里的这几样只许在这里建，各处调它，不各画各的。

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Ui};

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
        self.items.retain(|t| t.text != text);
        self.items.push(Toast {
            text,
            action,
            left: TOAST_SECS,
        });
        if self.items.len() > 3 {
            self.items.remove(0);
        }
    }

    /// 画在右下角（压在最上层）。回这一帧点了第几条的动作。
    pub fn show(&mut self, ctx: &egui::Context, bottom_gap: f32) -> Option<usize> {
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
        self.items.retain(|t| t.left > 0.0);
        if !self.items.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        hit
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

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/kit_tests.rs"]
mod tests;
