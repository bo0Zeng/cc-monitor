package com.ccmonitor.mobile.service

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 前台服务的 turn-end 两条路（α = 后端流 `DaemonTurnEndSource`，β = 自己 tail jsonl）：那几个「调用点」还在不在。
 *
 * 扫源码而不是跑服务：要守的全在 `SshKeepAliveService.watchHost` 那个循环里，而 `app` 模块没引 Robolectric、
 * 门禁不跑 `androidTest`，循环本身在 JVM 判据里碰不到。判断那一半住在 [TurnEndPathArbiter] /
 * [TurnEndNotifyDecision] / [watchTargets] 并真测（`TurnEndPathPolicyTest` · `WatchTargetsTest`）；
 * 这里只守「那段代码还在原位、且真的在问它们吗」，写法同 `ForegroundServiceCrashGuardTest`。
 *
 * ### 它守得住什么 / 守不住什么
 *
 * | 这一格 | 本判据 |
 * |---|---|
 * | 有人把 α 那条路整个拆了（不再起 `DaemonTurnEndSource`） | 红 |
 * | 有人把 α 接上判定搬走、β 从此永不撤 tail（白付流量） | 红（断 `watchTargets(` 真带 `alphaCoversTurnEnd`） |
 * | 有人把 α 那条路的 `replaysHistory` 写成 `true`（⇒ α 首轮被吞、少一条通知） | 红 |
 * | 有人把 β 那条路的 `replaysHistory` 写成 `false`（⇒ 挂上去就灌一串历史通知） | 红 |
 * | 有人新开一条发通知的路、绕过那个唯一出口（⇒ 不进共用去重账本 ⇒ 重复通知） | 红（断 `notifyAgentDone(` 恰好一处调用） |
 * | 有人把「α 没说原因就停了」那条兜底删了（⇒ β 永远不起来 ⇒ 静默不通知） | 红 |
 * | 有人把 Codex 主机那道闸删了（α 不发 Codex 的 turn_end ⇒ 那些主机再也收不到通知） | 红 |
 * | 有人把 β 的 `debounce` 换成一个私有常量（两条路的延迟从此能各自漂） | 红 |
 * | 调用都在，但参数算错 / 协程时序不对 / 通知真发没发 | 看不见：要真机 |
 * | 反射 / 按名拼接 / 死分支 | 看不见（逐条在 [KotlinSourceScanner] 头注里） |
 */
class TurnEndAlphaWiringTest {
    private fun moduleFile(relativeToApp: String): File =
        File(relativeToApp).takeIf { it.exists() } ?: File("app/$relativeToApp")

    private fun serviceSource(): String {
        val f = moduleFile("src/main/kotlin/com/ccmonitor/mobile/service/SshKeepAliveService.kt")
        assertTrue("前提：扫不到服务源文件（本判据会恒绿）：${f.absolutePath}", f.isFile)
        val text = f.readText()
        assertTrue("前提：服务源文件得真有内容（实得 ${text.lineSequence().count()} 行）", text.lineSequence().count() > 300)
        return text
    }

    /**
     * 剥注释、把字面也删掉，走全仓唯一一份词法扫描器。
     *
     * 选「删字面」那一支：本文件问的全是「代码里有没有这个调用 / 这个实参」，
     * 而服务的头注里反复提到这些名字（`--tail-only`、`replaysHistory`…），不剥会被散文骗到。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyDroppingLiterals(text)

    private fun countOf(
        text: String,
        mark: String,
    ): Int = text.split(mark).size - 1

    /**
     * 取某个声明的函数体（含两头花括号），在已剥注释并删掉字面的文本上走。
     *
     * 全程不把引号当字符比：字面内容已经被扫描器删掉了，本文件压根不需要自己认词法
     * （那正是 `SharedScannerGateTest` ⑥ 禁的形态）。
     */
    private fun bodyOf(
        code: String,
        signature: String,
    ): String {
        val at = code.indexOf(signature)
        assertTrue("前提：代码里找不到 `$signature`（扫描面或签名写错了 ⇒ 判据恒绿）", at >= 0)
        val open = code.indexOf('{', at)
        assertTrue("前提：`$signature` 之后没有左花括号", open > at)
        var depth = 0
        for (i in open until code.length) {
            when (code[i]) {
                '{' -> depth++
                '}' -> {
                    depth--
                    if (depth == 0) return code.substring(open, i + 1)
                }
                else -> Unit
            }
        }
        error("前提：`$signature` 的函数体花括号没配平")
    }

    // ─── 前提：剥注释这一步真的在起作用 ───

    /**
     * 先证剥注释有用。服务的头注里把这些名字反复提了好几遍，光 `contains` 会被散文骗到
     * （扫全文的话，删掉调用照样绿）。
     */
    @Test
    fun strippingCommentsActuallyRemovesTheProseThatNamesTheseThings() {
        val raw = serviceSource()
        val code = codeOnly(raw)
        for (mark in listOf("TurnEndPathArbiter", "settleRound", "replaysHistory", "alphaCoversTurnEnd")) {
            assertTrue(
                "前提：`$mark` 该在注释里也出现过（否则「剥注释有用」这一条就没靶子了）：" +
                    "原文 ${countOf(raw, mark)} 处、剥完 ${countOf(code, mark)} 处",
                countOf(raw, mark) > countOf(code, mark),
            )
        }
    }

    // ─── ① α 这条路真的起得来 ───

    /** `watchHost` 真的在问 [TurnEndPathArbiter]，并据此起 α。 */
    @Test
    fun theWatcherAsksTheArbiterAndActuallyStartsTheDaemonPath() {
        val code = codeOnly(serviceSource())
        val body = bodyOf(code, "private suspend fun watchHost(")
        assertTrue(
            "`watchHost` 里不再建 `TurnEndPathArbiter` ⇒ 这台主机永远没有「该不该走 α」这个判断。",
            body.contains("TurnEndPathArbiter("),
        )
        assertTrue(
            "`watchHost` 不问 `shouldTryAlpha` ⇒ α 永远不会被探一次。",
            body.contains("shouldTryAlpha("),
        )
        assertTrue(
            "`watchHost` 不起 `runAlpha` ⇒ α 这条路一个消费方都没有。",
            body.contains("runAlpha("),
        )
        assertEquals(
            "全仓只许从 `runAlpha` 这一处起 α 的流（第二处就是第二套去重与降级逻辑）。",
            1,
            countOf(code, "DaemonTurnEndSource("),
        )
        // α 这一趟抛了没想到的异常时整个 watcher 不许跟着死：`coroutineScope` 会传播子失败，
        //     而 `sessions` 不变时没人重建 watcher ⇒ turn-end 通知静默死（β 那条 tail 用的是同一个护栏）。
        val startAt = body.indexOf("runAlpha(")
        val guardAt = body.lastIndexOf("runCatching", startAt)
        assertTrue(
            "`runAlpha(` 的调用点没有包在 `runCatching` 里 ⇒ α 一个没想到的异常会杀掉整个 watcher " +
                "⇒ 这台主机的完成通知两条路一起静默死。",
            guardAt in 0 until startAt && !body.substring(guardAt, startAt).contains("}"),
        )
    }

    /**
     * β 真的会被撤。
     *
     * [watchTargets] 的 `alphaCoversTurnEnd` 档是「α 供货时 β 一条 tail 都不留」的本体，
     * 而它默认 `false`：调用点不传的话一切照样编译通过、判据照样全绿，只是两条路一直并行搬字节。
     */
    @Test
    fun theDiscoveryLoopReallyTellsWatchTargetsWhetherAlphaIsServing() {
        val code = codeOnly(serviceSource())
        val body = bodyOf(code, "private suspend fun watchHost(")
        assertTrue(
            "`watchTargets(` 没带 `alphaCoversTurnEnd` ⇒ α 接上了而 β 的 tail 一条都不撤 ⇒ " +
                "两条路并行搬字节，白付流量。",
            body.contains("alphaCoversTurnEnd ="),
        )
        assertEquals("本服务只许有一处 `watchTargets(` 调用", 1, countOf(code, "watchTargets("))
    }

    /**
     * 「α 没说原因就停了」那条兜底不许删。
     *
     * 没有它：只要 arbiter 还以为 α 在供货，β 就永远不会起来 ⇒
     * 再也收不到完成通知，而且没有任何线索。这是最坏的失败模式。
     */
    @Test
    fun runAlphaAlwaysHandsTheServingRoleBackEvenIfTheStreamSaysNothing() {
        val code = codeOnly(serviceSource())
        val body = bodyOf(code, "private suspend fun runAlpha(")
        assertTrue(
            "`runAlpha` 里没有 `finally` ⇒ 流从别的路走到尽头时没人把供货权交还 β。",
            body.contains("finally"),
        )
        val finallyPart = body.substring(body.indexOf("finally"))
        assertTrue(
            "`finally` 里不再问 `alphaEngaged` / 不再调 `onUnavailable` ⇒ " +
                "α 静默停掉之后 β 永远不起来（静默不通知）。实得：$finallyPart",
            finallyPart.contains("alphaEngaged") && finallyPart.contains("onUnavailable("),
        )
    }

    /**
     * Codex 那道闸不许删。
     *
     * 后端的流式 watcher 只跟记录树那一家（Claude），α 不会为 Codex 发 `turn_end`。
     * 闸删了 ⇒ Codex 主机会「成功」接上 α、β 被撤 ⇒ 那些主机再也收不到完成通知。
     */
    @Test
    fun aHostWhoseFamilyTheDaemonDoesNotCoverIsPinnedToTheOldPath() {
        val code = codeOnly(serviceSource())
        val body = bodyOf(code, "private suspend fun watchHost(")
        assertTrue(
            "那道闸没了 ⇒ 非 Claude 主机会接上一条永远不发 turn_end 的 α、而 β 被撤。",
            body.contains("AlphaUnavailable.NotClaudeHost"),
        )
        assertTrue(
            "闸的判据必须问档案（`AgentProfile.daemonStreamReportsTurnEnd`），" +
                "不许在服务里写 agent 种类字面" +
                "（`AgentProfileSingleAddressTest` 会单独红一次）。",
            body.contains("daemonStreamReportsTurnEnd"),
        )
        assertTrue(
            "闸要在起 α 之前合上，否则第一轮照样会去探。",
            body.indexOf("AlphaUnavailable.NotClaudeHost") < body.indexOf("runAlpha("),
        )
    }

    // ─── ② 唯一的发通知出口 ───

    /**
     * 两条路都从 [settleRound] 出去，而通知只在那一处发。
     *
     * 新开一条发通知的路 ⇒ 绕过共用的去重账本 `baselineByPath` ⇒ 重复通知（或两条路各发一次、
     * 投到不同的槽）。断「`notifyAgentDone(` 恰好一处调用」就是这条的防线。
     */
    @Test
    fun thereIsExactlyOneNotificationExitAndBothPathsGoThroughIt() {
        val code = codeOnly(serviceSource())
        assertEquals(
            "`notifyAgentDone(` 的调用点必须恰好一处（声明 1 + 调用 1 = 2 次出现）。" +
                "多出来的那一处就绕过了共用去重账本 ⇒ 重复通知。",
            2,
            countOf(code, "notifyAgentDone("),
        )
        val exit = bodyOf(code, "private fun settleRound(")
        assertTrue("唯一出口里不再问 `TurnEndNotifyDecision` ⇒ 两条路的首轮语义从此各走各的。", exit.contains("TurnEndNotifyDecision.decide("))
        assertTrue("前台门不许丢（前台在看就不打扰）", exit.contains("AppForeground.isForeground"))
        assertTrue("账本不许不写（不写就永远不去重 ⇒ 每轮都重复通知）", exit.contains("baselineByPath["))
        assertEquals("两条路各调一次，恰好两处（声明 1 + 调用 2 = 3 次出现）", 3, countOf(code, "settleRound("))
    }

    /**
     * 两条路的 `replaysHistory` 一个 true 一个 false，而且各在各的位置。
     *
     * 这是两条路唯一一处语义差（`--tail-only` ⇒ 后端只 `prime_file_cursor`、零行帧）。
     * 写反的后果各有一条，两头都要红：
     * - α 写成 `true` ⇒ α 的首轮被当成历史吞掉 ⇒ 少一条通知；
     * - β 写成 `false` ⇒ β 挂上去就把历史里最后一轮当成新的 ⇒ 多一条假通知。
     */
    @Test
    fun theTwoPathsDisagreeAboutHistoryReplayInExactlyTheRightWay() {
        val code = codeOnly(serviceSource())
        val alpha = bodyOf(code, "private suspend fun runAlpha(")
        assertTrue(
            "α 这条路的 `replaysHistory` 不是 `false` ⇒ 它的首轮会被当成历史吞掉 ⇒ 少一条通知。" +
                "（`--tail-only` 时后端只 prime 游标、零行帧，α 收到的第一条 turn_end 就是真的新一轮。）",
            alpha.contains("replaysHistory = false"),
        )
        val beta = bodyOf(code, "private suspend fun tailAndNotify(")
        assertTrue(
            "β 这条路的 `replaysHistory` 不是 `true` ⇒ 它挂上去会把历史里最后一轮当成新的 ⇒ 多一条假通知。",
            beta.contains("replaysHistory = true"),
        )
        // 不数 `replaysHistory = ` 的总数：唯一出口 `settleRound` 里还有一处把它转交给
        //   `TurnEndNotifyDecision.decide(replaysHistory = replaysHistory)`（总数 3 不是 2）。
        //   所以按值各数一次：这也更准，多出一条路会让其中一个数变成 2。
        assertEquals("全服务只许有一处 α 语义（`false`）", 1, countOf(code, "replaysHistory = false"))
        assertEquals("全服务只许有一处 β 语义（`true`）", 1, countOf(code, "replaysHistory = true"))
    }

    // ─── ③ 两条路的静默窗口不许各自漂 ───

    /**
     * β 的 `debounce` 直接用 α 那个常量：不许有第二个常量可以漂。
     *
     * 「切换前后感知到的延迟一样」不靠判据守、靠只有一个常量。本条守的是「别人又加回来一个」。
     */
    @Test
    fun bothPathsShareTheOneSettleWindowConstant() {
        val code = codeOnly(serviceSource())
        assertTrue(
            "β 的 `debounce(...)` 不再用 `TurnEndDebouncer.SETTLE_MS` ⇒ " +
                "两条路的静默窗口又能各自漂了（用户切换前后感知到的延迟会不一样）。",
            code.contains("debounce(TurnEndDebouncer.SETTLE_MS)"),
        )
        assertEquals(
            "服务里不许再出现一个自己的去抖常量（那就是第二份真相）。",
            0,
            countOf(code, "DEBOUNCE_MS"),
        )
    }
}
