package com.ccmonitor.mobile.ui.nav

import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.ui.Modifier

/** 一个屏要让开的系统区域。纯本机概念，与后端枚举无关。 */
enum class ScreenInset {
    StatusBars,
    NavigationBars,
    Ime,
}

/**
 * 「设置 · 服务器」那一组屏要让开的三块，按施加顺序排。
 *
 * `MainActivity` 开着 edge-to-edge 且没设 `windowSoftInputMode`，不垫的话标题被状态栏压、
 * 键盘弹起时焦点框被遮。垫的是系统报的值，不写死 dp。
 *
 * 注意：顺序承重。`windowInsetsPadding` 系列会消耗自己垫掉的部分，`NavigationBars` 排在 `Ime`
 * 前面时键盘弹起后底部总量是「手势条 + (键盘 − 手势条) = 键盘」；反过来会把手势条算两遍。
 * 焦点框是否真被滚进可视区，单元测试量不了，需真机验。
 */
val SETTINGS_GROUP_SCREEN_INSETS: List<ScreenInset> =
    listOf(ScreenInset.StatusBars, ScreenInset.NavigationBars, ScreenInset.Ime)

/** 按 [insets] 的顺序逐个垫上去。写成遍历数据，好让垫哪几块、什么顺序能被单元测试钉住。 */
fun Modifier.windowInsetPadding(insets: List<ScreenInset>): Modifier =
    insets.fold(this) { acc, inset ->
        when (inset) {
            ScreenInset.StatusBars -> acc.statusBarsPadding()
            ScreenInset.NavigationBars -> acc.navigationBarsPadding()
            ScreenInset.Ime -> acc.imePadding()
        }
    }

/**
 * 「设置 · 服务器」那一组屏的窗口内边距，挂在每个屏最外层容器上。
 *
 * 注意：要挂在滚动之前（`fillMaxSize().settingsGroupScreenInsets().verticalScroll(…)`），
 * 键盘弹起时缩的才是可滚视口；挂在滚动之后只是在内容里加一段空白，焦点框照样被遮。
 */
fun Modifier.settingsGroupScreenInsets(): Modifier = windowInsetPadding(SETTINGS_GROUP_SCREEN_INSETS)
