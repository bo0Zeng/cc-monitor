package com.ccmonitor.mobile.core.claude.transport

import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 跨通路信号汇：两条到达路径、新鲜度与丢帧、上屏文案、配额两套键名。
 *
 * 几条阴性对照，各自证明另一条判据抓不到某个 bug：
 * - [onlyListeningToStatusFramesStillPassesTheHappyPathSequence]：「先 busy 后 waiting」那条 happy-path 判据
 *   抓不到「只接 `session_status`」。
 * - [judgingOnlyByStatusPassesEverySequenceThatCarriesStatus]：带 `status` 的那几条等待态判据
 *   抓不到「帧上只有 `activity`」。
 * - [payloadsWithoutUtilizationProduceNoPercentAtAll]：「解得出 status」那类断言抓不到「百分比被编出来了」。
 * - [aWaitingThatNeverGoesStaleWouldKeepBlockingForever]：「有 `observedAtMs` 这个字段」抓不到「从不降级」。
 */
class SessionSignalsTest {
    private val sid = "8cfb1b93-285d-42ca-bcc1-24f02419f930"

    private fun bus(freshnessMs: Long = 60_000L) = SessionSignals(freshnessMs)

    // ---- 两条到达路径都要接 ------------------------------------------

    /**
     * 只有 `session_added`、之后一条 `session_status` 都没有。
     *
     * 这是手机上最常见的形状：合上手机、过一小时回来、app 重连。后端首次是同步全扫，
     * 已经在等的会话只走 `SessionAdded` 那一支。只接 `session_status` 的实现在这条序列下一条断言都不会红，
     * 表现是「它在等，而上行照样发得出去」。
     */
    @Test
    fun aSessionThatWasAlreadyWaitingWhenWeConnectedStillBlocks() {
        val b = bus()
        DaemonSignalBridge.onFrame(
            b,
            JsonlFrame.SessionAdded(sessionId = sid, path = null, status = "waiting", waitingFor = "sandbox request"),
            nowMs = 1_000,
        )
        val gate = b.waitingGate(sid, nowMs = 1_100)
        assertTrue("session_added 上宣告的初始 waiting 必须拦得住", gate.blocks)
        assertEquals(SignalSource.DAEMON_SESSION_ADDED, gate.signal?.source)
    }

    /** 先 `busy`、后 `session_status{waiting}`：happy path。 */
    @Test
    fun aSessionThatTurnsWaitingLaterAlsoBlocks() {
        val b = bus()
        DaemonSignalBridge.onFrame(
            b,
            JsonlFrame.SessionAdded(sessionId = sid, path = null, status = "busy"),
            nowMs = 1_000,
        )
        assertFalse("前提：busy 不该拦", b.waitingGate(sid, 1_000).blocks)
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, "waiting", "permission prompt", null), 2_000)
        val gate = b.waitingGate(sid, nowMs = 2_100)
        assertTrue(gate.blocks)
        assertEquals(SignalSource.DAEMON_SESSION_STATUS, gate.signal?.source)
    }

    /**
     * 阴性对照：造一个「只接 `session_status`」的桥，证明 happy path 那条判据照样全绿，只有「连上之前就在等」红。
     *
     * 没有这一条的话，两条判据一起绿会被读成「两条路径都守住了」。
     */
    @Test
    fun onlyListeningToStatusFramesStillPassesTheHappyPathSequence() {
        fun crippled(
            b: SessionSignals,
            fr: JsonlFrame,
            now: Long,
        ) {
            // 这就是那个 bug：只认 session_status，不读 session_added 上的初始 status
            if (fr is JsonlFrame.SessionStatus) {
                b.publishWaiting(fr.sessionId, fr.status, fr.waitingFor, SignalSource.DAEMON_SESSION_STATUS, now)
            }
        }

        // happy path 在 bug 存在时照样绿
        val happy = bus()
        crippled(happy, JsonlFrame.SessionAdded(sessionId = sid, path = null, status = "busy"), 1_000)
        crippled(happy, JsonlFrame.SessionStatus(sid, "waiting", "permission prompt", null), 2_000)
        assertTrue(
            "要点：bug 存在时 happy-path 判据仍然绿 ⇒ 它什么都没守住",
            happy.waitingGate(sid, 2_100).blocks,
        )

        // 「连上之前就在等」在 bug 存在时红
        val cold = bus()
        crippled(cold, JsonlFrame.SessionAdded(sessionId = sid, path = null, status = "waiting", waitingFor = "sandbox request"), 1_000)
        assertFalse(
            "只接 session_status 的实现看不见「连上之前就在等」",
            cold.waitingGate(sid, 1_100).blocks,
        )
    }

    // ---- 帧上只有 `activity`、没有 `status` ------------------

    /**
     * `session_status` 上只有 `activity`，没有 `status`。
     *
     * 后端 `SessionStatus` 的字段是 `sid` · `activity` · `waiting_for` · `liveness_confidence`；
     * `activity` 是闭集三值，线上是 `working` / `needs_you` / `idle`。
     *
     * 只按 `status` 判的话，`status` 恒 `null` ⇒ 每一帧都走进清空分支 ⇒ 等待态每帧被扔掉一次，
     * 而 `waiting_for` 还好好地在线上。屏上永远不说「在等」、`waitingGate` 永远不拦上行，不报错、不崩。
     */
    @Test
    fun aStatusFrameWithOnlyActivityStillBlocks() {
        val b = bus()
        DaemonSignalBridge.onFrame(
            b,
            JsonlFrame.SessionStatus(sid, status = null, waitingFor = "permission prompt", activity = "needs_you"),
            nowMs = 1_000,
        )
        val gate = b.waitingGate(sid, nowMs = 1_100)
        assertTrue("只说 activity，照样必须拦得住", gate.blocks)
        assertEquals("等什么那一格还在线上，不许连它一起扔掉", "permission prompt", gate.signal?.waitingFor)
        assertEquals(SignalSource.DAEMON_SESSION_STATUS, gate.signal?.source)
    }

    /**
     * 同一个 bug 在另一条到达路径上：`session_added` 宣告时就只有 `activity`。
     *
     * 两条合起来才覆盖两条到达路径。只补 `session_status` 那一支的话，「合上手机、过一小时回来、app 重连」
     * 这条最常见的形状仍然失效。
     */
    @Test
    fun anAddedSessionWithOnlyActivityAlreadyNeedingYouStillBlocks() {
        val b = bus()
        DaemonSignalBridge.onFrame(
            b,
            JsonlFrame.SessionAdded(
                sessionId = sid,
                path = null,
                waitingFor = "sandbox request",
                activity = "needs_you",
            ),
            nowMs = 1_000,
        )
        val gate = b.waitingGate(sid, nowMs = 1_100)
        assertTrue("宣告时就只有 activity ⇒ 照样要拦", gate.blocks)
        assertEquals(SignalSource.DAEMON_SESSION_ADDED, gate.signal?.source)
    }

    /**
     * 不能「一投上去就黏住」：activity 说它在跑了，等待态必须消失。
     *
     * 宁可少拦一次，也不拿一条可能过期的 `waiting` 去挡上行 ⇒ 两格都不说「在等」时清空。
     */
    @Test
    fun leavingTheWaitingStateByActivityClearsTheSignal() {
        val b = bus()
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, activity = "needs_you"), 1_000)
        assertTrue("前提", b.waitingGate(sid, 1_000).blocks)
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, activity = "working"), 2_000)
        assertNull("它在跑了 ⇒ 等待态必须消失", b.waitingGate(sid, 2_000).signal)
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, activity = "needs_you"), 3_000)
        assertTrue("前提：又回到等待", b.waitingGate(sid, 3_000).blocks)
        // 两格都缺席（后端自己说不清）⇒ 仍然清空：保守方向与「宁可少拦」一致
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid), 4_000)
        assertNull("两格都不说「在等」⇒ 清空（含后端说不清那一形）", b.waitingGate(sid, 4_000).signal)
    }

    /**
     * 阴性对照：造一个「只按 `status` 判」的桥，证明带 `status` 的那几条等待态判据全部照绿，
     * 只有只带 `activity` 的那两条红。
     *
     * 即 [aSessionThatWasAlreadyWaitingWhenWeConnectedStillBlocks] ＋ [aSessionThatTurnsWaitingLaterAlsoBlocks]
     * ＋ [leavingTheWaitingStateClearsTheSignal] ＋ [waitingWithoutASubtypeStillBlocks] 一条都抓不到「帧上没有 `status`」。
     */
    @Test
    fun judgingOnlyByStatusPassesEverySequenceThatCarriesStatus() {
        // 这就是那个 bug：判定只看 status，不看 activity
        fun crippled(
            b: SessionSignals,
            fr: JsonlFrame,
            now: Long,
        ) {
            when (fr) {
                is JsonlFrame.SessionAdded ->
                    b.publishWaiting(fr.sessionId, fr.status, fr.waitingFor, SignalSource.DAEMON_SESSION_ADDED, now)
                is JsonlFrame.SessionStatus ->
                    b.publishWaiting(fr.sessionId, fr.status, fr.waitingFor, SignalSource.DAEMON_SESSION_STATUS, now)
                else -> Unit
            }
        }

        // ── 带 status 的三条序列：bug 存在时全绿 ⇒ 它们什么都没守住 ──
        val oldAdded = bus()
        crippled(oldAdded, JsonlFrame.SessionAdded(sessionId = sid, path = null, status = "waiting"), 1_000)
        assertTrue("要点：bug 存在时「连上之前就在等」（带 status）仍然绿", oldAdded.waitingGate(sid, 1_100).blocks)

        val oldStatus = bus()
        crippled(oldStatus, JsonlFrame.SessionStatus(sid, "waiting", "permission prompt", null), 1_000)
        assertTrue("要点：bug 存在时 happy path（带 status）仍然绿", oldStatus.waitingGate(sid, 1_100).blocks)

        crippled(oldStatus, JsonlFrame.SessionStatus(sid, "busy", null, null), 2_000)
        assertNull("要点：bug 存在时「离开等待就清空」仍然绿", oldStatus.waitingGate(sid, 2_000).signal)

        // ── 只带 activity 的两条序列：bug 存在时红 ──
        val newStatus = bus()
        crippled(newStatus, JsonlFrame.SessionStatus(sid, status = null, waitingFor = "permission prompt", activity = "needs_you"), 1_000)
        assertFalse(
            "只按 status 判 ⇒ 只带 activity 时永远看不见等待态",
            newStatus.waitingGate(sid, 1_100).blocks,
        )

        val newAdded = bus()
        crippled(newAdded, JsonlFrame.SessionAdded(sessionId = sid, path = null, waitingFor = "sandbox request", activity = "needs_you"), 1_000)
        assertFalse("同上，另一条到达路径", newAdded.waitingGate(sid, 1_100).blocks)
    }

    /**
     * 跨仓双写点：`needs_you` 这个字面量漂了不会报错，只会让等待态静默消失。
     *
     * 后端 `SessionActivity` 序列化为 snake_case：`"working" | "needs_you" | "idle"`。
     */
    @Test
    fun theActivityWireLiteralIsPinnedBecauseDriftWouldBeSilent() {
        assertEquals("needs_you", SessionSignals.ACTIVITY_NEEDS_YOU)
        // 别的两态不许被当成「在等」
        val b = bus()
        listOf("working", "idle", "shell", "NeedsYou", "needs-you").forEach { a ->
            DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, activity = a), 1_000)
            assertNull("「$a」不是「在等人」，不许拦", b.waitingGate(sid, 1_000).signal)
        }
    }

    /**
     * 快照口也要带 `activity`：总览面 VM 走的正是 `states()`。
     * 漏了它 ⇒「快照 → 总线」这条路在只带 activity 时恒哑，而 [aSnapshotDrivesBothDirections] 照样绿。
     */
    @Test
    fun theSnapshotPathAlsoCarriesTheActivity() {
        val b = bus()
        val state =
            DaemonSessionSource.State(
                sessions = mapOf(sid to DaemonSessionSource.Session(sid, waitingFor = "input needed", activity = "needs_you")),
            )
        DaemonSignalBridge.onSnapshot(b, state, 1_000)
        assertTrue("快照里只有 activity 时也要拦得住", b.waitingGate(sid, 1_000).blocks)
        assertEquals(SignalSource.DAEMON_SNAPSHOT, b.waitingGate(sid, 1_000).signal?.source)
    }

    /**
     * `status` 不是 `waiting` 时清空，不是「什么都不做」。
     * 不清的话「它不再等了」会永远挂在屏上（同 `DaemonSessionSource` 里 `waitingFor` 缺席即清空那条）。
     */
    @Test
    fun leavingTheWaitingStateClearsTheSignal() {
        val b = bus()
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, "waiting", "input needed", null), 1_000)
        assertTrue("前提", b.waitingGate(sid, 1_000).blocks)
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, "busy", null, null), 2_000)
        assertNull("不再等了 ⇒ 信号必须消失", b.waitingGate(sid, 2_000).signal)
    }

    /** 判定认的是 `status`，不是 `waitingFor`：有的 Claude Code 不写后者。 */
    @Test
    fun waitingWithoutASubtypeStillBlocks() {
        val b = bus()
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, "waiting", null, null), 1_000)
        val gate = b.waitingGate(sid, 1_000)
        assertTrue("waitingFor 缺席不影响拦不拦", gate.blocks)
        assertEquals(
            "缺席那一格必须有它自己的话，不许回退成通用句",
            WaitingCopy.WAITING_BUT_UNSAID,
            WaitingCopy.headlineFor(gate.signal?.waitingFor),
        )
    }

    // ---- 新鲜度 / 丢帧 -----------------------------------------------

    /**
     * 过期的 waiting 自动降级成「提示但不拦」。
     *
     * 判据跑在行为上（`blocks` 真的翻了），不是「有这个字段」。
     */
    @Test
    fun aWaitingReadingThatWentStaleStopsBlocking() {
        val b = bus(freshnessMs = 5_000)
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, "waiting", "dialog open", null), 1_000)
        assertTrue("前提：刚到的读数要拦", b.waitingGate(sid, 2_000).blocks)
        val stale = b.waitingGate(sid, nowMs = 1_000 + 5_001)
        assertFalse("超过阈值 ⇒ 不再拦", stale.blocks)
        assertNotNull("但读数还在 —— 「不拦」也要说出来，不能变成「什么都没发生」", stale.signal)
        assertTrue(
            "不拦的理由要读得出来",
            stale.degradedWhy?.startsWith(SessionSignals.STALE_PREFIX) == true,
        )
    }

    /**
     * 阴性对照：一个「有 `observedAtMs` 字段但从不降级」的实现（= 阈值无限大）在同一段素材上永远拦着。
     *
     * 证明「断言那个字段存在」抓不到「从不降级」：字段在、时刻也在，而行为一格没变。
     */
    @Test
    fun aWaitingThatNeverGoesStaleWouldKeepBlockingForever() {
        val never = bus(freshnessMs = Long.MAX_VALUE)
        DaemonSignalBridge.onFrame(never, JsonlFrame.SessionStatus(sid, "waiting", "dialog open", null), 1_000)
        assertTrue(
            "要点：没有降级时，一个月前的读数照样在拦",
            never.waitingGate(sid, nowMs = 1_000 + 30L * 24 * 3600 * 1000).blocks,
        )
    }

    /**
     * 丢过 `session_status` ⇒ 等待态立刻作废。
     *
     * 后端把 `session_status` 判为 unrecoverable：丢了永远补不回来 ⇒ 手上那个 `waiting` 可能是过期的最后一帧。
     *
     * 这里作废的是全部 sid（见 [DaemonSignalBridge.OVERFLOW_LOST_UNPARSED]）。宁可少拦，也不拿可能过期的读数挡上行。
     */
    @Test
    fun anOverflowInvalidatesTheWaitingStateBecauseStatusFramesNeverComeBack() {
        val b = bus()
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, "waiting", "worker request", null), 1_000)
        assertTrue("前提", b.waitingGate(sid, 1_000).blocks)
        DaemonSignalBridge.onFrame(b, JsonlFrame.Overflow(dropped = 7), 1_500)
        val gate = b.waitingGate(sid, 1_600)
        assertFalse("丢过状态帧 ⇒ 不许继续拦", gate.blocks)
        assertEquals(SessionSignals.LOST_STATUS_FRAME, gate.degradedWhy)
    }

    /** 会话没了 ⇒ 信号一起忘掉（进程级的汇，不忘就是泄漏）。 */
    @Test
    fun removingASessionForgetsItsSignals() {
        val b = bus()
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionStatus(sid, "waiting", null, null), 1_000)
        DaemonSignalBridge.onFrame(b, JsonlFrame.SessionRemoved(sid, cause = null), 1_100)
        assertTrue(b.signals.value.isEmpty())
    }

    /** 快照口：不在快照里的 sid，等待态作废（快照是全量的）。 */
    @Test
    fun aSnapshotDrivesBothDirections() {
        val b = bus()
        val state =
            DaemonSessionSource.State(
                sessions = mapOf(sid to DaemonSessionSource.Session(sid, status = "waiting", waitingFor = "input needed")),
            )
        DaemonSignalBridge.onSnapshot(b, state, 1_000)
        assertTrue(b.waitingGate(sid, 1_000).blocks)
        DaemonSignalBridge.onSnapshot(b, DaemonSessionSource.State(), 2_000)
        val gone = b.waitingGate(sid, 2_000)
        assertFalse(gone.blocks)
        assertEquals(DaemonSignalBridge.GONE_FROM_SNAPSHOT, gone.degradedWhy)
    }

    // ---- 文案：机器码一个都不许上屏 --------------------------------------------

    /**
     * 六个已知机器码一个都不许出现在屏上的那句话里。
     *
     * 判据是「那句人话里没有 ASCII 字母」，比「不等于原文」强得多：`"等：permission prompt"` 也不等于原文。
     */
    @Test
    fun noMachineCodeEverReachesTheScreen() {
        WaitingCopy.knownCodes.forEach { code ->
            val human = WaitingCopy.headlineFor(code)
            assertFalse("「$code」翻出来的这句话里还有英文：$human", human.any { it in 'a'..'z' || it in 'A'..'Z' })
        }
        assertEquals("六值，不是五值（含 `goal proposal`）", 6, WaitingCopy.knownCodes.size)
    }

    /** 已知值之外的新值原样带回，不翻译：读不懂后端的每一种等待。 */
    @Test
    fun anUnknownWaitingReasonIsCarriedBackVerbatim() {
        val human = WaitingCopy.headlineFor("brand new thing")
        assertTrue(human.contains("brand new thing"))
        assertTrue(human.startsWith(WaitingCopy.UNKNOWN_PREFIX))
    }

    // ---- 配额，两套键名 ---------------------------------------------------

    @Suppress("UNCHECKED_CAST")
    private fun json(s: String): Map<String, Any?> =
        Moshi
            .Builder()
            .build()
            .adapter(Any::class.java)
            .fromJson(s) as Map<String, Any?>

    /**
     * 真跑录到的活流形状：比 SDK golden 多一层 `unifiedWindows`，那一层里才有真的百分比。
     *
     * 读数（CC 2.1.261，`--permission-mode default`，headless 管道）：
     * `"unifiedWindows":{"five_hour":{"utilization":0.63,...},"seven_day":{"utilization":0.19,...},...}`
     */
    @Test
    fun theLiveCliShapeCarriesRealPercentages() {
        val raw =
            json(
                """
                {"rate_limit_info":{"status":"allowed","resetsAt":1788588000,"rateLimitType":"five_hour",
                "isUsingOverage":false,"unifiedWindows":{"five_hour":{"utilization":0.63,"resetsAt":1788588000},
                "seven_day":{"utilization":0.19,"resetsAt":1788584400}}},"session_id":"$sid"}
                """.trimIndent().replace("\n", ""),
            )
        val r = RateLimitReading.parse(raw, nowMs = 9)!!
        assertEquals("allowed", r.status)
        assertEquals("five_hour", r.limitType)
        assertEquals(1788588000L, r.resetsAtEpochSec)
        assertEquals(listOf("five_hour", "seven_day"), r.windows.map { it.name })
        // 方向：0.63 是已用不是剩余。两者搞反的话屏幕会在快满时说「还很宽裕」，
        // 而两个都是 0..1 的数 ⇒ 没有别的断言会红。这一条专钉方向。
        assertEquals("最紧的那个窗口 = 用量最高的那个", "five_hour", r.tightest?.name)
        assertEquals(0.63, r.tightest!!.utilization, 1e-9)
    }

    /**
     * 配额会超过 100%。
     *
     * 读数（PIPE_FLAGS + `--permission-mode manual`，CC 2.1.261）：
     * ```
     * {"rate_limit_info":{"status":"rejected","resetsAt":1788588000,"rateLimitType":"five_hour",
     *  "unifiedWindows":{"five_hour":{"utilization":1.02,"resetsAt":1788588000},
     *                    "seven_day":{"utilization":0.03,...},
     *                    "seven_day_overage_included":{"utilization":0,...}}}}
     * ```
     *
     * 三件事都要钉住：
     * ① `utilization` 超过 1（1.02）：加一个 `coerceAtMost(1.0)` 就把「已经超了」压成「刚好用完」；
     * ② `utilization == 0` 的窗口照样是一条读数，不许当成「没给」丢掉（`seven_day_overage_included` 就是 0）；
     * ③ `status` 是 `"rejected"` 而不是 `"allowed"`：原始机器码原样收，不翻译。
     */
    @Test
    fun aQuotaThatIsAlreadyOverTheLimitIsNotClampedToAHundredPercent() {
        val raw =
            json(
                """
                {"rate_limit_info":{"status":"rejected","resetsAt":1788588000,"rateLimitType":"five_hour",
                "overageStatus":"rejected","isUsingOverage":false,
                "unifiedWindows":{"five_hour":{"utilization":1.02,"resetsAt":1788588000},
                "seven_day":{"utilization":0.03,"resetsAt":1789189200},
                "seven_day_overage_included":{"utilization":0,"resetsAt":1789189200}}}}
                """.trimIndent().replace("\n", ""),
            )
        val r = RateLimitReading.parse(raw, nowMs = 9)!!
        assertEquals("① 原始机器码原样收", "rejected", r.status)
        assertEquals("② 用量为 0 的窗口也是一条读数，不许被丢掉", 3, r.windows.size)
        assertEquals("five_hour", r.tightest?.name)
        assertEquals("③ 超过 1 不许被压成 1.0", 1.02, r.tightest!!.utilization, 1e-9)
    }

    /**
     * [RateLimitReading.tightest] 取的是用量最高的，不是第一个。
     *
     * 真读数里 `five_hour: 0.63` / `seven_day: 0.19`，而 [RateLimitReading.windows] 按窗口名排序，
     * 「第一个」与「最高的」恰好是同一个答案，把 `maxByOrNull` 换成 `firstOrNull` 在
     * [theLiveCliShapeCarriesRealPercentages] 上不红。所以这里的夹具刻意反着造：字典序第一的窗口用量最低
     * （顺序是合成的，唯一用途就是让「取第一个」红）。
     */
    @Test
    fun theTightestWindowIsTheFullestOneNotTheFirstOne() {
        val raw =
            json(
                """
                {"rate_limit_info":{"status":"allowed","rateLimitType":"five_hour",
                "unifiedWindows":{"a_window":{"utilization":0.11},"z_window":{"utilization":0.97}}}}
                """.trimIndent().replace("\n", ""),
            )
        val r = RateLimitReading.parse(raw, nowMs = 9)!!
        assertEquals("前提：夹具里字典序第一的是用量最低的那个", listOf("a_window", "z_window"), r.windows.map { it.name })
        assertEquals("取的是用量最高的，不是第一个", "z_window", r.tightest?.name)
        assertEquals(0.97, r.tightest!!.utilization, 1e-9)
    }

    /**
     * 两套键名对账：snake_case 那侧要能独立读得出来。
     *
     * SDK 录制的 `rate_limit_info` 里还带一份 camelCase 镜像
     * （`"resets_at":1785543000, …, "raw":{"resetsAt":1785543000,"rateLimitType":"five_hour"}`），
     * 只认 camelCase 的实现从内层 `raw` 里照样拿到同样的值，[bothRecordedEnvelopeSpellingsParse] 守不住。
     * 这里把那份镜像摘掉，钉的是「不依赖那份镜像」。夹具是「真录制减去它的 camelCase 镜像」。
     */
    @Test
    fun theSnakeCaseSpellingIsReadOnItsOwnWithoutTheCamelCaseMirror() {
        val raw =
            json(
                """
                {"rate_limit_info":{"__type__":"RateLimitInfo","status":"allowed","resets_at":1785543000,
                "rate_limit_type":"five_hour","utilization":null,"overage_status":"rejected"}}
                """.trimIndent().replace("\n", ""),
            )
        val r = RateLimitReading.parse(raw, nowMs = 9)!!
        assertEquals("没有内层 raw 兜着时，外层 snake_case 的 resets_at 必须自己读得出来", 1785543000L, r.resetsAtEpochSec)
        assertEquals("同上：rate_limit_type", "five_hour", r.limitType)
        assertTrue("这份素材依旧没有百分比 ⇒ 一格都不许造", r.windows.isEmpty())
    }

    /**
     * 阴性对照：SDK golden 是 snake_case、`utilization` 恒 `null`、没有 `unifiedWindows`。
     *
     * 解得出 `status`，但 [RateLimitReading.windows] 必须是空的：屏上一个百分比都不许有。
     * 这是「没有百分比」与「百分比是 0」的分界线：给 `utilization` 填个 `0.0` 兜底的话，
     * 活流上表现完全正常（那儿有真值），只有这类素材会静静地显示「已用 0%」。
     */
    @Test
    fun payloadsWithoutUtilizationProduceNoPercentAtAll() {
        val raw =
            json(
                """
                {"rate_limit_info":{"__type__":"RateLimitInfo","status":"allowed","resets_at":1785543000,
                "rate_limit_type":"five_hour","utilization":null,"overage_status":"rejected",
                "raw":{"status":"allowed","resetsAt":1785543000,"rateLimitType":"five_hour"}}}
                """.trimIndent().replace("\n", ""),
            )
        val r = RateLimitReading.parse(raw, nowMs = 9)!!
        assertEquals("snake_case 那侧也要认得出来", "allowed", r.status)
        assertEquals("five_hour", r.limitType)
        assertEquals(1785543000L, r.resetsAtEpochSec)
        assertTrue("流上没给百分比 ⇒ 一个窗口都不许造出来", r.windows.isEmpty())
        assertNull(r.tightest)
    }

    /** 解不出来回 null ⇒ 调用方据此不覆盖上一条真读数。 */
    @Test
    fun anEnvelopeWithoutRateLimitInfoIsNotAReading() {
        assertNull(RateLimitReading.parse(json("""{"session_id":"$sid"}"""), 1))
        assertNull(RateLimitReading.parse(null, 1))
    }

    /**
     * 跨两套素材的对账：真的去读 `bridge/vectors` 里的两种录制，两边都必须解得出 `status`。
     *
     * 只喂手写的夹具时，抄错一个键名恒 null 而判据全绿。
     */
    @Test
    fun bothRecordedEnvelopeSpellingsParse() {
        val dir = File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")
        val cli = File(dir, "cli-text-short.cli.ndjson").readLines().first { it.contains("rate_limit_event") }
        val sdk = File(dir, "text-short.ndjson").readLines().first { it.contains("rate_limit_info") }

        // 两份录制各有自己的外层信封（CLI 是 `line`，SDK 是 `msg`），剥到带 `rate_limit_info` 的那层。
        // 解析器认的是那一层，不是信封。
        fun payloadOf(row: String): Map<String, Any?> {
            val m = json(row)
            val body = (m["line"] ?: m["msg"]) as? Map<*, *> ?: m
            return mapOf("rate_limit_info" to body["rate_limit_info"])
        }

        val cliInfo = RateLimitReading.parse(payloadOf(cli), 1)
        assertNotNull("CLI 录制解不出来 —— camelCase 那侧漏了", cliInfo)
        assertEquals("allowed", cliInfo!!.status)
        assertEquals("camelCase 的 resetsAt 漏了", 1785651000L, cliInfo.resetsAtEpochSec)
        assertEquals("camelCase 的 rateLimitType 漏了", "five_hour", cliInfo.limitType)

        val sdkInfo = RateLimitReading.parse(payloadOf(sdk), 1)
        assertNotNull("SDK 录制解不出来 —— snake_case 那侧漏了", sdkInfo)
        assertEquals("allowed", sdkInfo!!.status)
        assertEquals("snake_case 的 resets_at 漏了", 1785543000L, sdkInfo.resetsAtEpochSec)
        assertEquals("snake_case 的 rate_limit_type 漏了", "five_hour", sdkInfo.limitType)
        assertTrue("SDK 录制里没有百分比 ⇒ 一格都不许造", sdkInfo.windows.isEmpty())
    }
}
