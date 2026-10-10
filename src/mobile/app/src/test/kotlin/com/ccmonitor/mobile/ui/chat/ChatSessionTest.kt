package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.bridge.BridgeFrame
import com.ccmonitor.mobile.core.claude.bridge.ChatTurnAssembler
import com.ccmonitor.mobile.core.claude.bridge.CliFrameEncoder
import com.ccmonitor.mobile.core.claude.bridge.Pacing
import com.ccmonitor.mobile.core.claude.bridge.ReplayRow
import com.ccmonitor.mobile.core.claude.bridge.ReplayTransport
import com.ccmonitor.mobile.core.claude.bridge.ReplayVectors
import com.ccmonitor.mobile.core.claude.bridge.SendOutcome
import com.ccmonitor.mobile.core.claude.bridge.SendRequest
import com.ccmonitor.mobile.core.claude.bridge.TurnState
import com.ccmonitor.mobile.core.claude.bridge.UplinkSink
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
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
import java.io.File
import java.util.concurrent.Executors

/**
 * [ChatSession]：缓释器与 assembler 的接线是这里唯一真有逻辑的地方。
 *
 * 注意：断言前先证明前提成立（下面几条都先 assert 前提）。一条只在 `?.let {}` 里断言的测试，
 * 遇到空输入会跑 0 个断言照绿；一条用 `advanceUntilIdle()` 把流跑完再测 `stop()` 的测试，
 * 把 `stop()` 换成空函数也照绿。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatSessionTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setMain() = Dispatchers.setMain(dispatcher)

    @After fun clearMain() = Dispatchers.resetMain()

    private fun rows(case: String): List<ReplayRow> {
        val dir = File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")
        return ReplayVectors.parse(File(dir, "$case.frames.ndjson").readLines().asSequence())
    }

    private fun frames(
        case: String,
        pacing: Pacing = Pacing.None,
    ): Flow<BridgeFrame> = ReplayTransport(rows(case), pacing).frames()

    private fun at(x: String) = BridgeFrame.AssistantText(m = "m1", i = 0, p = null, x = x)

    private fun ChatUiState.textLen() =
        units.filterIsInstance<RenderUnit.AssistantMarkdown>().sumOf { it.markdown.length }

    // ---- 基础：跑完一条真 vector ------------------------------------------------

    @Test
    fun replayingARealVectorEndsWithCompleteTextAndClosedTurn() =
        runTest {
            val vm = ChatSession()
            vm.start(frames("long-reply"), smooth = false)
            advanceUntilIdle()

            val s = vm.state.value
            assertTrue("turn 必须收口", s.turn is TurnState.Done)
            assertFalse("收口后不该还显示流式", s.streaming)
            assertNull("正常结束不该有失败原因", s.failedWhy)

            val expected =
                rows("long-reply").mapNotNull { it.frame as? BridgeFrame.AssistantText }.sumOf { it.x.length }
            assertEquals("跑完之后正文不许缺字", expected, s.textLen())
        }

    // ---- 越聊越卡 ---------------------------------------------------------

    /**
     * 一批帧到达，只该发布一次状态，不是每帧一次。
     *
     * `publish()` 会重建整份渲染列表，代价与对话长度线性；流式一轮有几百个增量帧，
     * 每帧一次就是「越聊越卡」，且全在主线程上。
     *
     * 判据数 [ChatSession.publishCount]（真被执行的重建），不数收集器收到几次：
     * `StateFlow` 会合并，重建上千次也可能只被观察到几次。换任何别的合并写法（节流、conflate、
     * 按 tick 发）同样该绿。前提先断言：这批帧确实产出了内容。
     */
    @Test(timeout = TEN_SECONDS)
    fun aBurstOfFramesPublishesOnceNotOncePerFrame() =
        runTest {
            val vm = ChatSession()

            val turns = 40
            val perTurn = 33
            val frames =
                flow {
                    for (n in 1..turns) {
                        emit(BridgeFrame.BlockStart(m = "m$n", i = 0, bt = "text", id = null, name = null))
                        repeat(30) { emit(BridgeFrame.TextDelta(m = "m$n", i = 0, x = "字")) }
                        emit(at("第 $n 轮的回答"))
                        emit(BridgeFrame.Result("s", true, null, 1, "end_turn", null, null, null, null, null, null))
                    }
                }
            vm.start(frames, smooth = false)
            advanceUntilIdle()

            // 前提：这批帧真的产出了内容 —— 否则下面「重建次数少」是因为什么都没发生
            assertEquals("前提：每轮应产出一个单元", turns, vm.state.value.units.size)

            val total = turns * perTurn
            assertTrue(
                "$total 帧只该重建很少几次整份列表，实得 ${vm.publishCount} —— 每帧一次就是「越聊越卡」的来源",
                vm.publishCount < total / 10,
            )
        }

    /**
     * 合并必须在跨了调度器边界之后依然成立，生产就是这个形状。
     *
     * `PipeSession.frames()` 带 `flowOn(cpu)`：帧从解析线程经一个 channel 交到收集者手上。
     * 上一条喂的是同线程的 `flow{}`，覆盖不到这一格；跨边界之后若变成「一个 dispatch 一帧」，
     * 合并就退化回每帧一次。
     */
    @Test(timeout = TEN_SECONDS)
    fun theCoalescingSurvivesADispatcherBoundary() {
        // 不能用 `runTest` 的虚拟时间：跨调度器那一侧跑在真线程上，`advanceUntilIdle()` 等不到它。
        //   所以全程真实调度器 + 有界等待。
        val sessionExec = Executors.newSingleThreadExecutor { Thread(it, "session") }
        val scope = CoroutineScope(SupervisorJob() + sessionExec.asCoroutineDispatcher())
        try {
            val vm = ChatSession(scope = scope)
            val turns = 40
            val perTurn = 33
            val frames =
                flow {
                    for (n in 1..turns) {
                        emit(BridgeFrame.BlockStart(m = "m$n", i = 0, bt = "text", id = null, name = null))
                        repeat(30) { emit(BridgeFrame.TextDelta(m = "m$n", i = 0, x = "字")) }
                        emit(at("第 $n 轮的回答"))
                        emit(BridgeFrame.Result("s", true, null, 1, "end_turn", null, null, null, null, null, null))
                    }
                    // 与生产同形：帧从另一个调度器上来
                }.flowOn(Dispatchers.Default.limitedParallelism(1))
            vm.start(frames, smooth = false)

            val deadline = System.currentTimeMillis() + WAIT_BUDGET_MS
            while (vm.state.value.units.size < turns && System.currentTimeMillis() < deadline) Thread.sleep(POLL_MS)

            assertEquals("前提：这批帧真的跑完了，否则下面判的是半截", turns, vm.state.value.units.size)
            val total = turns * perTurn
            assertTrue(
                "跨调度器之后 $total 帧仍只该重建很少几次，实得 ${vm.publishCount}",
                vm.publishCount < total / 10,
            )
        } finally {
            scope.cancel()
            sessionExec.shutdownNow()
        }
    }

    // ---- 一条全新管道上，发得出第一句话吗 -------------------

    /**
     * 新起的对话必须能发出第一句话。
     *
     * 若 `ChatTurnAssembler.turn` 的初值是 `TurnState.Streaming`，刚起的管道从出生就自称「生成中」
     * ⇒ `ChatScreen` 把动作键画成「停止」⇒ 发不出第一句 ⇒ `res` 永远不会来 ⇒ 那个键再也变不回去。
     *
     * 判据落在「界面此刻给不给发」（[ChatUiState.streaming]）上，是一条完整的时间线：
     * ① 刚起的管道吐完启动帧之后给发；② 真的开始一轮之后不给发；③ `res` 到了之后又给发。
     * 只有 ① 会被「初值 Streaming」打红，只有 ②③ 会被「干脆恒为 false」打红。
     *
     * 启动帧取远端刚起管道时真实吐出的 `SessionStart` 钩子事件，喂给真的 [CliFrameEncoder] 转成帧。
     */
    @Test
    fun aFreshlyStartedPipeLetsTheUserSendTheFirstMessage() =
        runTest {
            val enc = CliFrameEncoder()
            // `~/.aterm/s/<id>/events.ndjson` 头两条就是这两行（只留下与判据相关的键）
            val startup =
                listOf(
                    mapOf(
                        "type" to "system",
                        "subtype" to "hook_started",
                        "hook_name" to "SessionStart:startup",
                        "hook_event" to "SessionStart",
                    ),
                    mapOf(
                        "type" to "system",
                        "subtype" to "hook_response",
                        "hook_name" to "SessionStart:startup",
                        "hook_event" to "SessionStart",
                    ),
                ).flatMap { enc.feed(it) }
            assertTrue("前提：这两行确实转出了帧，否则下面什么都没喂", startup.isNotEmpty())
            assertTrue(
                "前提：启动帧里没有块级帧或 init —— 有的话那就真开始一轮了，本测就失去意义",
                startup.none { it is BridgeFrame.Init || it is BridgeFrame.BlockStart || it is BridgeFrame.AssistantText },
            )

            val downlink = MutableSharedFlow<BridgeFrame>(extraBufferCapacity = 64)
            val vm = ChatSession()
            vm.start(downlink, smooth = false)
            advanceUntilIdle()
            // 前提，别删：`MutableSharedFlow` 在没有订阅者时 `emit` 是直接丢的；
            //   丢了的话后面的断言都落在什么都没喂的默认 state 上。
            assertEquals("前提：收集器必须已经订阅，否则下面 emit 的帧全丢", 1, downlink.subscriptionCount.value)

            startup.forEach { downlink.emit(it) }
            advanceUntilIdle()
            assertFalse("刚起的管道必须给发 —— 否则第一句话永远发不出去", vm.state.value.streaming)

            // ② 真的开始一轮了 —— 这时才该收起发送键、换成停止键
            downlink.emit(at("在生成了"))
            advanceUntilIdle()
            assertTrue("一轮真开始之后必须收起发送键，否则停止键永远不出现", vm.state.value.streaming)

            // ③ `res` 收口 —— 又该给发
            downlink.emit(BridgeFrame.Result("s", true, null, 1, "end_turn", null, null, null, null, null, null))
            advanceUntilIdle()
            assertFalse("收口之后要重新给发", vm.state.value.streaming)
        }

    // ---- 下行抛异常不许掀掉整个 app ----------------------------

    /**
     * 下行断了要说出来，而不是崩 app。
     *
     * 断网或管道被 kill 时 `ConnectionDeadException` 会从 `viewModelScope.launch` 里逃出去，
     * 未捕获的协程异常会让整个 app 崩。
     *
     * 判据三条，缺一不可：① 不许把异常抛出去；② 原因要说出来；③ 已经收到的内容留在屏上（降级不是删除）。
     */
    @Test
    fun aDownlinkFailureIsReportedInsteadOfCrashingAndKeepsWhatArrived() =
        runTest {
            val vm = ChatSession()
            val boom =
                flow {
                    emit(BridgeFrame.AssistantText(m = "m1", i = 0, p = null, x = "已经到了的正文"))
                    throw java.io.IOException("远端断了")
                }
            vm.start(boom, smooth = false) // 抛出去的话这行就直接把测试炸了
            advanceUntilIdle()

            val s = vm.state.value
            assertEquals("原因要说出来，不许静默", "远端断了", s.downlinkError)
            assertTrue("已经收到的不许被抹掉", s.textLen() > 0)
            assertFalse("断了就不该还显示流式", s.streaming)
        }

    /** 取消不是断线 —— `stop()` 走的是 CancellationException，不许被报成故障。 */
    @Test
    fun cancellingIsNotReportedAsADownlinkFailure() =
        runTest {
            val vm = ChatSession()
            vm.start(frames("long-reply", Pacing.Original), smooth = true)
            advanceTimeBy(TWO_SECONDS)
            runCurrent()
            vm.stop()
            advanceUntilIdle()
            assertNull("用户点停止不是「断了」", vm.state.value.downlinkError)
        }

    // ---- 下行必须能重挂 ----------------------------------------

    /**
     * 流干净结束不许静默。
     *
     * 远端 tail 套了 `timeout 6h`，到点后正常退出：没有异常、没有 `res`。而 `streaming` 的判据里
     * `done == null` 仍成立 ⇒ 界面会永远显示「生成中」且再也不更新，用户完全看不出。
     */
    @Test
    fun aCleanlyEndedDownlinkSaysSoInsteadOfPretendingToStillStream() =
        runTest {
            val vm = ChatSession()
            // 一条正常结束的流：给一帧就完，没有 res、没有异常
            vm.start(flowOf(at("到这儿就断了")), smooth = false)
            advanceUntilIdle()

            val s = vm.state.value
            assertNotNull("干净结束也要说出来，不许静默", s.downlinkError)
            assertFalse("不许还显示「生成中」", s.streaming)
            assertTrue("而且要给得出重新接上的入口", s.canReattach)
            assertTrue("已收到的内容留在屏上（降级不是删除）", s.textLen() > 0)
        }

    /**
     * `stop()` 之后还能再接回来。
     *
     * 若 `started` 永不复位且 `start()` 首行 `if (started) return`，点一次「停止」下行就永久失联。
     */
    @Test
    fun stoppingIsNotPermanentTheDownlinkCanBeAttachedAgain() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(at("第一段")), smooth = false)
            advanceUntilIdle()
            vm.stop()
            advanceUntilIdle()
            assertTrue("前提：确实停住了", vm.state.value.stoppedByUser)

            assertTrue("停过之后必须还能重挂", vm.reattach(flowOf(at("重新接上之后的内容")), smooth = false))
            advanceUntilIdle()

            val s = vm.state.value
            assertFalse("重挂就是「我要再看」，旧的停止结论要清掉", s.stoppedByUser)
            assertTrue(
                "重挂之后收到的内容必须上屏",
                s.units.any { it is RenderUnit.AssistantMarkdown && it.markdown.contains("重新接上之后的内容") },
            )
        }

    /** 重挂幂等：还在收的时候再调不许开第二条流（否则内容双份）。 */
    @Test
    fun reattachingWhileStillReceivingIsANoOp() =
        runTest {
            val vm = ChatSession()
            vm.start(
                flow {
                    emit(at("在收"))
                    kotlinx.coroutines.awaitCancellation()
                },
                smooth = false,
            )
            advanceUntilIdle()
            assertFalse("还在收 ⇒ 不该说能重挂", vm.state.value.canReattach)
            assertFalse("再调一次必须是 no-op", vm.reattach(flowOf(at("第二条流"))))
        }

    // ---- 阻塞回归：停止之后不许翻回「生成中」 --------------------------------

    /**
     * 点一次「停止」，页面不许永久卡在「生成中」。
     *
     * 机制：`stop()` 里 `feedJob.cancel()` 是异步的，随后的 `publish()` 可能被协程展开时
     * `finally { … publish() }` 那个不带停止标记的 publish 覆盖；此时没收到 `res`，
     * `assembler.turn` 仍是 `Streaming` ⇒ `streaming` 又变回 true，「发送」键再也回不来。
     *
     * 这条必须真的停在流式中途（`advanceTimeBy` 而非 `advanceUntilIdle`），否则恒真。
     */
    @Test
    fun stoppingMidStreamStaysStoppedAfterTheCoroutineUnwinds() =
        runTest {
            val vm = ChatSession()
            vm.start(frames("long-reply", Pacing.Original), smooth = true)
            advanceTimeBy(TWO_SECONDS)
            runCurrent()

            assertTrue("前提：此刻必须真的还在流式，否则这条测试什么都没测", vm.state.value.streaming)

            vm.stop()
            assertFalse("停止当下应立刻不再流式", vm.state.value.streaming)

            advanceUntilIdle() // ← 让被取消的协程展开，跑到 finally
            assertFalse("展开之后仍然不许翻回流式", vm.state.value.streaming)
            assertTrue("应记下是用户停的", vm.state.value.stoppedByUser)
        }

    /** `turn` 与 `stoppedByUser` 是两个事实：对端没发 `res`，turn 就该如实停在 Streaming。 */
    @Test
    fun stopDoesNotFakeAProtocolLevelTurnCompletion() =
        runTest {
            val vm = ChatSession()
            vm.start(frames("long-reply", Pacing.Original), smooth = true)
            advanceTimeBy(TWO_SECONDS)
            runCurrent()
            vm.stop()
            advanceUntilIdle()

            assertTrue("没收到 res，协议层轮次就没结束", vm.state.value.turn is TurnState.Streaming)
            assertTrue("但本地确实停了", vm.state.value.stoppedByUser)
        }

    // ---- 阻塞回归：重复 key 会当场崩 LazyColumn -------------------------------

    /**
     * `start(prompt=…)` 产的 key 不许与 [ChatSession.submitLocalEcho] 的撞，
     * 否则 `LazyColumn(items(key))` 当场抛异常（重复 key）。
     *
     * 可达路径：点「停止」→ 旋转屏幕 → `LaunchedEffect` 重跑 → 第二次 `start(prompt=…)`。
     */
    @Test
    fun localUserKeysAreNeverDuplicatedEvenAcrossStopAndRestart() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(), smooth = false, prompt = "第一次")
            advanceUntilIdle()
            vm.send("再说一句")
            vm.stop()
            vm.start(flowOf(), smooth = false, prompt = "第二次") // 重入必须被挡住
            advanceUntilIdle()

            val keys =
                vm.state.value.units
                    .map { it.key }
            assertEquals("key 重复会让 LazyColumn 当场抛异常：$keys", keys.size, keys.toSet().size)
        }

    // ---- 子 agent 不许污染主流缓释 --------------------------------------------

    /**
     * 子 agent 的 `at` 到达时，主流已露出的文字不许倒退。
     *
     * 若帧型判定与 key 计算在 `ChatSession` 与 assembler 里各有一份，子 agent 的 `at` 会把
     * `streamingKey` 指到主流不存在的块，并 `replaceAll` 清掉主 agent 的缓冲：已读的文字当场消失、从头再冒。
     */
    @Test
    fun subagentOutputDoesNotRewindTheMainStream() =
        runTest {
            val vm = ChatSession()
            val main = "主 agent 正在说一段挺长的话，长到足够被缓释器分好几拍放出来。".repeat(4)
            vm.start(
                kotlinx.coroutines.flow
                    .flow {
                        emit(BridgeFrame.TextDelta("msg_main", 0, main))
                        emit(BridgeFrame.AssistantText("msg_main", 0, null, main))
                        kotlinx.coroutines.delay(THREE_HUNDRED_MS)
                        // 子 agent 的权威全文插进来
                        emit(BridgeFrame.AssistantText("msg_sub", 0, "toolu_parent", "子 agent 的东西"))
                        kotlinx.coroutines.delay(THREE_HUNDRED_MS)
                    },
                smooth = true,
            )

            val seen = mutableListOf<Int>()
            repeat(TICKS) {
                advanceTimeBy(ONE_TICK)
                runCurrent()
                seen += vm.state.value.textLen()
            }

            assertTrue("前提：必须真的分多拍露出（否则测不到倒退）：$seen", seen.distinct().size > 2)
            assertTrue("已露出的长度不许回退：$seen", seen.zipWithNext().all { (a, b) -> b >= a })
            assertTrue(
                "子 agent 的正文不许出现在主流",
                vm.state.value.units.filterIsInstance<RenderUnit.AssistantMarkdown>().none {
                    it.markdown.contains("子 agent 的东西")
                },
            )
        }

    // ---- 缓释：露出的必须是权威正文的前缀，且真的是渐进的 ----------------------

    /**
     * 缓释期间显示的必须是权威正文的前缀，且真的分多拍露出。
     *
     * `long-reply` 前一半里没有 `at` 帧，所以先 assert 前提，再 assert 结论。
     */
    @Test
    fun revealIsGradualAndAlwaysAPrefixOfTheAuthoritativeText() =
        runTest {
            val full = "这是一段足够长的权威正文，用来观察缓释器是不是真的一拍一拍地放。".repeat(6)
            val vm = ChatSession()
            vm.start(
                kotlinx.coroutines.flow
                    .flow {
                        emit(BridgeFrame.AssistantText("msg_1", 0, null, full))
                        kotlinx.coroutines.delay(LONG_ENOUGH)
                    },
                smooth = true,
            )

            val seen = mutableListOf<String>()
            repeat(TICKS) {
                advanceTimeBy(ONE_TICK)
                runCurrent()
                vm.state.value.units.filterIsInstance<RenderUnit.AssistantMarkdown>().firstOrNull()?.let {
                    seen += it.markdown
                }
            }

            assertTrue("前提：必须真的分多拍露出，否则这条测试是空的（拍数 ${seen.distinct().size}）", seen.distinct().size > 3)
            seen.forEach { assertTrue("露出的必须是权威正文的前缀：${it.take(20)}…", full.startsWith(it)) }
            assertTrue("最终必须补齐全文", seen.last() == full || full.startsWith(seen.last()))
        }

    // ---- 失败必须可见 -------------------------------------------

    /**
     * 中断那一轮的失败原因上屏时必须是人话，且机器码不许出现。
     *
     * `bridge/vectors/interrupt.frames.ndjson` 那条 `res` 的 `raw.result` 是 `null`，上游没给人话，
     * 这一格只能由我们补。两侧都钉：人话出现 + 机器码不出现。只钉一侧的话，
     * 「有人把映射改成另一个机器码」会照绿。
     */
    @Test
    fun abortedVectorSurfacesItsFailureReason() =
        runTest {
            val vm = ChatSession()
            vm.start(frames("interrupt"), smooth = false)
            advanceUntilIdle()

            val why = vm.state.value.failedWhy
            assertEquals("上屏的必须是人话", RemoteStop.ABORTED_HUMAN, why)
            assertFalse("机器码绝不许上屏：$why", why!!.contains(RemoteStop.ABORTED_STREAMING))
            assertFalse((vm.state.value.turn as TurnState.Done).ok)
        }

    // ---- 停下来 ------------------------------------------------------------

    /**
     * 按停止 ⇒ 中断真的被投到上行去了。
     *
     * 这一条只证明「`stop()` 调到了 `UplinkSink.interrupt()`」；
     * 「远端那个进程真的收到了中断」单测钉不到。
     */
    @Test
    fun pressingStopSendsARealInterruptUpstream() =
        runTest {
            val sink = StoppableSink()
            val vm = ChatSession(sink)
            vm.start(MutableSharedFlow<BridgeFrame>(), smooth = false)
            runCurrent()

            vm.stop()
            advanceUntilIdle()

            assertEquals("停止必须真的投一条中断上去", 1, sink.interrupts)
        }

    /**
     * 阴性对照：「几条断言都绿」不等于「守住了」。
     *
     * 造出真 bug（`stop()` 一个字节都不发给远端），然后跑三条「看起来在验停止」的断言：
     *
     * | 断言 | bug 在场时 |
     * |---|---|
     * | 按钮点得动（`stop()` 不抛） | 绿 |
     * | `streaming` 翻回 false | 绿 |
     * | 输入框回来了（`sending == false`、有 `canReattach`） | 绿 |
     *
     * 三条全绿，所以停止的判据只能认远端产出的字节，机检那半只能认
     * 「`interrupt()` 被调到了」（[pressingStopSendsARealInterruptUpstream]）。
     *
     * 这条判据本身永远是绿的，它不抓回归，它把「这三条抓不到」钉在仓库里。删它之前先想清楚它守的是什么。
     *
     * 造「远端没收到」用的是可达的那条路（中断被拒：走 `TmuxSendKeysSink` 的用户每次按停止
     * 都落在这一格），而不是改产品代码。
     */
    @Test
    fun theThreeObviousStopAssertionsAllPassEvenWhenTheRemoteNeverHearsAboutIt() =
        runTest {
            // 造「远端收不到」：中断被拒 ⇒ 一个字节都没到远端
            val sink = StoppableSink(interruptOutcome = SendOutcome.Rejected("这条路停不了", retryable = false))
            val vm = ChatSession(sink)
            val incoming = MutableSharedFlow<BridgeFrame>()
            vm.start(incoming, smooth = false)
            runCurrent()
            incoming.emit(BridgeFrame.AssistantText("m", 0, null, "生成中…"))
            advanceUntilIdle()

            vm.stop()
            advanceUntilIdle()

            val s = vm.state.value
            assertFalse("① 「streaming 翻回 false」—— 远端没收到照样成立", s.streaming)
            assertFalse("② 「输入框回来了」（不在发送中）—— 远端没收到照样成立", s.sending)
            assertTrue("③ 「界面给得出下一步」—— 远端没收到照样成立", s.canReattach)
            // 这才是分界线：上面三条全绿，而远端那一轮压根没停。
            assertTrue(
                "阴性对照的要害：三条断言全绿，而远端从没确认过停止",
                vm.state.value.remoteStop !is RemoteStop.Confirmed,
            )
        }

    /**
     * 「投出去了」与「远端确认了」之间那道线。
     *
     * 把两者合并 = 按钮一按就说「已经停下」= 假报，而假报在 bug 存在时看起来完全正常。
     */
    @Test
    fun theStopLineNeverClaimsTheRemoteStoppedUntilTheRemoteSaysSo() =
        runTest {
            val sink = StoppableSink()
            val vm = ChatSession(sink)
            val incoming = MutableSharedFlow<BridgeFrame>()
            vm.start(incoming, smooth = false)
            runCurrent()

            vm.stop()
            advanceUntilIdle()
            val pending = vm.state.value
            assertEquals("① 中断投出去了、还没确认 ⇒ 只许说「正在让远端停下」", REMOTE_STOP_PENDING, sessionModeText(pending))
            assertTrue("前提：状态确实停在 Requested", pending.remoteStop is RemoteStop.Requested)

            // 远端自己说停了 —— 判据是它产出的那个串，app 伪造不出来
            incoming.emit(abortedResult())
            advanceUntilIdle()
            assertEquals("② 拿到远端的字节之后才许说「已经停下了」", REMOTE_STOP_CONFIRMED, sessionModeText(vm.state.value))
        }

    /**
     * 没按过停止时，同一条 `res` 不许被说成「按的那下起作用了」。
     *
     * 也可能是电脑上那个界面里按了停止，那时 `aborted_streaming` 照样会来。
     * 转移条件只看 `res.why` 的话就分不清这两种。
     */
    @Test
    fun anAbortThatWeDidNotAskForIsNotReportedAsOurStopSucceeding() =
        runTest {
            val vm = ChatSession(StoppableSink())
            val incoming = MutableSharedFlow<BridgeFrame>()
            vm.start(incoming, smooth = false)
            runCurrent()
            incoming.emit(abortedResult())
            advanceUntilIdle()

            assertEquals("没按过停止 ⇒ 状态必须还是 None", RemoteStop.None, vm.state.value.remoteStop)
            assertNull("不许冒出「远端那一轮已经停下了」", sessionModeText(vm.state.value))
            // 但那一轮是被打断的这件事仍然要说，且说人话
            assertEquals(RemoteStop.ABORTED_HUMAN, vm.state.value.failedWhy)
        }

    /**
     * 中断投不出去时退回本地停的那句，而且原因不许被吞。
     *
     * 走 `TmuxSendKeysSink` 的用户按停止拿到的正是这一格。
     */
    @Test
    fun anUndeliverableInterruptFallsBackToTheLocalStopSentenceAndKeepsTheReason() =
        runTest {
            val sink = StoppableSink(interruptOutcome = SendOutcome.Rejected("这条路停不了", retryable = false))
            val vm = ChatSession(sink)
            vm.start(MutableSharedFlow<BridgeFrame>(), smooth = false)
            runCurrent()

            vm.stop()
            advanceUntilIdle()

            val line = sessionModeText(vm.state.value)!!
            assertTrue("本地停的那句必须逐字还在：$line", line.contains(STOPPED_LOCALLY_ONLY))
            assertTrue("而且原因不许被吞掉：$line", line.contains("这条路停不了"))
        }

    /**
     * 再按一次停止 = 「不等了」，退回本地停显示。
     *
     * 不许写成 `return`：中断投出去而远端一直不确认时，屏幕会永远停在「正在让远端停下…」，
     * 没有任何动作能离开它。也不许发第二条中断（既不去重也不等答复）。
     */
    @Test
    fun aSecondStopPressGivesUpWaitingInsteadOfDeadEndingTheScreen() =
        runTest {
            val sink = StoppableSink()
            val vm = ChatSession(sink)
            vm.start(MutableSharedFlow<BridgeFrame>(), smooth = false)
            runCurrent()

            vm.stop()
            advanceUntilIdle()
            assertEquals("前提：第一下投出去了", 1, sink.interrupts)
            assertFalse("前提：第一下不停本地显示（否则看不见远端的确认帧）", vm.state.value.stoppedByUser)

            vm.stop()
            advanceUntilIdle()
            assertEquals("第二下不许再投一条", 1, sink.interrupts)
            assertTrue("第二下要把显示停住，别把用户困在「正在让远端停下…」", vm.state.value.stoppedByUser)
            assertEquals("而且说的是本地停那句：此刻它字面为真", STOPPED_LOCALLY_ONLY, sessionModeText(vm.state.value))
        }

    /** 上一轮的停止结论不许挂到下一轮上 —— 发新消息那一刻「那一轮」就已经不是它了。 */
    @Test
    fun theStopVerdictDoesNotLeakIntoTheNextTurn() =
        runTest {
            val sink = StoppableSink()
            val vm = ChatSession(sink)
            val incoming = MutableSharedFlow<BridgeFrame>()
            vm.start(incoming, smooth = false)
            runCurrent()
            vm.stop()
            advanceUntilIdle()
            incoming.emit(abortedResult())
            advanceUntilIdle()
            assertEquals("前提：上一轮确实被确认停了", REMOTE_STOP_CONFIRMED, sessionModeText(vm.state.value))

            vm.send("再来一句")
            advanceUntilIdle()
            assertEquals("新一轮开始 ⇒ 上一轮的停止结论必须清掉", RemoteStop.None, vm.state.value.remoteStop)
        }

    // ---- 开屏与「重新接上内容」是同一个入口 ----------------------------

    /**
     * 第一次是 `start`，之后是 `reattach`，判定在 [ChatSession.openOrReattach] 一处。
     *
     * 判据钉的是第二次真的又收到了内容。写成「调了两次不抛异常」抓不到任何东西：
     * `start()` 首行就是 `if (started) return`，第二次会静默什么都不做。
     */
    @Test
    fun openOrReattachStartsTheFirstTimeAndReattachesAfterwards() =
        runTest {
            val vm = ChatSession(StoppableSink())
            vm.openOrReattach(flowOf(BridgeFrame.AssistantText("a", 0, null, "第一条流")), smooth = false)
            advanceUntilIdle()
            assertTrue(
                "前提：第一条流收到了",
                vm.state.value.units
                    .isNotEmpty(),
            )

            vm.openOrReattach(flowOf(BridgeFrame.AssistantText("b", 0, null, "第二条流")), smooth = false)
            advanceUntilIdle()

            val texts =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.AssistantMarkdown>()
                    .map { it.markdown }
            assertTrue("第二次必须真的又接上了（`start()` 的 started 守卫会静默吃掉它）：$texts", texts.any { it.contains("第二条流") })
        }

    /**
     * 门打开之后，上一趟那句人话必须消失。
     *
     * 账号门那句话走 `reportStartFailure` 落进 `downlinkError`；首次 `start()` 若不清它，
     * 「没选账号 ⇒ 去设置里填好 ⇒ 回来」之后管道明明起来了、内容也在流，
     * 屏幕上那句「这台机器还没选账号」还挂着。
     */
    @Test
    fun aFixedGateDoesNotLeaveItsHumanSentenceStuckOnScreen() =
        runTest {
            val vm = ChatSession(StoppableSink())
            vm.reportStartFailure("这台机器还没选账号；去顶栏「切换机器」…")
            assertNotNull("前提：那句话确实上屏了", vm.state.value.downlinkError)

            vm.openOrReattach(MutableSharedFlow<BridgeFrame>(), smooth = false)
            runCurrent()

            assertNull("门修好、这一趟真起来了 ⇒ 上一趟那句话不许留着", vm.state.value.downlinkError)
        }

    /**
     * `interrupt` 的 `res`：`ok=false` + `why=aborted_streaming`，与 golden 同形。
     *
     * 见 `bridge/vectors/interrupt.frames.ndjson`（`"ok":false,"why":"aborted_streaming"`，
     * 且 `raw.result` 是 `null`，上游没给人话）。
     */
    private fun abortedResult() =
        BridgeFrame.Result(
            sid = "s",
            ok = false,
            cost = null,
            turns = 1,
            why = RemoteStop.ABORTED_STREAMING,
            api = null,
            dur = null,
            usage = null,
            den = null,
            mu = null,
            raw = null,
        )

    /**
     * 会数中断次数的假 sink。
     *
     * 与 [SendOnlySink] 相反：这一条专门用来测停止，所以它 `interrupt()` 有真行为。
     */
    private class StoppableSink(
        private val interruptOutcome: SendOutcome = SendOutcome.Accepted,
    ) : UplinkSink {
        override val echoesBack = false
        var interrupts = 0
            private set

        override suspend fun send(request: SendRequest): SendOutcome = SendOutcome.Accepted

        override suspend fun interrupt(): SendOutcome {
            interrupts++
            return interruptOutcome
        }
    }

    /**
     * `res.ok=false` 但没有 `why` 时，失败仍必须可见。
     *
     * 编码器的 `_fr()` 会丢掉值为 None 的键，`terminal_reason` 缺失时 `why` 根本不存在；
     * 读成 `?.why` 的话 `failedWhy = null`，失败的一轮看起来像正常结束。
     */
    @Test
    fun failureWithoutATerminalReasonIsStillVisible() =
        runTest {
            val vm = ChatSession()
            vm.start(
                flowOf(BridgeFrame.Result("s", ok = false, null, 1, null, null, null, null, null, null, null)),
                smooth = false,
            )
            advanceUntilIdle()

            assertNotNull("失败不许静默（loud never silent）", vm.state.value.failedWhy)
        }

    /** 认证失败：`err` 帧的 code 应当成为失败文案的兜底。 */
    @Test
    fun authFailureUsesTheErrorCodeWhenNoTerminalReasonIsGiven() =
        runTest {
            val vm = ChatSession()
            vm.start(frames("auth-fail"), smooth = false)
            advanceUntilIdle()
            assertNotNull("认证失败必须有可见文案", vm.state.value.failedWhy)
        }

    // ---- 本地回显 ---------------------------------------------------------------

    @Test
    fun localEchoPutsTheUserMessageOnScreenWithoutSending() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(), smooth = false)
            vm.send("帮我看看这个 bug")
            advanceUntilIdle()

            assertTrue(
                "用户消息应上屏",
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .any { it.text == "帮我看看这个 bug" },
            )
        }

    @Test
    fun blankInputIsIgnored() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(), smooth = false)
            vm.send("   ")
            advanceUntilIdle()
            assertTrue(
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .isEmpty(),
            )
        }

    // ---- 演示素材 ---------------------------------------------------------------

    /** 演示素材：工具卡要真的产出（否则演示画面里只有一段文字，看不出 Claude 干了活）。 */
    @Test
    fun demoVectorProducesToolCards() =
        runTest {
            val vm = ChatSession()
            vm.start(frames("demo-debug"), smooth = false, prompt = "跑测试发现算错了")
            advanceUntilIdle()

            val units = vm.state.value.units
            assertTrue("应有用户提问", units.filterIsInstance<RenderUnit.UserText>().isNotEmpty())
            val tools = units.filterIsInstance<RenderUnit.ToolCall>()
            assertTrue("应有工具调用，实际 ${tools.size}", tools.size >= MIN_DEMO_TOOLS)
            assertTrue("演示要含一次真的 Edit", tools.any { it.name == "Edit" })
            assertTrue("工具都应拿到结果", tools.count { !it.pending } >= MIN_DEMO_TOOLS)
        }

    /**
     * 流里没有 `res` 收尾时不许挂死。
     *
     * tickJob 若写成只在收到 `res` 时才取消的 `while (true) { delay(100) }`，截断的 vector 或断网
     * 就会漏一个 10Hz 常驻协程，测试里表现为 `advanceUntilIdle()` 永久死转。
     */
    @Test
    fun aStreamThatNeverSendsResultStillTerminatesInsteadOfSpinningForever() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(BridgeFrame.TextDelta("msg_1", 0, "没有 res 收尾")), smooth = true)
            advanceUntilIdle() // ← 常驻协程漏掉的话这里永久挂死
            assertTrue("正文仍应补齐", vm.state.value.textLen() > 0)
        }

    // ---- 上滑翻历史 --------------------------------------------------------

    /**
     * 分两段载 == 一次全载（ViewModel 层的端到端版）。
     *
     * core-claude 侧已在两个生产者上各钉了一条；这里钉的是经过 ViewModel 的缓释与截断之后结果仍然一致：
     * `publish()` 会按 `streamingKey` 截断正文，翻页时若截错了块，正文就会缺一段。
     */
    @Test
    fun pagedLoadingEndsUpIdenticalToLoadingEverythingAtOnce() =
        runTest {
            val all = rows("tool-call").map { it.frame }
            val splitAt = all.size / 2

            val whole = ChatSession()
            whole.start(flowOf(*all.toTypedArray()), smooth = false)
            advanceUntilIdle()

            val paged = ChatSession()
            paged.start(flowOf(*all.drop(splitAt).toTypedArray()), smooth = false)
            advanceUntilIdle()
            paged.attachHistoryPaging { all.take(splitAt) }
            paged.loadOlder()
            advanceUntilIdle()

            assertEquals(
                "分两段载与一次全载的 key 序列必须逐项相等",
                whole.state.value.units
                    .map { it.key },
                paged.state.value.units
                    .map { it.key },
            )
            assertEquals(whole.state.value.units, paged.state.value.units)
        }

    /** 加载器返回空 ⇒ 关掉 `canLoadOlder`，UI 才能显示「已是最早」而不是无限转圈。 */
    @Test
    fun exhaustingHistoryFlipsTheTerminalStateInsteadOfSpinningForever() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(), smooth = false)
            var calls = 0
            vm.attachHistoryPaging {
                calls++
                emptyList()
            }
            assertTrue("接上翻页后应先认为还有更早的", vm.state.value.canLoadOlder)

            vm.loadOlder()
            advanceUntilIdle()
            assertFalse("到顶后必须关掉，否则 UI 会一直请求", vm.state.value.canLoadOlder)

            vm.loadOlder()
            advanceUntilIdle()
            assertEquals("到顶之后不许再请求", 1, calls)
        }

    /** 防重入：连点/连续滚动不许并发拉同一段。 */
    @Test
    fun concurrentLoadOlderRequestsAreCoalesced() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(), smooth = false)
            var calls = 0
            vm.attachHistoryPaging {
                calls++
                kotlinx.coroutines.delay(THREE_HUNDRED_MS)
                listOf(BridgeFrame.AssistantText("msg_old$calls", 0, null, "老内容 $calls"))
            }

            vm.loadOlder()
            vm.loadOlder()
            vm.loadOlder()
            advanceUntilIdle()

            assertEquals("三次请求只该真的拉一次", 1, calls)
        }

    /**
     * 加载器抛异常时必须复位 `loadingOlder`。
     *
     * 不复位的话，一次网络抖动就把翻页永久卡死在「正在载入…」——
     * 而且因为防重入闸还开着，之后再也不会重试。
     */
    @Test
    fun aFailingLoaderDoesNotWedgePagingForever() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(), smooth = false)
            var attempts = 0
            vm.attachHistoryPaging {
                attempts++
                if (attempts == 1) error("网络抖了一下") else listOf(BridgeFrame.AssistantText("msg_o", 0, null, "拿到了"))
            }

            vm.loadOlder()
            advanceUntilIdle()
            assertFalse("失败后不许卡在「正在载入」", vm.state.value.loadingOlder)
            assertNotNull("失败必须可见", vm.state.value.historyError)

            vm.loadOlder()
            advanceUntilIdle()
            assertEquals("必须还能重试", 2, attempts)
        }

    /** 老段插进来后应排在前面（这是「翻历史」的字面意思）。 */
    @Test
    fun olderContentIsPrependedNotAppended() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(BridgeFrame.AssistantText("msg_new", 0, null, "新的回答")), smooth = false)
            advanceUntilIdle()
            vm.attachHistoryPaging { listOf(BridgeFrame.AssistantText("msg_old", 0, null, "更早的回答")) }
            vm.loadOlder()
            advanceUntilIdle()

            assertEquals(
                listOf("更早的回答", "新的回答"),
                vm.state.value.units
                    .filterIsInstance<RenderUnit.AssistantMarkdown>()
                    .map { it.markdown },
            )
        }

    // ---- 上行发送 ----------------------------------------------------------

    // ---- 锚点 × 翻页 × 本地消息的交叉路径 ----

    /**
     * 翻完历史之后，用户自己那条提问必须还在正确位置。
     *
     * `shiftLocalAnchors(added)` 容易混用两个计量单位：`anchor` 是 `units()` 的下标，
     * 而 `prependFrames` 返回的是 `order` 里新增的 key 数。两者不等（`toUnit()` 对「正文为空的块」
     * 和「没见过工具身份的块」返回 null），混用会把提问挤到整段助手回答之后。
     *
     * 判据用「分两段载 == 一次全载」：一次全载 vs 先载尾段再翻历史，本地消息所在的位置必须相同。
     */
    @Test
    fun aLocalMessageKeepsItsPositionAfterPagingThroughHistory() =
        runTest {
            val all = rows("demo-debug").map { it.frame }
            val prompt = "用户最初问的那句"

            // 参照组：一次全载
            val whole = ChatSession()
            whole.start(flowOf(*all.toTypedArray()), smooth = false, prompt = prompt)
            advanceUntilIdle()
            val expectedIndex =
                whole.state.value.units
                    .indexOfFirst { it.key.startsWith(ChatSession.LOCAL_KEY_PREFIX) }
            assertTrue("前提：一次全载时本地消息要能定位到", expectedIndex >= 0)

            // 实验组：只载尾段，然后一页页翻回去
            val recentFrom = all.size - RECENT_FRAMES
            assertTrue("前提：demo-debug 必须真的够长到会分页（实际 ${all.size} 帧）", recentFrom > 0)
            val paged = ChatSession()
            paged.start(flowOf(*all.drop(recentFrom).toTypedArray()), smooth = false, prompt = prompt)
            advanceUntilIdle()

            var cursor = recentFrom
            paged.attachHistoryPaging {
                if (cursor <= 0) {
                    null
                } else {
                    val from = (cursor - PAGE_FRAMES).coerceAtLeast(0)
                    all.subList(from, cursor).also { cursor = from }
                }
            }
            var pages = 0
            while (paged.state.value.canLoadOlder && pages < MAX_PAGES) {
                paged.loadOlder()
                advanceUntilIdle()
                pages++
            }
            assertTrue("前提：必须真的翻了好几页（实际 $pages）", pages >= 2)

            val actualIndex =
                paged.state.value.units
                    .indexOfFirst { it.key.startsWith(ChatSession.LOCAL_KEY_PREFIX) }
            assertEquals(
                "翻完历史后本地消息的位置必须与一次全载相同 —— " +
                    "错位说明锚点平移又混用了计量单位（units 下标 vs order key 数）",
                expectedIndex,
                actualIndex,
            )
            assertEquals(
                "分两段载与一次全载的 key 序列应逐项相等",
                whole.state.value.units
                    .map { it.key },
                paged.state.value.units
                    .map { it.key },
            )
        }

    /**
     * 锚点的两个方向必须同时守住：pinned 不动、非 pinned 随 prepend 后移。
     *
     * `aLocalMessageKeepsItsPositionAfterPagingThroughHistory` 守的是 pinned 不该被平移；
     * 这一条再钉「非 pinned 必须后移」与 `loadOlder()` 里按 units 差值平移那一行。
     * 把整个 `shiftLocalAnchors` 改成 no-op 时这条会红。
     *
     * `prompt` 那条恒在 0，`send()` 那条恒在末尾。
     */
    @Test
    fun prependShiftsSentMessagesButNeverThePinnedPrompt() =
        runTest {
            val vm = ChatSession()
            vm.start(
                flowOf(BridgeFrame.AssistantText("msg_new", 0, null, "已经在屏上的回答")),
                smooth = false,
                prompt = "原始提问",
            )
            advanceUntilIdle()

            vm.send("我此刻打的字")
            advanceUntilIdle()
            assertEquals(
                "前提：翻页前顺序应是 原始提问 / 回答 / 此刻打的字",
                listOf("原始提问", "已经在屏上的回答", "我此刻打的字"),
                vm.state.value.units
                    .map { it.textOf() },
            )

            var served = false
            vm.attachHistoryPaging {
                if (served) {
                    null
                } else {
                    served = true
                    listOf(BridgeFrame.AssistantText("msg_old", 0, null, "更早的回答"))
                }
            }
            vm.loadOlder()
            advanceUntilIdle()

            assertEquals(
                "pinned 的原始提问仍在最前；此刻打的字必须跟着后移、留在末尾",
                listOf("原始提问", "更早的回答", "已经在屏上的回答", "我此刻打的字"),
                vm.state.value.units
                    .map { it.textOf() },
            )
        }

    private fun RenderUnit.textOf(): String =
        when (this) {
            is RenderUnit.UserText -> text
            is RenderUnit.AssistantMarkdown -> markdown
            else -> "?"
        }

    // ---- 回声认领 / 在飞门禁 / key 空间 -------------------------------------

    /**
     * 对端会回显时不做乐观回显。
     *
     * `echoesBack=true` 的路径下认领不了（回声帧没有 id、TUI 回显里也塞不进 localId），
     * 所以宁可等一个往返再上屏，也不要同一条消息显示两遍。
     *
     * 两种 sink 对照：只测一种的话 `echoesBack` 就成了死参数。
     */
    @Test
    fun anEchoingSinkSuppressesOptimisticEchoWhileANonEchoingOneDoesNot() =
        runTest {
            val echoing = ChatSession(FakeSink(mutableListOf(), echoesBack = true))
            echoing.start(flowOf(), smooth = false)
            echoing.send("会被回显的")
            advanceUntilIdle()
            assertTrue(
                "会回显的路径：成功之后不该有本地那条（等回声帧上屏）",
                echoing.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .isEmpty(),
            )

            val direct = ChatSession(FakeSink(mutableListOf(), echoesBack = false))
            direct.start(flowOf(), smooth = false)
            direct.send("不会被回显的")
            advanceUntilIdle()
            assertEquals(
                "不回显的路径：本地那条就是最终形态",
                1,
                direct.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .size,
            )
        }

    /** 回显路径下失败仍必须上屏，否则写的那段话连同失败一起静默消失。 */
    @Test
    fun anEchoingSinkStillShowsTheMessageWhenSendingFails() =
        runTest {
            val vm = ChatSession(FakeSink(mutableListOf(SendOutcome.Rejected("没送出去")), echoesBack = true))
            vm.start(flowOf(), smooth = false)
            vm.send("发失败的这段话")
            advanceUntilIdle()

            val msg =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .single()
            assertEquals("失败的消息不许消失", "发失败的这段话", msg.text)
            assertEquals(DeliveryState.FAILED, msg.delivery)
        }

    /**
     * 回显路径下也要有在飞门禁与「停止」键。
     *
     * `echoesBack=true` 时那条 `LocalMessage` 成功后从不进 `localMessages`（等回声帧上屏），
     * 所以「扫 `localMessages` 找 SENDING」的写法在这条路径上什么都拦不住，
     * `sending` 也会是 false ⇒ 连停止键都不出现。判据必须是独立计数器。
     */
    @Test
    fun anEchoingSinkStillGatesReentryAndShowsSendingState() =
        runTest {
            val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
            var calls = 0
            val sink =
                object : SendOnlySink() {
                    override val echoesBack = true

                    override suspend fun send(request: SendRequest): SendOutcome {
                        calls++
                        gate.await() // 卡住，制造「在飞」
                        return SendOutcome.Accepted
                    }
                }
            val vm = ChatSession(sink)
            vm.start(flowOf(), smooth = false)

            vm.send("第一条")
            runCurrent()
            assertTrue("回显路径下也必须报 sending（否则没有停止键）", vm.state.value.sending)

            vm.send("第二条") // 应被门禁挡住
            runCurrent()
            assertEquals("在飞时只该发出去一条", 1, calls)

            gate.complete(Unit)
            advanceUntilIdle()
            assertFalse("送完之后不该还报 sending", vm.state.value.sending)
        }

    /**
     * 发送完成后计数器落回 0（正常路径）。
     *
     * 判别力边界：这条走的是正常路径。`stop()` 只 cancel `feedJob`/`tickJob`，
     * 而 `deliver()` 起的是独立的 `viewModelScope.launch`，所以生产代码里专为取消写的
     * `finally { inFlight-- }` 这里覆盖不到；要测它得 cancel 整个 `viewModelScope`。
     */
    @Test
    fun theInFlightGateReturnsToZeroAfterASendCompletes() =
        runTest {
            val vm =
                ChatSession(
                    object : SendOnlySink() {
                        override val echoesBack = false

                        override suspend fun send(request: SendRequest): SendOutcome {
                            kotlinx.coroutines.delay(LONG_ENOUGH)
                            return SendOutcome.Accepted
                        }
                    },
                )
            vm.start(flowOf(), smooth = false)
            vm.send("会被取消的")
            runCurrent()
            assertTrue("前提：此刻确实在飞", vm.state.value.sending)

            vm.stop() // 取消 viewModelScope 里的活儿
            advanceUntilIdle()
            assertFalse("取消后计数器必须复位，否则再也发不出去", vm.state.value.sending)
        }

    /** 在飞门禁在 VM 里，不靠「草稿清空 ⇒ 按钮 disabled」。 */
    @Test
    fun sendingWhileOneIsInFlightIsRejectedByTheViewModelItself() =
        runTest {
            val sink = FakeSink(mutableListOf(SendOutcome.Accepted, SendOutcome.Accepted))
            val vm = ChatSession(sink)
            vm.start(flowOf(), smooth = false)

            vm.send("第一条") // 立刻进 SENDING
            vm.send("第二条") // ← 应被 VM 挡掉，而不是靠 UI
            advanceUntilIdle()

            assertEquals("在飞时只该发出去一条", 1, sink.requests.size)
            assertEquals("第一条", sink.requests.single().text)
        }

    /**
     * 本地 key 与另外两个 key 空间三方互不相交（重复 key 会崩 `LazyColumn`）。
     *
     * 判据是代码性质（具名前缀常量）而不是「uuid 碰巧不长这样」。
     */
    @Test
    fun localKeysNeverCollideWithEitherProducerKeySpace() =
        runTest {
            val vm = ChatSession()
            vm.start(frames("tool-call"), smooth = false, prompt = "本地这条")
            advanceUntilIdle()

            val all =
                vm.state.value.units
                    .map { it.key }
            val local = all.filter { it.startsWith(ChatSession.LOCAL_KEY_PREFIX) }
            val fromFrames = all.filter { it.startsWith(ChatTurnAssembler.KEY_PREFIX) }
            assertTrue("前提：两类 key 都要真的出现（实际 local=${local.size} frame=${fromFrames.size}）", local.isNotEmpty() && fromFrames.isNotEmpty())
            assertEquals("key 不许重复", all.size, all.toSet().size)
            assertTrue(
                "本地前缀不许落进帧侧空间",
                local.none { it.startsWith(ChatTurnAssembler.KEY_PREFIX) },
            )
            assertTrue(
                "本地前缀不许长得像 classifier 的 uuid#i",
                local.none { Regex("^[0-9a-f]{8}-[0-9a-f]{4}-").containsMatchIn(it) },
            )
        }

    // ---- 接上真 sink（echoesBack=true）才可达的两条 ---------------------

    /**
     * echo 路径下失败 late-add 的那条消息，锚点也必须随 prepend 平移。
     *
     * 它在 `send()` 里构造（`anchor = units().size`）但不进 `localMessages`，
     * 于是 `shiftLocalAnchors` 遍历不到它；等失败时才 late-add，anchor 已经过期。
     * 表现：此刻打的字浮到打字时屏幕上已经有的回答之前。
     */
    @Test
    fun anEchoPathFailureKeepsItsAnchorInSyncAcrossPrepend() =
        runTest {
            val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
            val sink =
                object : SendOnlySink() {
                    override val echoesBack = true

                    override suspend fun send(request: SendRequest): SendOutcome {
                        gate.await()
                        return SendOutcome.Rejected("没送出去")
                    }
                }
            val vm = ChatSession(sink)
            vm.start(flowOf(BridgeFrame.AssistantText("msg_new", 0, null, "已经在屏上的回答")), smooth = false)
            advanceUntilIdle()

            vm.send("我此刻打的字") // 在飞，尚未进 localMessages
            runCurrent()

            var served = false
            vm.attachHistoryPaging {
                if (served) null else listOf(BridgeFrame.AssistantText("msg_old", 0, null, "更早的回答")).also { served = true }
            }
            vm.loadOlder()
            advanceUntilIdle()

            gate.complete(Unit) // 现在才失败 → late-add
            advanceUntilIdle()

            assertEquals(
                "late-add 的失败消息必须排在它打字时已有内容之后",
                listOf("更早的回答", "已经在屏上的回答", "我此刻打的字"),
                vm.state.value.units
                    .map { it.textOf() },
            )
        }

    /**
     * echo 路径「失败 → 重试成功 → 回声帧到达」不许显示两遍。
     *
     * `retry()` 走的是 `showLocally = true`，重试成功后消息仍留在 `localMessages`；
     * 随后对端回声以 `f:ut#…` 的 key 到达 ⇒ 两条不同 key、同样正文。
     */
    @Test
    fun anEchoPathRetrySuccessDoesNotDuplicateOnceTheEchoArrives() =
        runTest {
            val sink = FakeSink(mutableListOf(SendOutcome.Rejected("第一次失败"), SendOutcome.Accepted), echoesBack = true)
            val vm = ChatSession(sink)
            // 用可控的 flow 源，这样回声帧能在「重试成功之后」才到 —— 不加测试专用 API
            val incoming = kotlinx.coroutines.flow.MutableSharedFlow<BridgeFrame>(extraBufferCapacity = 8)
            vm.start(incoming, smooth = false)
            advanceUntilIdle()

            vm.send("重复的这句话")
            advanceUntilIdle()
            val failed =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .single()
            assertEquals("前提：第一次必须真的失败并上屏", DeliveryState.FAILED, failed.delivery)

            vm.retry(failed.key)
            advanceUntilIdle()

            // 对端的回声帧到达（TUI 把输入回显出来）
            incoming.emit(BridgeFrame.UserText(null, "重复的这句话"))
            advanceUntilIdle()

            val texts =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .map { it.text }
            assertEquals("重试成功后回声到达，不许显示两遍：$texts", listOf("重复的这句话"), texts)
        }

    /**
     * 只测「发出去」那半的判据用它，[interrupt] 一律拒绝。
     *
     * 生产那侧 `UplinkSink.interrupt` 不给默认实现，好让编译器逼每个真实现表态。
     * 测试这侧集中一处、且拒绝（不是静默 `Accepted`）：哪条判据不小心走到停止路径，它会红，不会假绿。
     */
    private abstract class SendOnlySink : UplinkSink {
        override suspend fun interrupt(): SendOutcome = SendOutcome.Rejected("这条假 sink 不测停止", retryable = false)
    }

    /** 可控的假 sink：按队列返回结果，并记录每次请求（供断言「重试用的是同一个 id」）。 */
    private class FakeSink(
        private val outcomes: MutableList<SendOutcome>,
        override val echoesBack: Boolean = false,
    ) : SendOnlySink() {
        val requests = mutableListOf<SendRequest>()

        override suspend fun send(request: SendRequest): SendOutcome {
            requests += request
            return outcomes.removeFirstOrNull() ?: SendOutcome.Accepted
        }
    }

    /** 乐观回显：点发送立刻上屏，不等对端。 */
    @Test
    fun sendingShowsTheMessageImmediatelyWithoutWaitingForTheSink() =
        runTest {
            val sink = FakeSink(mutableListOf())
            val vm = ChatSession(sink)
            vm.start(flowOf(), smooth = false)
            vm.send("先上屏再说")

            // 这里刻意不 advanceUntilIdle —— 要验的正是「不等对端」
            assertTrue(
                "点了发送就该立刻看见",
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .any { it.text == "先上屏再说" },
            )
            assertEquals(
                "此刻应处于发送中",
                DeliveryState.SENDING,
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .first()
                    .delivery,
            )
            advanceUntilIdle()
        }

    /**
     * 发送失败：消息留在 transcript 里且标为可重试，不许静默消失。
     *
     * 写了一段长 prompt，失败后要能看见、能重发。
     */
    @Test
    fun aFailedSendKeepsTheMessageOnScreenAndMarksItRetryable() =
        runTest {
            val sink = FakeSink(mutableListOf(SendOutcome.Rejected("网络抖了")))
            val vm = ChatSession(sink)
            vm.start(flowOf(), smooth = false)
            vm.send("一段很长的 prompt")
            advanceUntilIdle()

            val msg =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .single()
            assertEquals("消息不许消失", "一段很长的 prompt", msg.text)
            assertEquals(DeliveryState.FAILED, msg.delivery)
            // 原因必须挂在这条消息上，不是全屏单槽：
            //   单槽会「A 失败、B 成功后被擦掉」，而 A 仍显示未送达。
            assertEquals("失败原因要贴在这条消息上", "网络抖了", msg.deliveryError)
        }

    /**
     * 两条消息交错时，失败原因不许互相覆盖。
     *
     * 全屏单槽的话「A 失败 → B 成功」会把 A 的错误擦掉，而 A 的气泡仍写着「未送达」。
     */
    @Test
    fun oneMessageSucceedingDoesNotEraseAnotherMessagesFailureReason() =
        runTest {
            val sink = FakeSink(mutableListOf(SendOutcome.Rejected("A 的原因"), SendOutcome.Accepted))
            val vm = ChatSession(sink)
            vm.start(flowOf(), smooth = false)
            vm.send("A")
            advanceUntilIdle()
            vm.send("B")
            advanceUntilIdle()

            val msgs =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
            assertEquals("前提：两条都在", 2, msgs.size)
            val a = msgs.first { it.text == "A" }
            assertEquals("A 仍是失败", DeliveryState.FAILED, a.delivery)
            assertEquals("A 的原因不许被 B 的成功擦掉", "A 的原因", a.deliveryError)
            assertNull("B 成功后不该有失败原因", msgs.first { it.text == "B" }.deliveryError)
        }

    /** 重试不产生第二条：用同一个本地 id（重复 key 会当场崩 `LazyColumn`）。 */
    @Test
    fun retryingUsesTheSameLocalIdSoNoDuplicateAppears() =
        runTest {
            val sink = FakeSink(mutableListOf(SendOutcome.Rejected("第一次失败"), SendOutcome.Accepted))
            val vm = ChatSession(sink)
            vm.start(flowOf(), smooth = false)
            vm.send("重试我")
            advanceUntilIdle()

            val failed =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .single()
            assertEquals("前提：第一次必须真的失败了", DeliveryState.FAILED, failed.delivery)

            vm.retry(failed.key)
            advanceUntilIdle()

            val after =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
            assertEquals("重试不许多出一条：${after.map { it.text }}", 1, after.size)
            // `Accepted` 只是「送出去了但未确认」（pane 在 copy-mode 时 `send-keys` 照样退 0），
            //   所以落点是 `SENT_UNCONFIRMED` 而不是 `null`（`null` = 气泡下什么都不显示 = 看着像已送达）。
            //   这里断的是「不再是失败态」。
            assertEquals(
                "重试成功后不该还是失败态",
                DeliveryState.SENT_UNCONFIRMED,
                after.single().delivery,
            )
            assertEquals(
                "两次请求必须用同一个 localId",
                1,
                sink.requests
                    .map { it.localId }
                    .toSet()
                    .size,
            )
        }

    /** 不可重试的失败不给重试按钮的依据（`FAILED_PERMANENT`），且 retry 是 no-op。 */
    @Test
    fun aPermanentFailureIsNotRetryable() =
        runTest {
            val sink = FakeSink(mutableListOf(SendOutcome.Rejected("权限被拒", retryable = false)))
            val vm = ChatSession(sink)
            vm.start(flowOf(), smooth = false)
            vm.send("会被拒的")
            advanceUntilIdle()

            val msg =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .single()
            assertEquals(DeliveryState.FAILED_PERMANENT, msg.delivery)

            vm.retry(msg.key)
            advanceUntilIdle()
            assertEquals("永久失败不许重发", 1, sink.requests.size)
        }

    /** sink 违约抛异常时不许弄崩页面，且要显示成失败（可重试）。 */
    @Test
    fun aSinkThatThrowsIsTreatedAsAFailureNotACrash() =
        runTest {
            val sink =
                object : SendOnlySink() {
                    override val echoesBack = false

                    override suspend fun send(request: SendRequest): SendOutcome = error("实现违约了")
                }
            val vm = ChatSession(sink)
            vm.start(flowOf(), smooth = false)
            vm.send("会炸的")
            advanceUntilIdle()

            assertEquals(
                DeliveryState.FAILED,
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .single()
                    .delivery,
            )
        }

    /** 没接上行时退化为「只回显」。 */
    @Test
    fun withoutAnUplinkSendingStillEchoesLocally() =
        runTest {
            val vm = ChatSession()
            vm.start(flowOf(), smooth = false)
            vm.send("只回显")
            advanceUntilIdle()

            val msg =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .single()
            assertEquals("只回显", msg.text)
            assertNull("没接上行就不该有发送态", msg.delivery)
            assertEquals(false, vm.state.value.uplinkAttached)
        }

    /**
     * 权限模式取自 `init.raw.permissionMode`（单数 camelCase），用真 golden 验，不手造键。
     *
     * golden 的 `init.raw` 里只有 `permissionMode`；手造一个不存在的键，生产代码读到的永远是 null，
     * 测试却照绿。喂真 golden，键名一旦对不上就会红。
     */
    @Test
    fun permissionModeComesFromTheRealInitFrame() =
        runTest {
            val vm = ChatSession()
            vm.start(frames("text-short"), smooth = false)
            advanceUntilIdle()

            assertEquals("真 golden 的 init 报的是 default", "default", vm.state.value.permissionMode)
        }

    /** 远端没报权限模式时不许我们编一个 —— null 就是 null，UI 据此不显示。 */
    @Test
    fun aMissingPermissionModeStaysNullInsteadOfBeingInvented() =
        runTest {
            val vm = ChatSession()
            vm.start(
                flowOf(
                    BridgeFrame.Init(
                        "s",
                        null,
                        null,
                        emptyList(),
                        emptyList(),
                        emptyList(),
                        emptyList(),
                        emptyList(),
                        emptyList(),
                        raw = null,
                    ),
                ),
                smooth = false,
            )
            advanceUntilIdle()
            assertNull("远端没给就该是 null", vm.state.value.permissionMode)
        }

    private companion object {
        /** 性能类判据的上限：挂死比失败糟。 */
        const val TEN_SECONDS = 10_000L
        const val TWO_SECONDS = 2_000L

        /** 跨真实调度器那条判据的有界等待。 */
        const val WAIT_BUDGET_MS = 5_000L
        const val POLL_MS = 10L
        const val THREE_HUNDRED_MS = 300L
        const val LONG_ENOUGH = 5_000L
        const val ONE_TICK = 100L
        const val TICKS = 30
        const val MIN_DEMO_TOOLS = 5

        /** 与 `ChatReplayActivity` 的演示切页参数一致：测的就是那条真机路径。 */
        const val RECENT_FRAMES = 40
        const val PAGE_FRAMES = 25
        const val MAX_PAGES = 50
    }
}
