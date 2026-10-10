package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** 连接引用计数（多 tab 共享同一条连接）与复用前的活性探测。 */
@OptIn(ExperimentalCoroutinesApi::class)
class SshConnectionRefcountTest {
    // 引用计数

    @Test
    fun releaseToZeroDisconnects() =
        runTest {
            val mgr = testManager()
            mgr.retain("id", "a")
            mgr.connect("id", "h", "L", testCfg("good"))
            assertTrue("连上", mgr.connection("id")?.isConnected == true)
            mgr.release("id", "a") // 无持有者 → 真断
            assertNull("无持有者 release 应断连", mgr.connection("id"))
            assertNull("会话条目应移除", mgr.sessions.value["id"])
        }

    @Test
    fun releaseWithRemainingHolderKeepsConnection() =
        runTest {
            val mgr = testManager()
            mgr.retain("id", "a") // 2 个不同持有者（模拟同主机 2 tab）
            mgr.retain("id", "b")
            mgr.connect("id", "h", "L", testCfg("good"))
            mgr.release("id", "a") // 还剩 b
            assertTrue("尚有持有者 release 连接应保留", mgr.connection("id")?.isConnected == true)
            mgr.release("id", "b") // 无持有者
            assertNull("最后一个持有者 release 才真断", mgr.connection("id"))
        }

    // 强制断开清掉持有者集后，存活的 VM 幂等重 retain 再重连，之后 release 能真断。
    @Test
    fun forceDisconnectThenReRetainReconnectReleaseDisconnects() =
        runTest {
            val mgr = testManager()
            mgr.retain("id", "a")
            mgr.connect("id", "h", "L", testCfg("good"))
            mgr.disconnect("id") // 强制断开：清持有者集并断开
            assertNull("force disconnect 立即断", mgr.connection("id"))
            // 存活的 VM 重试：幂等重 retain 后重连。
            mgr.retain("id", "a")
            assertTrue("重连成功", mgr.connect("id", "h", "L", testCfg("good")).isConnected)
            // 关 tab：release 应真断。
            mgr.release("id", "a")
            assertNull("重连后 release 真断", mgr.connection("id"))
        }

    // 多持有者被强制断开后各自重 retain，持有者集精确重建；用计数做不到。
    @Test
    fun forceDisconnectMultiHolderReRetainKeepsCorrectCount() =
        runTest {
            val mgr = testManager()
            mgr.retain("id", "a")
            mgr.retain("id", "b")
            mgr.connect("id", "h", "L", testCfg("good"))
            mgr.disconnect("id") // 强制断开清集
            mgr.retain("id", "a") // 两 tab 各自重试重 retain
            mgr.retain("id", "b")
            mgr.connect("id", "h", "L", testCfg("good"))
            mgr.release("id", "a") // 关 tab a → 仍有 b
            assertTrue("关一个 tab 不断另一个正用连接", mgr.connection("id")?.isConnected == true)
            mgr.release("id", "b")
            assertNull("两 tab 都关才真断", mgr.connection("id"))
        }

    // release 幂等：重复或陈旧的 token 不二次断、不误断。
    @Test
    fun duplicateReleaseIsNoop() =
        runTest {
            val mgr = testManager()
            mgr.retain("id", "a")
            mgr.retain("id", "b")
            mgr.connect("id", "h", "L", testCfg("good"))
            mgr.release("id", "a")
            mgr.release("id", "a") // 重复 release a，不该误断 b
            assertTrue("重复 release 不误断尚有持有者的连接", mgr.connection("id")?.isConnected == true)
            mgr.release("id", "b")
            assertNull(mgr.connection("id"))
        }

    // 断开时的物理 close 是网络写，不得在调用线程同步执行，须派发到 manager 的后台 scope。
    // 推进后台 dispatcher 之前 close 未发生（确实切走了），之后必然发生（不泄漏）；状态摘除保持同步。
    @Test
    fun releaseToZeroDispatchesCloseToBackgroundScope() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.retain("id", "a")
            mgr.connect("id", "h", "L", testCfg("good"))
            mgr.release("id", "a") // 无持有者 → 真断
            assertNull("map 摘除同步生效（对外立即不可见）", mgr.connection("id"))
            assertNull("会话条目同步移除", mgr.sessions.value["id"])
            assertEquals("close 不在调用线程同步执行（已派发后台）", false, produced.single().closed)
            advanceUntilIdle()
            assertTrue("推进后台 dispatcher 后 close 必然落地", produced.single().closed)
        }

    // disconnectAll 路径同样：close 全部走后台且必然执行。
    @Test
    fun disconnectAllDispatchesCloseToBackgroundScope() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.connect("a", "h", "A", testCfg("good"))
            mgr.connect("b", "h", "B", testCfg("good2"))
            mgr.disconnectAll()
            assertNull(mgr.connection("a"))
            assertNull(mgr.connection("b"))
            assertTrue("close 不在调用线程同步执行（已派发后台）", produced.none { it.closed })
            advanceUntilIdle()
            assertTrue("推进后台 dispatcher 后全部 close 落地", produced.all { it.closed })
        }

    @Test
    fun connectReusesLiveConnectionForSecondTab() =
        runTest {
            // 同主机第 2 个 tab 复用第 1 个的活连接，不重建、不顶掉。
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.retain("id", "a")
            val t1 = mgr.connect("id", "h", "L", testCfg("good")) // tab 1
            mgr.retain("id", "b")
            val t2 = mgr.connect("id", "h", "L", testCfg("good")) // tab 2 同 id
            assertSame("第 2 tab 应复用同一连接", t1, t2)
            assertEquals("只造了 1 个 transport（第 2 次未重建）", 1, produced.size)
            assertTrue("第 1 个连接未被顶掉", t1.isConnected)
        }

    @Test
    fun connectRebuildsWhenPriorConnectionDead() =
        runTest {
            // 只复用活连接：连接掉了（isConnected=false）就完整重建。
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.retain("id", "a")
            mgr.connect("id", "h", "L", testCfg("good"))
            produced.single().simulateDrop() // 连接死
            mgr.connect("id", "h", "L", testCfg("good")) // 应重建（非复用死连接）
            assertEquals("死连接不复用，重建了第 2 个 transport", 2, produced.size)
        }

    @Test
    fun releaseToZeroInterruptsReconnectLikeDisconnect() =
        runTest {
            // release 归零走 disconnectLocked 的代号自增，和 disconnect 一样中止在飞的退避重连、不复活会话。
            val produced = mutableListOf<FakeTransport>()
            var failAll = false
            val mgr = testManager(produced) { if (failAll) error("boom") }
            mgr.retain("id", "a")
            mgr.connect("id", "h", "L", testCfgP("good", ReconnectPolicy(baseDelayMs = 1_000, maxDelayMs = 5_000, maxAttempts = 0)))
            produced.single().simulateDrop()
            failAll = true
            mgr.probeAndReconnectStale()
            runCurrent() // 第 1 次重连尝试失败 → 进入退避 delay
            val before = produced.size
            mgr.release("id", "a") // 无持有者 → gen++ → 退避循环下一轮应中止、不复活
            advanceUntilIdle()
            assertNull("release 归零后会话应移除", mgr.sessions.value["id"])
            assertTrue("release 归零后不再新增重连尝试（gen++ 生效）", produced.size <= before)
        }

    // 复用前的活性探测与合成事件

    // 复用命中（探测通过）时发合成事件（「校验可用性」与合成 ESTABLISHED「复用已有连接」），连接日志不空；仍只建 1 条连接。
    @Test
    fun fastPathReuseEmitsSyntheticEvents() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            val t1 = mgr.connect("id", "h", "L", testCfg("good"))
            val events = mutableListOf<SshConnectEvent>()
            val t2 = mgr.connect("id", "h", "L", testCfg("good"), onEvent = { events.add(it) })
            assertSame("探测过 → 复用同一连接", t1, t2)
            assertEquals("复用不重建", 1, produced.size)
            assertTrue(
                "应发「校验可用性」提示（TCP）",
                events.any { it.stage == SshConnectEvent.Stage.TCP && it.message.contains("校验可用") },
            )
            assertTrue(
                "应发合成 ESTABLISHED「复用已有连接」",
                events.any { it.stage == SshConnectEvent.Stage.ESTABLISHED && it.message.contains("复用已有连接") },
            )
            assertTrue("合成事件带首地址 host:port", events.all { it.host == "good" && it.port == 22 })
        }

    // isConnected 仍为 true 的死 TCP（probeAlive=false）：摘除、后台关、走真 connect，全程有事件。
    @Test
    fun fastPathDiscardsStaleWhenProbeFails() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.connect("id", "h", "L", testCfg("good"))
            produced.single().probeAliveResult = false // 死 TCP：本地标志 true、真实往返失败
            val events = mutableListOf<SshConnectEvent>()
            val t2 = mgr.connect("id", "h", "L", testCfg("good"), onEvent = { events.add(it) })
            assertEquals("stale 不复用 → 重建第 2 条", 2, produced.size)
            assertSame("拿到的是新建连接", produced[1], t2)
            assertTrue("新连接是活的", t2.isConnected)
            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["id"]?.status)
            advanceUntilIdle() // 推进后台 close（closeInBackground 派发在 reconnectScope）
            assertTrue("stale 旧连接被后台关闭", produced[0].closed)
            assertTrue("verbose 非空", events.isNotEmpty())
            assertTrue("应发「既有连接已失效」提示", events.any { it.message.contains("已失效") })
            assertTrue(
                "真 connect 事件随后流出（真 ESTABLISHED）",
                events.any { it.stage == SshConnectEvent.Stage.ESTABLISHED && it.message.contains("fake established") },
            )
        }

    // 摘除死连接是替换，不是断开：代号不变，带旧 expectedGen 的 connect 不被误弃；持有者与配置不动，
    // release 归零仍能真断（持有者若被误清，release 会变成 no-op，连接滞留即暴露）。
    @Test
    fun staleDiscardIsReplaceNotDisconnect() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.retain("id", "a")
            mgr.connect("id", "h", "L", testCfg("good")) // gen=0
            produced[0].probeAliveResult = false
            mgr.connect("id", "h", "L", testCfg("good")) // 摘除 stale + 重建
            advanceUntilIdle()
            assertTrue("stale 已被后台关闭", produced[0].closed)
            // 代号未变：旧纪元（expectedGen=0）的 connect 仍被接受。
            produced[1].simulateDrop()
            val result = runCatching { mgr.connect("id", "h", "L", listOf(testCfg("good")), expectedGen = 0) }
            assertTrue("stale 摘除不 gen++ → 旧纪元 connect 不被误弃", result.isSuccess)
            assertEquals("drop 后旧纪元重连重建第 3 条", 3, produced.size)
            // 持有者未损：单持有者 release 归零仍真断。
            mgr.release("id", "a")
            assertNull("release 归零仍真断（holders 未被 stale 摘除清掉）", mgr.connection("id"))
            assertNull("会话条目随真断移除", mgr.sessions.value["id"])
        }

    // 单飞等待者不再静默排队：进锁前收到「另一连接尝试进行中」，锁内复查命中收到合成 ESTABLISHED；仍只建 1 条连接。
    @Test
    fun singleFlightWaiterGetsReuseEvent() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val gate = CompletableDeferred<Unit>()
            val mgr = testManager(produced) { gate.await() }
            val eventsB = mutableListOf<SshConnectEvent>()
            val a = async { mgr.connect("h", "h", "L", testCfg("good")) }
            runCurrent() // A 进锁建 transport、卡在握手（gate）
            val b = async { mgr.connect("h", "h", "L", testCfg("good"), onEvent = { eventsB.add(it) }) }
            runCurrent() // B：无既有连接 → isLocked=true 发等待提示 → 排队在 single-flight 锁上
            gate.complete(Unit)
            val ta = a.await()
            val tb = b.await()
            advanceUntilIdle()
            assertEquals("single-flight：仍只建 1 条连接", 1, produced.size)
            assertSame("等待者复用首者连接", ta, tb)
            assertTrue(
                "等待者收到「另一连接尝试进行中」提示（TCP）",
                eventsB.any { it.stage == SshConnectEvent.Stage.TCP && it.message.contains("另一连接尝试进行中") },
            )
            assertTrue(
                "等待者收到锁内复查合成 ESTABLISHED（复用已有连接）",
                eventsB.any { it.stage == SshConnectEvent.Stage.ESTABLISHED && it.message.contains("复用已有连接") },
            )
        }

    // 探测期间并发 disconnect(id)：不返回那条正被摘除的旧引用，改走真 connect。
    // 用 gate 拉开探测在飞的窗口，让 disconnect 落在探测通过与复查之间。
    @Test
    fun fastPathDoesNotReuseStaleRefWhenDisconnectedMidProbe() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.connect("id", "h", "L", testCfg("good")) // produced[0]
            val gate = CompletableDeferred<Unit>()
            produced[0].probeGate = gate // 探测挂起在此 gate → 制造在飞窗口
            val job = async { mgr.connect("id", "h", "L", testCfg("good")) }
            runCurrent() // 进 reusableAfterProbe：isConnected 过 → probeAlive 挂在 gate 上
            mgr.disconnect("id") // 探测在飞时并发断开
            gate.complete(Unit) // 放行探测：返回 true，但复查发现已不是它，走真 connect
            val t = job.await()
            advanceUntilIdle()
            assertEquals("中途 disconnect → 不复用陈旧引用，真建第 2 条", 2, produced.size)
            assertSame("返回的是新建活连接", produced[1], t)
            assertTrue("陈旧连接已被后台关闭", produced[0].closed)
        }
}
