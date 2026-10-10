package com.ccmonitor.mobile.ui.nav

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 打开 app 就是聊天屏：落到上次那台机器上的一个空的新对话。
 *
 * 判据落在纯函数 [launchDestination]（启动那一刻去哪儿）上；不回上次那条会话。
 * 组合层（`launchRoute`）只负责把三个异步事实读出来喂给它们。
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
                newUi = true,
                landingHostId = "h1",
                // 种类是必填的（无默认值）—— Codex 那一格在 `AgentArchetypeTest` 里单独钉
                agentKind = AgentKind.ClaudeCode,
                newConversationId = newId,
            )
        // 返回值可以是 null（=「还不知道，别导航」）；真值到了却回 null 就是永远不导航，比落错屏更糟。
        assertNotNull("真值「开」已经到了，必须给得出落点，不许还回「不知道」", to)
        assertTrue("启动必须落在聊天屏：$to", to!!.startsWith("chat/"))
        assertFalse("不许落在对话总览上", to.startsWith("conversations"))
        assertEquals("打开 app 落在空的新对话，不回上次那条", Screen.Chat.of("h1", newId(), isNew = true), to)
    }

    /**
     * 「还不知道」不是「关」：开关没读出来之前一步都不许导航。
     *
     * `AppNavHost` 读开关走 `collectAsStateWithLifecycle`，第一帧拿到的是初值。初值若是 `false`，
     * `LaunchedEffect` 会拿着假的「关」先导航去服务器列表，并用 `popUpTo(Screen.Launch) { inclusive = true }`
     * 把自己弹掉，真值随后到了也没人再看。所以落地判定的输入是三值：`null` ⇒ 回 `null` ⇒ 不导航。
     *
     * 这条只钉纯函数契约；composable 里初值写的是什么，由
     * [theLaunchFlagStartsOutUnknownInsteadOfPretendingItIsOff]（源码扫描）接住。
     */
    @Test
    fun whileTheFlagIsStillUnknownNothingIsNavigatedTo() {
        val to =
            launchDestination(
                newUi = null,
                landingHostId = "h1",
                agentKind = AgentKind.ClaudeCode,
                newConversationId = newId,
            )
        assertNull(
            "开关还没读出来时不许给落点 —— 给了就会把 Launch 弹掉（popUpTo inclusive），" +
                "真值到了也没地方回。实得：$to",
            to,
        )
    }

    /**
     * 三个输入必须给出三种结果 —— null 不许被折进 `true`/`false` 任何一侧。
     *
     * 上面那条只造 `null` 一格。只有那一格的话，
     * 一个「无论输入什么都回 null」的实现也全绿 —— 那等于 app 永远停在 `Launch` 空屏上。
     * 这里把三值一次过完，并断言三个结果互不相同：
     * 把 null 折成 `false`（回 Hosts）⇒ 最后那一句当场红；折成 `true` 亦然。
     */
    @Test
    fun theRealValueDecidesOnlyAfterItArrives() {
        fun landing(newUi: Boolean?) =
            launchDestination(
                newUi = newUi,
                landingHostId = "h1",
                agentKind = AgentKind.ClaudeCode,
                newConversationId = newId,
            )
        assertNull("还不知道 ⇒ 不导航", landing(null))
        assertEquals("真值「开」到了 ⇒ 聊天屏上空的新对话", Screen.Chat.of("h1", newId(), isNew = true), landing(true))
        assertEquals("真值「关」到了 ⇒ 服务器列表", Screen.Hosts.route, landing(false))
        assertEquals(
            "三个输入必须给出三个不同的结果 —— 相同就说明「还不知道」又被折成了「关」或「开」，" +
                "而那正是冷启动被弹去服务器列表的原形",
            3,
            setOf(landing(null), landing(true), landing(false)).size,
        )
    }

    /**
     * 采集点的初值必须是「还不知道」(`null`)，不是 `false`。
     *
     * 纯函数契约再对，只要 composable 里那一行仍写 `collectAsStateWithLifecycle(false)`，
     * 第一帧送进去的就是假的「关」。这条把那一行的字面钉住。
     *
     * 判别力边界：初值改回 `false` 会红；换成 `produceState<Boolean?>(null)` 之类会误红（安全侧）；
     * 把 `null` 藏进常量再传进去抓不到。它量的是源码字面，不是运行期第一帧。
     */
    @Test
    fun theLaunchFlagStartsOutUnknownInsteadOfPretendingItIsOff() {
        val code = codeOnly(appNavHostSource())
        val at = code.indexOf(FLAG_COLLECT)
        assertTrue("前提：得找到读开关那一句 `$FLAG_COLLECT`，否则本条判据恒绿", at >= 0)
        val line = code.lineSequence().first { FLAG_COLLECT in it }
        assertTrue(
            "开关的初值必须是「还不知道」(`null`)：写 `false` 的话第一帧就拿着一个假的「关」" +
                "跑完落地判定、导航去服务器列表并把 Launch 弹掉，真值到了也晚了" +
                "。实得那一行：$line",
            UNKNOWN_INITIAL in line,
        )
        assertFalse("不许把「还不知道」写成 `false`。实得那一行：$line", FALSE_INITIAL in line)
    }

    /**
     * 上次那台机器没有（第一次用，或那台主机被删了）⇒ 只能去主机列表。
     *
     * 这条不是补边界，是防一类真事故：`landingHostId` 为 null 时若照样拼聊天路由，
     * 拼出来的是 `chat/null/…` 或 `chat//…` —— 要么匹配不上模板（导航当场找不到目的地），
     * 要么带着一个不存在的主机进屏（连不上 + 满屏报错）。
     */
    @Test
    fun withNoRememberedHostItFallsBackToTheHostList() {
        val to =
            launchDestination(
                newUi = true,
                landingHostId = null,
                agentKind = AgentKind.ClaudeCode,
                newConversationId = newId,
            )
        assertEquals("没有可落地的机器时只能去主机列表", Screen.Hosts.route, to)
    }

    /** 开关关着 ⇒ 启动落点是服务器列表。 */
    @Test
    fun withTheFlagOffTheAppStillOpensOnTheHostList() {
        val to =
            launchDestination(
                newUi = false,
                landingHostId = "h1",
                agentKind = AgentKind.ClaudeCode,
                newConversationId = newId,
            )
        assertEquals(Screen.Hosts.route, to)
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
     * 扫的是 `collectAsStateWithLifecycle<Boolean?>(null)` 这类代码表达式；它看不见什么写在
     * [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    companion object {
        /** `AppNavHost` 里读那个开关的那一句 —— 定位用，认不出就前提断言先红。 */
        private const val FLAG_COLLECT = "newUiEnabled().collectAsStateWithLifecycle"

        /** 初值必须逐字是它：`Boolean?` 那个显式类型参数是「三值」这件事在源码里唯一看得见的痕迹。 */
        private const val UNKNOWN_INITIAL = "collectAsStateWithLifecycle<Boolean?>(null)"

        /** 竞态的原形：初值写成「关」。 */
        private const val FALSE_INITIAL = "collectAsStateWithLifecycle(false)"
    }
}
