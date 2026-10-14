package com.ccmonitor.mobile.core.ui.theme

import androidx.compose.ui.unit.dp

/**
 * 手机适配层：手指比鼠标指针粗、手机有顶栏 —— 这些尺寸只有手机有，不进 `tokens.css`。
 * 画出来的按钮可以小于 48，命中区不能（M3 `minimumInteractiveComponentSize` 同值）。
 */
object Touch {
    /** 可点物命中区下限。 */
    val target = 48.dp

    /** 顶栏高。 */
    val topBar = 56.dp

    /** 列表行：单行 · 双行 · 设置一项 · 文件行 · 抽屉项。 */
    val rowSingle = 48.dp
    val rowDouble = 64.dp
    val rowSetting = 56.dp
    val rowFile = 52.dp
    val rowDrawer = 52.dp

    /** 图标：顶栏 / 抽屉 / 行首 20 · 行内与键帽 16（`--icon-size`）· 空态 32（`--icon-size-empty`）。 */
    val iconBar = 20.dp
    val iconInline = ThemeTokens.IconSize
    val iconEmpty = ThemeTokens.IconSizeEmpty
}
