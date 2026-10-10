package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 「重新接上内容」重跑整条开屏序列，且门关着时那句人话不被它擦掉。
 *
 * 两条缺陷形态：
 * - 按钮若只重挂下行（`vm.reattach(pipe.frames())`），管道已经死了的时候，重挂只是再 tail 一次一个不再增长的文件：
 *   按钮点得动、界面变回正常、对面一个字都收不到。
 * - `canReattach = !feeding` 时，账号门/建连失败那两条早退路径没调过 `start()`，`feeding` 恒 false，按钮出得来；
 *   按下去若只 `reattach`，它会把 `downlinkError` 清掉，屏上那句唯一的人话当场消失且屏内不可恢复。
 *
 * 不把按钮藏掉（`canReattach = !feeding && started`）：那样门关着时少了屏内唯一一个能重试的动作，
 * 成了一条响亮的死路。做法是让那个动作真的有用：按钮重跑整条序列，门还关着就再报一次那句人话。
 *
 * ### 判别力边界
 *
 * | 形态 | 这份判据 |
 * |---|---|
 * | `onReattach` 退回只重挂下行 | 红 |
 * | 重试计数没进 `LaunchedEffect` 的 key（= 按了不重跑） | 红 |
 * | 重试计数没进建连那道门（= 接不上主机时按了白按） | 红 |
 * | 账号门那句人话被挪到早退之后 | 红（`ChatAccountGateTest` 那条守着） |
 * | 计数被包进恒假条件 / 谓词写反 | 抓不到：同 `ChatAccountGateTest` 的边界，只能靠真机 |
 * | 格式化成多行 | 抓不到（字符串匹配吃格式） |
 */
class ChatReattachWiringTest {
    private fun appMainSources(): List<File> {
        val dir = File("src/main/kotlin").takeIf { it.isDirectory } ?: File("app/src/main/kotlin")
        return dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
    }

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 扫的是 `onReattach` 接线与 `PipeLauncher.start(` 这类代码；它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    private fun chatRoute(): String = codeOnly(appMainSources().first { it.name == "ChatRoute.kt" }.readText())

    /** 前提自检：剥注释这一步真在起作用，否则下面几条可能因「整份被当注释剥光」而恒绿。 */
    @Test
    fun theCommentStrippingActuallyStripsCommentsAndKeepsCode() {
        val sample =
            """
            /** KDoc 里提 onReattach = { attempt++ } 是合法的 */
            // 行注释里也提 onReattach = { attempt++ }
            onReattach = { attempt++ },
            """.trimIndent()
        val code = codeOnly(sample)
        assertTrue("代码里的必须留下来", code.contains("onReattach = { attempt++ },"))
        assertFalse("KDoc 里的必须被剥掉", code.contains("KDoc 里提"))
        assertFalse("行注释里的必须被剥掉", code.contains("行注释里也提"))
    }

    /**
     * 那个动作存在。
     *
     * 判据钉的是「`ChatScreen` 收得到一个非空的 `onReattach`」。
     * 把它改成 `onReattach = null` 或整行删掉，这条红。
     */
    @Test
    fun theReattachActionIsStillWiredIntoTheScreen() {
        assertTrue(
            "「重新接上内容」那个动作必须在",
            chatRoute().contains("onReattach = "),
        )
        assertFalse("而且不许被接成空的", chatRoute().contains("onReattach = null"))
    }

    /**
     * 那个动作重跑整条序列，不是只重挂下行。
     *
     * 判据钉两件事，缺一不可：
     * ① 按钮接的是重试计数，不是 `vm.reattach(...)`；
     * ② 那个计数真的进了 `LaunchedEffect` 的 key，否则按了也不重跑
     *    （「已 build 零效果」：函数对了，没人按对的方式调它）。
     */
    @Test
    fun pressingReattachReRunsTheWholeOpeningSequenceNotJustTheDownlink() {
        val code = chatRoute()
        assertTrue("按钮要触发重跑（重试计数），不是直接重挂下行", code.contains("onReattach = { attempt++ }"))
        assertFalse(
            "绝不许是「只重挂下行」：管道死了的时候那等于什么都没做",
            code.contains("onReattach = { vm.reattach("),
        )
        // ② 计数必须真的进 key，否则按了不重跑
        val effectKeys = code.substringAfter("LaunchedEffect(hostId, sessionId, launch").substringBefore(")")
        assertTrue("重试计数必须进 LaunchedEffect 的 key，实得「$effectKeys」", effectKeys.contains("attempt"))
    }

    /**
     * 建连那道门也要跟着重跑。
     *
     * 接不上主机时 `link` 是 `HostLink.Failed`；`rememberHostLink` 自己的 key 里没有重试计数的话，
     * 按多少下都还是那个失败结果，按钮仍然是个死胡同，只是死得不那么明显。
     */
    @Test
    fun theRetryAlsoReRunsTheConnectionGate() {
        val code = chatRoute()
        // 不能 `substringBefore(")")`：实参里有嵌套括号（`chatKey(hostId, sessionId)`），
        //   那样会在第一个内层 `)` 处截断成「hostId, chatHolder(chatKey(hostId, sessionId」，
        //   判据因为错误的原因红。
        val call = code.substringAfter("rememberHostLink(").lineSequence().first()
        assertTrue("建连那道门必须收到重试计数，实得「$call」", call.contains("attempt = attempt"))
    }

    /**
     * 门关着时那句人话在重跑之后仍然会被报出来。
     *
     * 判据量的是「早退路径里那两句 `reportStartFailure` 还在」：只要它们还在、且整条序列会被重跑，
     * 那句话就不可能被这个动作擦掉（重跑会再报一次，中间没有任何一处把它清空）。
     *
     * 判别力边界：它守不住「有人把清空挪到早退之前」。相邻那一格（门打开之后那句话必须消失）由
     * `ChatSessionTest.aFixedGateDoesNotLeaveItsHumanSentenceStuckOnScreen` 守。
     */
    @Test
    fun bothEarlyExitsStillSayTheirHumanSentence() {
        val code = chatRoute()
        assertTrue("接不上主机那句必须还在", code.contains("接不上这台主机："))
        assertTrue("没选账号那句必须还在", code.contains("reportStartFailure(NO_ACCOUNT_CHOSEN)"))
    }

    /**
     * 上行那道探活真的接在生产的 sink 上，而且用的是那条幂等重建。
     *
     * 光有 `PipeListening` 这个接口不算数（「已 build 零引用」）。
     * 这条量的是接线：`PipeUplinkSink` 的第三个实参真的是那个探活。
     */
    @Test
    fun theProductionUplinkReallyChecksThatSomeoneIsListening() {
        val code = chatRoute()
        assertTrue("生产的 sink 必须带探活", code.contains("ensureListeningFor("))
        assertTrue("探活必须走那条幂等重建（不另造探针）", code.contains("startPipeFor(manager, hostId, sessionId, ctx, permissionMode, isNew)"))
        // 起管道只有一条真实现：探活是复用它，不是第二条路
        val starters = appMainSources().filter { codeOnly(it.readText()).contains("PipeLauncher" + ".start(") }.map { it.name }
        assertEquals("main 源集里起管道只许有一条路：$starters", 1, starters.size)
    }
}
