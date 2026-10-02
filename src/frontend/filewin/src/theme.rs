//! 窗口的样子：开窗种子里那一套主题（主界面此刻的设计令牌）映射成 egui 的配色、字号与间距。
//!
//! 窗口里的颜色只有两个来处：egui 的 `Visuals`（本模块按主题填好）与 [`Palette`]（`Visuals` 装不下的那几格：
//! 次级文字、深一级的底、强调色、边线层级）。组件一律从这两处取，自己不写色值。
//! 没装主题时（判据夹具）[`Palette`] 从 egui 当下的 `Visuals` 推出来，同样不写色值。

use egui::{Color32, CornerRadius, Stroke};
use filewin_contract::{Rgba, Theme};

/// 详情列表一行的高度（行与行之间不留缝，悬停 / 选中那一层底连成一片）。
pub const ROW_HEIGHT: f32 = 28.0;
/// 工具条一行的高度。
pub const BAR_HEIGHT: f32 = 40.0;
/// 圆角。
pub const RADIUS: u8 = 6;

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

/// 选中那一层的不透明度（强调色叠在暗底上）。
const PICKED_ALPHA: u8 = 0x47;

impl Palette {
    /// 主题 → 色板。
    pub fn of_theme(t: &Theme) -> Self {
        let a = t.accent;
        Self {
            bg: c(t.bg),
            bg2: c(t.bg2),
            card: c(t.card),
            text: c(t.text),
            text2: c(t.text2),
            faint: c(t.text_faint),
            accent: c(a),
            picked: Color32::from_rgba_unmultiplied(a[0], a[1], a[2], PICKED_ALPHA),
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
            hover: v.widgets.hovered.weak_bg_fill,
            border: v.widgets.noninteractive.bg_stroke.color,
            border_soft: v.widgets.noninteractive.bg_stroke.color,
            success: v.hyperlink_color,
            warn: v.warn_fg_color,
            error: v.error_fg_color,
        }
    }
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
    ctx.data_mut(|d| d.insert_temp(palette_id(), p));
    ctx.all_styles_mut(|s| apply(s, t, &p));
}

/// 主题 → 一份 `Style`（纯函数，判据直接读它）。
pub fn apply(s: &mut egui::Style, t: &Theme, p: &Palette) {
    use egui::{FontFamily, FontId, TextStyle};
    let r = CornerRadius::same(RADIUS);
    let v = &mut s.visuals;
    v.dark_mode = true;
    v.override_text_color = None;
    v.weak_text_color = Some(p.faint);
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, c(t.border_medium));
    v.window_corner_radius = CornerRadius::same(RADIUS + 2);
    v.menu_corner_radius = r;
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
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
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
            FontId::new(t.size_base + 4.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(t.size_mono, FontFamily::Monospace),
        ),
    ]
    .into();

    // 间距：按钮 28 高、工具条里留得开。
    let sp = &mut s.spacing;
    sp.item_spacing = egui::vec2(8.0, 6.0);
    sp.button_padding = egui::vec2(10.0, 5.0);
    sp.interact_size.y = 28.0;
    sp.menu_margin = egui::Margin::same(6);
    sp.window_margin = egui::Margin::same(12);
    sp.icon_spacing = 6.0;
}

/// 深一级底的那种条（工具条 · 命令栏 · 状态栏 · 侧栏）。
pub fn bar_frame(ctx: &egui::Context) -> egui::Frame {
    let p = palette(ctx);
    egui::Frame::new()
        .fill(p.bg2)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .stroke(Stroke::new(1.0, p.border_soft))
}

/// 主底那种侧板（预览）：与列表同一层底，靠一条淡边线分开。
pub fn pane_frame(ctx: &egui::Context) -> egui::Frame {
    let p = palette(ctx);
    egui::Frame::new()
        .fill(p.bg)
        .inner_margin(egui::Margin::same(12))
        .stroke(Stroke::new(1.0, p.border_soft))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/theme_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/theme_testing.rs"]
pub(crate) mod testing;
