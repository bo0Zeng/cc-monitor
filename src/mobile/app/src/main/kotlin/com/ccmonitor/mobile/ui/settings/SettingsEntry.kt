package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.ui.nav.Screen

/**
 * 设置面上一条可点的去处：属于哪一节、显示什么字、点了去哪条路由。
 *
 * @property section 节标题（上屏的字）。
 * @property title 行标题（上屏的字）。
 * @property route 目的地路由，必须是 [Screen] 里真有的那一条。
 */
data class SettingsEntry(
    val section: String,
    val title: String,
    val route: String,
)

/**
 * 设置面上那些入口的唯一住址，别把任何一条写回 composable 里。
 *
 * 入口做成数据，测试才守得住「入口还在」：只看路由还在不在导航图里，把入口删了也照样绿；
 * 而 Compose 里的表达式没有单元测试够得着。注意：这份数据守不住那一行真的画出来、也不管几跳可达。
 *
 * 「打开终端」不在这里：终端从抽屉「终端」进，每个功能只有一处入口。
 */
fun settingsEntries(): List<SettingsEntry> =
    listOf(
        SettingsEntry(SECTION_TERMINAL, "自定义按钮（全局）", Screen.ButtonSettings.route),
        // 身份从服务器列表里进；这里给的是到服务器列表那一跳。
        SettingsEntry(SECTION_TERMINAL, "服务器与身份", Screen.Hosts.route),
    )

/** 节标题就叫「终端」，不叫「高级」「更多」：找终端的人应该直接看见这两个字。 */
const val SECTION_TERMINAL = "终端"
