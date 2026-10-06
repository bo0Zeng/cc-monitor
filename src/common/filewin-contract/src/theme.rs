//! 文件窗口的样子：monitor 开窗那一刻主界面 `:root` 上解析出来的那一套设计令牌（含用户在设置里改过的），
//! 解成数放进开窗种子；窗口照它映射 egui 的样子，自己不另写一份色值。
//!
//! 两边对上的只有这一份：令牌名单 [`THEME_TOKENS`]（主界面那一侧按它读计算值）· 解法 [`Theme::from_tokens`] · 解出来的形状 [`Theme`]。

use copy_core::copy_text;
use std::collections::BTreeMap;

/// 一个颜色：`[r, g, b, a]`，未预乘。
pub type Rgba = [u8; 4];

/// 主界面那一侧要读、要交过来的令牌，**恰好**这些（多一个少一个都是错，[`Theme::from_tokens`] 两向都拒）。
pub const THEME_TOKENS: [&str; 47] = [
    "--bg",
    "--bg-2",
    "--card",
    "--text",
    "--text-2",
    "--text-faint",
    "--accent",
    "--accent-strong",
    "--selected-bg",
    "--hit-bg",
    "--border-strong",
    "--border-medium",
    "--border-soft",
    "--border-faint",
    "--state-hover",
    "--state-active",
    "--field-bg",
    "--success",
    "--warn",
    "--error",
    "--error-strong",
    "--error-text",
    "--color-link",
    "--font-base",
    "--font-mono",
    "--font-size-base",
    "--font-size-mono",
    "--font-size-small",
    "--font-size-title",
    "--space-1",
    "--space-2",
    "--space-3",
    "--space-4",
    "--space-5",
    "--space-6",
    "--space-7",
    "--space-8",
    "--radius-s",
    "--radius-m",
    "--radius-l",
    "--radius-xl",
    "--control-h",
    "--control-h-compact",
    "--row-h",
    "--icon-size",
    "--shadow-float",
    "--shadow-modal",
];

/// 解出来的那一套。字段与 [`THEME_TOKENS`] 一一对应。
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Theme {
    /// 主背景（列表底）。
    pub bg: Rgba,
    /// 比主背景深一级：窗口框、工具条、侧栏、状态栏。
    pub bg2: Rgba,
    /// 浮层：菜单、对话框、悬浮提示。
    pub card: Rgba,
    /// 三级文字：主 · 次 · 淡。
    pub text: Rgba,
    pub text2: Rgba,
    pub text_faint: Rgba,
    /// 唯一的强调色（当前 / 选中 · 焦点 · 开关开）。
    pub accent: Rgba,
    /// 主按钮底（白字过 4.5:1 的那一档）。
    pub accent_strong: Rgba,
    /// 选中行的淡底。
    pub selected: Rgba,
    /// 搜索命中那几个字的底。
    pub hit: Rgba,
    /// 边线四级（强 → 极淡）。
    pub border_strong: Rgba,
    pub border_medium: Rgba,
    pub border_soft: Rgba,
    pub border_faint: Rgba,
    /// 悬停 · 按下那一层淡底。
    pub state_hover: Rgba,
    pub state_active: Rgba,
    /// 输入框底。
    pub field_bg: Rgba,
    pub success: Rgba,
    pub warn: Rgba,
    pub error: Rgba,
    /// 危险按钮底 · 红字。
    pub error_strong: Rgba,
    pub error_text: Rgba,
    pub link: Rgba,
    /// 字族列表（CSS 的写法拆开、去引号，按先后）。
    pub font_base: Vec<String>,
    pub font_mono: Vec<String>,
    /// 字号（逻辑像素）。
    pub size_base: f32,
    pub size_mono: f32,
    pub size_small: f32,
    pub size_title: f32,
    /// 间距刻度 `--space-1` … `--space-8`。
    pub space: [f32; 8],
    /// 圆角四档：徽标 · 控件 · 浮层与菜单 · 对话框。
    pub radius_s: f32,
    pub radius_m: f32,
    pub radius_l: f32,
    pub radius_xl: f32,
    /// 控件两档高 · 列表行高 · 图标尺寸。
    pub control_h: f32,
    pub control_h_compact: f32,
    pub row_h: f32,
    pub icon_size: f32,
    /// 浮层（菜单 · 悬浮提示）与对话框的投影。
    pub shadow_float: Shadow,
    pub shadow_modal: Shadow,
}

/// 一道投影：偏移 · 模糊半径 · 颜色（CSS `box-shadow` 的 `x y blur color` 那一形）。
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub color: Rgba,
}

/// 一格令牌的计算值 → 颜色。认 `#rgb` · `#rgba` · `#rrggbb` · `#rrggbbaa` · `rgb()` / `rgba()`（逗号或空格分隔，`/` 带透明度）。
/// 别的写法（颜色名、`color-mix()`）一律不认，回 `None`（不猜）。
pub fn parse_css_color(raw: &str) -> Option<Rgba> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()?;
        return match digits.as_slice() {
            [r, g, b] => Some([r * 17, g * 17, b * 17, 255]),
            [r, g, b, a] => Some([r * 17, g * 17, b * 17, a * 17]),
            [r1, r2, g1, g2, b1, b2] => Some([r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, 255]),
            [r1, r2, g1, g2, b1, b2, a1, a2] => {
                Some([r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, a1 * 16 + a2])
            }
            _ => None,
        };
    }
    let lower = s.to_ascii_lowercase();
    let inner = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = inner
        .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .collect();
    let chan = |p: &str| -> Option<u8> {
        let v: f32 = p.parse().ok()?;
        (0.0..=255.0).contains(&v).then(|| v.round() as u8)
    };
    let alpha = |p: &str| -> Option<u8> {
        let v: f32 = match p.strip_suffix('%') {
            Some(pc) => pc.parse::<f32>().ok()? / 100.0,
            None => p.parse().ok()?,
        };
        (0.0..=1.0).contains(&v).then(|| (v * 255.0).round() as u8)
    };
    match parts.as_slice() {
        [r, g, b] => Some([chan(r)?, chan(g)?, chan(b)?, 255]),
        [r, g, b, a] => Some([chan(r)?, chan(g)?, chan(b)?, alpha(a)?]),
        _ => None,
    }
}

/// `0 6px 24px #00000073` → 一道投影（两段偏移 ＋ 模糊 ＋ 颜色；只认这一形，不猜）。
pub fn parse_shadow(raw: &str) -> Option<Shadow> {
    let parts: Vec<&str> = raw.split_whitespace().collect();
    let [x, y, blur, color] = parts.as_slice() else {
        return None;
    };
    let len = |p: &str| -> Option<f32> {
        if p == "0" {
            return Some(0.0);
        }
        let v: f32 = p.strip_suffix("px")?.parse().ok()?;
        v.is_finite().then_some(v)
    };
    Some(Shadow {
        x: len(x)?,
        y: len(y)?,
        blur: len(blur)?,
        color: parse_css_color(color)?,
    })
}

/// CSS 字族列表 → 名字一串（去引号、去空白，空项丢掉）。
pub fn parse_font_families(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\'').trim())
        .filter(|f| !f.is_empty())
        .map(str::to_string)
        .collect()
}

/// `14px` → `14.0`（只认 `px`，别的单位不猜）。
pub fn parse_px(raw: &str) -> Option<f32> {
    let v: f32 = raw.trim().strip_suffix("px")?.trim().parse().ok()?;
    (v > 0.0 && v.is_finite()).then_some(v)
}

impl Theme {
    /// 令牌名 → 计算值 ⇒ 那一套。名单必须与 [`THEME_TOKENS`] 恰好相等；任一格解不出来就是错（带名字与原值），不补默认。
    ///
    /// # Errors
    ///
    /// 缺令牌 · 多出不认识的令牌 · 某一格不是认得的写法（报文带那一格的原值，不带令牌名：令牌名不上屏）。
    pub fn from_tokens(map: &BTreeMap<String, String>) -> Result<Self, String> {
        for t in THEME_TOKENS {
            if !map.contains_key(t) {
                return Err(copy_text("rsFilewinTheme.tokens.missing", &[]));
            }
        }
        if map.keys().any(|k| !THEME_TOKENS.contains(&k.as_str())) {
            return Err(copy_text("rsFilewinTheme.tokens.unknown", &[]));
        }
        let bad = |name: &str| {
            copy_text(
                "rsFilewinTheme.tokens.unreadable",
                &[("value", &map[name].clone())],
            )
        };
        let color = |name: &str| parse_css_color(&map[name]).ok_or_else(|| bad(name));
        let px = |name: &str| parse_px(&map[name]).ok_or_else(|| bad(name));
        let shadow = |name: &str| parse_shadow(&map[name]).ok_or_else(|| bad(name));
        let mut space = [0.0; 8];
        for (i, v) in space.iter_mut().enumerate() {
            *v = px(&format!("--space-{}", i + 1))?;
        }
        let fams = |name: &str| {
            let v = parse_font_families(&map[name]);
            if v.is_empty() {
                Err(bad(name))
            } else {
                Ok(v)
            }
        };
        Ok(Self {
            bg: color("--bg")?,
            bg2: color("--bg-2")?,
            card: color("--card")?,
            text: color("--text")?,
            text2: color("--text-2")?,
            text_faint: color("--text-faint")?,
            accent: color("--accent")?,
            accent_strong: color("--accent-strong")?,
            selected: color("--selected-bg")?,
            hit: color("--hit-bg")?,
            border_strong: color("--border-strong")?,
            border_medium: color("--border-medium")?,
            border_soft: color("--border-soft")?,
            border_faint: color("--border-faint")?,
            state_hover: color("--state-hover")?,
            state_active: color("--state-active")?,
            field_bg: color("--field-bg")?,
            success: color("--success")?,
            warn: color("--warn")?,
            error: color("--error")?,
            error_strong: color("--error-strong")?,
            error_text: color("--error-text")?,
            link: color("--color-link")?,
            font_base: fams("--font-base")?,
            font_mono: fams("--font-mono")?,
            size_base: px("--font-size-base")?,
            size_mono: px("--font-size-mono")?,
            size_small: px("--font-size-small")?,
            size_title: px("--font-size-title")?,
            space,
            radius_s: px("--radius-s")?,
            radius_m: px("--radius-m")?,
            radius_l: px("--radius-l")?,
            radius_xl: px("--radius-xl")?,
            control_h: px("--control-h")?,
            control_h_compact: px("--control-h-compact")?,
            row_h: px("--row-h")?,
            icon_size: px("--icon-size")?,
            shadow_float: shadow("--shadow-float")?,
            shadow_modal: shadow("--shadow-modal")?,
        })
    }
}
