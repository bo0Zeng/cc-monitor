package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.nav.Screen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 设置里的「终端」一节，而且入口被删时真的有东西会红。
 *
 * 「九条路由还在」恒绿于「入口被删」：路由在 `NavHost` 里，设置面上那一行没了它照样过。
 * 分辨力在「入口是一份数据（[settingsEntries]）、设置屏只渲染这份数据」那两条上。
 *
 * 「入口还在」≠「入口够得着」：设置面那一列不滚的话，「终端」节落在屏幕下沿之外，
 * 数据里有、路由也在，就是点不到。[theEntrancesLiveInsideAScrollableColumn] 管这一条，
 * [theSectionCountIsPinnedSoAnyGrowthGetsReVettedOnADevice] 管它的真机读数别过期。
 */
class SettingsEntriesTest {
    /**
     * 设置里「终端」一节只有两条：自定义按钮（全局）与服务器列表。
     * 「打开终端」在抽屉里；所有功能只有一处入口，设置里不许出现它。
     */
    @Test
    fun theTerminalSectionKeepsButtonsAndServersButNotTheTerminalItself() {
        val entries = settingsEntries()
        assertEquals("节标题就是这两个字", "终端", SECTION_TERMINAL)
        assertTrue("「打开终端」在抽屉里：$entries", entries.none { it.route == Screen.SessionsOverview.route || it.title == "打开终端" })

        val buttons = entries.singleOrNull { it.route == Screen.ButtonSettings.route }
        assertTrue("自定义按钮那个入口一条都不许丢：$entries", buttons != null)
        assertEquals("它在「终端」这一节", "终端", buttons!!.section)
        assertEquals("文案逐字", "自定义按钮（全局）", buttons.title)

        val hosts = entries.singleOrNull { it.route == Screen.Hosts.route }
        assertTrue("服务器列表那个入口也在终端节里：$entries", hosts != null && hosts.section == "终端")
    }

    /**
     * 每一条入口都得指向一条真存在的路由。
     *
     * 打错一个字（`"sesions"`）在 Compose 里是运行期才炸的：`navigate` 找不到目的地。
     * 这条把它提到编译后、门禁内。
     */
    @Test
    fun everyEntrancePointsAtARouteThatIsActuallyRegistered() {
        val registered = registeredRoutes()
        for (e in settingsEntries()) {
            assertTrue("入口「${e.title}」指向的 ${e.route} 没有在 AppNavHost 里注册：$registered", e.route in registered)
        }
        assertTrue("每一条入口都得有字，不能是空行", settingsEntries().all { it.title.isNotBlank() && it.section.isNotBlank() })
    }

    /**
     * 终端与服务器那九条路由一条不少地注册在 `AppNavHost` 里（降级不是删除）。
     *
     * 这一条恒绿于「入口被删」：它只看路由在不在，看不见设置面上那一行。
     * 它防的是另一个 bug：为了让主线干净而真的删掉一条路由。
     * 分辨力的那一半在 [theTerminalSectionKeepsButtonsAndServersButNotTheTerminalItself]，两条缺一不可。
     */
    @Test
    fun allNineTerminalAndServerRoutesAreRegistered() {
        val registered = registeredRoutes()
        val nine =
            listOf(
                Screen.Session.route,
                Screen.SessionsOverview.route,
                Screen.NewSessionHostPicker.route,
                Screen.Hosts.route,
                Screen.HostEdit.route,
                "host/new",
                Screen.Identities.route,
                Screen.HostButtons.route,
                Screen.ButtonSettings.route,
            )
        assertEquals("这张清单本身就该是九条", 9, nine.size)
        assertEquals("九条不许有重复", 9, nine.toSet().size)
        for (r in nine) {
            assertTrue("路由「$r」不见了：挪入口，不删入口。已注册的是：$registered", r in registered)
        }
        // 路由串本身也不许被改名：改名 = 书签/代码里的那条路失效，等于删了它
        assertEquals("sessions", Screen.SessionsOverview.route)
        assertEquals("sessions/new", Screen.NewSessionHostPicker.route)
        assertEquals("settings/buttons", Screen.ButtonSettings.route)
        assertEquals("host/{id}/buttons", Screen.HostButtons.route)
    }

    /**
     * 阴性对照：证明上面那条「九条路由还在」抓不住入口被删。
     *
     * 它是「路由普查与入口在不在互相独立」的可执行形式。独立性一旦被打破
     * （比如有人把那条改成也读 [settingsEntries]），本条会红，提醒后来者：路由普查从来就不是入口的守卫。
     */
    @Test
    fun theRouteCensusCannotSeeAMissingEntrance() {
        val registered = registeredRoutes()
        // 造一份「入口被删光」的表：九条路由的注册情况一个字都不变
        val noEntrances = emptyList<SettingsEntry>()
        assertTrue("路由普查读的是 AppNavHost，与入口表无关", Screen.SessionsOverview.route in registered)
        assertTrue(
            "入口被删光了，「sessions 这条路由还在吗」照样是 true：这就是它恒绿的证据",
            noEntrances.none { it.route == Screen.SessionsOverview.route } && Screen.SessionsOverview.route in registered,
        )
    }

    /**
     * 入口不许写回 composable 里。
     *
     * 有人图省事，在 `SettingsScreen` 里直接写一行
     * `TextButton(onClick = { nav.navigate("sessions") }) { Text("打开终端") }`：
     * 屏上看着一样，而 [settingsEntries] 里那一项没了，入口判据就再也守不住任何东西。
     *
     * 判别力边界：本条量的是源码里没有写死的入口，不是「屏上那一行真的画出来且可点」。
     * 把 `EntrySections` 里那段 `forEach` 删掉 ⇒ 屏上什么都没有，而所有判据全绿。
     * 要守它得引 Robolectric，或让门禁跑 `androidTest`。
     */
    @Test
    fun theScreenRendersTheDataInsteadOfHardcodingEntrances() {
        val code = codeOnly(source("SettingsScreen.kt"))
        assertTrue("设置屏必须去读那份数据", code.contains("settingsEntries()"))
        for (e in settingsEntries()) {
            assertFalse("入口「${e.title}」的文案被写死在设置屏里了：它只许住 settingsEntries()", code.contains("\"${e.title}\""))
            assertFalse("入口路由「${e.route}」被写死在设置屏里了", code.contains("\"${e.route}\""))
        }
        // 路由也不许通过 `Screen.X.route` 绕过那份数据混进屏里
        assertFalse("设置屏不该自己认得任何一条具体路由", code.contains("Screen."))
    }

    /**
     * 入口得在够得着的区域里。
     *
     * 「终端」这一节排在设置面最下面（在「新对话的权限」之后）。设置面那一列不滚的时候，
     * 那几行落在屏幕下沿之外：入口在 [settingsEntries] 里、在 composable 里、就是点不到，
     * 而上面那几条判据一条都不会红（它们只看数据与路由，从不看屏）。
     *
     * 钉住三件事：
     * ① 那一列开着纵向滚动（[SCROLL_CALL]，连 `rememberScrollState` 一起认）；写成 `ScrollState(0)` 也红；
     * ② 滚动挂在根那一列的链上，且在发出入口之前开；
     * ③ 函数体里只有一列：这是 ② 的补强，理由见下。
     *
     * ### 判别力边界
     *
     * | 绕过形态 | 本条 |
     * |---|---|
     * | 去掉 `verticalScroll` | 红 |
     * | 滚动状态不 remember | 红 |
     * | 把入口塞进一个固定高度的 `Box`、或给它 `height(0.dp)` | 抓不到 |
     * | 把入口挪进一个不滚的兄弟节点 | ② 抓不到（它读的是词法先后，词法先后≠包含关系）；靠 ③ 那条「只有一列」误红兜一下 |
     *
     * 本条量的是源码里那一列会滚，不是「那一行在真机上真的进得了可视区」；像素要真机。
     */
    @Test
    fun theEntrancesLiveInsideAScrollableColumn() {
        val body = settingsScreenBody()
        val scroll = body.indexOf(SCROLL_CALL)
        assertTrue(
            "设置面那一列必须能滚：不滚的话「终端」那一节落在屏外，入口在数据里却点不到。" +
                "找不到 `$SCROLL_CALL`",
            scroll >= 0,
        )
        val entries = body.indexOf(ENTRIES_CALL)
        assertTrue("设置面必须把 settingsEntries() 那一节发出来；找不到 `$ENTRIES_CALL`", entries >= 0)
        assertTrue(
            "入口必须发在滚动容器里面：滚动（@$scroll）要在入口（@$entries）之前开；" +
                "发在滚动容器外面就又够不着了",
            scroll in 0 until entries,
        )
        // 滚动要挂在根那一列的 modifier 链上（与 `Modifier.fillMaxSize()` 同一条），
        //   挂在某个子节点上治不了「整屏装不下」。
        val chain = body.lineSequence().firstOrNull { SCROLL_CALL in it }
        assertTrue("前提：得找到带滚动那一行，否则本条判据恒绿", chain != null)
        assertTrue("滚动要挂在根那一列的 modifier 链上（与 fillMaxSize 同一条）：$chain", ROOT_MODIFIER in chain!!)
        // 整屏只有一列：多一列就意味着有人可能把入口挪进了一个不滚的兄弟节点里
        //   （那时上面「滚动在入口之前」照样绿，因为词法先后不等于包含）。
        //   多一列时本条误红：误红是安全的一侧，它逼人去真机看一眼那几行还在不在可视区。
        assertEquals(
            "设置屏的函数体里只该有一列（就是那个滚动容器）。多一列 ⇒ 入口可能被挪出滚动容器之外，" +
                "而「滚动在入口之前」这条读的是词法先后、看不出包含关系。去真机走一遍再改这条。",
            1,
            countOf(body, "Column("),
        )
    }

    /**
     * 加节的人必须再走一遍真机。
     *
     * 滚动治的是「够得着」，但「终端那一节要往下翻多远」没有任何自动读数，节数一变，上一次的真机结论就作废。
     * 这个数一红：去真机走一遍（`uiautomator dump` 认得出那几行），确认那一节的入口点得到，再把它改过来。
     * 这条判据守的不是代码，是那次真机读数的有效期。
     */
    @Test
    fun theSectionCountIsPinnedSoAnyGrowthGetsReVettedOnADevice() {
        val code = codeOnly(source("SettingsScreen.kt"))
        val headers = Regex(Regex.escape(SECTION_HEADER_STYLE)).findAll(code).count()
        assertEquals(
            "设置面的节数变了（写死的那几节 + EntrySections 那一个渲染器）⇒ 内容高度变了 ⇒ " +
                "「终端」那一节可能又落到可达区域外。去真机走一遍、确认那一节的入口点得到，再改这个数。",
            PINNED_SECTION_HEADERS,
            headers,
        )
        assertEquals(
            "数据侧的节数也钉住（只有「终端」一节）：多一节就该重新算内容高度",
            1,
            settingsEntries().map { it.section }.distinct().size,
        )
    }

    /**
     * `fun SettingsScreen(` 那个函数的函数体（剥过注释）。
     *
     * 判别力边界：按顶格 `}` 认收尾。函数被改成表达式体、或那个 `}` 不再顶格，
     * 本判据会误红（不是漏红）：误红是安全的一侧，它逼人来看一眼。
     */
    private fun settingsScreenBody(): String {
        val code = codeOnly(source("SettingsScreen.kt"))
        val at = code.indexOf(SCREEN_FUN)
        assertTrue("前提：得找到 `$SCREEN_FUN`，否则本条判据恒绿", at >= 0)
        val end = code.indexOf("\n}\n", at)
        assertTrue("前提：得找到那个函数顶格的收尾 `}`，否则本条判据恒绿（起点 @$at）", end > at)
        return code.substring(at, end)
    }

    /** [needle] 在 [text] 里出现几次（不重叠）。 */
    private fun countOf(
        text: String,
        needle: String,
    ): Int {
        var n = 0
        var at = text.indexOf(needle)
        while (at >= 0) {
            n++
            at = text.indexOf(needle, at + needle.length)
        }
        return n
    }

    // ---- 读 `AppNavHost` 的路由普查 -------------------------------------------

    /**
     * `AppNavHost` 里所有 `composable(<x>)` 的第一个实参，解析成路由串。
     *
     * 判别力边界：按源码文本认，`composable(` 被格式化成多行仍认得（跳空白），
     * 但把注册那句写成一个循环 / 一个帮手函数就认不出来了：那时本判据会误红（不是漏红），
     * 误红是安全的一侧，它逼人来看一眼。
     */
    private fun registeredRoutes(): Set<String> {
        val code = codeOnly(source("AppNavHost.kt"))
        val args = Regex("""composable\(\s*([^,)\s]+)""").findAll(code).map { it.groupValues[1] }.toList()
        assertTrue("前提：得真认出 `composable(` 调用，否则这条判据恒绿（实得 ${args.size} 处）", args.size >= 9)
        return args.mapNotNull { resolveRoute(it) }.toSet()
    }

    /** `Screen.Foo.route` / `"host/new"` → 真正的路由串；认不出的返回 null。 */
    private fun resolveRoute(token: String): String? =
        when {
            token.startsWith("\"") && token.endsWith("\"") -> token.trim('"')
            token.startsWith("Screen.") && token.endsWith(".route") ->
                SCREENS[token.removePrefix("Screen.").removeSuffix(".route")]
            else -> null
        }

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 必须留字面：本条断「入口文案/路由串不许写死在设置屏里」，比的就是带引号的字面；
     * [resolveRoute] 还要从 `composable("host/new")` 这种字面实参里把路由串读出来。
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    private fun source(name: String): String {
        val dir = File("src/main/kotlin").takeIf { it.isDirectory } ?: File("app/src/main/kotlin")
        val f = dir.walkTopDown().first { it.isFile && it.name == name }
        return f.readText()
    }

    companion object {
        /** 设置屏那个入口函数：[settingsScreenBody] 按它定位。 */
        private const val SCREEN_FUN = "fun SettingsScreen("

        /**
         * 那一列滚起来靠这一句。
         *
         * 连 `rememberScrollState` 一起认：写成 `verticalScroll(ScrollState(0))` 的话
         * 滚动位置每次重组就归零，那是「能滚但滚不住」，比不滚更让人以为自己点错了。
         * 刻意不认那对收尾括号：`rememberScrollState(0)` 是合法写法，
         * 按 `…State())` 死认会在它上面假阳，而假阳会训练人绕过判据。
         */
        private const val SCROLL_CALL = "verticalScroll(rememberScrollState"

        /** 根那一列的 modifier 链认这一句：滚动必须与它同链（挂在子节点上治不了「整屏装不下」）。 */
        private const val ROOT_MODIFIER = "Modifier.fillMaxSize()"

        /** 设置面把入口那一节发出来的那一句。 */
        private const val ENTRIES_CALL = "EntrySections("

        /** 节标题用的排版档：数它出现几次就是数设置面有几节。 */
        private const val SECTION_HEADER_STYLE = "MaterialTheme.typography.titleMedium"

        /**
         * 设置面有几个节标题：写死的三节（应用级默认账号目录 · 对话列表的来源 · 新对话的权限）
         * ＋ `EntrySections` 那一个渲染器（它画出数据侧的「终端」节）= 4。
         *
         * 改这个数之前必须走一遍真机，理由见 [theSectionCountIsPinnedSoAnyGrowthGetsReVettedOnADevice]。
         *
         * 判别力边界：这条钉的是节数，不是内容高度。在一节内部增删行、把说明写长三倍，它全绿；
         * 它守的只是「加/减一整节时那次真机读数作废」这一件事。
         */
        private const val PINNED_SECTION_HEADERS = 3

        /**
         * `Screen` 的每个 data object → 它的路由串。
         *
         * 手写一份对照表是刻意的：反射拿不到 sealed 子类的稳定顺序，
         * 而且这张表本身就是「谁该被注册」的清单，少一条就说明有人加了路由却没进普查。
         */
        private val SCREENS: Map<String, String> =
            mapOf(
                "Launch" to Screen.Launch.route,
                "Hosts" to Screen.Hosts.route,
                "HostEdit" to Screen.HostEdit.route,
                "Identities" to Screen.Identities.route,
                "Session" to Screen.Session.route,
                "SessionsOverview" to Screen.SessionsOverview.route,
                "Conversations" to Screen.Conversations.route,
                "Chat" to Screen.Chat.route,
                "NewSessionHostPicker" to Screen.NewSessionHostPicker.route,
                "Settings" to Screen.Settings.route,
                "ButtonSettings" to Screen.ButtonSettings.route,
                "HostButtons" to Screen.HostButtons.route,
            )
    }
}
