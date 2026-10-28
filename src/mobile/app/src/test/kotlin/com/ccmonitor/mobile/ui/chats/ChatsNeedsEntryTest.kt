package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 会话列表顶栏的「需手动 n」与聊天屏那一颗是同一份（`NeedsEntry`：同一个数 · 同一个去处 · 0 件不画 · 没读全不带数），不另写一颗。
 * 那颗本身的字（0 ⇒ 不画 · n ⇒「需手动 n」· 说不准 ⇒ 不带数）由 `NeedsBoardTest` 钉；这里钉两个顶栏都摆它、点了都进需手动屏。
 */
class ChatsNeedsEntryTest {
    private val root = generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }.first { File(it, "settings.gradle.kts").isFile }

    private fun code(rel: String) = KotlinSourceScanner.codeOnlyKeepingLiterals(File(root, "app/src/main/kotlin/com/ccmonitor/mobile/$rel").readText())

    @Test
    fun `会话列表顶栏摆的是聊天屏那同一颗需手动，点了进需手动屏`() {
        val chats = code("ui/chats/ChatsRoute.kt")
        val bar = chats.substringAfter("private fun ChatsTopBar(").substringBefore("\n}\n")
        assertTrue("会话列表顶栏里要有那一颗（NeedsEntry）", bar.contains("NeedsEntry("))
        assertTrue("聊天屏顶栏照旧摆那一颗", code("ui/chat/ChatRoute.kt").contains("NeedsEntry("))
        val nav = code("ui/nav/AppNavHost.kt")
        val chatsRoute = nav.substringAfter("ChatsRoute(").substringBefore("\n        )\n")
        assertTrue("会话列表那一颗点了进需手动屏", chatsRoute.contains("onOpenOverview = { nav.navigate(Screen.Overview.of(hostId)) }"))
        assertTrue("会话列表里不另写一颗（只有 NeedsEntry 那一份画它）", !chats.contains("NeedsEntryView("))
    }
}
