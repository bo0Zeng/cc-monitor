package com.ccmonitor.mobile.ui.nav

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.testing.CopyTableForTests
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 两家都落聊天屏、都能送字（送字走 `terminal-input`，不分哪家）；落点只看新界面开没开。产品名按种类各说各的。
 */
class AgentArchetypeTest {
    @Test
    fun theStartupLandingIsTheSameForEveryAgent() {
        fun launch(host: String?) = launchDestination(landingHostId = host, newConversationId = { "s-new" })
        assertEquals("没有上次那台机器时先去服务器列表", Screen.Hosts.route, launch(null))
        assertEquals("有 ⇒ 聊天屏上空的新对话（Claude、Codex 一样）", Screen.Chat.of("h1", "s-new", isNew = true), launch("h1"))
    }

    /**
     * 产品名从一个地方来，两种服务器各说各的名字。
     */
    @Test
    fun theProductNameComesFromOnePlaceAndDiffersByKind() {
        assertEquals("Claude", agentDisplayName(AgentKind.ClaudeCode))
        assertEquals("Codex", agentDisplayName(AgentKind.Codex))
        assertNotEquals(
            "两种服务器的产品名必须不同：相同就等于又把 Codex 叫成了 Claude",
            agentDisplayName(AgentKind.ClaudeCode),
            agentDisplayName(AgentKind.Codex),
        )
        // 覆盖全枚举：以后加一种 agent 时，这里会因为 `when` 穷尽性当场编译不过（而不是静默漏一档）
        val names = AgentKind.entries.map(::agentDisplayName)
        assertEquals("每一种 agent 都要有名字，且互不相同：$names", AgentKind.entries.size, names.toSet().size)
        assertTrue("名字不能是空的：$names", names.all { it.isNotBlank() })
    }

    /**
     * `app/src/main` 里没有新的硬写产品名上屏。
     *
     * 硬写「Claude」的那句话，在一台 Codex 服务器上就是一句假话。往任何一个 `Text(...)` 里加一句硬写的产品名 ⇒ 本条红。
     *
     * 钉住已知残余，而不是「一条都不许有」：还剩几处硬写的（理由见 [PINNED_HARDCODED_PRODUCT_NAMES]）。
     * 多一条红、少一条也红（有人清掉了，得重新过一遍眼再改这张表），
     *
     * ### 判别力边界
     *
     * | 绕过形态 | 这条判据 |
     * |---|---|
     * | `Text("…Claude…")` 新增一句 | 红 |
     * | `"Cla" + "ude"` 拼接 | 抓不到 |
     * | 产品名住一个常量再 `Text(那个常量)` | 抓不到（聊天屏顶栏的标题已改读 `AgentProfile.displayName`） |
     * | `strings.xml` 里的任何一句 | 扫不到 |
     */
    @Test
    fun noNewHardcodedProductNameReachesTheScreen() {
        val sources = appMainSources()
        assertTrue("前提：得真扫到源文件，否则这条测试是空跑（实得 ${sources.size} 个）", sources.size > 20)
        var calls = 0
        val found = sortedMapOf<String, List<String>>()
        for (f in sources) {
            val (n, literals) = textArgLiterals(f.readText())
            calls += n
            val named = literals.filter { l -> PRODUCT_NAMES.any { it in l } }
            val bad = named.distinct().sorted()
            if (bad.isNotEmpty()) found[f.name] = bad
        }
        assertTrue("前提：扫描器得真认出 `Text(` 调用，否则恒绿（实得 $calls 处）", calls > 100)
        assertEquals(
            "上屏的产品名要走 agentDisplayName(kind)。多出来的那条请改掉；" +
                "确属「远端那个 Claude」而非本服务器产品名的，连同理由加进 PINNED_HARDCODED_PRODUCT_NAMES。",
            PINNED_HARDCODED_PRODUCT_NAMES,
            found.toMap(),
        )
    }

    /**
     * 扫描器自检（同 `ChatAccountGateTest.theCommentStrippingActuallyStripsCommentsAndKeepsCode` 的用意）。
     *
     * 上面那条判据全靠 [textArgLiterals] 认得出 `Text(` 的实参。它要是认错了
     * （比如把注释里的字算进来、或者被一个带引号的字符串模板带跑），判据就会恒绿，
     * 而恒绿的判据比没有更坏。
     */
    @Test
    fun theScannerReadsRealTextArgumentsAndNothingElse() {
        val sample =
            "// Text(\"注释里的 Claude\")\n" +
                "Text(\"真的 Claude\", style = s)\n" +
                "OutlinedTextField(value = v, label = { Text(if (k) \"Codex 档\" else \"Claude 档\") })\n" +
                "Text(\"删除「\${t.label}」？\")\n" +
                "Text(CHAT_TITLE)\n" +
                "val notAnArg = \"Claude 常量\"\n"
        val (calls, lits) = textArgLiterals(sample)
        assertEquals("认出的 `Text(` 处数不对：$lits", 4, calls)
        assertTrue("真实参没读到：$lits", "真的 Claude" in lits)
        assertTrue("`if/else` 两侧都要读到：$lits", "Codex 档" in lits && "Claude 档" in lits)
        assertTrue("带 \${} 的字符串模板不许把扫描带跑：$lits", "删除「\${t.label}」？" in lits)
        assertFalse("注释里的不算上屏：$lits", lits.any { "注释里" in it })
        assertFalse("`Text(` 之外的字面量不许被算进来（这条一旦失效，判据就开始误红）：$lits", lits.any { "常量" in it })
    }

    // ---- 扫描器 --------------------------------------------------------------

    /**
     * 取出每个 `Text(…)` 调用实参里的字符串字面量，走全仓唯一一份词法扫描器
     * [KotlinSourceScanner.literalsInCallsTo]。
     *
     * 载荷就是字面内容：本文件抠的是 `Text(…)` 里的上屏文案。接到 `codeOnlyDroppingLiterals` 上的话
     * 一句都抠不出来，[noNewHardcodedProductNameReachesTheScreen] 那条零命中然后全绿；
     * 那条自带的「前提：扫描器得真认出 `Text(` 调用（实得 N 处）」就是为这件事立的。
     *
     * 模板插值 `${'$'}{… "x" …}` 那种串会按引号翻面切成几段（[KotlinSourceScanner] 头注里那一格）。
     * 切开只可能让一条横跨内层引号的产品名漏掉，而产品名里出不来引号。
     *
     * @return （认出的 `Text(` 处数, 那些字面量）：处数用来自检「扫描器没有静默失灵」。
     */
    private fun textArgLiterals(text: String): Pair<Int, List<String>> {
        val got = KotlinSourceScanner.literalsInCallsTo(text, TEXT_CALLEE)
        // 界面上的字收进了文案表：字面量是表里的键（`Text(copyText("键"))`）⇒ 换成那句 zh 再比。
        return got.callSites to got.literals.map { CopyTableForTests.shown(it.body) }
    }

    private fun appMainSources(): List<File> {
        val dir = File("src/main/kotlin").takeIf { it.isDirectory } ?: File("app/src/main/kotlin")
        return dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
    }

    companion object {
        /** 本文件问的是「上屏的那句话」，所以点名 Compose 的 `Text(…)`。 */
        private const val TEXT_CALLEE = "Text"

        private val PRODUCT_NAMES = listOf("Claude", "Codex")

        /**
         * 还硬写着产品名的那几处（`app/src/main` 里，按文件名 → 那几句）。今天一处都没有：
         * 那几句（设置页、服务器编辑页的配置目录框）随那一格删掉了。这张表只许缩短，不许悄悄变长。
         */
        private val PINNED_HARDCODED_PRODUCT_NAMES: Map<String, List<String>> = emptyMap()
    }
}
