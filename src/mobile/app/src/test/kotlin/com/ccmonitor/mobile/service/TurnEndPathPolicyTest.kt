package com.ccmonitor.mobile.service

import com.ccmonitor.mobile.core.claude.transport.AlphaUnavailable
import com.ccmonitor.mobile.core.claude.transport.TurnEndDebouncer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [TurnEndPathArbiter]（走哪条路 · 回落后多久再试）＋ [TurnEndNotifyDecision]（发不发通知 · 新基线是什么）。
 * α = 后端流，β = 自己 tail jsonl。
 *
 * 要判的东西住在 `SshKeepAliveService.watchHost` 的循环里，而 `app` 没引 Robolectric、门禁不跑 `androidTest`，
 * 循环本身在 JVM 判据里碰不到。所以判断搬出来真测（本文件），「服务到底有没有问它」由
 * `TurnEndAlphaWiringTest` 从结构上钉。
 *
 * | 这一格 | 本文件 |
 * |---|---|
 * | 五档失败各自回落 β | 测到 |
 * | 不许静默钉死在 β（会再试） | 测到 |
 * | 交接宽限（β→α 不留空窗） | 测到 |
 * | 两条路首轮语义对齐 | 测到 |
 * | `DaemonTurnEndSource` 真的会发那些事件 | 看不见（`DaemonTurnEndSourceTest` 管那一头） |
 * | α 在真机上通不通 | 看不见，要真机 |
 */
class TurnEndPathPolicyTest {
    // ===== TurnEndPathArbiter：走哪条路 =====

    /** 一上来就走 β：α 还没探过，而「探测那十几秒里通知照发」是不漏的根。 */
    @Test
    fun beforeAnythingIsKnownTheOldPathIsTheOneServing() {
        val a = TurnEndPathArbiter()
        assertEquals(TurnEndPathArbiter.Path.Beta, a.path())
        assertTrue("该去探一次 α", a.shouldTryAlpha(0))
        assertFalse("探测期间 β 不许被撤 —— 那十几秒的空窗就是漏通知", a.alphaCoversTurnEnd(0))
    }

    /**
     * 接上 ≠ 可以撤 β。还要熬过 [TurnEndPathArbiter.HANDOVER_GRACE_MS]。
     *
     * 没有这段宽限的话 β→α 交接处有一个真的漏通知窗：一轮正好在交接那一刻结束时，
     * β 的 tail 被撤（撤 tail 不推进 baseline ⇒ 不发），而 α 是在 `session_added` 那一刻
     * `prime_file_cursor` 推到 EOF 的 ⇒ 那一行在 prime 之前就写进文件了 ⇒ α 也看不见
     * ⇒ 两条路都不发。
     * 负控形状：把 `alphaCoversTurnEnd` 改成 `engagedAtMs != null` ⇒ 本条当场红。
     */
    @Test
    fun theOldPathIsKeptAliveForAGraceWindowAfterAlphaEngages() {
        val a = TurnEndPathArbiter()
        a.onEngaged(1_000)
        assertEquals(TurnEndPathArbiter.Path.Alpha, a.path())
        assertFalse("刚接上，β 还不许撤", a.alphaCoversTurnEnd(1_000))
        assertFalse("宽限差一毫秒也不算", a.alphaCoversTurnEnd(1_000 + TurnEndPathArbiter.HANDOVER_GRACE_MS - 1))
        assertTrue("熬满了才撤", a.alphaCoversTurnEnd(1_000 + TurnEndPathArbiter.HANDOVER_GRACE_MS))
        assertFalse("α 在供货就别再探了", a.shouldTryAlpha(99_999))
    }

    /** 宽限要盖得住「交接那一刻正在结算的那一轮」⇒ 必须 ≥ 一个静默窗。 */
    @Test
    fun theGraceWindowCoversAtLeastOneSettleWindow() {
        assertTrue(
            "宽限 ${TurnEndPathArbiter.HANDOVER_GRACE_MS}ms 必须盖得住静默窗 ${TurnEndDebouncer.SETTLE_MS}ms",
            TurnEndPathArbiter.HANDOVER_GRACE_MS >= TurnEndDebouncer.SETTLE_MS,
        )
    }

    /**
     * α 一死，β 当场复职。这是「别让它变成『连不上就静默不通知』」的正面判据。
     */
    @Test
    fun theMomentAlphaDiesTheOldPathIsBackInCharge() {
        val a = TurnEndPathArbiter()
        a.onEngaged(0)
        assertTrue(a.alphaCoversTurnEnd(TurnEndPathArbiter.HANDOVER_GRACE_MS))
        a.onUnavailable(AlphaUnavailable.StreamDied, 100_000)
        assertEquals(TurnEndPathArbiter.Path.Beta, a.path())
        assertFalse("β 必须立刻复职（下一轮发现就重建 tail）", a.alphaCoversTurnEnd(100_000))
        assertEquals(AlphaUnavailable.StreamDied, a.lastUnavailable)
    }

    /**
     * 回落之后一定会再试。不试的后果是「一次 SSH 抖动把这台机器永久钉在 β 上」，
     * 而 β 是那条贵的路。
     */
    @Test
    fun fallingBackIsNeverPermanentForTheFourReasonsThatCanGetBetter() {
        for (why in AlphaUnavailable.entries.filter { it != AlphaUnavailable.NotClaudeHost }) {
            val a = TurnEndPathArbiter()
            a.onUnavailable(why, 0)
            assertFalse("$why：退避没到点之前不许探", a.shouldTryAlpha(TurnEndPathArbiter.RETRY_BASE_MS - 1))
            assertTrue("$why：到点必须再试一次（不许静默钉死在 β）", a.shouldTryAlpha(TurnEndPathArbiter.RETRY_BASE_MS))
        }
    }

    /**
     * 唯一不再试的那一档：这台主机不是 Claude。
     *
     * 后端的流式 watcher 只跟记录树那一家（Claude），α 不会为 Codex 发 `turn_end`，而这不会自己变好。
     * 每 10 分钟去烧两次 exec 问一个已知的答案是纯浪费。
     */
    @Test
    fun aHostThatIsNotClaudeIsNeverProbedAgain() {
        val a = TurnEndPathArbiter()
        a.onUnavailable(AlphaUnavailable.NotClaudeHost, 0)
        assertEquals(TurnEndPathArbiter.Path.Beta, a.path())
        assertFalse("一分钟后不探", a.shouldTryAlpha(60_000))
        assertFalse("一天后也不探", a.shouldTryAlpha(86_400_000))
        assertEquals(Long.MAX_VALUE, a.nextTryAtMs())
    }

    /** 退避是翻倍的，而且封得住顶（装好 daemon 之后最多等 [TurnEndPathArbiter.RETRY_CAP_MS] 就自动切过去）。 */
    @Test
    fun theBackoffDoublesAndIsCapped() {
        val a = TurnEndPathArbiter()
        val base = TurnEndPathArbiter.RETRY_BASE_MS
        a.onUnavailable(AlphaUnavailable.NotSpeaking, 0)
        assertEquals(base, a.backoffMs())
        a.onUnavailable(AlphaUnavailable.NotSpeaking, 0)
        assertEquals(base * 2, a.backoffMs())
        a.onUnavailable(AlphaUnavailable.NotSpeaking, 0)
        assertEquals(base * 4, a.backoffMs())
        repeat(30) { a.onUnavailable(AlphaUnavailable.NotSpeaking, 0) }
        assertEquals("封顶，而且不许因为 `shl` 溢出翻成负数", TurnEndPathArbiter.RETRY_CAP_MS, a.backoffMs())
        assertTrue("封顶也还是会再试", a.shouldTryAlpha(TurnEndPathArbiter.RETRY_CAP_MS))
    }

    /**
     * 接上过但没稳住 ⇒ 算一次失败、继续退避。
     *
     * 把阶梯无条件复位的后果：α 要是每次接上两秒就死（抖动 / 对端起不来），
     * 我们会永远每 30 秒烧两次 exec ＋ 一次 daemon 冷启。
     * 负控形状：在 `onEngaged` 里加一行 `failures = 0` ⇒ 本条当场红。
     */
    @Test
    fun engagingBrieflyBeforeDyingDoesNotResetTheBackoffLadder() {
        val a = TurnEndPathArbiter()
        a.onUnavailable(AlphaUnavailable.NotSpeaking, 0)
        a.onUnavailable(AlphaUnavailable.NotSpeaking, 0)
        assertEquals(TurnEndPathArbiter.RETRY_BASE_MS * 2, a.backoffMs())
        a.onEngaged(100_000)
        a.onUnavailable(AlphaUnavailable.StreamDied, 102_000) // 只活了 2 秒
        assertEquals("闪断不算成功 ⇒ 阶梯继续涨", TurnEndPathArbiter.RETRY_BASE_MS * 4, a.backoffMs())
    }

    /** 反过来：稳住够久再死，就算一次成功 ⇒ 阶梯归零（下次抖动从第一级重新等，不用等 10 分钟）。 */
    @Test
    fun stayingEngagedLongEnoughResetsTheBackoffLadder() {
        val a = TurnEndPathArbiter()
        repeat(8) { a.onUnavailable(AlphaUnavailable.NotSpeaking, 0) }
        assertEquals(TurnEndPathArbiter.RETRY_CAP_MS, a.backoffMs())
        a.onEngaged(100_000)
        a.onUnavailable(AlphaUnavailable.StreamDied, 100_000 + TurnEndPathArbiter.RETRY_BASE_MS)
        assertEquals("稳过一段 ⇒ 回到第一级", TurnEndPathArbiter.RETRY_BASE_MS, a.backoffMs())
    }

    // ===== TurnEndNotifyDecision：两条路的首轮语义 =====

    /**
     * β 首轮吞历史。
     *
     * β 是 `tail -c +N`，首次 N=0 ⇒ 整份历史都流过来 ⇒ 首轮只记基线、不通知。
     */
    @Test
    fun theOldPathSwallowsTheVeryFirstRoundBecauseItReplaysHistory() {
        val o =
            TurnEndNotifyDecision.decide(
                previousBaseline = null,
                baselineKnown = false,
                settledUuid = "u-history",
                replaysHistory = true,
            )
        assertFalse("首轮不许通知（那是历史）", o.notify)
        assertTrue("但要记下基线", o.writeBaseline)
        assertEquals("u-history", o.newBaseline)
    }

    /**
     * α 首轮照发：这是两条路唯一一处语义差，而它有一句可核的理由：
     * 带 `--tail-only` 时后端只 `prime_file_cursor`（「推进 cursor/seq 到当前完整行数 L，零行帧」），
     * α 收到的第一条 `turn_end` 就是一条真的新轮次。
     *
     * 负控形状：把服务里 α 那个调用点的 `replaysHistory` 改成 `true` ⇒ 本条还是绿（它测的是纯函数），
     * 但 `TurnEndAlphaWiringTest` 会红：两条判据合起来才钉得住，单独任一条都不够。
     */
    @Test
    fun theDaemonPathDoesNotSwallowItsFirstRoundBecauseTailOnlyReplaysNothing() {
        val o =
            TurnEndNotifyDecision.decide(
                previousBaseline = null,
                baselineKnown = false,
                settledUuid = "u1",
                replaysHistory = false,
            )
        assertTrue("α 的首轮是真的新一轮 ⇒ 必须通知", o.notify)
        assertEquals("u1", o.newBaseline)
    }

    /** 同一轮再来一遍（重连重读）⇒ 不通知、账本也不动。 */
    @Test
    fun theSameRoundArrivingAgainIsNotNotifiedTwiceOnEitherPath() {
        for (replays in listOf(true, false)) {
            val o =
                TurnEndNotifyDecision.decide(
                    previousBaseline = "u1",
                    baselineKnown = true,
                    settledUuid = "u1",
                    replaysHistory = replays,
                )
            assertFalse("replaysHistory=$replays：同一个 uuid 不许再通知", o.notify)
            assertFalse("账本也不用动（写回去是同一个值）", o.writeBaseline)
        }
    }

    /**
     * α→β 回落那一下不漏也不重：这是「两条路首轮语义要对齐」要的那个结果。
     *
     * 场景：α 死前把未结算的那一轮倒出来、顺手把基线写进共用账本。β 第一条 tail 于是发现
     * 「这个槽已经有基线了」⇒ 不走吞历史那支。
     */
    @Test
    fun afterFallingBackTheOldPathNeitherSwallowsNorRepeatsTheHandoverRound() {
        // α 倒出来的那一轮 ⇒ 账本里是 u-alpha
        val seeded = "u-alpha"
        // ① β 第一条 tail 读到的就是同一轮 ⇒ 不许再发一条
        val same =
            TurnEndNotifyDecision.decide(
                previousBaseline = seeded,
                baselineKnown = true,
                settledUuid = seeded,
                replaysHistory = true,
            )
        assertFalse("已经发过了 ⇒ 不重", same.notify)
        // ② 空窗里又跑完了一轮 ⇒ β 必须补发
        val newer =
            TurnEndNotifyDecision.decide(
                previousBaseline = seeded,
                baselineKnown = true,
                settledUuid = "u-after",
                replaysHistory = true,
            )
        assertTrue("空窗里那一轮 β 必须补发 ⇒ 不漏", newer.notify)
        assertEquals("u-after", newer.newBaseline)
    }

    /**
     * `baselineKnown` 必须与 `previousBaseline == null` 分开。
     *
     * `baselineByPath` 里 `null` 是一个合法的值（「首次见此 path、而那一刻一个 uuid 都没读到」）。
     * 拿 `previousBaseline == null` 当「没见过」会让那个槽每一轮都重新吞一次历史 ⇒ 永远不通知。
     * 负控形状：把 `decide` 里 `!baselineKnown` 换成 `previousBaseline == null` ⇒ 本条当场红。
     */
    @Test
    fun aKnownSlotWhoseBaselineIsNullMustNotSwallowHistoryAgain() {
        val o =
            TurnEndNotifyDecision.decide(
                previousBaseline = null,
                baselineKnown = true, // 见过这个槽，只是那时没读出 uuid
                settledUuid = "u1",
                replaysHistory = true,
            )
        assertTrue("见过的槽不许再吞一次历史 —— 否则它永远不通知", o.notify)
        assertEquals("u1", o.newBaseline)
    }

    /** 读不出 uuid（β 的历史回放里会有这种结算）⇒ 什么都不做。 */
    @Test
    fun aSettlementWithNoUuidChangesNothing() {
        val o =
            TurnEndNotifyDecision.decide(
                previousBaseline = "u1",
                baselineKnown = true,
                settledUuid = null,
                replaysHistory = true,
            )
        assertFalse(o.notify)
        assertFalse(o.writeBaseline)
        assertEquals("u1", o.newBaseline)
    }
}
