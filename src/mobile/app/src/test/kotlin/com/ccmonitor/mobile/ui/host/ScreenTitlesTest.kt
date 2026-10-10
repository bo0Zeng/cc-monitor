package com.ccmonitor.mobile.ui.host

import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * 「设置 · 服务器」那一组屏的标题判据。
 *
 * 英文路由名（`Hosts`、`Identities`）不许直接上屏当标题；`Hosts` 那一屏还是没有机器时冷启动的落点。
 * `ChatMenuTest` 的词表只管聊天屏，这一组六个屏的标题由本文件管。
 */
class ScreenTitlesTest {
    /** 一个串是不是「看起来像路由名」＝ 全是 ASCII 可见字符且含字母。 */
    private fun looksLikeRouteName(s: String): Boolean =
        s.isNotEmpty() && s.all { it.code in 32..126 } && s.any { it.isLetter() }

    @Test
    fun settingsGroupTitlesAreNotRouteNames() {
        for (t in listOf(HOSTS_SCREEN_TITLE, IDENTITY_SCREEN_TITLE)) {
            assertFalse(
                "屏标题「$t」看起来是英文路由名 —— 路由名漏上屏这个病犯过两次（Hosts / Identities）",
                looksLikeRouteName(t),
            )
        }
    }

    @Test
    fun hostsTitleUsesTheProductWordForMachines() =
        assertFalse(
            "「SSH 主机」一律说「服务器」。实得「$HOSTS_SCREEN_TITLE」",
            HOSTS_SCREEN_TITLE.contains("主机") || HOSTS_SCREEN_TITLE.contains("SSH", ignoreCase = true),
        )
}
