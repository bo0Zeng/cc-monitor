package com.ccmonitor.mobile.ui.drawer

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.chat.CHAT_MENU_BANNED_WORDS
import com.ccmonitor.mobile.ui.nav.Screen
import com.ccmonitor.mobile.ui.nav.drawerDestination
import com.ccmonitor.mobile.ui.nav.freshLanding
import com.ccmonitor.mobile.ui.overview.ConversationRow
import com.ccmonitor.mobile.ui.overview.HistoryPaging
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 抽屉：有哪几项、什么顺序、每项去哪、「最近」何时出、别处不再另开这些入口。
 *
 * 布局与手势（☰ 拉出、左边缘右滑、85% 宽）门禁量不到：app 单测不跑 Compose，这几条只钉数据与接线。
 */
class DrawerTest {
    private val newId = { "11111111-2222-3333-4444-555555555555" }

    // ---- 项与顺序 ---------------------------------------------------------------

    @Test
    fun theItemsAndTheirOrderFollowTheDesign() {
        val entries = drawerEntries()
        assertEquals(
            "抽屉的项与顺序：⊕ 新对话 · 对话 · 文件 · 终端（项目那一屏还没有，不画）",
            listOf(DrawerItem.NewConversation, DrawerItem.Conversations, DrawerItem.Files, DrawerItem.Terminal),
            entries.map { it.item },
        )
        assertEquals(listOf("新对话", "对话", "文件", "终端"), entries.map { it.label })
        assertEquals("只有第一项「新对话」是强调色", listOf(true, false, false, false), entries.map { it.accent })
        assertEquals("顶上的名字", "cc-monitor", DRAWER_TITLE)
        assertEquals("「最近」小标题", "最近", RECENT_HEADER)
        assertEquals("底行", "服务器 · devbox", serverRowLabel("devbox"))
    }

    @Test
    fun longPressingNewConversationDoesNothingYet() {
        // 选账号要先接 accounts-list；这次只有一个默认账号 ⇒ 长按没有反应。
        assertTrue("没有一项带长按动作：${drawerEntries()}", drawerEntries().all { it.onLongPress == null })
        val drawer = codeOnly(source("AppDrawer.kt"))
        assertFalse("抽屉里不许接长按", drawer.contains("onLongClick"))
    }

    // ---- 每项去哪 ---------------------------------------------------------------

    @Test
    fun eachItemGoesWhereTheDesignSays() {
        fun go(action: DrawerAction) =
            drawerDestination(
                action = action,
                hostId = "h1",
                newConversationId = newId,
                openFiles = { "session/files-$it" },
                openTerminal = { "session/term-$it" },
            )
        assertEquals(DrawerNav(Screen.Chat.of("h1", newId(), isNew = true), replacesChat = true), go(DrawerAction.NewConversation))
        assertEquals(DrawerNav(Screen.Conversations.of("h1"), replacesChat = false), go(DrawerAction.Conversations))
        assertEquals(DrawerNav("session/files-h1", replacesChat = false), go(DrawerAction.Files))
        assertEquals(DrawerNav("session/term-h1", replacesChat = false), go(DrawerAction.Terminal))
        assertEquals(DrawerNav(Screen.Chat.of("h1", "s7"), replacesChat = true), go(DrawerAction.OpenRecent("s7")))
        assertEquals(DrawerNav(Screen.Hosts.route, replacesChat = false), go(DrawerAction.Servers))
        assertEquals(DrawerNav(Screen.Settings.route, replacesChat = false), go(DrawerAction.Settings))
    }

    @Test
    fun everyItemMapsToItsOwnAction() {
        assertEquals(
            listOf(DrawerAction.NewConversation, DrawerAction.Conversations, DrawerAction.Files, DrawerAction.Terminal),
            drawerEntries().map { it.item.action() },
        )
    }

    @Test
    fun theChatScreenOpensTheDrawerAndTheNavLayerFollowsIt() {
        val route = codeOnly(source("ChatRoute.kt"))
        assertTrue("聊天屏要包在抽屉里", route.contains("ChatDrawer("))
        assertTrue("顶栏左上是 ☰", route.contains("Text(DRAWER_OPEN_GLYPH"))
        val drawer = codeOnly(source("AppDrawer.kt"))
        assertTrue("抽屉画的是那份数据", drawer.contains("drawerEntries()"))
        assertTrue("从左边缘右滑也能拉出", drawer.contains("gesturesEnabled = true"))
        val nav = codeOnly(source("AppNavHost.kt"))
        assertTrue("导航层按 drawerDestination 走", nav.contains("drawerDestination("))
    }

    // ---- 「最近」 -----------------------------------------------------------------

    @Test
    fun recentShowsOnlyTitlesNewestFirst() {
        val rows =
            listOf(
                row("a", "旧的", 100),
                row("b", "新的", 300),
                row("c", "中间", 200),
            )
        assertEquals(listOf("新的", "中间", "旧的"), recentEntries(HistoryPaging(rows = rows, attached = true))!!.map { it.title })
        assertEquals(
            "一行只有标题（和打开它要的编号），没有时间、状态、消息数",
            setOf("sessionId", "title"),
            RecentEntry::class.java.declaredFields
                .map { it.name }
                .filterNot { it.startsWith("$") }
                .toSet(),
        )
    }

    @Test
    fun recentIsAbsentUntilTheListArrives() {
        assertNull("列表还没拿到 ⇒ 整段不出", recentEntries(null))
        assertNull("拿到了但一条都没有 ⇒ 也不出", recentEntries(HistoryPaging(attached = true, loading = true)))
        assertNull(recentEntries(HistoryPaging(attached = true, exhausted = true)))
    }

    @Test
    fun recentFillsTheSpaceWithoutScrollingOrHalfRows() {
        assertEquals("放得下几行就几行", 3, recentFitCount(availablePx = 100 + 3 * 48 + 47, headerPx = 100, rowPx = 48, total = 10))
        assertEquals("不超过手上有的条数", 2, recentFitCount(availablePx = 1000, headerPx = 100, rowPx = 48, total = 2))
        assertEquals("一行都放不下 ⇒ 小标题也不画", 0, recentFitCount(availablePx = 140, headerPx = 100, rowPx = 48, total = 5))
        assertEquals(0, recentFitCount(availablePx = 1000, headerPx = 100, rowPx = 48, total = 0))
        val drawer = codeOnly(source("AppDrawer.kt"))
        assertFalse("「最近」不滚动", drawer.contains("verticalScroll") || drawer.contains("LazyColumn"))
    }

    // ---- 加完一台 ⇒ 这台上空的新对话 --------------------------------------------------

    @Test
    fun addingAServerLandsOnItsEmptyNewConversation() {
        assertEquals(Screen.Chat.of("h9", newId(), isNew = true), freshLanding("h9", AgentKind.ClaudeCode, newId))
        assertEquals("不落聊天屏的档照旧去对话列表", Screen.Conversations.of("h9"), freshLanding("h9", AgentKind.Codex, newId))
        val nav = codeOnly(source("AppNavHost.kt"))
        assertTrue("加一台的那条路由要接上加完之后去哪", nav.contains("onAdded ="))
    }

    // ---- 别处不另开的入口 ---------------------------------------------------------

    @Test
    fun theEntrancesTheDrawerReplacedAreGone() {
        val route = codeOnly(source("ChatRoute.kt"))
        assertFalse("聊天屏 ⋮ 菜单没有项了 ⇒ 不画", route.contains("DropdownMenu"))
        for (gone in listOf("onOpenConversations", "onNewConversation", "onSwitchHost", "onOpenSettings")) {
            assertFalse("聊天屏不再有 $gone 这个入口", route.contains(gone))
        }
        assertFalse("聊天屏顶栏不再有「新对话」", route.contains("CHAT_TOPBAR_VISIBLE_LABEL"))

        val menu = codeOnly(source("ChatMenu.kt"))
        for (gone in listOf("历史对话", "切换机器", "enum class ChatMenuItem")) {
            assertFalse("⋮ 菜单里的「$gone」该归抽屉了", menu.contains(gone) || route.contains(gone))
        }

        val settings = codeOnly(source("SettingsEntry.kt"))
        assertFalse("设置里不再有「打开终端」", settings.contains("打开终端"))

        val conversations = codeOnly(source("ConversationsRoute.kt"))
        assertFalse("对话列表不再有自己的「新对话」", conversations.contains("ExtendedFloatingActionButton"))

        val hosts = codeOnly(source("HostScreens.kt"))
        assertFalse("服务器列表不再有自己进设置的那颗", Regex("""\bonSettings\b""").containsMatchIn(hosts))
    }

    // ---- 文案 -----------------------------------------------------------------------

    @Test
    fun theDrawerWordingFollowsTheCopyRules() {
        val onScreen =
            drawerEntries().map { it.label } +
                listOf(DRAWER_TITLE, RECENT_HEADER, serverRowLabel("devbox"), SETTINGS_DESCRIPTION, DRAWER_OPEN_DESCRIPTION)
        for (text in onScreen) {
            assertFalse("不许第二人称：$text", text.contains("你") || text.contains("您"))
            val hit = CHAT_MENU_BANNED_WORDS.filter { it.lowercase() in text.lowercase() }
            assertTrue("「$text」里有内部词 $hit", hit.isEmpty())
        }
    }

    private fun row(
        id: String,
        title: String,
        at: Long,
    ) = ConversationRow(sessionId = id, title = title, cwd = null, jsonlPath = null, messageCount = 3, updatedAtMs = at)

    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    private fun source(name: String): String {
        val dir = File("src/main/kotlin").takeIf { it.isDirectory } ?: File("app/src/main/kotlin")
        return dir.walkTopDown().first { it.isFile && it.name == name }.readText()
    }
}
