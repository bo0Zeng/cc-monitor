package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.bridge.BridgeFrame
import com.ccmonitor.mobile.core.claude.bridge.SendOutcome
import com.ccmonitor.mobile.core.claude.bridge.SendRequest
import com.ccmonitor.mobile.core.claude.bridge.UplinkSink
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.core.claude.transport.SignalSource
import com.ccmonitor.mobile.core.claude.transport.WaitingCopy
import com.ccmonitor.mobile.core.claude.util.SmoothRelease
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * 聊天屏这一侧的三件事：等待态进得来 · 配额画得上 · 逐字流量得到。
 *
 * 文件里有四条阴性对照，它们的作用不是证明功能对，而是证明别的那条判据抓不到那个 bug：
 *
 * | 阴性对照 | 证明什么 |
 * |---|---|
 * | [greyingOutTheButtonWouldNotHaveStoppedAnything] | 「按钮是灰的」那类断言什么都没守住 |
 * | [aOneShotFlushAlsoEndsAtTheFullLengthSoThatAssertionProvesNothing] | 「末项 == N」在一次性刷穿下照样绿 |
 * | [aChatSessionThatNeverAttachedSeesNothingOnTheBus] | 汇里有信号 ≠ 聊天屏消费了（方向一） |
 * | [nothingIsInventedWhenTheBusIsEmpty] | 聊天屏没有凭空造出汇里没有的信号（方向二） |
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatSignalsTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setMain() = Dispatchers.setMain(dispatcher)

    @After fun clearMain() = Dispatchers.resetMain()

    private val sid = "8cfb1b93-285d-42ca-bcc1-24f02419f930"

    /** 数「上行到底被调了几次」的 sink：等待态拦截的判定落点。 */
    private open class CountingSink : UplinkSink {
        override val echoesBack = false
        var sends = 0
            private set
        val texts = mutableListOf<String>()

        override suspend fun send(request: SendRequest): SendOutcome {
            sends++
            texts += request.text
            return SendOutcome.Accepted
        }

        override suspend fun interrupt(): SendOutcome = SendOutcome.Accepted
    }

    /** 一个「此刻正在等人」的汇。 */
    private fun waitingBus(
        atMs: Long = 1_000,
        freshnessMs: Long = 60_000,
        waitingFor: String? = "sandbox request",
    ): SessionSignals =
        SessionSignals(freshnessMs).also {
            it.publishWaiting(sid, "waiting", waitingFor, SignalSource.DAEMON_SESSION_ADDED, atMs)
        }

    private fun sessionOn(
        bus: SessionSignals?,
        sink: UplinkSink?,
        now: () -> Long = { 1_000 },
    ): ChatSession =
        ChatSession(uplink = sink, clock = now).also { s ->
            bus?.let { s.attachSignals(it, sid) }
        }

    private fun ChatUiState.userCards() = units.filterIsInstance<RenderUnit.UserText>()

    // ---- ① 等待态：拦的是上行，不是屏幕 --------------------------------------

    /**
     * 等待态下 `send()` 一个字节都不许写出去。
     *
     * 判定落在 [CountingSink.sends] 上，不在 UI 状态上：`ChatSession.send()` 是公开 API，
     * 靠按钮 disabled 成立是一条侥幸。
     */
    @Test
    fun whileItIsWaitingForYouTheMessageNeverLeavesTheDevice() =
        runTest {
            val sink = CountingSink()
            val vm = sessionOn(waitingBus(), sink)
            runCurrent()
            vm.send("在等的时候打的字")
            advanceUntilIdle()

            assertEquals("上行必须一次都没被调用", 0, sink.sends)
            val card =
                vm.state.value
                    .userCards()
                    .single()
            assertEquals("消息不许静默消失", "在等的时候打的字", card.text)
            assertEquals(DeliveryState.FAILED, card.delivery)
            assertEquals(
                "与模态拒绝路径共用同一句",
                WaitingCopy.SENDING_NOW_ANSWERS_THE_QUESTION,
                card.deliveryError,
            )
        }

    /**
     * 阴性对照：「按钮是灰的」那条断言在 bug 存在时照样绿。
     *
     * 这里手工造一个只有 UI 态、`send()` 一个字节没改的世界：像素级断言
     * （[inputBlockedBy] 非 null）通过，而真正的上行拦截根本不存在。
     * 所以 [whileItIsWaitingForYouTheMessageNeverLeavesTheDevice] 才是有判别力的那条。
     */
    @Test
    fun greyingOutTheButtonWouldNotHaveStoppedAnything() =
        runTest {
            val fakeUiOnly =
                ChatUiState(
                    waiting = WaitingNotice("待放行一次越界操作", "这条要在电脑上回答。", blocksSending = true, reason = "假的"),
                )
            assertNotNull("像素级断言在「只画灰」的世界里照样绿", inputBlockedBy(fakeUiOnly, hasEscape = true))

            // 而同一时刻，一个没接汇的 session（= `send()` 没有任何拦截）照样把话送了出去
            val sink = CountingSink()
            val vm = sessionOn(bus = null, sink = sink)
            vm.send("照样发得出去")
            advanceUntilIdle()
            assertEquals("这正是要点：屏幕说被拦了，而 in.ndjson 照写", 1, sink.sends)
        }

    /**
     * 「我知道，还是发」永远走得通，而且不依赖等待态自己解除。
     *
     * 走它发出去的消息按成功路径处理：不标失败、不静默丢弃。
     */
    @Test
    fun theUserCanAlwaysOverrideAndSendAnyway() =
        runTest {
            val sink = CountingSink()
            val bus = waitingBus()
            val vm = sessionOn(bus, sink)
            runCurrent()

            vm.sendAnyway("我知道，还是发")
            advanceUntilIdle()

            assertEquals(1, sink.sends)
            assertEquals("我知道，还是发", sink.texts.single())
            val card =
                vm.state.value
                    .userCards()
                    .single()
            assertEquals(
                "逃生路径上的消息按成功路径处理（送出未确认）",
                DeliveryState.SENT_UNCONFIRMED,
                card.delivery,
            )
            assertNull(card.deliveryError)

            // 按过一次之后不再拦：等待态一格没变，而普通 send 现在也走得通
            assertTrue("前提：汇里那条等待态还在", bus.waitingGate(sid, 1_000).blocks)
            vm.send("后面这句也该发得出去")
            advanceUntilIdle()
            assertEquals(2, sink.sends)
        }

    /** 等待不是断线：停止照常可用，不许顺手把整屏冻住。 */
    @Test
    fun waitingDoesNotFreezeTheWholeScreen() =
        runTest {
            val sink = CountingSink()
            val vm = sessionOn(waitingBus(), sink)
            vm.start(MutableSharedFlow(), smooth = false)
            runCurrent()

            vm.stop()
            advanceUntilIdle()
            assertTrue("停止必须照常送得出去", vm.state.value.remoteStop is RemoteStop.Requested)
            // 「重新接上」这条出口也还在（`canReattach` 的判据没被等待态动过）
            assertNotNull(vm.state.value.waiting)
        }

    /** 屏上那句话里一个机器码都没有。 */
    @Test
    fun theSentenceOnScreenNeverContainsAMachineCode() =
        runTest {
            val vm = sessionOn(waitingBus(waitingFor = "permission prompt"), CountingSink())
            runCurrent()
            val notice = vm.state.value.waiting!!
            assertFalse("机器码原样上屏", notice.headline.contains("permission"))
            assertEquals("等批准一个操作", notice.headline)
            // 理由要读得出来：哪条帧、什么时刻、什么值
            assertTrue(notice.reason.contains(SignalSource.DAEMON_SESSION_ADDED.name))
            assertTrue(notice.reason.contains("permission prompt"))
        }

    /** 读数过期 ⇒ 不拦了，但那句提示还在（不能变成「什么都没发生」）。 */
    @Test
    fun aStaleWaitingStopsBlockingButStillSaysSomething() =
        runTest {
            val sink = CountingSink()
            var now = 1_000L
            val vm = sessionOn(waitingBus(atMs = 1_000, freshnessMs = 5_000), sink) { now }
            runCurrent()
            assertTrue(
                "前提：新鲜时要拦",
                vm.state.value.waiting!!
                    .blocksSending,
            )

            now = 1_000 + 5_001 // 推时钟，不 sleep
            vm.send("过期之后这句该发得出去")
            advanceUntilIdle()

            assertEquals("判据跑在行为上：拦真的失效了", 1, sink.sends)
            assertFalse(
                vm.state.value.waiting!!
                    .blocksSending,
            )
            assertTrue(
                vm.state.value.waiting!!
                    .reason
                    .contains(SessionSignals.STALE_PREFIX),
            )
        }

    /** 丢过 `session_status` ⇒ 等待态作废，上行放行。 */
    @Test
    fun aLostStatusFrameInvalidatesTheBlock() =
        runTest {
            val sink = CountingSink()
            val bus = waitingBus()
            val vm = sessionOn(bus, sink)
            runCurrent()
            bus.invalidateWaiting(sid, SessionSignals.LOST_STATUS_FRAME)
            runCurrent()

            assertNull("作废之后屏上不该再有等待条", vm.state.value.waiting)
            vm.send("放行")
            advanceUntilIdle()
            assertEquals(1, sink.sends)
        }

    /**
     * 阴性对照 · 方向一：汇里有信号，而聊天屏没接 ⇒ 屏上什么都没有。
     *
     * 它证明「汇里有这条信号」这件事本身不构成「聊天屏消费了它」（「已 build 零引用」的形状）。
     */
    @Test
    fun aChatSessionThatNeverAttachedSeesNothingOnTheBus() =
        runTest {
            val sink = CountingSink()
            val bus = waitingBus()
            val vm = sessionOn(bus = null, sink = sink) // 就是不接
            runCurrent()
            assertTrue("前提：汇里确实有这条信号", bus.waitingGate(sid, 1_000).blocks)
            assertNull("没接 ⇒ 屏上没有", vm.state.value.waiting)
            vm.send("没接的时候照常发")
            advanceUntilIdle()
            assertEquals(1, sink.sends)
        }

    /** 阴性对照 · 方向二：汇是空的 ⇒ 聊天屏不许凭空造一条等待态出来。 */
    @Test
    fun nothingIsInventedWhenTheBusIsEmpty() =
        runTest {
            val vm = sessionOn(SessionSignals(), CountingSink())
            runCurrent()
            assertNull(vm.state.value.waiting)
            assertNull(vm.state.value.quota)
            assertNull(vm.state.value.blocked)
        }

    /** 没有逃生口就不许禁输入框（否则是一条响亮的死路）。 */
    @Test
    fun withoutAnEscapeHatchTheInputIsNotDisabledAtAll() {
        val blocking = ChatUiState(waiting = WaitingNotice("等批准一个操作", "…", blocksSending = true, reason = "r"))
        assertNotNull("有出口 ⇒ 禁", inputBlockedBy(blocking, hasEscape = true))
        assertNull("没出口 ⇒ 连禁都不禁（不许把用户关进死胡同）", inputBlockedBy(blocking, hasEscape = false))
        val advisory = ChatUiState(waiting = WaitingNotice("等批准一个操作", "…", blocksSending = false, reason = "r"))
        assertNull("只提示不拦的那一格也不禁", inputBlockedBy(advisory, hasEscape = true))
    }

    // ---- ② 配额上屏 -------------------------------------------------------------

    private fun rlFrame(unified: Boolean): BridgeFrame.RateLimit {
        val info =
            mutableMapOf<String, Any?>(
                "status" to "allowed",
                "resetsAt" to 1788588000.0,
                "rateLimitType" to "five_hour",
            )
        if (unified) {
            info["unifiedWindows"] =
                mapOf(
                    "five_hour" to mapOf("utilization" to 0.63, "resetsAt" to 1788588000.0),
                    "seven_day" to mapOf("utilization" to 0.19, "resetsAt" to 1788584400.0),
                )
        }
        return BridgeFrame.RateLimit(mapOf("rate_limit_info" to info))
    }

    /** `rate_limit_event` 一到，配额就在 [ChatUiState] 上，不许解出来然后扔掉。 */
    @Test
    fun aRateLimitFrameLandsOnTheUiState() =
        runTest {
            val bus = SessionSignals()
            val vm = sessionOn(bus, null)
            vm.start(flowOf(rlFrame(unified = true)), smooth = false)
            advanceUntilIdle()

            val q = vm.state.value.quota!!
            assertEquals("five_hour", q.tightest?.name)
            assertEquals(0.63, q.tightest!!.utilization, 1e-9)
            // 同一条信号也进了汇：总览面要用同一份读数，而不是自己再解一遍
            assertEquals(
                0.63,
                bus
                    .signalFor(sid)
                    ?.rateLimit
                    ?.tightest
                    ?.utilization!!,
                1e-9,
            )
        }

    /**
     * 阴性对照：老素材（没有 `unifiedWindows`）⇒ 一个百分比都不许出现。
     *
     * 「没有百分比」和「百分比是 0」是两件事：有人给 `utilization` 填个 0 兜底，
     * 在活流上表现完全正常，只有老素材静静地说「已用 0%」。
     */
    @Test
    fun anOldStyleRateLimitFrameShowsNoPercentage() =
        runTest {
            val vm = sessionOn(SessionSignals(), null)
            vm.start(flowOf(rlFrame(unified = false)), smooth = false)
            advanceUntilIdle()

            val q = vm.state.value.quota!!
            assertEquals("解得出来", "allowed", q.status)
            assertTrue("但没有百分比就是没有", q.windows.isEmpty())
            assertNull(q.tightest)
        }

    /**
     * 配额那句话的三档。
     *
     * 活流里能读到 `five_hour.utilization = 1.02`，同一条帧 `rate_limit_info.status = "rejected"`：
     * 超过 100% 是真实形状，只有「快用完了」那一档的话，一个已经被拒的账号在屏上读到的是一句假话。
     *
     * 边界逐个断（`>= 0.9` 与 `>= 1.0` 各自的那一点）：只断中间值的话，把 `>=` 写成 `>` 一条断言都不红。
     */
    @Test
    fun theQuotaLineSaysSomethingDifferentOnceYouAreActuallyOverTheLimit() {
        assertEquals("远没满 ⇒ 什么都不多说", "", quotaTailFor(0.63))
        assertEquals("0.9 那一点本身就算「快用完了」", " · 快用完了", quotaTailFor(0.9))
        assertEquals("", quotaTailFor(0.8999))
        assertEquals(" · 快用完了", quotaTailFor(0.999))
        assertEquals("1.0 那一点就是「用满」", " · 已经超了，得等它重置", quotaTailFor(1.0))
        assertEquals("超过 100% 的真实读数", " · 已经超了，得等它重置", quotaTailFor(1.02))
    }

    /**
     * 「它想做的一件事被挡住了」（`system/permission_denied`）到得了屏幕上。
     *
     * 那条帧走 `CliFrameEncoder` 的 catch-all，收到了、语义留着，屏上要有字。
     *
     * 本条钉的是「帧来了会上屏」，不是「这条帧真的会来」：真实会话里还没见过它。
     * 这一段是加法的，帧不来就什么都不显示。
     */
    @Test
    fun aPermissionDeniedFrameIsSaidOutLoud() =
        runTest {
            val bus = SessionSignals()
            val vm = sessionOn(bus, null)
            val frame =
                BridgeFrame.Event(
                    k = ChatSession.EVENT_PERMISSION_DENIED,
                    i = null,
                    raw = mapOf("tool_name" to "Bash", "message" to "rm in '/tmp/x' was blocked."),
                )
            vm.start(flowOf(frame), smooth = false)
            advanceUntilIdle()

            val b = vm.state.value.blocked!!
            assertEquals("Bash", b.toolName)
            assertEquals("远端产出的那句话原样带回，不翻译", "rm in '/tmp/x' was blocked.", b.humanText)
            assertEquals("Bash", bus.signalFor(sid)?.blocked?.toolName)
            assertNull("它不是等待态 —— 那件事已经过去了", vm.state.value.waiting)
        }

    // ---- ③ 逐字流：读数序列 -------------------------------------------------------

    /**
     * 一拍能放多少字：从常量算，不写死。
     * 夹具 N 太小的话逐字实现下序列长度也是 1（假阳），而 `CHAT_CHUNK` / `CHAT_INTERVAL_MS` 是会变的。
     */
    private val perTick: Int
        get() =
            maxOf(
                ChatSession.CHAT_CHUNK,
                ((SmoothRelease.MIN_CHARS_PER_SECOND * ChatSession.CHAT_INTERVAL_MS + 999) / 1000).toInt(),
            )

    /** 取「去掉连续重复」之后的读数序列。 */
    private fun readingsOf(seq: List<Int>): List<Int> =
        seq.fold(mutableListOf()) { acc, v ->
            if (acc.lastOrNull() != v) acc += v
            acc
        }

    private fun blockOf(n: Int) = BridgeFrame.AssistantText(m = "m1", i = 0, p = null, x = "字".repeat(n))

    /**
     * 一条 JVM 单测能区分「逐字上屏」与「一次性刷穿」。
     *
     * 断言三条：① 序列长度 ≥ 3 ② 严格递增 ③ 末项 == N。
     * 分辨力全在①，见下面那条阴性对照。
     */
    @Test
    fun theRevealedLengthClimbsInSteps() =
        runTest {
            val n = perTick * 5 // 五拍：够 ≥3，又远低于 SmoothRelease 的追赶阈值（10 拍）
            val vm = ChatSession()
            // 必须用不结束的流：流一结束 `attachFeed` 的 `finally` 就 `release.flush()`，
            //   整块当场全放，那正是下面那条阴性对照要造的形状，不能混进正向这条里。
            val frames = MutableSharedFlow<BridgeFrame>(extraBufferCapacity = 8)
            vm.start(frames, smooth = true)
            runCurrent()
            frames.tryEmit(blockOf(n))
            runCurrent()

            val seq = mutableListOf<Int>()
            repeat(12) {
                advanceTimeBy(ChatSession.CHAT_INTERVAL_MS)
                runCurrent()
                seq += vm.state.value.revealedChars
            }
            // 收完读数就把会话收掉：流不结束 ⇒ 节拍协程永不退出 ⇒ `runTest` 推虚拟时间推到挂死。
            vm.close()
            val readings = readingsOf(seq)

            assertTrue("① 一次性刷穿的话这里就是 1（读数序列 = $readings）", readings.size >= 3)
            assertEquals("② 严格递增", readings.sorted().distinct(), readings)
            assertEquals("③ 末项 == N", n, readings.last())
        }

    /**
     * 阴性对照：同一段素材，整块一次性放出来（缓释没参与）：
     * ① 序列长度塌成 1 ⇒ 上面那条判据红；
     * ② 而「末项 == N」照样成立 ⇒ 证明那条断言一个字都不区分。
     *
     * 「一次性放出来」这里靠流当场结束触发（`attachFeed` 的 `finally` 会 `release.flush()` 把整块一次吐净），
     * 这正是「一次性上屏」在数据上的样子。
     */
    @Test
    fun aOneShotFlushAlsoEndsAtTheFullLengthSoThatAssertionProvesNothing() =
        runTest {
            val n = perTick * 5
            val vm = ChatSession()
            vm.start(flowOf(blockOf(n)), smooth = true) // 流一结束 ⇒ finally 里 flush 整块
            advanceUntilIdle()

            val seq = mutableListOf<Int>()
            repeat(12) {
                advanceTimeBy(ChatSession.CHAT_INTERVAL_MS)
                runCurrent()
                seq += vm.state.value.revealedChars
            }
            val readings = readingsOf(seq)

            assertEquals("①：一次性放完 ⇒ 序列长度塌成 1（$readings）", 1, readings.size)
            assertEquals("②：而「末项 == N」在这里照样绿 ⇒ 它没有判别力", n, readings.last())
        }

    /** `revealedChars` 焊成恒 0 会怎样：这里正向钉住「它真的等于已露出的长度」。 */
    @Test
    fun theRevealedLengthMatchesWhatIsActuallyOnScreen() =
        runTest {
            val n = perTick * 5
            val vm = ChatSession()
            val frames = MutableSharedFlow<BridgeFrame>(extraBufferCapacity = 8)
            vm.start(frames, smooth = true)
            runCurrent()
            frames.tryEmit(blockOf(n))
            runCurrent()
            advanceTimeBy(ChatSession.CHAT_INTERVAL_MS * 2)
            runCurrent()

            val st = vm.state.value
            vm.close() // 同上：不收掉的话节拍协程会把 `runTest` 的虚拟时间推到挂死
            val shown = st.units.filterIsInstance<RenderUnit.AssistantMarkdown>().sumOf { it.markdown.length }
            assertTrue("前提：此刻只露出了一部分", st.revealedChars in 1 until n)
            assertEquals("读数必须等于屏上那段的长度，不是一个自由变量", shown, st.revealedChars)
        }

    // ---- ④ 「已发出，还没看到对面的反应」-----------------------------------------

    /**
     * `Accepted` 的默认落点是 [DeliveryState.SENT_UNCONFIRMED]，
     * 不是 `null`（= 什么都不显示 = 视觉上的「已送达」）。
     *
     * 断言必须逐格断言等于哪一格。写成「不等于 FAILED」的话，把 `Accepted → null` 接回去它照样绿。
     */
    @Test
    fun anAcceptedMessageIsNotClaimedToHaveArrived() =
        runTest {
            val vm = sessionOn(bus = null, sink = CountingSink())
            vm.send("送出去了但没确认")
            advanceUntilIdle()
            assertEquals(
                DeliveryState.SENT_UNCONFIRMED,
                vm.state.value
                    .userCards()
                    .single()
                    .delivery,
            )
        }

    // ---- 阴性对照：一条 waiting 都不喂时，既有行为一个字节不变 ------------------

    /**
     * 没有等待态时不改变任何既有行为。
     *
     * 不接汇的 session（重放屏走的正是这条）：发得出去、`sending` 会翻、停止照常。
     */
    @Test
    fun withoutAnySignalEverythingBehavesExactlyAsBefore() =
        runTest {
            val sink = CountingSink()
            val vm = ChatSession(sink)
            vm.start(MutableSharedFlow(), smooth = false)
            runCurrent()

            vm.send("普通一句话")
            advanceUntilIdle()
            assertEquals(1, sink.sends)
            assertNull(vm.state.value.waiting)
            assertNull(vm.state.value.quota)
            assertFalse(vm.state.value.sending)
            vm.stop()
            advanceUntilIdle()
            assertTrue(vm.state.value.remoteStop is RemoteStop.Requested)
        }
}
