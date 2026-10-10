package com.ccmonitor.mobile.ui.nav

import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.ui.chat.REMOTE_FAILED_PREFIX
import com.ccmonitor.mobile.ui.chat.chatKey
import com.ccmonitor.mobile.ui.chat.newConversationId
import com.ccmonitor.mobile.ui.chat.newSessionIdFor
import com.ccmonitor.mobile.ui.chat.resumeIdFor
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
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
                lastConversationId = null,
                newConversationId = { "s-new" },
            ) { "session/tab-$it" }
        assertTrue("关着时必须还是终端那条路：$off", off.startsWith("session/"))
        assertFalse("关着时不许出现对话总览", off.startsWith("conversations"))
    }

    /**
     * 开关打开 ⇒ 直接进聊天屏。
     *
     * 落点表在 `chatLanding` 的 KDoc 里，逐条判据在 `LaunchLandingTest`。
     */
    @Test
    fun withTheFlagOnTheDestinationIsTheChatScreen() {
        val on =
            onConnectDestination(
                hostId = "h1",
                newUi = true,
                // 种类是必填的（无默认值）：`newUi × agentKind` 四格在 `AgentArchetypeTest` 里钉
                agentKind = AgentKind.ClaudeCode,
                lastConversationId = "s7",
                newConversationId = { "s-new" },
                // 记住的编号 == 确认过的 ⇒ 才允许按「已存在」恢复它。
                confirmedConversationId = "s7",
            ) { "session/tab-$it" }
        assertEquals(Screen.Chat.of("h1", "s7"), on)
        assertFalse("不许落在对话总览上", on.startsWith("conversations"))
    }

    /**
     * 记住了但从没被确认过的编号，必须按「新对话」起。
     *
     * 连接失败那次也可能把编号记下了；下次落地 `--resume` 一个远端从没有过的编号，Claude 当场退出
     * （`No conversation found with session ID: …`），整轮死掉。`SettingsRepository.getLastConversation`
     * 的头注写着「拿它的地方必须容忍打开一个空对话」，本条就是那句话的判据形态。
     *
     * 判据钉的是 `isNew=true` 这个具体差别，不是「返回了某个聊天路由」：后者在 bug 存在时也成立。
     */
    @Test
    fun aRememberedButUnconfirmedConversationStartsFreshInsteadOfResuming() {
        val unconfirmed =
            onConnectDestination(
                hostId = "h1",
                newUi = true,
                // 种类是必填的（无默认值）：`newUi × agentKind` 四格在 `AgentArchetypeTest` 里钉
                agentKind = AgentKind.ClaudeCode,
                lastConversationId = "s7",
                newConversationId = { "s-new" },
                confirmedConversationId = null,
            ) { "session/tab-$it" }
        assertEquals(
            "没被确认过的编号必须按新对话起（否则就是 resume 一个不存在的对话）",
            Screen.Chat.of("h1", "s7", isNew = true),
            unconfirmed,
        )

        // 阴性对照：确认过的是另一个编号 ⇒ 同样不许当成已存在
        val mismatched =
            onConnectDestination(
                hostId = "h1",
                newUi = true,
                // 种类是必填的（无默认值）：`newUi × agentKind` 四格在 `AgentArchetypeTest` 里钉
                agentKind = AgentKind.ClaudeCode,
                lastConversationId = "s7",
                newConversationId = { "s-new" },
                confirmedConversationId = "s-other",
            ) { "session/tab-$it" }
        assertEquals(
            "判据必须是逐字相等，不是「确认过的非空」",
            Screen.Chat.of("h1", "s7", isNew = true),
            mismatched,
        )
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

        // 上屏的失败说明同理：它后面还会拼上远端原话，
        //   所以前缀本身必须干净，不能把 `bridge.log` 这种内部名字端给用户。
        for (w in listOf("会话", "连接", "SSH", "tmux", "bridge", "daemon", "exec", "offset", "log")) {
            assertFalse("失败说明不许出现禁用词「$w」：$REMOTE_FAILED_PREFIX", REMOTE_FAILED_PREFIX.lowercase().contains(w.lowercase()))
        }
        assertTrue("它得说清是远端那个 Claude 的事", REMOTE_FAILED_PREFIX.contains("远端") && REMOTE_FAILED_PREFIX.contains("Claude"))
    }

    // ---- 新界面必须能开出第一个对话 -------------------------------

    /**
     * 「开新对话」这条路由真的存在，且与「打开已有对话」可辨。
     *
     * 只有对话列表行点击一个调用方的话，列表读不到（远端没有那个来源）时新界面就是死胡同，
     * 连第一个对话都开不出来。
     *
     * 判据落在「导航层能不能表达『这是新开的』」上：下游 `ChatRoute` 要靠它
     * 决定加不加 `--resume`，拿一个远端从没见过的编号去 resume 必然失败。
     */
    @Test
    fun openingANewConversationIsDistinguishableFromOpeningAnExistingOne() {
        val existing = Screen.Chat.of("h1", "sid-1")
        val fresh = Screen.Chat.of("h1", "sid-1", isNew = true)
        assertNotEquals("两者必须可辨，否则新对话会被当成已有的去 resume", existing, fresh)
        assertTrue("新开的那条要带得出标记：$fresh", fresh.contains("${Screen.Chat.ARG_NEW}=true"))
        assertTrue("缺省必须是保守的一侧（已有对话）：$existing", existing.contains("${Screen.Chat.ARG_NEW}=false"))
    }

    /**
     * 新编号要能安全地当作远端标识。
     *
     * 它不只是界面 key：会变成远端目录名与那条常驻命令的标识，
     * 所以必须过 `ClaudeInvocation.isValidSessionId`。这里调真的那个校验函数，
     * 不在测试里另写一份正则（同一份判据两份实现会各自漂移）。
     */
    @Test
    fun aFreshConversationIdIsAcceptedByTheRealSessionIdCheck() {
        val ids = List(20) { newConversationId() }
        ids.forEach {
            assertTrue("生成的编号被真校验拒了：$it", ClaudeInvocation.isValidSessionId(it))
            // 它还要当得了 Claude 自己的对话编号：`--session-id` 只认合法 UUID。
            //   过不了这关，我们就只能让 Claude 自己另发一个 ⇒ 两个编号分叉 ⇒ 下次找不回来。
            assertTrue("还必须是合法 UUID，否则交不给 Claude：$it", ClaudeInvocation.isValidUuid(it))
        }
        assertEquals("编号不许重复 —— 撞号就是两个对话共用一条远端管道", ids.size, ids.toSet().size)
    }

    /**
     * 「接着跑」与「把编号交出去」是同一个布尔的两面，永远只能有一个。
     *
     * 两个都给会被 `ClaudeInvocation.pipeInvocation` 当场炸掉；
     * 两个都不给则 Claude 自己另发一个编号 ⇒ 分叉。
     * 判据落在「任何一种情形下恰好给一个」上，而不是分别断言两个函数。
     */
    @Test
    fun exactlyOneOfResumeAndHandOverIsChosen() {
        for (isNew in listOf(true, false)) {
            val chosen = listOfNotNull(resumeIdFor("s1", isNew), newSessionIdFor("s1", isNew))
            assertEquals("isNew=$isNew 时必须恰好给一个，实得 $chosen", 1, chosen.size)
            assertEquals("给出去的必须就是这条对话的编号", "s1", chosen.single())
        }
        assertNull("新对话不 resume", resumeIdFor("s1", isNew = true))
        assertNull("已有对话不重发编号", newSessionIdFor("s1", isNew = false))
    }

    /**
     * 新开的对话不许 `--resume`；已有的必须 resume。
     *
     * 新对话的编号是本机现生成的、远端从没见过，接它必然失败。
     * 反过来漏掉已有对话的 resume 也不行：那会起一条同名的空对话。
     *
     * 判据用真的 `newConversationId()` 产的编号，不自己捏一个，免得测的是想象中的形状。
     */
    @Test
    fun aFreshConversationIsNotResumedButAnExistingOneIs() {
        val fresh = newConversationId()
        assertNull("新开的对话不许 resume：远端根本没有这个编号", resumeIdFor(fresh, isNew = true))

        val existing = "abc-123"
        assertEquals("已有对话必须接着跑，否则会起一条同名空对话", existing, resumeIdFor(existing, isNew = false))
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
