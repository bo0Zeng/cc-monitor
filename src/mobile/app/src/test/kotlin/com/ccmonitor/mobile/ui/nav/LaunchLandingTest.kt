package com.ccmonitor.mobile.ui.nav

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 打开 app 就是聊天屏：落到上次那台机器上的一个空的新对话。
 *
 * 判据落在纯函数 [launchDestination]（启动那一刻去哪儿）上；不回上次那条会话。
 * 组合层（`launchRoute`）只负责把上次那台机器读出来、核对过还在，再喂给它。
 *
 * 判别力边界：纯函数判据挡不住「`launchRoute` 忘了核对那台主机还在不在」，
 * 也挡不住顶栏入口点了没反应；前者要 instrumented，后者要真机。
 */
class LaunchLandingTest {
    private val newId = { "11111111-2222-3333-4444-555555555555" }

    /** 启动即聊天：上次那台机器还在 ⇒ 落到聊天屏，而不是先看一屏列表。 */
    @Test
    fun theAppOpensStraightIntoTheChatScreen() {
        val to =
            launchDestination(
                landingHostId = "h1",
                newConversationId = newId,
            )
        assertTrue("启动必须落在聊天屏：$to", to.startsWith("chat/"))
        assertFalse("不许落在对话总览上", to.startsWith("conversations"))
        assertEquals("打开 app 落在空的新对话，不回上次那条", Screen.Chat.of("h1", newId(), isNew = true), to)
    }

    /**
     * 上次那台机器没有（第一次用，或那台主机被删了）⇒ 只能去主机列表。
     *
     * 这条不是补边界，是防一类真事故：`landingHostId` 为 null 时若照样拼聊天路由，
     * 拼出来的是 `chat/null/…` 或 `chat//…` —— 要么匹配不上模板（导航当场找不到目的地），
     * 要么带着一个不存在的主机进屏（断开 + 满屏报错）。
     */
    @Test
    fun withNoRememberedHostItFallsBackToTheHostList() {
        val to =
            launchDestination(
                landingHostId = null,
                newConversationId = newId,
            )
        assertEquals("没有可落地的机器时只能去主机列表", Screen.Hosts.route, to)
    }

    /**
     * 对话列表、服务器列表、设置只从抽屉进 ⇒ 聊天屏那一个回调必须是必填参数：
     * 给了 `= {}` 默认值，某个调用方就能把整个抽屉静默焊死。
     */
    @Test
    fun theDrawerActionIsAMandatoryParameter() {
        val src = codeOnly(chatRouteSource())
        assertTrue("前提：得真读到 ChatRoute.kt 的代码", src.contains("fun ChatRoute("))
        val decl = Regex("""onDrawerAction: \(DrawerAction\) -> Unit(\s*=)?""").find(src)
        assertTrue("前提：onDrawerAction 得真是 ChatRoute 的参数", decl != null)
        assertFalse("onDrawerAction 不许有默认值", decl!!.value.contains("="))
        assertFalse("导航层不许把它接成空动作", codeOnly(appNavHostSource()).contains("onDrawerAction = {}"))
    }

    private fun chatRouteSource(): String {
        val f =
            java.io.File("src/main/kotlin/com/ccmonitor/mobile/ui/chat/ChatRoute.kt").takeIf { it.isFile }
                ?: java.io.File("app/src/main/kotlin/com/ccmonitor/mobile/ui/chat/ChatRoute.kt")
        return f.readText()
    }

    /** 读 `AppNavHost.kt` 本体 —— 采集点那一行住在这里。 */
    private fun appNavHostSource(): String {
        val f =
            java.io.File("src/main/kotlin/com/ccmonitor/mobile/ui/nav/AppNavHost.kt").takeIf { it.isFile }
                ?: java.io.File("app/src/main/kotlin/com/ccmonitor/mobile/ui/nav/AppNavHost.kt")
        return f.readText()
    }

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)
}
