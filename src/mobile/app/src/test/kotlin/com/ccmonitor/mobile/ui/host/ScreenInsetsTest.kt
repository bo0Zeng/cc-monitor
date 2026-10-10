package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.nav.SETTINGS_GROUP_SCREEN_INSETS
import com.ccmonitor.mobile.ui.nav.ScreenInset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 「设置 · 服务器」那一组屏的窗口内边距判据（这一组屏跨包的判据都住 `ui/host` 的测试包）。
 *
 * `MainActivity` 开着 `enableEdgeToEdge()`、`AndroidManifest.xml` 里没有 `windowSoftInputMode`，
 * 所以每个画像素的屏都得自己垫 `statusBarsPadding` / `navigationBarsPadding` / `imePadding`；
 * 不垫的后果两样：标题被状态栏压、键盘弹起时焦点框被遮。
 *
 * 判别力边界：键盘弹起时 `TextField` 自带的 `BringIntoViewRequester` 会不会把焦点框滚进可视区，
 * 门禁里量不了（app 没引 Robolectric，Compose 的布局行为要 `androidTest` 或真机）。
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 哪个屏漏掉了那一句 | 红（[everyScreenInThisGroupLetsTheSystemBarsThrough]） |
 * | 底部两块的顺序被调反（手势条算两遍） | 红（[theBottomInsetsAreOrderedSoTheyDoNotDoubleCount]） |
 * | 垫在 `verticalScroll` 之后（视口没缩、焦点框照样在键盘底下） | 红（[theTwoScrollingScreensPadBeforeTheyScroll]） |
 * | 真机上焦点那个框到底遮不遮 | 量不了：真机 / `androidTest` |
 * | 状态栏真实高度（刘海屏常见 40–48dp） | 没量过：这也正是垫系统报的那个值而不是写死一个 dp 的理由 |
 */
class ScreenInsetsTest {
    // === 垫哪几块、按什么顺序 ====================================================

    @Test
    fun allThreeSystemAreasAreAccountedFor() =
        assertEquals(
            "三块都得让开：状态栏（标题被压）· 手势条（FAB/底部动作被压）· 输入法（焦点框被遮）。" +
                "少一块就是没让开",
            ScreenInset.entries.toSet(),
            SETTINGS_GROUP_SCREEN_INSETS.toSet(),
        )

    /**
     * 顺序是承重的。
     *
     * Compose 的 `windowInsetsPadding` 系列会消耗掉自己垫掉的那部分，内层只垫剩下的。
     * ⇒ 手势条排在输入法前面时，键盘弹起那一刻底部总量是 `手势条 + (键盘 − 手势条) = 键盘`；
     * 反过来排就把手势条那一段算两遍：屏下沿凭空多出一条空白，而那条空白在截图上
     * 看着像「设计就是这样」，没人会去查。
     */
    @Test
    fun theBottomInsetsAreOrderedSoTheyDoNotDoubleCount() {
        val nav = SETTINGS_GROUP_SCREEN_INSETS.indexOf(ScreenInset.NavigationBars)
        val ime = SETTINGS_GROUP_SCREEN_INSETS.indexOf(ScreenInset.Ime)
        assertTrue("前提：两块都得在表里，否则本条判据恒绿（nav=$nav, ime=$ime）", nav >= 0 && ime >= 0)
        assertTrue(
            "手势条必须排在输入法之前：反了就会把手势条那一段算两遍（实得 nav=$nav, ime=$ime）",
            nav < ime,
        )
    }

    // === 射程：一个屏都不许漏 ====================================================

    /**
     * 这一组每一个画像素的屏都得真的调上那一句。
     *
     * 一个新屏加进这一组而忘了垫 ⇒ 本条点名那个文件。
     */
    @Test
    fun everyScreenInThisGroupLetsTheSystemBarsThrough() {
        val missing =
            SCREENS_IN_THIS_GROUP.filter { (path, atLeast) ->
                countOf(codeOnly(sourceOf(path)), INSET_CALL) < atLeast
            }
        assertEquals(
            "这几个屏没把 `$INSET_CALL` 垫上：" +
                missing.map { it.first },
            emptyList<Pair<String, Int>>(),
            missing,
        )
    }

    /** 前提自检：扫描面真的读到了东西，否则上一条是一句「零命中 = 守住了」。 */
    @Test
    fun theScanSurfaceIsReal() {
        for ((path, _) in SCREENS_IN_THIS_GROUP) {
            val code = codeOnly(sourceOf(path))
            assertTrue("前提：$path 读不到代码（`@Composable` 一个都没有）⇒ 本文件恒绿", "@Composable" in code)
        }
    }

    /**
     * 两个会滚的屏：内边距必须垫在滚动之前。
     *
     * 垫在滚动之后等于在滚动内容里加一段空白：视口没缩，焦点框照样在键盘底下
     * （机器编辑器整表一千多 dp，弹了键盘只剩不到一半）。
     *
     * 判别力边界：读的是同一个函数体里的词法先后，不是包含关系
     * （同 `SettingsEntriesTest.theEntrancesLiveInsideAScrollableColumn` 那条的边界）。
     */
    @Test
    fun theTwoScrollingScreensPadBeforeTheyScroll() {
        for ((path, fn) in SCROLLING_SCREENS) {
            val body = topLevelFunctionBody(codeOnly(sourceOf(path)), fn)
            val pad = body.indexOf(INSET_CALL)
            val scroll = body.indexOf(SCROLL_CALL)
            assertTrue("前提：$fn 里得找到 `$INSET_CALL`（实得 @$pad）", pad >= 0)
            assertTrue("前提：$fn 里得找到 `$SCROLL_CALL`（实得 @$scroll）", scroll >= 0)
            assertTrue(
                "$fn：内边距（@$pad）必须垫在滚动（@$scroll）之前：" +
                    "垫在后面的话视口没缩，键盘弹起时焦点框照样被遮",
                pad < scroll,
            )
        }
    }

    // === 工具 ===================================================================

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器。
     *
     * 选留字面那支：本文件要按顶格 `}` 切函数体，留字面那支与原文的行结构逐行对齐
     * （同 `SettingsEntriesTest.settingsScreenBody`）。针（`settingsGroupScreenInsets()` / `verticalScroll(` /
     * `@Composable`）都是代码记号，这几个文件的字面里一个都不会出现，留着字面不会假阳。
     * 注释必须剥：本组源码的注释里逐字引着这几个针。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    private fun sourceOf(relative: String): String =
        (File(relative).takeIf { it.isFile } ?: File("app/$relative")).let {
            assertTrue("前提：得找得到 $relative（实际找的是 ${it.absolutePath}）", it.isFile)
            it.readText()
        }

    /**
     * [fn] 那个顶层函数的函数体（剥过注释）。
     *
     * 按顶格 `}` 认收尾，同 `SettingsEntriesTest.settingsScreenBody`。
     * 函数被改成表达式体、或那个 `}` 不再顶格 ⇒ 本判据误红（不是漏红），
     * 误红是安全的一侧，它逼人来看一眼。
     */
    private fun topLevelFunctionBody(
        code: String,
        fn: String,
    ): String {
        val at = code.indexOf(fn)
        assertTrue("前提：得找到 `$fn`，否则本条判据恒绿", at >= 0)
        val end = code.indexOf("\n}\n", at)
        assertTrue("前提：得找到 `$fn` 顶格的收尾 `}`，否则本条判据恒绿（起点 @$at）", end > at)
        return code.substring(at, end)
    }

    /** [needle] 在 [text] 里出现几次（不重叠），同 `SettingsEntriesTest.countOf`。 */
    private fun countOf(
        text: String,
        needle: String,
    ): Int = text.split(needle).size - 1

    private companion object {
        private const val INSET_CALL = "settingsGroupScreenInsets()"
        private const val SCROLL_CALL = "verticalScroll("

        private const val HOST_SCREENS = "src/main/kotlin/com/ccmonitor/mobile/ui/host/HostScreens.kt"
        private const val SETTINGS_SCREEN = "src/main/kotlin/com/ccmonitor/mobile/ui/settings/SettingsScreen.kt"

        /**
         * 这一组画像素的屏 → 那个文件里至少该有几处内边距调用。
         *
         * 启动判定那个屏一个像素都不画（composable 体内只有一个 `LaunchedEffect`），所以不在表里。
         * `HostScreens.kt` 里住着两个（服务器列表 ＋ 机器编辑器），所以它要 2 处。
         * 按钮编辑器（`ButtonSettingsScreen.kt`）跟终端那一族在一起。
         */
        private val SCREENS_IN_THIS_GROUP =
            listOf(
                HOST_SCREENS to 2,
                "src/main/kotlin/com/ccmonitor/mobile/ui/identity/IdentityScreen.kt" to 1,
                SETTINGS_SCREEN to 1,
                "src/main/kotlin/com/ccmonitor/mobile/ui/settings/ButtonSettingsScreen.kt" to 1,
            )

        /** 会滚、而且屏上有输入框的那两个：「垫在滚动之前」这条只对它们有意义。 */
        private val SCROLLING_SCREENS =
            listOf(
                HOST_SCREENS to "fun HostEditorScreen(",
                SETTINGS_SCREEN to "fun SettingsScreen(",
            )
    }
}
