package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * 用 FakeTransport 在 JVM 上确定性地测 manager 的代号守门（防幽灵复活）、重连与单飞。
 * reconnectDispatcher 注入 TestDispatcher，重连可用 advanceUntilIdle 驱动。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SshConnectionManagerTest {
    private val good = ConnectionConfig(host = "good", port = 22, username = "u", auth = AuthMethod.Password("p"))

    private fun cfg(host: String) = ConnectionConfig(host = host, port = 22, username = "u", auth = AuthMethod.Password("p"))

    private fun cfgP(
        host: String,
        policy: ReconnectPolicy,
    ) = ConnectionConfig(host = host, port = 22, username = "u", auth = AuthMethod.Password("p"), reconnect = policy)

    // 重连策略：退避、maxAttempts、auto、onNetworkChange、disconnect 中断

    @Test
    fun backoffRetriesUntilSuccess() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            var failuresRemaining = 0
            val mgr =
                manager(produced) {
                    if (failuresRemaining > 0) {
                        failuresRemaining--
                        error("boom")
                    }
                }
            mgr.connect("id", "h", "L", cfgP("good", ReconnectPolicy(baseDelayMs = 100, maxDelayMs = 400, maxAttempts = 0)))
            produced.single().simulateDrop()
            failuresRemaining = 2 // 前 2 次重连失败，第 3 次成功
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["id"]?.status)
            assertEquals("1 初连 + 3 次重连尝试（退避后终成）", 4, produced.size)
        }

    // reconnecting 标志的复位真生效：一轮掉线重连连上后，新连接再掉线，第二轮必须能重新进入。
    // 复位失效时第二轮 compareAndSet 失败早退，不新建连接（produced 停在 2）；所以 produced.size == 3 即证复位。
    @Test
    fun reconnectingFlagResetsAllowingSecondReconnect() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced)
            mgr.connect("id", "h", "L", cfgP("good", ReconnectPolicy(baseDelayMs = 100, maxDelayMs = 200, maxAttempts = 0)))
            // 第 1 轮：掉线、网络恢复重连、连上（reconnecting 置 true，finally 复位 false）
            produced.single().simulateDrop()
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertEquals("第 1 轮重连连上", SessionStatus.CONNECTED, mgr.sessions.value["id"]?.status)
            assertEquals("初连 + 第 1 次重连", 2, produced.size)
            // 第 2 轮：新连接再掉线、再触发重连；只有 reconnecting 已复位才能重新进入
            produced.last().simulateDrop()
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertEquals("reconnecting 已复位 → 第 2 次重连也新建连接连上", 3, produced.size)
            assertTrue("第 2 轮后当前连接是活的", mgr.connection("id")?.isConnected == true)
        }

    @Test
    fun maxAttemptsStopsRetrying() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            var failAll = false
            val mgr = manager(produced) { if (failAll) error("boom") }
            mgr.connect("id", "h", "L", cfgP("good", ReconnectPolicy(baseDelayMs = 100, maxDelayMs = 200, maxAttempts = 2)))
            produced.single().simulateDrop()
            failAll = true
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertEquals("初连 + 恰 2 次重连尝试后停", 3, produced.size)
            assertTrue("达上限未连上", mgr.sessions.value["id"]?.status != SessionStatus.CONNECTED)
        }

    @Test
    fun autoFalseSkipsAutoReconnect() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced)
            mgr.connect("id", "h", "L", cfgP("good", ReconnectPolicy(auto = false)))
            produced.single().simulateDrop()
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertEquals("auto=false（断线自动重连关）→ 不自动重连", 1, produced.size)
        }

    @Test
    fun onNetworkChangeFalseSkipsNetworkTrigger() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced)
            mgr.connect("id", "h", "L", cfgP("good", ReconnectPolicy(onNetworkChange = false)))
            produced.single().simulateDrop()
            mgr.probeAndReconnectStale() // 网络恢复触发
            advanceUntilIdle()
            assertEquals("onNetworkChange=false → 网络恢复不重连", 1, produced.size)
        }

    @Test
    fun staleExpectedGenAbortsWithoutReviving() =
        runTest {
            // 重连纪元守门：重连启动前会话已被 disconnect（代号变了），带旧 expectedGen 的 connect 必须弃用、不复活。
            val mgr = manager()
            mgr.connect("id", "h", "L", good) // gen=0，连上
            mgr.disconnect("id") // gen→1，移除会话
            val result = runCatching { mgr.connect("id", "h", "L", listOf(good), expectedGen = 0) } // 旧纪元
            assertTrue("过期纪元重连应弃用（CE）", result.exceptionOrNull() is CancellationException)
            assertNull("不得复活已断开的会话", mgr.sessions.value["id"])
        }

    // retain 返回捕获的重连纪元：强制断开若落在 retain 之后、connect 建连之前，带该纪元的首次 connect 必须弃用，
    // 不装入无人持有的连接（否则活跃数恒 ≥1，保活服务永不自停）。真实窗口是纳秒级交错，这里直接测这条链。
    @Test
    fun retainEpochGuardsInitialConnectAgainstForceOrphan() =
        runTest {
            val mgr = manager()
            val epoch = mgr.retain("id", "tok") // VM retain，原子捕获纪元（gen=0）
            mgr.disconnect("id") // 窄窗内强制断开：gen 0→1，清持有者
            val result = runCatching { mgr.connect("id", "h", "L", listOf(good), expectedGen = epoch) }
            assertTrue("纪元过期 → connect 弃用（CE）", result.exceptionOrNull() is CancellationException)
            assertNull("不得装入无人持有的孤儿连接", mgr.connection("id"))
            assertEquals("activeCount 不因孤儿恒≥1", 0, mgr.activeCount())
        }

    @Test
    fun retainThenConnectWithItsEpochSucceedsWhenNoForce() =
        runTest {
            // 无强制断开时带该纪元的 connect 正常连上。
            val mgr = manager()
            val epoch = mgr.retain("id", "tok")
            val t = mgr.connect("id", "h", "L", listOf(good), expectedGen = epoch)
            assertTrue("无 force → 正常连上", t.isConnected)
            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["id"]?.status)
        }

    // 「检查仍需重连 + 标 RECONNECTING」原子守门：disconnect 之后，带旧代号的标记尝试必须拒绝且不重插条目，
    // 否则会留下永久的 RECONNECTING 幽灵。真实窗口无法确定性复现，这里直接测守门的判定。
    @Test
    fun markReconnectingAfterDisconnectDoesNotReviveGhostEntry() =
        runTest {
            val mgr = manager()
            mgr.connect("id", "h", "L", good) // gen=0，连上
            val st = mgr.sessions.value.getValue("id")
            mgr.disconnect("id") // gen 0→1，条目移除（相当于用户在检查与写入之间断开）
            assertFalse("gen 已变 → 拒绝标 RECONNECTING", mgr.tryMarkReconnecting(st, genAtStart = 0))
            assertNull("不得重插幽灵条目", mgr.sessions.value["id"])
        }

    @Test
    fun markReconnectingWhileStillActiveMarksEntry() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced)
            mgr.connect("id", "h", "L", good)
            val st = mgr.sessions.value.getValue("id")
            produced.single().simulateDrop() // 底层掉线（未 disconnect，代号不变）：仍是合法重连对象
            assertTrue("gen 未变且未连上 → 标记成功", mgr.tryMarkReconnecting(st, genAtStart = 0))
            assertEquals(SessionStatus.RECONNECTING, mgr.sessions.value["id"]?.status)
        }

    @Test
    fun disconnectInterruptsBackoffLoop() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            var failAll = false
            val mgr = manager(produced) { if (failAll) error("boom") }
            mgr.connect("id", "h", "L", cfgP("good", ReconnectPolicy(baseDelayMs = 1_000, maxDelayMs = 5_000, maxAttempts = 0)))
            produced.single().simulateDrop()
            failAll = true
            mgr.probeAndReconnectStale()
            runCurrent() // 第 1 次重连尝试失败，进入退避 delay
            val before = produced.size
            mgr.disconnect("id") // 代号自增：退避循环下一轮应中止、不复活
            advanceUntilIdle()
            assertNull("disconnect 后会话应移除", mgr.sessions.value["id"])
            assertTrue("disconnect 后不再新增重连尝试", produced.size <= before)
        }

    private fun TestScope.manager(
        produced: MutableList<FakeTransport>? = null,
        behavior: suspend FakeTransport.(ConnectionConfig) -> Unit = {},
    ): SshConnectionManager =
        SshConnectionManager(
            knownHostStore = null,
            transportFactory = { FakeTransport(behavior).also { produced?.add(it) } },
            reconnectDispatcher = StandardTestDispatcher(testScheduler),
        )

    @Test
    fun connectSuccessMarksConnected() =
        runTest {
            val mgr = manager()
            val t = mgr.connect("id", "h", "L", good)
            assertTrue(t.isConnected)
            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["id"]?.status)
            assertEquals(1, mgr.activeCount())
        }

    // 同 id 两个 tab 并发首连：单飞只建一条连接、都拿到同一条，不会各建一条互相关掉。
    @Test
    fun concurrentFirstConnectSingleFlight() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val gate = CompletableDeferred<Unit>() // 卡住首连的握手，让两个 connect 同时在飞
            val mgr = manager(produced) { gate.await() }
            val a = async { mgr.connect("h", "h", "L", good) } // tab A 首连
            val b = async { mgr.connect("h", "h", "L", good) } // tab B 同主机同 id 并发首连
            runCurrent() // A 进锁建 transport 卡在握手；B 卡在单飞锁上
            gate.complete(Unit) // 放行握手
            val ta = a.await()
            val tb = b.await()
            advanceUntilIdle()
            assertEquals("single-flight：只建一条连接（B 复用 A 的，不再自建）", 1, produced.size)
            assertSame("两 tab 拿到同一条连接", ta, tb)
            assertTrue("连接是活的", ta.isConnected)
            assertEquals(false, (ta as FakeTransport).closed) // A 正用的连接没被 B 顶掉关闭
            assertEquals(1, mgr.activeCount())
        }

    // 首连失败后，第二个 connect 应能自建：锁内复查见 null，不复用失败者。
    @Test
    fun concurrentFirstConnectFailureThenSecondSelfConnects() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            var failFirst = true
            val mgr =
                manager(produced) {
                    if (failFirst) {
                        failFirst = false
                        error("boom") // 首连失败
                    }
                }
            runCatching { mgr.connect("h", "h", "L", good) } // A 失败
            val t = mgr.connect("h", "h", "L", good) // B 自建，不复用失败者
            advanceUntilIdle()
            assertTrue(t.isConnected)
            assertEquals("失败 1 + 成功 1 = 建 2 条", 2, produced.size)
            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["h"]?.status)
        }

    @Test
    fun connectThreadsVerboseEventsToOnEvent() =
        runTest {
            // manager 把连接阶段事件透传给 onEvent，并带对应地址的 host:port。
            val mgr = manager()
            val events = mutableListOf<SshConnectEvent>()
            mgr.connect("id", "h", "L", good, onEvent = { events.add(it) })
            assertTrue("应收到连接事件", events.isNotEmpty())
            assertTrue("事件应带对应地址 host:port", events.all { it.host == "good" && it.port == 22 })
            assertTrue("应含 ESTABLISHED 阶段", events.any { it.stage == SshConnectEvent.Stage.ESTABLISHED })
        }

    @Test
    fun allConnectedSessionsCountAsActive() =
        runTest {
            // 同主机多个会话（终端与 SFTP）都计入活跃。
            val mgr = manager()
            mgr.connect("term", "h", "T", good)
            mgr.connect("sftp", "h", "S", good)
            assertEquals(2, mgr.sessions.value.size)
            assertEquals(2, mgr.activeCount()) // 两者都计入
        }

    // 全失败时置 ERROR 并抛出。竞速本身的语义在 RaceConnectTest。

    @Test
    fun allAddressesFailMarksErrorAndThrows() =
        runTest {
            val mgr = manager { throw IOException("fail ${it.host}") }
            val ex = runCatching { mgr.connect("id", "h", "L", listOf(cfg("a"), cfg("b"))) }.exceptionOrNull()
            assertTrue("全失败应抛聚合错误", ex is IOException)
            assertEquals(SessionStatus.ERROR, mgr.sessions.value["id"]?.status)
        }

    @Test
    fun disconnectDuringConnectDiscardsGhost() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced) { delay(1_000) } // 慢成功
            val job = launch { runCatching { mgr.connect("id", "h", "L", good) } }
            runCurrent() // connect 注册 CONNECTING 并卡在 delay 里
            assertEquals(SessionStatus.CONNECTING, mgr.sessions.value["id"]?.status)
            mgr.disconnect("id") // 代号自增，移除会话条目
            advanceUntilIdle() // connect 完成，finalize 发现代号变了：弃用并 close，不复活
            job.join()
            assertNull("断开后的在飞连接不得复活会话", mgr.sessions.value["id"])
            assertTrue("被弃用的新连接应被 close", produced.single().closed)
        }

    // disconnect 应能中止卡在握手的在飞连接（awaitClose 只有被 close 才解阻塞，相当于黑洞对端）。
    // 不关在飞 transport 的话 connect 永挂，本测在 job.join() 超时暴露。
    @Test
    fun disconnectAbortsInFlightHungConnect() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced) { awaitClose() } // 卡在握手，直到被 close
            val job = launch { runCatching { mgr.connect("id", "h", "L", good) } }
            runCurrent() // connect 进锁、登记 CONNECTING、发起握手并卡在 awaitClose()
            assertEquals(SessionStatus.CONNECTING, mgr.sessions.value["id"]?.status)
            mgr.disconnect("id") // 应关在飞连接，解开握手，connect 返回、单飞锁释放
            advanceUntilIdle()
            job.join() // 在飞连接没被中止的话这里永挂
            assertTrue("在飞连接应被 disconnect 关闭中止", produced.single().closed)
            assertNull("断开后会话不复活", mgr.sessions.value["id"])
        }

    // 中止卡死的首连后，同 id 后续 connect 必须能成功：单飞锁已释放。
    @Test
    fun reconnectAfterAbortingHungConnectSucceeds() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            var block = true
            val mgr = manager(produced) { if (block) awaitClose() } // 首连卡死；放行后续
            val job = launch { runCatching { mgr.connect("id", "h", "L", good) } }
            runCurrent()
            mgr.disconnect("id") // 中止卡死首连
            advanceUntilIdle()
            job.join()
            block = false
            val t = mgr.connect("id", "h", "L", good) // 单飞锁若仍被卡死首连占用，此处会挂
            advanceUntilIdle()
            assertTrue("中止卡死首连后重连应成功（单飞锁已释放）", t.isConnected)
            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["id"]?.status)
        }

    @Test
    fun probeReconnectsAllDroppedSessions() =
        runTest {
            // 所有掉线的会话都应被后台探测重连。
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced) // connect 立即成功
            mgr.connect("a", "h", "A", good)
            mgr.connect("b", "h", "B", good)
            assertEquals(2, produced.size) // [0]=a, [1]=b
            produced.forEach { it.simulateDrop() } // 两者底层都掉（isConnected 变 false），但会话状态仍是 CONNECTED

            mgr.probeAndReconnectStale()
            advanceUntilIdle()

            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["a"]?.status)
            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["b"]?.status)
            assertEquals("两会话都重连 → 各多造 1 个 transport", 4, produced.size)
        }

    // 黑洞断网：`isConnected` 本地标志仍为 true，但连接已死。probeAndReconnectStale 对这类 CONNECTED 会话主动探测，判死即重连。
    @Test
    fun probeAliveFailReconnectsBlackholeWhenIsConnectedStillTrue() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced)
            mgr.connect("id", "h", "L", good)
            assertTrue("初连活、本地标志 true", produced.single().isConnected)
            produced[0].probeAliveResult = false // 黑洞：isConnected 仍 true，主动探测判死
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertEquals("主动探测判死 → 重连造第 2 条", 2, produced.size)
            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["id"]?.status)
        }

    // 健康连接（probeAlive 走默认，即 isConnected=true）：网络恢复时探测通过，不误重连。
    @Test
    fun probeAliveTrueHealthyConnectionNotReconnected() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced)
            mgr.connect("id", "h", "L", good) // probeAliveResult=null → probeAlive 返回 isConnected=true
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertEquals("健康连接探测过 → 不重连", 1, produced.size)
            assertEquals("probe 分支确实运行了一次（非死代码）——探测过才不重连，锁死正原因", 1, produced[0].probeCalls.get())
            assertEquals(SessionStatus.CONNECTED, mgr.sessions.value["id"]?.status)
        }

    // onNetworkChange=false 的主机即便探测判死，也不被网络恢复触发重连。
    @Test
    fun probeAliveFailButOnNetworkChangeFalseStillSkips() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced)
            mgr.connect("id", "h", "L", cfgP("good", ReconnectPolicy(onNetworkChange = false)))
            produced[0].probeAliveResult = false // 黑洞
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertEquals("onNetworkChange=false → 即便探测判死也不网络恢复重连", 1, produced.size)
            assertEquals("onNetworkChange=false 应在 probe 前被过滤掉、根本不探测", 0, produced[0].probeCalls.get())
        }

    // auto=false（关了断线自动重连）：网络恢复时既不探测也不重连。
    @Test
    fun probeAliveFailButAutoFalseStillSkips() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced)
            mgr.connect("id", "h", "L", cfgP("good", ReconnectPolicy(auto = false)))
            produced[0].probeAliveResult = false // 黑洞
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertEquals("auto=false → 即便探测判死也不网络恢复重连", 1, produced.size)
            assertEquals("auto=false 应在 probe 前被过滤、免无谓探测", 0, produced[0].probeCalls.get())
        }

    // 探测在飞时并发 disconnect(id)：不复活、不新建（配置已清、代号已变两道守门）。与复用前探测的同类用例对称。
    @Test
    fun probeBranchDoesNotReviveWhenDisconnectedMidProbe() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = manager(produced)
            mgr.connect("id", "h", "L", good) // produced[0]，isConnected 仍为 true
            val gate = CompletableDeferred<Unit>()
            produced[0].probeAliveResult = false // 探测将判死
            produced[0].probeGate = gate // 探测挂在 gate 上，拉开在飞窗口
            mgr.probeAndReconnectStale() // 进探测分支，probeAlive 挂在 gate 上
            runCurrent()
            mgr.disconnect("id") // 探测在飞时用户断开
            gate.complete(Unit) // 放行探测：判死后摘除是 no-op，重连应被配置与代号守门弃用
            advanceUntilIdle()
            assertEquals("中途 disconnect → 不复活、不新建第 2 条", 1, produced.size)
            assertNull("会话条目应保持移除", mgr.sessions.value["id"])
            assertNull("连接应保持断开", mgr.connection("id"))
        }
}
