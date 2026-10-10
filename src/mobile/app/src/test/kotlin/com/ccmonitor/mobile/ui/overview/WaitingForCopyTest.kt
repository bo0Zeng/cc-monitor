package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.catalog.SessionLight
import com.ccmonitor.mobile.core.claude.transport.WaitingCopy
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 「Claude 在等人」这件事在总览面上说的是人话，而且没有一句是编的。
 *
 * 分三组：
 * - 值域表在仓里、有主、写对（机检的是「表里写对了」）；
 * - 生产代码里那六个值只在那一张表里出现，别处一个字面量都没有；
 * - 第三档（说不出为什么下线）不许在任何一层被翻译成「死了」。
 */
class WaitingForCopyTest {
    /**
     * `2.1.261` 那份读数里的全部六个值。
     *
     * 注意：不是五个。`goal proposal`（`dk` 的 `nj` 条目）不在对端协议文档的那张表里。
     */
    private val allValues =
        listOf(
            "input needed",
            "permission prompt",
            "worker request",
            "sandbox request",
            "dialog open",
            "goal proposal",
        )

    /** CC 那份读数的版本号。表里每一行都要带它（CC 版本跳得很快）。 */
    private val ccVersion = "2.1.261"

    // ---- 找源文件 ---------------------------------------------------------

    /** 仓根 = 有 `settings.gradle.kts` 的那一层（测试的工作目录可能是仓根，也可能是模块目录）。 */
    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根：判据没法量，宁可当场红也不许空跑")

    private fun screenSource(): String =
        File(repoRoot(), "app/src/main/kotlin/com/ccmonitor/mobile/ui/overview/SessionOverviewScreen.kt").readText()

    private fun sectionsSource(): String =
        File(repoRoot(), "app/src/main/kotlin/com/ccmonitor/mobile/ui/overview/Sections.kt").readText()

    /**
     * 值域表住在 `core-claude` 的 `WaitingCopy.kt`。
     *
     * `WaitingCopy` 是聊天屏、投递路径与总览屏三处的共同下游。同一张表若在两处各存一份，
     * 两份各自内部自洽，CC 加第七个值那天两边各错各的，没有任何判据会红。
     * 下面 [noWaitingForValueLiteralAppearsOutsideTheOneTable] 钉的正是「只有这一份」。
     */
    private fun waitingCopySource(): String =
        File(
            repoRoot(),
            "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/transport/WaitingCopy.kt",
        ).readText()

    /** 值域表所在文件的文件名，判据用它对唯一那次命中的住址。 */
    private val tableFileName = "WaitingCopy.kt"

    /** 全部生产源（app + 各 core 模块的 `src/main`）。测试与文档不在内。 */
    private fun allMainSources(): List<File> =
        repoRoot()
            .listFiles()
            .orEmpty()
            .filter { it.isDirectory && (it.name == "app" || it.name.startsWith("core-")) }
            .flatMap { m -> File(m, "src/main").walkTopDown().filter { it.isFile && it.extension == "kt" } }

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 必须留字面：本条数的是带引号的值域字面（`"sandbox request"` 等）在生产代码里出现几次。
     * 换成「删字面」那支，六个值全零命中，而断言要的是「恰好 1 次」，当场误红；
     * 改成只断「不超过 1 次」又会变成假绿。
     *
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    /** 值域表的那几行（KDoc 里的 markdown 表格行）。 */
    private fun tableRowFor(value: String): String? =
        waitingCopySource()
            .lineSequence()
            .map { it.trim().removePrefix("*").trim() }
            .firstOrNull { it.startsWith("|") && it.contains("`$value`") }

    // ---- 值域表 -------------------------------------------------------

    /**
     * 表里必须有 `goal proposal`。
     *
     * 对端协议文档只有五值，而产出方 CC 的 `dk` 表里第六条就在那儿。
     * 判据钉的是这一行在不在，不是「表非空」或「表里有 N 行」：那样一张五值表也过。
     */
    @Test
    fun theTableNamesTheSixthValueThatBothDocsMissed() {
        val row = tableRowFor("goal proposal")
        assertTrue("值域表里必须有 `goal proposal` 这一行（对端协议文档漏了它）", row != null)
        assertTrue(
            "这一行要写清它是从哪来的（notification 逐字），否则下一个人会以为是我们编的：$row",
            row!!.contains("Claude proposed a session goal"),
        )
    }

    /**
     * `permission prompt` 那一行必须写明它是 `??` 的兜底默认。
     *
     * 它不是某一个框的名字：`dk[x]?.waitingFor ?? "permission prompt"`。
     * 「批准计划」「另一个对话要批准」「用浏览器」全都落进它。表里不写这句的话，
     * 下一个人会拿它当「有一个工具权限请求」去写分档逻辑，而那是错的。
     */
    @Test
    fun theTableSaysPermissionPromptIsAFallbackDefaultNotAConcreteDialog() {
        val row = tableRowFor("permission prompt")
        assertTrue("前提：表里得有这一行", row != null)
        assertTrue("必须写明它是兜底默认：$row", row!!.contains("兜底"))
    }

    /**
     * 每一行都要带版本号。
     *
     * 不带版本号的表会越放越旧，而没有任何判据会因为它旧而红。版本号是给人对账用的，不是告警。
     */
    @Test
    fun everyRowInTheTableCarriesTheCcVersionItWasReadFrom() {
        for (v in allValues) {
            val row = tableRowFor(v)
            assertTrue("表里缺 `$v` 这一行", row != null)
            assertTrue("`$v` 那一行没带版本号（要求 $ccVersion）：$row", row!!.contains(ccVersion))
        }
    }

    // ---- 生产代码里的字面量 -------------------------------------------

    /**
     * 那六个值的字符串字面量在整个生产代码里只出现一次，就在那张唯一的表里。
     *
     * 扫字面量而不扫语法：只扫 `when (waitingFor)` 的话，`if (row.waitingFor == "permission prompt")` 就逃掉了。
     * 映射表必须有这六个键，所以钉的是「总数 == 1 且那一次落在 [tableFileName] 里」。
     * 往 `SessionOverviewViewModel` 加一行 `==` 比较照样红。
     *
     * `ui/overview/Sections.kt` 的危险度序转调 `WaitingCopy.dangerRank`，序由表的键序给，
     * 见 [dangerRankFollowsTheOneTable]。
     *
     * 注释里的出现不算（`ZEt` 那个表达式必须能逐字抄进 KDoc），所以扫描前先剥注释；
     * 剥注释这一步本身由下面那条自检守着。
     */
    @Test
    fun noWaitingForValueLiteralAppearsOutsideTheOneTable() {
        val files = allMainSources()
        assertTrue("前提：得真扫到生产源文件，否则这条判据是空跑（实得 ${files.size} 个）", files.size > 50)

        for (v in allValues) {
            val hits =
                files.flatMap { f ->
                    codeOnly(f.readText())
                        .lineSequence()
                        .withIndex()
                        .filter { (_, line) -> line.contains("\"$v\"") }
                        .map { (i, line) -> "${f.name}:${i + 1} ${line.trim().take(90)}" }
                        .toList()
                }
            assertEquals(
                "`$v` 的字面量只许在那张表里出现一次，实得：\n" + hits.joinToString("\n"),
                1,
                hits.size,
            )
            assertTrue(
                "唯一那次必须落在值域表所在的文件里（$tableFileName）：${hits.single()}",
                hits.single().startsWith("$tableFileName:"),
            )
        }
    }

    /**
     * 危险度序跟着那张唯一的表走，`ui/overview` 里不许再有第二份。
     *
     * 扫字面量的那条能抓到「有两处字面量」，抓不到「两处的语义不一致」（比如一份漏了 `goal proposal`），
     * 所以这条从行为上再钉一次。
     *
     * 断言的是序关系不是具体数字：`dangerRank` 的返回值换一套编码（1..6 → 0..5）照样绿，
     * 那是有意的，钉住数字会把一次无害的重构变成一次假红。
     */
    @Test
    fun dangerRankFollowsTheOneTable() {
        // 表的键序就是危险度序：逐对比较相邻两个，避免「只比首尾」漏掉中间乱序
        val ordered = WaitingCopy.knownCodesByDanger
        assertEquals("前提：表里得是那六个值", allValues.toSet(), ordered.toSet())
        ordered.zipWithNext().forEach { (higher, lower) ->
            assertTrue(
                "`$higher` 该排在 `$lower` 前面（表的键序 = 危险度序）",
                dangerRank(higher) < dangerRank(lower),
            )
        }
        // 认不出的新值与缺席同档，且排在全部已知值之后 —— 猜它危险也是猜
        assertEquals("认不出与缺席必须同档", dangerRank(null), dangerRank("brand new waiting kind"))
        ordered.forEach {
            assertTrue("认不出的新值不许排到已知值 `$it` 前面", dangerRank(it) < dangerRank("brand new waiting kind"))
        }
    }

    /**
     * 前提自检：剥注释这一步真的在起作用。
     *
     * 不验它的话，上面那条可能是因为「把整份文件都当注释剥光了」而绿。
     */
    @Test
    fun theCommentStrippingKeepsCodeAndDropsComments() {
        val sample =
            """
            /** KDoc 里逐字抄 dk[x]?.waitingFor ?? "permission prompt" 是合法的 */
            // 行注释里写 "dialog open" 也合法
            val real = mapOf("sandbox request" to "待放行一次越界操作")
            """.trimIndent()
        val code = codeOnly(sample)
        assertTrue("代码里的必须留下来", code.contains("\"sandbox request\""))
        assertFalse("KDoc 里的必须被剥掉", code.contains("\"permission prompt\""))
        assertFalse("行注释里的必须被剥掉", code.contains("\"dialog open\""))
    }

    // ---- 屏上那句话 ------------------------------------------------

    /**
     * 六个值各有自己的一句人话，且没有一句把英文原样带上屏。
     *
     * 副标题里写成 `等：{原文}` 的话，机器码原样上屏，而且它多数情况下还是假的（兜底默认）。
     */
    @Test
    fun everyKnownValueGetsItsOwnHumanSentenceAndNoneLeaksTheRawValue() {
        val said = allValues.associateWith { WaitingCopy.headlineFor(it) }
        for ((v, s) in said) {
            assertFalse("`$v` 的人话不许把机器码带上屏：$s", s.contains(v))
            assertTrue("`$v` 得有一句话", s.isNotBlank())
        }
        assertEquals("六句各不相同（挤成一句就等于没分档）", 6, said.values.toSet().size)
    }

    /**
     * `null` 那一格有它自己的一句，不许回退成通用句。
     *
     * `session_status` 帧里 `status` 缺席意味着「现在没有 status」而不是「未变」，
     * 而粘旧 status 却把 `waitingFor` 清空，就造得出「在需手动区却说不出等什么」的行。
     * 回退成通用句会把那条真 bug 藏起来。
     */
    @Test
    fun theMissingValueHasASentenceOfItsOwnInsteadOfTheGenericOne() {
        val absent = WaitingCopy.headlineFor(null)
        assertTrue("要如实说「它没说等什么」：$absent", absent.contains("没说"))
        assertEquals("空串与缺席是同一件事", absent, WaitingCopy.headlineFor(""))
        for (v in allValues) {
            assertNotEquals("不许与任何一个已知值撞成同一句", WaitingCopy.headlineFor(v), absent)
        }
        assertNotEquals("也不许与「认不出的新值」那句撞成一句", WaitingCopy.headlineFor("某个新值"), absent)
    }

    /**
     * 认不出的新值：一句诚实的通用话 + 原文原样带回（同 `TmuxSendKeysSink` 把模态屏幕原文带回那条）。
     *
     * 读不懂对面每一种等待时，编一句假的比带回原文更坏。
     */
    @Test
    fun anUnknownValueIsCarriedBackVerbatimBehindAnHonestSentence() {
        val said = WaitingCopy.headlineFor("brand new waiting kind")
        assertTrue("原文要带回去（对端加第七个值那天，这是唯一的线索）：$said", said.contains("brand new waiting kind"))
        assertTrue("前面要有一句人话，不许光甩一个英文串：$said", said.startsWith("需手动"))
    }

    /**
     * `permission prompt` 那句故意不具体：它是 CC 的兜底默认，很多种框都落进它。
     *
     * 这是一条黑名单：换个说法照样绿。它挡的是把这个值翻成「有个权限请求在等批准」，不是安全。
     */
    @Test
    fun thePermissionPromptSentenceStaysGenericBecauseTheValueIsAFallback() {
        val said = WaitingCopy.headlineFor("permission prompt")
        assertFalse("不许说成一件具体的事（「权限」）：$said", said.contains("权限"))
    }

    /**
     * 那句人话真的接到了行上，而副标题里那句 `等：{原文}` 没了。
     *
     * 只验 [WaitingCopy.headlineFor] 不够：「函数写了、行上没用」就是「已 build 零引用」。
     * 所以这条从总览面真正上屏的那两个函数（[statusWordFor] / [overviewRowSubtitle]）出发量。
     * `markFor` 只给形状（`◆`），那句人话归 `statusWordFor`；把 `statusWordFor` 里那句
     * `WaitingCopy.headlineFor(...)` 换成一个固定词，本条红。
     *
     * 这里刻意不断言 `markFor` 里含那句人话：那会把「记号与词分开」这条设计反向钉死。
     */
    @Test
    fun theSentenceReachesTheRowAndTheRawValueIsGoneFromTheSubtitle() {
        val state =
            SessionOverviewUiState(
                needsYou =
                    listOf(
                        SessionRow(
                            sessionId = "s1",
                            title = "活儿",
                            cwd = "/proj",
                            light = SessionLight.WaitingInput,
                            attachable = true,
                            waitingFor = "permission prompt",
                        ),
                    ),
            )
        // 从 `sections()` 出发取那一行 —— 量的是屏上真会拿到的东西，不是自己造的 `OverviewRow`
        val row = sections(state).first().rows.single()

        val word = statusWordFor(row)
        assertTrue(
            "行上那句状态词要说人话：'$word'",
            word.contains(WaitingCopy.headlineFor("permission prompt")),
        )
        assertFalse("状态词里不许出现机器码：'$word'", word.contains("permission prompt"))

        val sub = overviewRowSubtitle(row, nowMs = 2_000L)
        assertFalse("副标题不许再出现机器码：'$sub'", sub.contains("permission prompt"))
        assertFalse("「等：」那个写法整个不许再有：'$sub'", sub.contains("等："))
        assertTrue("那句人话要在副标题里（它是副标题的第一格）：'$sub'", sub.contains(word))
    }

    // ---- 第三档不许被翻译成「死了」 ------------------------------

    /**
     * 第三档的分区标题里不许出现「已停 / 结束 / 崩」任何一个词：
     * 那三个词都是在替对端宣布一件它没说过的事。
     *
     * 这是一条黑名单：换个说法（「不在了」「没了」「×」）照样绿。全仓扫措辞假阳率太高，故只枚举这一处。
     */
    @Test
    fun theCannotSaySectionTitleNeverDeclaresTheConversationDead() {
        for (word in listOf("已停", "结束", "崩", "死")) {
            assertFalse("标题不许说「$word」：$UNCLEAR_REMOVAL_TITLE", UNCLEAR_REMOVAL_TITLE.contains(word))
        }
        assertTrue("标题要说出「说不出」这件事：$UNCLEAR_REMOVAL_TITLE", UNCLEAR_REMOVAL_TITLE.contains("说不出"))
    }

    /**
     * 那句解释要说满三件事：发生了什么 / 为什么说不出 / 不是用户的错。
     *
     * 只说「说不出为什么」而不说原因的话，唯一能得出的结论是「这个 app 有 bug」，
     * 而真实原因是服务器上那个助手太旧，`cause` 那个字段它根本不发。
     *
     * 「不是用户的错」钉的是语义、不是某几个字：文案里不许有第二人称（见 `SecondPersonBanTest`），
     * 所以锚是无人称的「不是操作」。再撞到同类（判据逐字钉着一句带第二人称的文案）时，
     * 先问这条判据要守的语义是什么，再换一个不带人称的锚，别去放宽第二人称那条。
     */
    @Test
    fun theExplanationSaysWhyAndSaysItIsNotTheUsersFault() {
        assertTrue("要说清是服务器那边的东西太旧：$UNCLEAR_REMOVAL_WHY", UNCLEAR_REMOVAL_WHY.contains("太旧"))
        assertTrue("要说出两种可能都排除不掉：$UNCLEAR_REMOVAL_WHY", UNCLEAR_REMOVAL_WHY.contains("/clear"))
        // 钉语义不钉人称：要说出「责任不在用户」，且不许借第二人称说（SecondPersonBanTest 那条）。
        assertTrue("要说出责任不在用户：$UNCLEAR_REMOVAL_WHY", UNCLEAR_REMOVAL_WHY.contains("不是操作"))
        for (person in listOf("你", "您")) {
            assertFalse(
                "不许用第二人称说它（见 SecondPersonBanTest）：$UNCLEAR_REMOVAL_WHY",
                UNCLEAR_REMOVAL_WHY.contains(person),
            )
        }
        // 这些词一个都不许上屏
        for (banned in listOf("daemon", "SSH", "exec", "cc-monitor", "build_id")) {
            assertFalse("不许把术语带上屏（$banned）：$UNCLEAR_REMOVAL_WHY", UNCLEAR_REMOVAL_WHY.contains(banned))
        }
    }

    /**
     * 第三档有生产消费方：屏幕真的把那一区渲染了，且用的是那两个常量
     * （有人直接在 composable 里另写一句话时，前两条判据会被骗过）。
     *
     * `SessionOverviewStateTest` 已经从 UI 状态出发量过「那一区非空」，这条补最后一跳。
     * `state.removedUnknown` 由 `Sections.kt` 的 `sections()` 读。
     *
     * 注意：不能直接 `contains("unclearRemovalSection(")`，函数声明本身也匹配它，把调用点删掉照样绿。
     * 所以切成两半各自定位：声明在函数体那一半找，调用点在 `sections(state).forEach` 那个 `when` 里找
     * （`is OverviewSection.UnclearRemoval ->`）。
     *
     * 射程：它量的是源码里有没有这几个标识符，不是像素。把那个 `Text` 删掉、或宽度设成 0，本条照样绿。
     */
    @Test
    fun theScreenActuallyRendersTheCannotSaySection() {
        val src = codeOnly(screenSource())
        assertTrue(
            "`sections()` 要读 state.removedUnknown（这一档并进分区模型之后它住 Sections.kt）",
            codeOnly(sectionsSource()).contains("state.removedUnknown"),
        )

        // 调用点：`when` 的那一支必须真的把这一段交给渲染函数。
        //   不许直接 `contains("unclearRemovalSection(")`：函数声明也长这样。
        val dispatch = src.substringAfter("is OverviewSection.UnclearRemoval ->", missingDelimiterValue = "")
        assertTrue("`sections()` 的 `when` 里根本没有这一段的分支", dispatch.isNotEmpty())
        assertTrue(
            "那一支没把这一段画出来（只声明了函数、没人调它 = 已 build 零引用）：${dispatch.take(120)}",
            dispatch.substringBefore('\n').contains("unclearRemovalSection("),
        )

        // 声明：函数体里要用那两个常量，不许另写一句话（否则前两条文案判据被绕过）
        val body = src.substringAfter("fun androidx.compose.foundation.lazy.LazyListScope.unclearRemovalSection", "")
        assertTrue("前提：得找得到那个渲染函数的声明", body.isNotEmpty())
        assertTrue("标题要用那个常量，不许另写一句", body.contains("section.title"))
        assertTrue("解释也要用那个常量", body.contains("UNCLEAR_REMOVAL_WHY"))
    }

    /**
     * 第三档不点状态灯（同归档行那条规矩）。
     *
     * 喂 `lightForStatus(_, alive = false)` 会恒得 `Stopped`，屏上就写成「× 已停」。
     */
    @Test
    fun theCannotSayRowShowsNoStatusLight() {
        val state =
            SessionOverviewUiState(
                removedUnknown =
                    listOf(
                        SessionRow(
                            sessionId = "s1",
                            title = "活儿",
                            cwd = null,
                            // 故意喂一个会点灯的 light：真实数据里它就是下线前最后一次的状态。
                            light = SessionLight.Working,
                            attachable = true,
                            removalUnknown = true,
                        ),
                    ),
            )
        // 从 `sections()` 出发取那一行：屏上那一行由 `markFor` + `statusWordFor` 两半拼成，
        //   所以这里量的是拼起来的整句。
        val unclear = sections(state).filterIsInstance<OverviewSection.UnclearRemoval>()
        // 前提先断出来：这一段被漏掉时本条否则死在 `NoSuchElementException` 上，
        //   那读起来是崩溃，不是一次读数。
        val rows = unclear.firstOrNull()?.rows.orEmpty()
        assertEquals("前提：那一段得真有这一行（否则下面读的是空气）", listOf("s1"), rows.map { it.sessionId })
        val row = rows.single()
        val line = "${markFor(row)} ${statusWordFor(row)}"
        // 两种形态都列：裸记号 + 记号带词。只列「记号+词」的整块的话，
        //   记号那半是光秃秃的 `●`，第三档掉回去点灯也一条都撞不上。
        for (l in listOf("◆", "●", "▸", "○", "·", "×", "● 在跑", "○ 空闲", "▸ shell", "× 已停", "· 未知", "在跑")) {
            assertFalse("第三档不许点任何一盏灯（撞上 $l）：'$line'", line.contains(l))
        }
        assertFalse("尤其不许写成「已停」：'$line'", line.contains("已停"))
        assertTrue("要说它下线了：'$line'", line.contains("下线"))
    }
}
