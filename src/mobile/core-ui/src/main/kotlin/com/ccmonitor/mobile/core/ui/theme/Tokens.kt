package com.ccmonitor.mobile.core.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp

/** 阴影一档：`tokens.css` 的 `--shadow-*`（`x y blur 色`）。Compose 画阴影只认 elevation，换算在 [shadowElevation]。 */
@Immutable
data class ThemeShadow(
    val x: Dp,
    val y: Dp,
    val blur: Dp,
    val color: Color,
)

/** 两档阴影在 Compose 里的 elevation：取偏移量（浮层 6 · 模态 12，与稿第 11 节同）。适配层：两边画阴影的方式不同，数值不照抄 blur。 */
val ThemeShadow.shadowElevation: Dp get() = y

/** 间距：桌面同一刻度 2 4 6 8 12 16 24 32（`--space-1` … `--space-8`）。 */
@Immutable
data class Spacing(
    val xxs: Dp = ThemeTokens.Space1,
    val xs: Dp = ThemeTokens.Space2,
    val s: Dp = ThemeTokens.Space3,
    val sm: Dp = ThemeTokens.Space4,
    val md: Dp = ThemeTokens.Space5,
    val lg: Dp = ThemeTokens.Space6,
    val xl: Dp = ThemeTokens.Space7,
    val xxl: Dp = ThemeTokens.Space8,
)

/** 圆角四档（`--radius-*`）：s 徽标 · 行内 code；m 按钮 · 输入 · chip · 键帽；l 卡 · 代码块 · 菜单 · 钉条；xl 对话框 · 抽屉 · 悬浮按钮。 */
@Immutable
data class Radii(
    val s: Dp = ThemeTokens.RadiusS,
    val m: Dp = ThemeTokens.RadiusM,
    val l: Dp = ThemeTokens.RadiusL,
    val xl: Dp = ThemeTokens.RadiusXl,
)

/** 账号身份色一格：底 ＋ 底上的字（`--acct-cN` · `--acct-inkN`）。 */
@Immutable
data class AcctColor(
    val bg: Color,
    val ink: Color,
)

/**
 * Material3 ColorScheme 装不下的令牌，名字照桌面（camelCase）。经 [LocalAppTokens] 取：`LocalAppTokens.current.errorText`。
 * 值全从 [ThemeTokens] 来（`src/shared/theme/tokens.css` 生成），这里只写哪个字段取哪个令牌。
 */
@Immutable
data class AppTokens(
    /** 骨架：顶栏 · 输入区 · 按钮区 · 代码块 · 路径条。 */
    val bg2: Color = ThemeTokens.Bg2,
    /** 凸起：气泡 · 卡 · 抽屉 · 底部抽屉 · 对话框 · 菜单。 */
    val card: Color = ThemeTokens.Card,
    /** 只给占位符 · 禁用态 · 装饰分隔；不写要读的字（要读的用 `onSurfaceVariant` ＝ `--text-2`）。 */
    val textFaint: Color = ThemeTokens.TextFaint,
    /** 当前 / 选中 · 焦点 · 开关开 · 进度 · 光标；不当字色。 */
    val accent: Color = ThemeTokens.Accent,
    /** 选中行淡底。 */
    val selectedBg: Color = ThemeTokens.SelectedBg,
    /** 按下（触屏没有悬停）。 */
    val stateActive: Color = ThemeTokens.StateActive,
    val disabledAlpha: Float = ThemeTokens.StateDisabledOpacity,
    val success: Color = ThemeTokens.Success,
    /** 需手动 · 过期 · 要重启。 */
    val warn: Color = ThemeTokens.Warn,
    /** 后台任务运行中（只给点）。 */
    val bgwork: Color = ThemeTokens.Bgwork,
    /** 红字：危险项 · stderr · 出错的那一句。`colorScheme.error` 只给点 · 条 · 图标。 */
    val errorText: Color = ThemeTokens.ErrorText,
    /** 危险按钮底（只在确认框里）。 */
    val errorStrong: Color = ThemeTokens.ErrorStrong,
    /** 能点开的链接与路径。 */
    val link: Color = ThemeTokens.ColorLink,
    val borderStrong: Color = ThemeTokens.BorderStrong,
    val borderFaint: Color = ThemeTokens.BorderFaint,
    /** 输入框底（与 `--bg-2` 同值）。 */
    val fieldBg: Color = ThemeTokens.FieldBg,
    /** 搜索命中的那几个字的底。 */
    val hitBg: Color = ThemeTokens.HitBg,
    /** diff：左条为主信号，底为辅。 */
    val diffAddBg: Color = ThemeTokens.DiffAddBg,
    val diffAddBorder: Color = ThemeTokens.DiffAddGutter,
    val diffDelBg: Color = ThemeTokens.DiffDelBg,
    val diffDelBorder: Color = ThemeTokens.DiffDelGutter,
    val acct: List<AcctColor> =
        listOf(
            AcctColor(ThemeTokens.AcctC0, ThemeTokens.AcctInk0),
            AcctColor(ThemeTokens.AcctC1, ThemeTokens.AcctInk1),
            AcctColor(ThemeTokens.AcctC2, ThemeTokens.AcctInk2),
            AcctColor(ThemeTokens.AcctC3, ThemeTokens.AcctInk3),
            AcctColor(ThemeTokens.AcctC4, ThemeTokens.AcctInk4),
            AcctColor(ThemeTokens.AcctC5, ThemeTokens.AcctInk5),
            AcctColor(ThemeTokens.AcctC6, ThemeTokens.AcctInk6),
            AcctColor(ThemeTokens.AcctC7, ThemeTokens.AcctInk7),
        ),
    val planKind: List<Color> =
        listOf(
            ThemeTokens.PlanKind0,
            ThemeTokens.PlanKind1,
            ThemeTokens.PlanKind2,
            ThemeTokens.PlanKind3,
            ThemeTokens.PlanKind4,
            ThemeTokens.PlanKind5,
            ThemeTokens.PlanKind6,
        ),
    val shadowFloat: ThemeShadow = ThemeTokens.ShadowFloat,
    val shadowModal: ThemeShadow = ThemeTokens.ShadowModal,
    val spacing: Spacing = Spacing(),
    val radius: Radii = Radii(),
    val dotSize: Dp = ThemeTokens.DotSize,
)

val LocalAppTokens = staticCompositionLocalOf { AppTokens() }
