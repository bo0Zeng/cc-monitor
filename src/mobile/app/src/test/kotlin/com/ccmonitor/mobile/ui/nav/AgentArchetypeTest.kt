package com.ccmonitor.mobile.ui.nav

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Codex 服务器不落进 Claude 的聊天屏。
 *
 * 判据分四条，各自防的 bug 写在每个测试的头注里。
 */
class AgentArchetypeTest {
    private fun terminalOf(hostId: String): String = "session/tab-$hostId"

    private fun destination(
        newUi: Boolean,
        kind: AgentKind,
    ): String =
        onConnectDestination(
            hostId = "h1",
            newUi = newUi,
            agentKind = kind,
            newConversationId = { "s-new" },
            openTerminal = ::terminalOf,
        )

    /**
     * `newUi × agentKind` 四格都要走到。
     *
     * 防的是 `onConnectDestination` 不看 `agentKind`：新界面开着时 Codex 服务器照样落进 `ChatRoute`。
     * 把 `agentKind` 那一格删掉、让两种服务器都走 `freshLanding`，第四行当场红。
     *
     * 非得四格都造：只造 `(开, Codex)` 的话，一个「把所有输入都返回总览面」的实现全绿；
     * 四格都造则另外三行同时红。`(开, Claude)` 那一行保证 Claude 没被一起推去总览面；
     * `(关, ·)` 两行钉的是开关关着时与 agent 种类无关。
     */
    @Test
    fun theLandingIsDecidedByBothTheFlagAndTheAgentKind() {
        // ①②：开关关着：两种服务器都开终端 tab（与 agent 种类无关）
        assertEquals("（关, Claude）必须还是终端那条路", "session/tab-h1", destination(false, AgentKind.ClaudeCode))
        assertEquals("（关, Codex）同样是终端：开关关着时不认 agent 种类", "session/tab-h1", destination(false, AgentKind.Codex))
        // ③：新界面 + Claude ⇒ 聊天屏
        assertEquals("（开, Claude）必须还是聊天屏", Screen.Chat.of("h1", "s-new", isNew = true), destination(true, AgentKind.ClaudeCode))
        // ④：新界面 + Codex ⇒ 只读档（对话总览），不是聊天屏
        val codex = destination(true, AgentKind.Codex)
        assertEquals("（开, Codex）必须落只读档（对话总览）", Screen.Conversations.of("h1"), codex)
        assertFalse("Codex 服务器落进了 Claude 的聊天屏：$codex", codex.startsWith("chat/"))
        // 两种服务器在新界面上必须去不同的地方：这一句在「忽略 agentKind」时也红，是上面那条的兜底
        assertNotEquals(
            "新界面下两种服务器的落点不许相同（相同就说明 agentKind 没被读）",
            destination(true, AgentKind.ClaudeCode),
            destination(true, AgentKind.Codex),
        )
    }

    /**
     * 启动那一刻的落点同样按种类分档。
     *
     * `launchDestination` 是同一个 bug 的另一道门：打开 app 直接落到上次那台服务器上，
     * 那台若是 Codex 就照样进 Claude 的聊天屏。
     */
    @Test
    fun theStartupLandingAlsoRespectsTheAgentKind() {
        fun launch(
            newUi: Boolean,
            host: String?,
            kind: AgentKind,
        ) = launchDestination(
            newUi = newUi,
            landingHostId = host,
            agentKind = kind,
            newConversationId = { "s-new" },
        )
        assertEquals("关着时去服务器列表", Screen.Hosts.route, launch(false, "h1", AgentKind.ClaudeCode))
        assertEquals("关着时去服务器列表（Codex 同）", Screen.Hosts.route, launch(false, "h1", AgentKind.Codex))
        assertEquals("没有上次那台机器时先去服务器列表", Screen.Hosts.route, launch(true, null, AgentKind.ClaudeCode))
        assertEquals("开着 + Claude ⇒ 聊天屏上空的新对话", Screen.Chat.of("h1", "s-new", isNew = true), launch(true, "h1", AgentKind.ClaudeCode))
        assertEquals("开着 + Codex ⇒ 只读档", Screen.Conversations.of("h1"), launch(true, "h1", AgentKind.Codex))
    }

    /**
     * Codex 的输入行是「不出现」，不是「灰掉」。
     *
     * 灰掉的输入框是一个会好起来的承诺（用户会等它恢复），而 Codex 的上行根本没有
     * （`ChatSession` / `UplinkSink` / `PipeLauncher` 全按 Claude CLI 的管道形状写）。
     * 把 Codex 也判成 `Disabled` ⇒ 第一行当场红。
     *
     * 判别力边界：这条断的是枚举值，不是「屏上真的没有那一行」。聊天屏里那个 `when`
     * 把 `Hidden` 也画成一个灰输入框的话，本条全绿；渲染要 `androidTest` 或真机。
     */
    @Test
    fun theCodexInputRowIsHiddenNotMerelyDisabled() {
        assertEquals("Codex ⇒ 整条不渲染", ChatInputMode.Hidden, chatInputMode(AgentKind.Codex))
        assertEquals(
            "Codex 撞上等待态也不许退回「灰掉」——灰掉在语义上是「等会儿就能发」，对它是假的",
            ChatInputMode.Hidden,
            chatInputMode(AgentKind.Codex, waitingOnDesktop = true),
        )
        assertEquals("Claude 正常时能打字", ChatInputMode.Editable, chatInputMode(AgentKind.ClaudeCode))
        assertEquals(
            "Claude 在电脑上等回答时是灰掉：那一档会好起来",
            ChatInputMode.Disabled,
            chatInputMode(AgentKind.ClaudeCode, waitingOnDesktop = true),
        )
        // 两者不许是同一个值：合并了就分不出「会好起来」与「不会好起来」
        assertNotEquals("Hidden 与 Disabled 必须可辨", ChatInputMode.Hidden, ChatInputMode.Disabled)
        assertEquals("三档必须是三个互不相同的值", 3, ChatInputMode.entries.toSet().size)
        // 三档都够得着：有一档永远返回不了的话，它就是个摆设，判据也就守不住任何东西
        val reachable =
            setOf(
                chatInputMode(AgentKind.Codex),
                chatInputMode(AgentKind.ClaudeCode),
                chatInputMode(AgentKind.ClaudeCode, waitingOnDesktop = true),
            )
        assertEquals("三档都要有输入能走到：$reachable", ChatInputMode.entries.toSet(), reachable)
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
     * 同 `NewUiSettingsTest.theDefaultSourceIsPinnedSoAnyChangeGetsReVetted` 的做法。
     *
     * ### 判别力边界
     *
     * | 绕过形态 | 这条判据 |
     * |---|---|
     * | `Text("…Claude…")` 新增一句 | 红 |
     * | `"Cla" + "ude"` 拼接 | 抓不到 |
     * | 产品名住一个常量再 `Text(那个常量)` | 抓不到。有这一形：`ChatRoute.CHAT_TITLE == "Claude"` 是聊天屏顶栏的标题 |
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
        return got.callSites to got.literals.map { it.body }
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
         * 还硬写着产品名的那几处（`app/src/main` 里，按文件名 → 那几句）。
         *
         * 这张表只许缩短，不许悄悄变长。每一条都得说得出为什么它还在：
         *
         * | 文件 | 为什么还在 |
         * |---|---|
         * | `SettingsScreen.kt` | 应用级设置说的是远端那个 Claude（账号目录），不是「这台服务器叫什么」，与 kind 无关 |
         * | `HostScreens.kt` | 「Claude 账号目录」是 `if (isCodex)` 的 else 支（已按 kind 分档）；Windows 那句说的是「哪些功能在 Windows 上不可用」 |
         * | `ClaudeReadingPaneConnected.kt` · `ClaudeHistorySheet.kt` · `SessionScreen.kt` | 终端面那套的文案，还没按 kind 分档 |
         */
        private val PINNED_HARDCODED_PRODUCT_NAMES: Map<String, List<String>> =
            mapOf(
                // 终端面的历史抽屉
                "ClaudeHistorySheet.kt" to listOf("无历史会话（该主机 Claude 数据目录下没有 .jsonl）"),
                // 终端面的阅读面
                "ClaudeReadingPaneConnected.kt" to listOf("未找到 Claude 会话（该工作目录下无 .jsonl）"),
                // 前两条是服务器编辑页那个目录框的标签，已按 kind 分档（`if (isCodex) … else …`），
                // 第三条说的是「哪些功能在 Windows 上不可用」，与这台服务器叫什么无关。
                "HostScreens.kt" to
                    listOf(
                        "Claude 账号目录",
                        "Codex 配置目录（可选）",
                        "Windows：走 PowerShell、跳过 tmux；tmux/Claude 阅读面/一键 cc 等在 Windows 下不可用（连接/终端/SFTP 可用）。",
                    ),
                // 终端面的 tab 名
                "SessionScreen.kt" to listOf("Claude 阅读"),
                // 第一条是应用级的话（账号目录），说的是「远端那个 Claude」
                // 而不是「这台服务器叫什么」，与 agentKind 无关。
                // 第二条是扫描器的产物、不是上屏的字：那句话上屏时是
                // 「留空 = 内置默认 /home/…/.claude」，一个产品名都没有，
                // 「Claude」来自 `${...}` 里那个标识符 `ClaudePaths`。
                // 扫描器不解析模板，`${}` 那一段原样留在字面里（见 [KotlinSourceScanner] 头注那一格），
                // 所以这一条是假阳，钉在这里只为让那条判据是等号；扫描器学会跳过 `${}` 里的代码之后可删。
                "SettingsScreen.kt" to
                    listOf(
                        "应用级默认 Claude 配置目录",
                        "留空 = 内置默认 \${ClaudePaths.DEFAULT_CLAUDE_DIR}",
                    ),
            )
    }
}
