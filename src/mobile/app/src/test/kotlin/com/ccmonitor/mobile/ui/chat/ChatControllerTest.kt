package com.ccmonitor.mobile.ui.chat

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import com.ccmonitor.mobile.core.claude.bridge.BridgeFrame
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.core.claude.transport.SignalSource
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** 应用级对话持有者。 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatControllerTest {
    /**
     * 用 `backgroundScope` 而不是测试体自己的 scope：
     * controller 的 `watchers`（盯 state 刷 `hasInFlightTurn`）与各对话的 feed 都是常驻协程，
     * 挂在测试 scope 上会让 `runTest` 一直等它们、最后报 `UncompletedCoroutinesError`。
     * 这是测试装配的问题，不是生产代码的问题：那些协程本来就该长活。
     */
    private fun TestScope.controller() = ChatController(scopeFactory = { backgroundScope })

    private fun at(x: String) = BridgeFrame.AssistantText(m = "m1", i = 0, p = null, x = x)

    /** 一条还没收口的轮次：先给正文，再挂很久。用来造「在飞」这个状态。 */
    private fun longRunningTurn(text: String = "生成中") =
        flow {
            emit(at(text))
            delay(LONG_TURN_MS)
            emit(res())
        }

    private fun RenderUnit.hasText(needle: String) = this is RenderUnit.AssistantMarkdown && markdown.contains(needle)

    private fun res() =
        BridgeFrame.Result(
            sid = null,
            ok = true,
            cost = null,
            turns = null,
            why = null,
            api = null,
            dur = null,
            usage = null,
            den = null,
            mu = null,
            raw = emptyMap(),
        )

    /**
     * 同一个 key 拿到同一个对话。
     *
     * 不幂等的话，每次进聊天屏都会新建一个 `ChatSession` ⇒ 下行从头再来、
     * 上一个还在后台收着没人管，多出一份泄漏。
     */
    @Test
    fun theSameKeyAlwaysYieldsTheSameConversation() =
        runTest {
            val c = controller()
            assertSame("同 key 必须是同一个", c.sessionFor("a"), c.sessionFor("a"))
            assertNotSame("不同 key 是不同对话", c.sessionFor("a"), c.sessionFor("b"))
        }

    /**
     * 屏幕没了，下行不能停：对话由应用级持有者持有，不挂在 `viewModelScope` 上。
     *
     * 判据挑得硬一点：不是「state 还在」（那只说明对象没被清），
     * 而是 VM 丢掉之后才发出的那一帧仍然到达。前者对着一个死掉的 feed 也成立。
     */
    @Test
    fun framesEmittedAfterTheViewModelIsGoneStillArrive() =
        runTest {
            val c = controller()
            val session = c.sessionFor("a")

            // 用真的 `ViewModelStore`，不是把局部变量置 null：
            //   置 null 只是丢了个引用，根本不会触发 `onCleared`，「`onCleared` 杀掉 feed」的变异在那种写法下存活。
            //   `store.clear()` 才是屏幕销毁的真实路径。
            val store = ViewModelStore()
            val vm =
                ViewModelProvider(
                    store,
                    object : ViewModelProvider.Factory {
                        @Suppress("UNCHECKED_CAST")
                        override fun <T : ViewModel> create(modelClass: Class<T>): T = ChatViewModel(session) as T
                    },
                )[ChatViewModel::class.java]
            vm.start(
                flow {
                    emit(at("屏幕还在时"))
                    delay(LATE_FRAME_DELAY_MS)
                    emit(at("屏幕没了之后")) // 这一帧是判据
                    emit(res())
                },
            )
            advanceTimeBy(EARLY_MS)
            runCurrent()
            assertTrue(
                "前提：第一帧确实到了",
                session.state.value.units
                    .isNotEmpty(),
            )
            assertFalse(
                "前提：此刻第二帧还没发出来",
                session.state.value.units
                    .any { it.hasText("屏幕没了之后") },
            )

            // 屏幕销毁：这会真的走 `ChatViewModel.onCleared()`。
            //   它没有覆写那个方法 ⇒ 对话不该受任何影响。
            store.clear()

            // 用 `advanceTimeBy` 而不是 `advanceUntilIdle`：session 的协程跑在 `backgroundScope` 上，
            //   而 `advanceUntilIdle` 见前台无活可干就返回、不推进虚拟时间，
            //   于是那一帧永远发不出来，测试会红得莫名其妙。
            advanceTimeBy(LATE_FRAME_DELAY_MS + EARLY_MS)
            runCurrent()
            assertTrue(
                "VM 丢掉之后发出的帧仍然到达；实得 ${session.state.value.units}",
                session.state.value.units
                    .any { it.hasText("屏幕没了之后") },
            )
        }

    /**
     * 有对话在生成 ⇒ [ChatController.hasInFlightTurn] 为真；收口后转假。
     *
     * `SshKeepAliveService` 的前台服务靠它决定要不要活着：恒假会让「离开 app 后回复完成推送」失效，
     * 恒真则是永远不放手的前台服务。
     */
    @Test
    fun inFlightTurnTracksGenerationAndClearsWhenTheTurnCloses() =
        runTest {
            val c = controller()
            assertFalse("什么都没有时不该占着前台服务", c.hasInFlightTurn.value)

            val s = c.sessionFor("a")
            s.start(longRunningTurn())
            advanceTimeBy(EARLY_MS)
            runCurrent()
            assertTrue("在生成 ⇒ 前台服务要活着", c.hasInFlightTurn.value)

            c.release("a")
            assertFalse("对话收掉了就要放手", c.hasInFlightTurn.value)
        }

    /** 轮次自然收口（`res` 到达）之后也要放手，不然前台服务永远不停。 */
    @Test
    fun aClosedTurnReleasesTheForegroundServiceCriterion() =
        runTest {
            val c = controller()
            val s = c.sessionFor("a")
            s.start(flowOf(at("答完了"), res()))
            advanceTimeBy(EARLY_MS)
            runCurrent()
            assertFalse("res 到了就不再是在飞", c.hasInFlightTurn.value)
        }

    /**
     * 在飞的对话绝不被上限逐出。
     *
     * 逐出一个正在生成的对话，就等于亲手做掉本类存在的理由。
     */
    @Test
    fun anInFlightConversationIsNeverEvictedByTheCap() =
        runTest {
            val c = controller()
            val busy = c.sessionFor("busy")
            busy.start(longRunningTurn())
            advanceTimeBy(EARLY_MS)
            runCurrent()
            assertTrue("前提：它确实在飞，否则这条测试验的是别的东西", c.hasInFlightTurn.value)
            // 塞满到上限，逼它逐出
            repeat(ChatController.MAX_SESSIONS + 2) { c.sessionFor("idle-$it") }

            assertSame("在飞的那个必须还在（而且还是同一个实例）", busy, c.sessionFor("busy"))
            assertTrue("确实发生过逐出（否则这条测试什么都没验）", c.evictedCount > 0)
        }

    /** 逐出不许静默：发生了就要有地方看得见。 */
    @Test
    fun evictionIsCounted() =
        runTest {
            val c = controller()
            assertEquals("一开始没逐过", 0, c.evictedCount)
            repeat(ChatController.MAX_SESSIONS + 3) { c.sessionFor("k$it") }
            assertTrue("逐了就要记下来：${c.evictedCount}", c.evictedCount > 0)
        }

    /**
     * 聊天会话必须 `manager.retain(id, "chat-<sid>")`，否则终端 tab 关光时 holders 归零 →
     * `release` → `disconnectLocked` → 连带杀掉管道。
     *
     * 判据落在持有的生命周期上，不是「调了 retain 没有」：真正会咬人的是配对，
     * retain 了不 release 是连接永不回收，release 多了是把别人的持有还掉。
     */
    @Test
    fun aConversationHoldsItsHostConnectionUntilTheConversationEndsNotTheScreen() =
        runTest {
            val log = mutableListOf<String>()
            val holder =
                object : ConnectionHolder {
                    override fun retain(
                        hostId: String,
                        holder: String,
                    ) {
                        log += "retain $hostId/$holder"
                    }

                    override fun release(
                        hostId: String,
                        holder: String,
                    ) {
                        log += "release $hostId/$holder"
                    }
                }
            val c = ChatController(scopeFactory = { backgroundScope }, connections = holder)

            c.sessionFor("h1/s1", hostId = "h1")
            assertEquals("建对话就要持有那条连接", listOf("retain h1/chat-h1/s1"), log)

            // 幂等：同一个对话再取一次不许重复 retain（否则持有计数永远还不清）
            c.sessionFor("h1/s1", hostId = "h1")
            assertEquals("同 key 再取不许再 retain", 1, log.count { it.startsWith("retain") })

            c.release("h1/s1")
            assertEquals(
                "对话结束才放手，且 holder 必须与 retain 时逐字一致",
                listOf("retain h1/chat-h1/s1", "release h1/chat-h1/s1"),
                log,
            )
        }

    /** `releaseAll` 也要把持有全部还回去：漏一个就是连接永不回收。 */
    @Test
    fun releaseAllReturnsEveryHeldConnection() =
        runTest {
            val held = mutableSetOf<String>()
            val holder =
                object : ConnectionHolder {
                    override fun retain(
                        hostId: String,
                        holder: String,
                    ) {
                        held += "$hostId/$holder"
                    }

                    override fun release(
                        hostId: String,
                        holder: String,
                    ) {
                        held -= "$hostId/$holder"
                    }
                }
            val c = ChatController(scopeFactory = { backgroundScope }, connections = holder)
            c.sessionFor("h1/s1", hostId = "h1")
            c.sessionFor("h2/s2", hostId = "h2")
            assertEquals("前提：两条都持有着", 2, held.size)
            c.releaseAll()
            assertTrue("一个都不许漏，实得 $held", held.isEmpty())
        }

    /** 上限逐出时也要把那条对话的持有还回去（逐出 = 对话没了）。 */
    @Test
    fun evictionAlsoReturnsTheHeldConnection() =
        runTest {
            val held = mutableSetOf<String>()
            val holder =
                object : ConnectionHolder {
                    override fun retain(
                        hostId: String,
                        holder: String,
                    ) {
                        held += holder
                    }

                    override fun release(
                        hostId: String,
                        holder: String,
                    ) {
                        held -= holder
                    }
                }
            val c = ChatController(scopeFactory = { backgroundScope }, connections = holder)
            repeat(ChatController.MAX_SESSIONS + 2) { c.sessionFor("k$it", hostId = "h") }
            assertTrue("前提：确实逐出过", c.evictedCount > 0)
            assertEquals("持有数要与还活着的对话数一致", ChatController.MAX_SESSIONS, held.size)
        }

    /** `release` 要如实回答有没有收到东西：收了个不存在的 key 不该假装成功。 */
    @Test
    fun releaseReportsWhetherItActuallyClosedSomething() =
        runTest {
            val c = controller()
            assertFalse("本来就没有", c.release("nope"))
            c.sessionFor("a")
            assertTrue("有就收掉", c.release("a"))
            assertFalse("收过一次就没有了", c.release("a"))
        }

    /** 收掉一个对话不该带走别的（各自独立的 scope）。 */
    @Test
    fun releasingOneConversationDoesNotKillTheOthers() =
        runTest {
            val c = ChatController(scopeFactory = { CoroutineScope(Job() + backgroundScope.coroutineContext) })
            val a = c.sessionFor("a")
            val b = c.sessionFor("b")
            b.start(longRunningTurn("b 在生成"))
            advanceTimeBy(EARLY_MS)
            runCurrent()
            c.release("a")
            assertTrue(
                "收掉 a 不该影响 b",
                b.state.value.units
                    .isNotEmpty(),
            )
            assertTrue("b 仍在飞", c.hasInFlightTurn.value)
            assertNotSame("a 收掉之后再取是新的", a, c.sessionFor("a"))
            c.releaseAll()
        }

    // ---- 跨通路信号汇的接线 ----------------------------

    /**
     * 一个 controller 里的每条对话都接同一个（进程级的）信号汇，
     * 而且按 Claude 自己的对话编号接、不是按 `ChatController` 的 `key`。
     *
     * 这一条要防的两个 bug，各自都是「静默失效」：
     * ① 汇不是进程级的（`AppModule` 写成 `factory` 而不是 `single`，或本类每条对话新造一个）
     *    ⇒ 总览通路投进去的等待态在聊天屏那侧永远看不见，而一条断言都不会红（判据各造自己的汇，各自都绿）。
     * ② 键拿错了（用 `<hostId>/<sid>` 去接，而 daemon / pidfile 两侧认的是 sid）⇒ 信号恒 null，同样一条断言都不红。
     *
     * 所以判据故意让 `key` 与 `claudeSessionId` 不相等：拿错键的实现在这里当场红。
     */
    @Test
    fun everySessionInOneControllerTalksToTheSameProcessWideBus() =
        runTest {
            val bus = SessionSignals()
            val c = ChatController(scopeFactory = { backgroundScope }, signals = bus)

            // key 与 claudeSessionId 刻意不同：混用两个键的实现在这里恒 null ⇒ 红
            val a = c.sessionFor("host-1/$SID_A", claudeSessionId = SID_A)
            val b = c.sessionFor("host-1/$SID_B", claudeSessionId = SID_B)
            runCurrent()

            bus.publishWaiting(SID_A, "waiting", "sandbox request", SignalSource.DAEMON_SESSION_ADDED, 1_000)
            runCurrent()

            assertNotNull("往汇里投一条，A 这条对话就该看见（同一个汇）", a.state.value.waiting)
            assertNull("而 B 不该看见 —— 汇是共用的，信号仍然按 sid 分得开", b.state.value.waiting)

            bus.publishWaiting(SID_B, "waiting", "input needed", SignalSource.DAEMON_SESSION_STATUS, 1_100)
            runCurrent()
            assertNotNull("第二条对话走的也是同一个汇", b.state.value.waiting)
            c.releaseAll()
        }

    /**
     * 阴性对照：不给汇（`signals = null`，判据与 debug 重放屏走的正是这条）
     * ⇒ 行为不变，屏上一个字都不多。
     */
    @Test
    fun aControllerWithoutABusChangesNothing() =
        runTest {
            val c = ChatController(scopeFactory = { backgroundScope })
            val a = c.sessionFor("host-1/$SID_A", claudeSessionId = SID_A)
            runCurrent()
            assertNull(a.state.value.waiting)
            assertNull(a.state.value.quota)
            c.releaseAll()
        }

    private companion object {
        /** 两个不同的 Claude 对话编号：判据要靠它们区分「同一个汇」与「拿错键」。 */
        const val SID_A = "8cfb1b93-285d-42ca-bcc1-24f02419f930"
        const val SID_B = "1d0e4c72-7a55-4f01-9b3e-0c5a2e8b6d14"

        /** 推进到「第一帧已上屏、后续帧还没发」的时刻。要大于缓释节拍、小于 [LATE_FRAME_DELAY_MS]。 */
        const val EARLY_MS = 500L

        /** 第二帧的发出时刻：必须明显晚于 [EARLY_MS]，判据才不是碰运气。 */
        const val LATE_FRAME_DELAY_MS = 5_000L

        /** 「长轮次」挂多久。只要远大于 [EARLY_MS] 即可。 */
        const val LONG_TURN_MS = 60_000L
    }
}
