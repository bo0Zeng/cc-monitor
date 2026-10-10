package com.ccmonitor.mobile.ui.settings

import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 可选的那几档里不许出现互为别名的两项。
 *
 * 「不选」(`null`) 与「每步都问」(`manual`) 做的是同一件事：
 *
 * | 屏上那颗 | 我们传下去 | 到了上游 |
 * |---|---|---|
 * | 「不选」 | `null` ⇒ `permissionModeFlag` 回空串 ⇒ 命令里不带 `--permission-mode` | `default` |
 * | 「每步都问」 | `"manual"` | `default`（别名，上游那句逐字 `nh(e){return e==="manual"?"default":e}`） |
 *
 * 两颗分开的键会让人以为在两档之间选，其实选哪颗都一样。所以没有「不选」那颗键，
 * 「不选也是它」写进第一档的副标题。
 *
 * 判据不断「有几颗键」，断「任何两档落到上游的同一个值上」（[noTwoOptionsAreUpstreamAliasesOfEachOther]）：
 * 下一个人很容易再加一颗「跟随默认」回来，而那颗键和第一档又是同一回事。加回任何一个别名档，它当场红。
 * 那两个别名值本身钉成回归用例（[theTwoUpstreamAliasesArePinnedAsARegressionCase]）。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 加回一个与某档同义的档（`null` / `manual` / 将来的 `default`） | 红 |
 * | 上游又改了归一化那一句（比如把 `auto` 也折成 `default`） | 抓不到：[effectivePermissionMode] 里那张映射是转引来的，没对着那个二进制跑过；上游改了要同步改那个函数 |
 * | 那三行真的画出来且点得到 | 抓不到：布局要 `androidTest` 或真机。[theScreenDrawsTheOptionsFromTheData] 只能钉「屏上读的是这份数据」 |
 * | 屏上那个选中标记看得出来 | 抓不到：那是看图才看出来的。这里只钉「用的是 `RadioButton` 而不是在标签前面拼字符」 |
 */
class PermissionModeOptionsTest {
    // === 承重那条：别名 =========================================================

    @Test
    fun noTwoOptionsAreUpstreamAliasesOfEachOther() {
        val collisions =
            PERMISSION_MODE_OPTIONS
                .groupBy { effectivePermissionMode(it.value) }
                .filterValues { it.size > 1 }
                .mapValues { (_, v) -> v.map { it.value } }
        assertEquals(
            "可选的档里有两项落到上游的同一个值上 ⇒ 以为在选，其实选哪个都一样。" +
                "别再加回别名档：$collisions",
            emptyMap<String, List<String>>(),
            collisions,
        )
    }

    /**
     * 那两个别名值原样留成用例：这两个值就是判据本身。
     *
     * 同 `CommandPaletteTest` 把 `discord:configure` / `photo-capture` 两个「尾音节巧合」
     * 原样留在判据里的做法：具体形态不留下来，下一个人只会再撞一次。
     */
    @Test
    fun theTwoUpstreamAliasesArePinnedAsARegressionCase() {
        assertEquals(
            "「不选」与「manual」在上游是同一档：所以没有「不选」那颗键",
            effectivePermissionMode(null),
            effectivePermissionMode("manual"),
        )
        assertEquals("不选 ⇒ 不带参数 ⇒ 跑上游自己的默认", UPSTREAM_DEFAULT_PERMISSION_MODE, effectivePermissionMode(null))
        assertEquals("manual 是上游 default 的别名", UPSTREAM_DEFAULT_PERMISSION_MODE, effectivePermissionMode("manual"))
        assertEquals("另两档各是自己", "acceptEdits", effectivePermissionMode("acceptEdits"))
        assertEquals("另两档各是自己", "plan", effectivePermissionMode("plan"))
    }

    /**
     * 白名单外的任何值（存量脏值、不给的档）也落在上游的默认档上。
     *
     * 这不是我们自己编的归一，是 [ClaudeInvocation.permissionModeFlag] 本来就 fail-closed：
     * 不过白名单就回空串 ⇒ 命令里不带那个参数。两边必须是同一句话，否则屏上高亮的那一档
     * 和实际跑的那一档会对不上，而「屏上说的和跑的不是一件事」比画错更坏。
     */
    @Test
    fun aValueOutsideTheWhitelistFallsBackTheSameWayTheCommandBuilderDoes() {
        for (bogus in listOf("bypassPermissions", "dontAsk", "auto", "", "Manual", "nonsense")) {
            assertEquals(
                "「$bogus」不在白名单里 ⇒ 不带参数 ⇒ 上游默认档",
                UPSTREAM_DEFAULT_PERMISSION_MODE,
                effectivePermissionMode(bogus),
            )
            assertEquals(
                "和真正拼命令那一句必须同口径（它不过白名单就回空串）",
                "",
                ClaudeInvocation.permissionModeFlag(bogus),
            )
        }
    }

    // === 高亮哪一档 =============================================================

    @Test
    fun everyValueWeCanStoreLandsOnExactlyOneOption() {
        val everything = listOf(null) + ClaudeInvocation.PERMISSION_MODES + listOf("bypassPermissions", "", "junk")
        for (stored in everything) {
            assertNotNull(
                "存着「$stored」时屏上一档都不亮 ⇒ 用户看不出现在是哪一档，而那正是这一屏的全部意义",
                selectedPermissionModeValue(stored),
            )
        }
    }

    @Test
    fun notHavingChosenHighlightsTheFirstOptionBecauseThatIsWhatActuallyRuns() =
        assertEquals(
            "「不选也是它」：没选过时高亮第一档是真话（不带参数跑的就是那一档），不是拿第一档当默认值糊弄",
            PERMISSION_MODE_OPTIONS.first().value,
            selectedPermissionModeValue(null),
        )

    @Test
    fun choosingAnOptionHighlightsThatSameOption() {
        for (o in PERMISSION_MODE_OPTIONS) {
            assertEquals("选了「${o.title}」就该亮「${o.title}」", o.value, selectedPermissionModeValue(o.value))
        }
    }

    // === 这一排有哪几档 =========================================================

    /**
     * 条数由 [ClaudeInvocation.PERMISSION_MODES] 那张白名单定，这里不许另列一份。
     *
     * 并且不许变成「上游有几档就画几档」：上游真值集是 6 个，而能切到哪几档是动态的
     * 2–4 档（`plan` / `dontAsk` 根本不在它 shift+tab 的循环里）。
     * 这一屏设的是起对话时带的那个参数，不是对话里那个循环。
     */
    @Test
    fun theOptionsAreExactlyOurWhitelistNeitherFewerNorTheUpstreamSix() {
        assertEquals(
            "档数必须等于白名单条数（白名单改了这里跟着改，别在两个地方各写一份）",
            ClaudeInvocation.PERMISSION_MODES,
            PERMISSION_MODE_OPTIONS.map { it.value },
        )
        for (never in listOf("bypassPermissions", "dontAsk", "auto", UPSTREAM_DEFAULT_PERMISSION_MODE)) {
            assertFalse(
                "「$never」不许上屏：`bypassPermissions` 是刻意不给的，`dontAsk`/`auto` 语义没核实过，" +
                    "`default` 则是第一档的别名",
                PERMISSION_MODE_OPTIONS.any { it.value == never },
            )
        }
    }

    @Test
    fun theFirstOptionSaysThatNotChoosingLandsHere() =
        assertTrue(
            "没有「不选」那颗键，「不选也是它」写进第一档的副标题：" +
                "副标题里没这句话的话，少一颗键就成了悄悄删掉一个选项。" +
                "实得「${PERMISSION_MODE_OPTIONS.first().subtitle}」",
            "不选也是它" in PERMISSION_MODE_OPTIONS.first().subtitle,
        )

    @Test
    fun everyOptionSpeaksHuman() {
        for (o in PERMISSION_MODE_OPTIONS) {
            assertFalse("档名漏成了那个裸值「${o.value}」：说人话，别把参数名推上屏", o.title == o.value)
            assertTrue("「${o.title}」没有副标题：三档之间的差别就全靠猜了", o.subtitle.isNotBlank())
        }
    }

    /**
     * 说不清就说说不清：另一半得明说出来。
     *
     * 我们管得着的只有起对话那一刻带的参数；开起来之后在那台机器上还能当场换，
     * 而能换到哪几档由那台自己定。不说这句话，这一屏就是在声称一个它守不住的边界。
     */
    @Test
    fun theScreenAdmitsWhatItDoesNotControl() {
        assertTrue(
            "得说清这里设的是「开始一段新对话时」的那一档：$PERMISSION_MODE_ELSEWHERE_NOTE",
            "新对话" in PERMISSION_MODE_ELSEWHERE_NOTE || "新开始" in PERMISSION_MODE_ELSEWHERE_NOTE,
        )
        assertTrue(
            "还得说清开起来之后在那台上还能换：$PERMISSION_MODE_ELSEWHERE_NOTE",
            "还能" in PERMISSION_MODE_ELSEWHERE_NOTE,
        )
        assertTrue(
            "管不着就说管不着，别含糊过去：$PERMISSION_MODE_ELSEWHERE_NOTE",
            "管不着" in PERMISSION_MODE_ELSEWHERE_NOTE,
        )
    }

    /** 这一排上屏的每一句都要过禁用词表（大小写不敏感）。 */
    @Test
    fun everyWordOnThisRowObeysTheProductWordList() {
        val spoken =
            PERMISSION_MODE_OPTIONS.flatMap { listOf(it.title, it.subtitle) } +
                listOf(PERMISSION_MODE_SECTION_TITLE, PERMISSION_MODE_ELSEWHERE_NOTE)
        for (s in spoken) {
            for (w in productWordList) {
                assertFalse("禁用词「$w」不许出现在用户看得见的地方：$s", w.lowercase() in s.lowercase())
            }
        }
    }

    // === 触控目标 ===============================================================

    /**
     * 一行至少是一个触控目标。
     *
     * 下界显式写成常量而不是靠 `padding` 撑：`padding` 撑不出下界（字号一小行就矮）。
     */
    @Test
    fun theRowIsAtLeastATouchTarget() =
        assertTrue(
            "Material3 最小触控目标是 48dp，实得 $PERMISSION_MODE_ROW_MIN_HEIGHT",
            PERMISSION_MODE_ROW_MIN_HEIGHT >= 48.dp,
        )

    // === 射程：屏上读的是这份数据 =================================================

    /**
     * 屏上那一排必须读这份数据，而且选中标记必须是真的单选件。
     *
     * 没有这一条的话，上面全部判据都可以在一个「屏上另写了一排硬编码的键」的世界里全绿。
     *
     * 判别力边界：它读的是源码里有没有这几句，不是「屏上真的画成了单选」。
     */
    @Test
    fun theScreenDrawsTheOptionsFromTheData() {
        val code = codeOnly(settingsScreenSource())
        assertTrue("前提：得真读到 SettingsScreen.kt 的代码，否则本条判据恒绿", "fun SettingsScreen(" in code)
        assertTrue("那一排必须遍历 PERMISSION_MODE_OPTIONS", "PERMISSION_MODE_OPTIONS" in code)
        assertTrue("高亮哪一档必须问 selectedPermissionModeValue", "selectedPermissionModeValue" in code)
        assertTrue("选中标记要用真的单选件", "RadioButton(" in code)
        assertFalse(
            "不许有「不选」键：`setPermissionMode(null)` 在 ⇒ 它与第一档又是同一档",
            "setPermissionMode(null)" in code,
        )
        assertFalse(
            "选中标记不许是在标签前面拼一个间隔点（图上分不出哪个选中，" +
                "而且它会改变那一行的宽度 ⇒ 点一下整排重排）",
            "· " in code,
        )
    }

    // === 工具 ===================================================================

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器。
     *
     * 本条既问代码（`PERMISSION_MODE_OPTIONS` / `RadioButton(` 在不在）、又问字面（选中标记不许是 `"· "`
     * 那个间隔点），所以字面不能丢。注释必须剥：本文件自己在注释里引了那个写法，不剥的话那一条当场假红。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    private fun settingsScreenSource(): String =
        (
            File("src/main/kotlin/com/ccmonitor/mobile/ui/settings/SettingsScreen.kt").takeIf { it.isFile }
                ?: File("app/src/main/kotlin/com/ccmonitor/mobile/ui/settings/SettingsScreen.kt")
        ).readText()

    /** 禁用词，逐字（同 `UiWordlistTest` / `ChatAccountGateTest` 那张表）。 */
    private val productWordList = listOf("连接", "SSH", "tmux", "会话", "exec", "bridge", "daemon", "offset")
}
