package com.ccmonitor.mobile.ui.nav

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.ui.chat.chatKey
import com.ccmonitor.mobile.ui.chat.newConversationId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** 新界面的路由：命名空间、身份、落点与新/旧对话之分。 */
class NavRoutesTest {
    /**
     * 两条路由不许撞进 `sessions*` 那个命名空间。
     *
     * `Screen.SessionsOverview`（`"sessions"`）是终端 tab 总览，与 [Screen.Conversations]（Claude 对话总览）
     * 完全是两件事，而它们的屏名只差一个字母（`SessionsOverviewScreen` vs `SessionOverviewScreen`）。
     * 撞了的话 nav 会把两个毫不相干的屏当成同一条路由。
     */
    @Test
    fun theNewRoutesDoNotCollideWithTheTerminalTabOverview() {
        assertNotEquals(Screen.SessionsOverview.route, Screen.Conversations.route)
        assertFalse(
            "对话总览不许落进 `sessions` 前缀：${Screen.Conversations.route}",
            Screen.Conversations.route.startsWith("sessions"),
        )
        assertFalse("聊天路由同理：${Screen.Chat.route}", Screen.Chat.route.startsWith("sessions"))
    }

    /**
     * 聊天路由必须带主机。
     *
     * 一个 Claude 对话由「哪台主机 + 哪个对话编号」共同确定：只用后者的话，
     * 两台主机上碰巧同号的对话会被 `ChatController` 当成同一个对话，
     * 于是串台：在 A 机的对话里看到 B 机的内容。
     */
    @Test
    fun aChatIsIdentifiedByHostAndSessionNotSessionAlone() {
        assertTrue("路由要带 hostId：${Screen.Chat.route}", Screen.Chat.route.contains("{hostId}"))
        assertTrue("路由要带 sid：${Screen.Chat.route}", Screen.Chat.route.contains("{sid}"))
        // 路由带一个可选查询参数 `?new=`，所以这里只钉路径那一段。
        //   钉整串会把「加了一个新参数」误报成「身份变了」；身份只由 host+sid 决定。
        assertEquals("chat/h1/s1", Screen.Chat.of("h1", "s1").substringBefore('?'))
        assertNotEquals("同号不同机必须是两个对话", chatKey("h1", "s"), chatKey("h2", "s"))
    }

    /** 路由构造出来的串要能被路由模板匹配上，否则 `navigate` 会当场找不到目的地。 */
    @Test
    fun theBuiltPathsMatchTheirTemplates() {
        assertEquals(Screen.Conversations.route.replace("{hostId}", "h1"), Screen.Conversations.of("h1"))
        assertEquals(
            Screen.Chat.route
                .replace("{hostId}", "h1")
                .replace("{sid}", "s1")
                // `new` 是可选查询参数，缺省填保守的一侧（已有对话）
                .replace("{${Screen.Chat.ARG_NEW}}", "false"),
            Screen.Chat.of("h1", "s1"),
        )
    }

    /** 开关关着（含从没设过）时，点主机去终端那条路。 */
    @Test
    fun withTheFlagOffConnectingOpensTheTerminal() {
        val off =
            onConnectDestination(
                hostId = "h1",
                newUi = false,
                // 种类是必填的（无默认值）：`newUi × agentKind` 四格在 `AgentArchetypeTest` 里钉
                agentKind = AgentKind.ClaudeCode,
                newConversationId = { "s-new" },
            ) { "session/tab-$it" }
        assertTrue("关着时必须还是终端那条路：$off", off.startsWith("session/"))
        assertFalse("关着时不许出现对话总览", off.startsWith("conversations"))
    }

    /** 开关打开 ⇒ 这台上空的新建会话（不回上次那条）。 */
    @Test
    fun withTheFlagOnTheDestinationIsAFreshChat() {
        val on =
            onConnectDestination(
                hostId = "h1",
                newUi = true,
                // 种类是必填的（无默认值）：`newUi × agentKind` 四格在 `AgentArchetypeTest` 里钉
                agentKind = AgentKind.ClaudeCode,
                newConversationId = { "s-new" },
            ) { "session/tab-$it" }
        assertEquals(Screen.Chat.of("h1", "s-new", isNew = true), on)
    }

    /**
     * 开关的判据是 `== "1"`，不是「有没有值」。
     *
     * 写成「有值即开」的话，显式关掉（存 `"0"`）会被读成开：
     * 关不掉这个开关，而且从代码上看不出来哪里错了。
     */
    @Test
    fun anExplicitlyDisabledFlagCountsAsOff() {
        assertEquals("开关的开值", "1", SettingsRepository.FLAG_ON)
        assertNotEquals("关值必须与开值不同，且都不是空串", SettingsRepository.FLAG_ON, SettingsRepository.FLAG_OFF)
        assertTrue("关值不能是空串（空串会与「没设过」混同）", SettingsRepository.FLAG_OFF.isNotEmpty())
    }

    /**
     * 用户可见的路由名要过禁用词表。
     *
     * 「会话」在技术义上是禁用词，对用户说「对话」。
     * 路由名虽然不直接上屏，但它是给后来者看的命名基准，跑偏了屏上文案迟早跟着跑偏。
     */
    @Test
    fun theNewRouteNamesFollowTheProductVocabulary() {
        val banned = listOf("ssh", "tmux", "bridge", "daemon", "offset", "exec")
        for (r in listOf(Screen.Conversations.route, Screen.Chat.route)) {
            for (w in banned) {
                assertFalse("路由名不许出现禁用词「$w」：$r", r.lowercase().contains(w))
            }
        }
        // 抽屉「新对话」那一项的字是真上屏的，比路由名更该守这份词表。
        val newLabel =
            com.ccmonitor.mobile.ui.drawer
                .drawerEntries()
                .first()
                .label
        for (w in listOf("会话", "连接", "SSH", "tmux", "bridge", "daemon")) {
            assertFalse("「新对话」的字不许出现禁用词「$w」：$newLabel", newLabel.contains(w))
        }
        assertTrue("它得真说「对话」：这是词表指定的替代说法", newLabel.contains("对话"))
    }

    // ---- 新界面必须能开出第一个对话 -------------------------------

    /**
     * 「开新对话」这条路由真的存在，且与「打开已有对话」可辨。
     *
     * 只有对话列表行点击一个调用方的话，列表读不到（远端没有那个来源）时新界面就是死胡同，
     * 连第一个对话都开不出来。
     *
     * 判据落在「导航层能不能表达『这是新开的』」上：下游 `ChatRoute` 要靠它
     * 决定是发第一条时请那台起（新建会话），还是读那条已有的。
     */
    @Test
    fun openingANewConversationIsDistinguishableFromOpeningAnExistingOne() {
        val existing = Screen.Chat.of("h1", "sid-1")
        val fresh = Screen.Chat.of("h1", "sid-1", isNew = true)
        assertNotEquals("两者必须可辨，否则新对话会被当成已有的去 resume", existing, fresh)
        assertTrue("新开的那条要带得出标记：$fresh", fresh.contains("${Screen.Chat.ARG_NEW}=true"))
        assertTrue("缺省必须是保守的一侧（已有对话）：$existing", existing.contains("${Screen.Chat.ARG_NEW}=false"))
    }

    /** 新建会话那张草稿的票：每张不同（同一张票再问 `session-new`，那台认出是同一趟、不起第二个）。 */
    @Test
    fun everyDraftGetsItsOwnTicket() {
        val ids = List(20) { newConversationId() }
        assertEquals("票不许重复：撞票就是两个草稿被那台当成同一趟", ids.size, ids.toSet().size)
    }

    /**
     * 路由字符串必须能被 nav 的模板匹配上。
     *
     * `of()` 与 `route` 是两处拼出来的，写歪了就会「点了没反应」，
     * 而那种失败在单测里不显形，只有真机点一下才知道。这里用模板逐段比对。
     */
    @Test
    fun theGeneratedRouteMatchesItsOwnTemplate() {
        val template =
            Screen.Chat.route
                .substringBefore('?')
                .split('/')
        val actual =
            Screen.Chat
                .of("h1", "sid-1", isNew = true)
                .substringBefore('?')
                .split('/')
        assertEquals("路径段数对不上：$template vs $actual", template.size, actual.size)
        template.forEachIndexed { i, seg ->
            if (!seg.startsWith("{")) assertEquals("第 $i 段应是字面量 $seg", seg, actual[i])
        }
        assertEquals("查询参数名要与模板一致", "${Screen.Chat.ARG_NEW}={${Screen.Chat.ARG_NEW}}", Screen.Chat.route.substringAfter('?'))
    }
}
