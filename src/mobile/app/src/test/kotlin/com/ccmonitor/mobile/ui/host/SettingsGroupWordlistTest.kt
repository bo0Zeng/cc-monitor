package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.identity.identityDeleteWarning
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 「设置 · 服务器」那一组屏（`ui/host` · `ui/identity`）的文案词表棘轮。
 *
 * 别的词表判据盖不到这一组：`ChatMenuTest` 只对聊天屏菜单判，`UiWordlistTest` 只认
 * `ui/overview` · `ui/chat` · `ui/claude` · `ui/settings`，`ChatAccountGateTest` 只守 `NO_ACCOUNT_CHOSEN` 一个常量。
 * `ScreenTitlesTest` 管这一组的标题；本文件管这两个包的全部上屏文案。
 *
 * 口径是棘轮，不是零命中：这两个包里还有不少带禁用词的句子。声称「已清零」是假话；
 * 放着不管则新漏进来的一句也拦不住。照 `UiWordlistTest` 的做法：记住现在的数，只许往下走。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 往 `ui/host` / `ui/identity` 的 `Text(…)` 里新加一句带禁用词的 | 红 |
 * | 把那句话搬进一个常量再 `Text(那个常量)` | 抓不到；所以这一组的上屏常量另有 [theNewCopyConstantsAreClean] 逐个点名 |
 * | `SwitchRow("…")` / `AlertDialog(title = …)` 这种不是 `Text(` 直接实参的字面 | 抓不到：扫描器认的是 `Text(` 的实参括号（「打开就直接进那台上的工作区」就住 `SwitchRow` 的第一个实参，没有判据守着） |
 * | 那句话真的画出来了没有 | 抓不到：布局要 `androidTest` 或真机 |
 * | 「实现词上屏」这一类 | 没有可执行形式，是人判的；本文件只机检裸词 |
 */
class SettingsGroupWordlistTest {
    /**
     * 禁用词，逐字，与 `UiWordlistTest` / `ChatAccountGateTest` 同源。
     *
     * 「会话」「连接」两个裸词不在 `CHAT_MENU_BANNED_WORDS` 里（那张表只管聊天屏菜单的技术义），
     * 但这里照禁。
     */
    private val banned = listOf("连接", "SSH", "tmux", "会话", "exec", "bridge", "daemon", "offset")

    private data class Hit(
        val where: String,
        val word: String,
        val literal: String,
    )

    @Test
    fun theSettingsAndServerFacesNeverGetWorse() {
        ratchets.forEach { (pkg, baseline) ->
            val hits = scan(pkg)
            assertTrue(
                "ui/$pkg 的词表违规从 $baseline 涨到了 ${hits.size}：棘轮只许往下走" +
                    "（还欠着的不少，但不许再往上漏）：\n" +
                    hits.joinToString("\n") { "  ${it.where}  「${it.word}」 in ${it.literal}" },
                hits.size <= baseline,
            )
        }
    }

    /**
     * 阴性对照：扫描器自己得真的在扫东西。
     *
     * 没有这条的话，一个「路径写错 ⇒ 永远返回空列表」的 bug 会让上面那条恒绿。
     */
    @Test
    fun theScannerIsActuallyReadingTheseTwoPackages() {
        for (pkg in ratchets.keys) {
            val calls = sourcesOf(pkg).sumOf { KotlinSourceScanner.literalsInCallsTo(it.readText(), TEXT).callSites }
            assertTrue("前提：ui/$pkg 里一处 `$TEXT(` 都没认出来 ⇒ 本文件恒绿（实得 $calls 处）", calls > 5)
        }
    }

    /**
     * 这一组的上屏常量：零容忍，不进棘轮。
     *
     * 棘轮管的是「还欠着的那些」；改过的这几句一个禁用词都不许有。
     */
    @Test
    fun theNewCopyConstantsAreClean() {
        for (s in listOf(HOSTS_SCREEN_TITLE, IDENTITY_SCREEN_TITLE)) {
            for (w in banned) {
                assertTrue("禁用词「$w」不许出现在「$s」里", w.lowercase() !in s.lowercase())
            }
        }
    }

    /**
     * 住在纯函数里的那几句上屏文案，逐条钉死残余。
     *
     * 扫描器看不见它们：它认的是 `Text(` 的字面实参，而这几句是
     * `Text(identityDeleteWarning(...))` 这种函数产物（头注边界表的第二行）。不钉的话这一族永远在棘轮的射程外。
     *
     * 表里还有一条：「删除身份「X」？引用它的主机将无法连接。」。`ui/DeleteWarningTest` 逐字断言
     * `w.contains("无法连接")`：「级联后果要点明」这件事它钉得对，但它钉的那个串本身含禁用词。
     * 改这句话必须同时改那个判据。
     *
     * 这张表多一条红、少一条也红：有人把它清掉了，得连这张表一起过一遍眼
     * （同 `AgentArchetypeTest.PINNED_HARDCODED_PRODUCT_NAMES` 的做法）。
     */
    @Test
    fun theCopyThatLivesInPureFunctionsIsPinnedBecauseTheScannerCannotSeeIt() {
        val produced =
            mapOf(
                "hostDeleteWarning" to hostDeleteWarning(SAMPLE),
                "identityDeleteWarning(硬件)" to identityDeleteWarning(SAMPLE, keystoreBacked = true),
                "identityDeleteWarning(软件)" to identityDeleteWarning(SAMPLE, keystoreBacked = false),
            )
        val hits = produced.flatMap { (who, text) -> banned.filter { it in text }.map { "$who 「$it」" } }.sorted()
        assertEquals(
            "住在纯函数里的上屏文案，词表命中变了。少一条 ⇒ 有人改好了，请连 `ui/DeleteWarningTest` " +
                "里那条逐字断言「无法连接」的判据一起过一遍眼，再改这张表。",
            PINNED_FUNCTION_COPY_HITS,
            hits,
        )
    }

    // === 工具 ===================================================================

    private fun sourcesOf(pkg: String): List<File> {
        val rel = "src/main/kotlin/com/ccmonitor/mobile/ui/$pkg"
        val dir = File(rel).takeIf { it.isDirectory } ?: File("app/$rel")
        assertTrue("找不到 $pkg 的源码目录（判据自己失效了，比不合规更坏）：${dir.absolutePath}", dir.isDirectory)
        return dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
    }

    /**
     * 抠出这个包里每个 `Text(…)` 实参里的字面，逐个比词表。
     *
     * 走全仓唯一一份词法扫描器 [KotlinSourceScanner.literalsInCallsTo]：
     * 本仓不许再长第二份手搓的抠字面器（`SharedScannerGateTest` ⑥ 守着这件事）。
     */
    private fun scan(pkg: String): List<Hit> =
        sourcesOf(pkg).flatMap { f ->
            KotlinSourceScanner.literalsInCallsTo(f.readText(), TEXT).literals.flatMap { lit ->
                banned.filter { it in lit.body }.map { Hit("${f.name}:${lit.line}", it, lit.body) }
            }
        }

    /**
     * 还欠着的包 → 现有的违规条数。
     *
     * 这些数只许往下改。往上改 = 又往 UI 上漏了一个技术词。
     */
    private val ratchets = mapOf("host" to HOST_BASELINE, "identity" to IDENTITY_BASELINE)

    private companion object {
        /** 本文件问的是「上屏的那句话」，所以点名 Compose 的 `Text(…)`，同 `UiWordlistTest`。 */
        private const val TEXT = "Text"

        /** 删除确认那两句里拿来当样本的名字（内容不重要，只要两处用同一个）。 */
        private const val SAMPLE = "devbox"

        /**
         * 住在纯函数里、还含禁用词的那几句。
         *
         * 两条都是同一句话的两个分支，理由见
         * [theCopyThatLivesInPureFunctionsIsPinnedBecauseTheScannerCannotSeeIt] 的头注。
         */
        private val PINNED_FUNCTION_COPY_HITS =
            listOf("identityDeleteWarning(硬件) 「连接」", "identityDeleteWarning(软件) 「连接」")

        /**
         * `ui/host` 在 `Text(` 字面里的词表命中数。
         *
         * 这 5 条是什么（报错信息会逐条列出来）：
         * - 2 条是同一句：那条 Windows 说明（`连接` ＋ `tmux`），它被
         *   `ui/nav/AgentArchetypeTest.PINNED_HARDCODED_PRODUCT_NAMES` 按逐字等号钉着，改它要两边一起改。
         * - 2 条是终端那条路的字（新建会话模式的自明提示 ＋「会话」那颗动作键）。
         * - 1 条是额外地址那行预览（「将并发连接 N 个地址」）。
         */
        private const val HOST_BASELINE = 5
        private const val IDENTITY_BASELINE = 0
    }
}
