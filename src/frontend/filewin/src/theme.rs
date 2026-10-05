//! 窗口的样子：开窗种子里那一套主题（主界面此刻的设计令牌）映射成 egui 的配色、字号与间距。
//!
//! 窗口里的颜色只有两个来处：egui 的 `Visuals`（本模块按主题填好）与 [`Palette`]（`Visuals` 装不下的那几格：
//! 次级文字、深一级的底、强调色、边线层级）。组件一律从这两处取，自己不写色值。
//! 没装主题时（判据夹具）[`Palette`] 从 egui 当下的 `Visuals` 推出来，同样不写色值。

use egui::{Color32, CornerRadius, Stroke};
use filewin_contract::{Rgba, Shadow, Theme};

/// `Visuals` 装不下、组件又要的那几格。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// 主底（列表）。
    pub bg: Color32,
    /// 深一级的底：工具条 · 侧栏 · 状态栏 · 标签栏。
    pub bg2: Color32,
    /// 浮层底。
    pub card: Color32,
    pub text: Color32,
    /// 次级文字（列表的修改时间 / 类型 / 大小 · 表头 · 侧栏小标题）。
    pub text2: Color32,
    /// 最淡一级（隐藏文件 · 占位）。
    pub faint: Color32,
    pub accent: Color32,
    /// 选中那一层：强调色的淡底。
    pub picked: Color32,
    /// 主按钮底 · 危险按钮底 · 红字。
    pub accent_strong: Color32,
    pub error_strong: Color32,
    pub error_text: Color32,
    /// 悬停那一层淡底。
    pub hover: Color32,
    pub border: Color32,
    pub border_soft: Color32,
    pub success: Color32,
    pub warn: Color32,
    pub error: Color32,
}

fn c(x: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(x[0], x[1], x[2], x[3])
}

impl Palette {
    /// 主题 → 色板。
    pub fn of_theme(t: &Theme) -> Self {
        Self {
            bg: c(t.bg),
            bg2: c(t.bg2),
            card: c(t.card),
            text: c(t.text),
            text2: c(t.text2),
            faint: c(t.text_faint),
            accent: c(t.accent),
            picked: c(t.selected),
            accent_strong: c(t.accent_strong),
            error_strong: c(t.error_strong),
            error_text: c(t.error_text),
            hover: c(t.state_active),
            border: c(t.border_strong),
            border_soft: c(t.border_soft),
            success: c(t.success),
            warn: c(t.warn),
            error: c(t.error),
        }
    }

    /// 没装主题时：从 egui 当下的样子推（判据夹具走这一支；生产开窗那一拍就装了主题）。
    pub fn of_visuals(v: &egui::Visuals) -> Self {
        Self {
            bg: v.panel_fill,
            bg2: v.extreme_bg_color,
            card: v.window_fill,
            text: v.text_color(),
            text2: v.widgets.inactive.text_color(),
            faint: v.weak_text_color(),
            accent: v.selection.stroke.color,
            picked: v.selection.bg_fill,
            accent_strong: v.selection.stroke.color,
            error_strong: v.error_fg_color,
            error_text: v.error_fg_color,
            hover: v.widgets.hovered.weak_bg_fill,
            border: v.widgets.noninteractive.bg_stroke.color,
            border_soft: v.widgets.noninteractive.bg_stroke.color,
            success: v.hyperlink_color,
            warn: v.warn_fg_color,
            error: v.error_fg_color,
        }
    }
}

/// 尺寸那几格（主界面的间距 · 圆角 · 控件高 · 行高 · 图标尺寸令牌）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// 详情列表一行的高度（行与行之间不留缝，悬停 / 选中那一层底连成一片）。
    pub row_h: f32,
    pub control_h: f32,
    pub control_h_compact: f32,
    /// 工具条一行：常规控件 ＋ 上下各一档 `--space-3`。
    pub bar_h: f32,
    pub icon: f32,
    /// `--space-1` … `--space-8`。
    pub space: [f32; 8],
    pub radius_m: f32,
}

impl Metrics {
    pub fn of_theme(t: &Theme) -> Self {
        Self {
            row_h: t.row_h,
            control_h: t.control_h,
            control_h_compact: t.control_h_compact,
            bar_h: t.control_h + 2.0 * t.space[2],
            icon: t.icon_size,
            space: t.space,
            radius_m: t.radius_m,
        }
    }

    /// 没装主题时：从当下的 `Style` 推（行高 ＝ 控件高）。
    pub fn of_style(s: &egui::Style) -> Self {
        let h = s.spacing.interact_size.y;
        let g = s.spacing.item_spacing.x;
        Self {
            row_h: h,
            control_h: h,
            control_h_compact: h,
            bar_h: h + s.spacing.item_spacing.y * 2.0,
            icon: s.text_styles[&egui::TextStyle::Body].size,
            space: [
                g / 4.0,
                g / 2.0,
                g * 0.75,
                g,
                g * 1.5,
                g * 2.0,
                g * 3.0,
                g * 4.0,
            ],
            radius_m: f32::from(s.visuals.widgets.inactive.corner_radius.nw),
        }
    }
}

fn metrics_id() -> egui::Id {
    egui::Id::new("filewin-metrics")
}

/// 这扇窗的尺寸（装了主题 ⇒ 主题那一套；没装 ⇒ 从 `Style` 推）。
pub fn metrics(ctx: &egui::Context) -> Metrics {
    ctx.data(|d| d.get_temp::<Metrics>(metrics_id()))
        .unwrap_or_else(|| Metrics::of_style(&ctx.global_style()))
}

fn radius(px: f32) -> CornerRadius {
    CornerRadius::same(px.round().clamp(0.0, 255.0) as u8)
}

fn shadow(s: Shadow) -> egui::Shadow {
    let i8_of = |v: f32| v.round().clamp(-128.0, 127.0) as i8;
    egui::Shadow {
        offset: [i8_of(s.x), i8_of(s.y)],
        blur: s.blur.round().clamp(0.0, 255.0) as u8,
        spread: 0,
        color: c(s.color),
    }
}

fn margin(px: f32) -> egui::Margin {
    egui::Margin::same(px.round().clamp(-128.0, 127.0) as i8)
}

fn palette_id() -> egui::Id {
    egui::Id::new("filewin-palette")
}

/// 这扇窗的色板（装了主题 ⇒ 主题那一套；没装 ⇒ 从 `Visuals` 推）。
pub fn palette(ctx: &egui::Context) -> Palette {
    ctx.data(|d| d.get_temp::<Palette>(palette_id()))
        .unwrap_or_else(|| Palette::of_visuals(&ctx.global_style().visuals))
}

/// 把主题装到这扇窗上：egui 的配色 · 字号 · 间距，外加 [`Palette`]。开窗第一拍调一次。
pub fn install(ctx: &egui::Context, t: &Theme) {
    let p = Palette::of_theme(t);
    ctx.data_mut(|d| {
        d.insert_temp(palette_id(), p);
        d.insert_temp(metrics_id(), Metrics::of_theme(t));
    });
    ctx.all_styles_mut(|s| apply(s, t, &p));
}

/// 主题 → 一份 `Style`（纯函数，判据直接读它）。
pub fn apply(s: &mut egui::Style, t: &Theme, p: &Palette) {
    use egui::{FontFamily, FontId, TextStyle};
    let r = radius(t.radius_m);
    let v = &mut s.visuals;
    v.dark_mode = true;
    v.override_text_color = None;
    v.weak_text_color = Some(p.faint);
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, c(t.border_medium));
    v.window_corner_radius = radius(t.radius_xl);
    v.menu_corner_radius = radius(t.radius_l);
    v.popup_shadow = shadow(t.shadow_float);
    v.window_shadow = shadow(t.shadow_modal);
    v.extreme_bg_color = c(t.field_bg);
    v.text_edit_bg_color = Some(c(t.field_bg));
    v.faint_bg_color = p.bg2;
    v.code_bg_color = p.bg2;
    v.hyperlink_color = c(t.link);
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.picked;
    v.selection.stroke = Stroke::new(1.0, p.text);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.striped = false;
    v.indent_has_left_vline = false;
    v.collapsing_header_frame = false;

    let w = &mut v.widgets;
    // 标签 · 分隔线：主文字 ＋ 淡边线。
    w.noninteractive.bg_fill = p.bg;
    w.noninteractive.weak_bg_fill = p.bg;
    w.noninteractive.bg_stroke = Stroke::new(1.0, c(t.border_soft));
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;
    // 按钮平时：透明底 ＋ 强边线 ＋ 次级文字（主界面按钮的样子）。
    w.inactive.bg_fill = c(t.field_bg);
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::new(1.0, c(t.border_medium));
    w.inactive.fg_stroke = Stroke::new(1.0, p.text2);
    w.inactive.corner_radius = r;
    w.inactive.expansion = 0.0;
    // 悬停：一层淡底，文字提亮。
    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, c(t.border_medium));
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;
    // 按下：深一层底 ＋ 强调色描边。
    w.active.bg_fill = c(t.state_active);
    w.active.weak_bg_fill = c(t.state_active);
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;
    // 展开着（下拉 / 菜单按钮）。
    w.open.bg_fill = p.card;
    w.open.weak_bg_fill = p.card;
    w.open.bg_stroke = Stroke::new(1.0, c(t.border_medium));
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = r;

    // 字号：正文 · 按钮 = 主界面正文；小字 · 等宽各按主界面那一格。
    s.text_styles = [
        (
            TextStyle::Small,
            FontId::new(t.size_small, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(t.size_base, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(t.size_base, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(t.size_title, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(t.size_mono, FontFamily::Monospace),
        ),
    ]
    .into();

    // 间距：常规控件高、同组之间一档 8、控件左右 12、菜单内边距 4、对话框内边距 24、图标与字 4。
    let k = &t.space;
    let sp = &mut s.spacing;
    sp.item_spacing = egui::vec2(k[3], k[2]);
    sp.button_padding = egui::vec2(k[4], k[1]);
    sp.interact_size.y = t.control_h;
    sp.menu_margin = margin(k[1]);
    sp.window_margin = margin(k[6]);
    sp.icon_spacing = k[1];
}

/// 深一级底的那种条（工具条 · 命令栏 · 状态栏 · 侧栏）。
pub fn bar_frame(ctx: &egui::Context) -> egui::Frame {
    let p = palette(ctx);
    egui::Frame::new()
        .fill(p.bg2)
        .inner_margin({
            let k = metrics(ctx).space;
            egui::Margin::symmetric(k[4] as i8, k[2] as i8)
        })
        .stroke(Stroke::new(1.0, p.border_soft))
}

/// 主底那种侧板（预览）：与列表同一层底，靠一条淡边线分开。
pub fn pane_frame(ctx: &egui::Context) -> egui::Frame {
    let p = palette(ctx);
    egui::Frame::new()
        .fill(p.bg)
        .inner_margin(margin(metrics(ctx).space[4]))
        .stroke(Stroke::new(1.0, p.border_soft))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/theme_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/theme_testing.rs"]
pub(crate) mod testing;
