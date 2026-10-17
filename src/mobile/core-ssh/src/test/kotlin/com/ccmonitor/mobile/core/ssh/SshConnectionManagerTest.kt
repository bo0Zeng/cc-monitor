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
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * 用 FakeTransport 在 JVM 上确定性地测 manager 的代号守门（防幽灵复活）与单飞。跟着共用循环换连接那几条在 [FollowSharedLoopTest]。
 * reconnectDispatcher 注入 TestDispatcher，后台那几步可用 advanceUntilIdle 驱动。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SshConnectionManagerTest {
    private val good = ConnectionConfig(host = "good", port = 22, username = "u", auth = AuthMethod.Password("p"))

    private fun cfg(host: String) = ConnectionConfig(host = host, port = 22, username = "u", auth = AuthMethod.Password("p"))

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

    private fun TestScope.manager(
        produced: MutableList<FakeTransport>? = null,
        behavior: suspend FakeTransport.(ConnectionConfig) -> Unit = {},
    ): SshConnectionManager =
        SshConnectionManager(
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
}
