package com.ccmonitor.mobile.ui.nav

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 「还不知道」不许写成一个合法值。
 *
 * 病型：一个还没读到真值的状态被初始化成一个合法值（`null` 当「不带这个参数」、
 * 回退值当「用户的选择」、`!= false` 把「还不知道」折成「开」），下游拿着假值先跑一趟。今天一处：
 *
 * | # | 处 | 读数 |
 * |---|---|---|
 * | ③ | `ui/nav/AppNavHost.kt` 的 `newUi != false` | 写不进 `FLAG_OFF` ⇒ 真值恒为「开」⇒ 折下来的值与真值逐字相同，实际不可达（[thereIsNoWayToWriteTheOffFlagSoTheFoldedUnknownCannotDiverge] 是它的哨兵） |
 *
 * ### 判别力边界
 *
 * 1. 一条 composable 都跑不起来（`app` 没引 Robolectric）。本文件里没有任何一条断言看得见
 *    「第一帧到底拿到了什么」「`LaunchedEffect` 到底重起了几趟」「两个 `ViewModel` 是不是真的都建出来了」，
 *    那三件事只有真机或 `androidTest` 量得到。
 * 2. 源码扫描那两条剥注释与字符串字面（见下面的 `codeOnly`），藏在字符串里的调用（反射、按名拼接）一概看不见。
 * 3. ③ 那条哨兵钉的是「我们自己的代码里没有写 `FLAG_OFF` 的路」。手改数据库写下过 `FLAG_OFF`，它一声不吭。
 */
class UnknownBeforeLoadedTest {
    // ---- ③ 点服务器那条门：证明它实际不可达 ------------------------------------

    /**
     * ③：`newUi != false` 把「还不知道」折成「开」，但折不出分歧，因为真值恒为「开」。
     *
     * `newUiEnabled()` 的判据是 `!= FLAG_OFF`，而 `FLAG_OFF` 只有一条写入路径：
     * `SettingsRepository.setNewUiEnabled(false)` ← `SettingsViewModel.setNewUi(false)` ← 没有调用方
     * （`NewUiSettingsTest` 钉着「设置屏里不许出现 `setNewUi`」）。我们自己的代码写不出 `FLAG_OFF`，
     * 真值恒为 `true`，未知窗口里折下来的 `null != false` 也是 `true`，分歧为空。
     *
     * 这条是哨兵，不是免罪符：第三组断言摆的正是「分歧长什么样」，真值若是 `false`，落点不同。
     * 谁哪天给 `setNewUi` 接上一个调用方，前两组断言当场红，提醒他 `AppNavHost` 那一行
     * `newUi != false` 要跟着改成三值。
     *
     * 它钉不住：手改数据库写下过 `FLAG_OFF`；未知窗口有多长、窗口里那一屏上有没有服务器可点
     * （起点是 `Screen.Launch`，那一屏没有任何可点的东西），要真机。
     */
    @Test
    fun thereIsNoWayToWriteTheOffFlagSoTheFoldedUnknownCannotDiverge() {
        val prod = productionCode()

        // 第一组（必须扫到东西）：两个定义都得在，否则就是扫描器瞎了而不是「守住了」
        val writerDefs = prod.filterValues { DEFINE_REPO_WRITER.containsMatchIn(it) }.keys
        val vmDefs = prod.filterValues { DEFINE_VM_WRITER.containsMatchIn(it) }.keys
        assertEquals("前提：`setNewUiEnabled` 的定义必须恰好一处，实得 $writerDefs", 1, writerDefs.size)
        assertEquals("前提：`setNewUi` 的定义必须恰好一处，实得 $vmDefs", 1, vmDefs.size)

        // 第二组（不该有的零命中）：写 FLAG_OFF 那条链上没有任何入口
        // 数的是出现次数不是文件数：按文件过滤的话，
        //   「在 `SettingsViewModel` 自己里面再加一个 `disableNewUi() = setNewUi(false)`」会整条漏掉。
        val vmHits = prod.mapValues { (_, c) -> TOKEN_VM_WRITER.findAll(c).count() }.filterValues { it > 0 }
        assertEquals(
            "`setNewUi` 全仓只许出现一次（它自己的定义）：多一处就是回退门的入口回来了，" +
                "`AppNavHost` 那句 `newUi != false` 当场从「不可达」变成真 bug。实得：$vmHits",
            mapOf(vmDefs.single() to 1),
            vmHits,
        )
        val writerHits = prod.mapValues { (_, c) -> TOKEN_REPO_WRITER.findAll(c).count() }.filterValues { it > 0 }
        assertEquals(
            "`setNewUiEnabled` 只许出现两次：定义，加 `SettingsViewModel` 那一跳（而它自己没有调用方）。实得：$writerHits",
            mapOf(writerDefs.single() to 1, vmDefs.single() to 1),
            writerHits,
        )
        val rowWriters = prod.filterValues { it.contains("Settings(KEY_NEW_UI_ENABLED") }.keys
        assertEquals(
            "那一行数据库记录只许在一个地方被写：多一处就是绕过上面两条的旁路。实得：$rowWriters",
            writerDefs,
            rowWriters,
        )

        // 第三组：分歧长什么样（它为空，但不是因为两边本来就一样）
        val folded = onConnectDestination(HOST, FOLDED_UNKNOWN, AgentKind.ClaudeCode, fixedId, ::terminalOf)
        val onlyReachableReal = onConnectDestination(HOST, true, AgentKind.ClaudeCode, fixedId, ::terminalOf)
        val hypotheticalOff = onConnectDestination(HOST, false, AgentKind.ClaudeCode, fixedId, ::terminalOf)
        assertTrue("前提：折下来的值就是「开」（`null != false`）", FOLDED_UNKNOWN)
        assertEquals("折下来的值与今天唯一可达的真值给出同一个落点 ⇒ 分歧为空", onlyReachableReal, folded)
        assertNotEquals(
            "前提：真值若是「关」，落点确实不同：否则上面那条是因为「怎么走都一样」而绿，等于恒绿",
            hypotheticalOff,
            folded,
        )
    }

    // ---- 扫描器 ---------------------------------------------------------------

    private fun readFile(relative: String): String =
        File(repoRoot(), relative).let {
            assertTrue("前提：得找得到 $relative", it.isFile)
            it.readText()
        }

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    /** 全部生产源（`app` + `core-*` 的 `src/main/kotlin`），值是剥过注释与字符串的代码。 */
    private fun productionCode(): Map<String, String> {
        val root = repoRoot()
        val roots =
            (listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList()))
                .map { File(it, "src/main/kotlin") }
                .filter { it.isDirectory }
                .sortedBy { it.path }
        assertTrue("前提：得找到扫描面（app + core-*），实得 ${roots.size} 个源根", roots.size >= 2)
        val files = roots.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
        assertTrue("前提：得真扫到源文件，实得 ${files.size} 个", files.size > 150)
        return files.associate { it.relativeTo(root).invariantSeparatorsPath to codeOnly(it.readText()) }
    }

    /**
     * 剥注释与字符串字面，走共享词法扫描器 [KotlinSourceScanner.codeOnlyDroppingLiterals]。
     *
     * 代价：字符串字面被整段删掉，藏在字面里的调用本文件一概看不见（共享实现的头注里写全了）。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyDroppingLiterals(text)

    /** 新对话编号工厂：写成函数是因为 `fixedId` 会被 detekt 判成「返回常量的函数」。 */
    private val fixedId = { "11111111-2222-3333-4444-555555555555" }

    private fun terminalOf(hostId: String): String = "session/tab-$hostId"

    companion object {
        private const val HOST = "h1"

        /** `AppNavHost.kt` 那句 `newUi != false` 在 `newUi` 还是 null 时折出来的值。 */
        private const val FOLDED_UNKNOWN = true

        private val DEFINE_REPO_WRITER = Regex("""fun\s+setNewUiEnabled\s*\(""")
        private val DEFINE_VM_WRITER = Regex("""fun\s+setNewUi\s*\(""")
        private val TOKEN_REPO_WRITER = Regex("""\bsetNewUiEnabled\b""")
        private val TOKEN_VM_WRITER = Regex("""\bsetNewUi\b""")

        private const val MAX_WALK_UP = 6
    }
}
