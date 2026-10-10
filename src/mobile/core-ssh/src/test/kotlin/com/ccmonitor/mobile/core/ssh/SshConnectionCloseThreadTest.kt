package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import java.util.concurrent.Executors

/**
 * 规矩：`close()` 是网络写，不能在主线程上调。
 *
 * manager 用真默认构造（不注入 dispatcher），调用面跑在一条自造的单线程上充当主线程；
 * `FakeTransport.close` 记下当时的 `Thread`，断言它不是那条线程。覆盖断连、握手失败、建连中途取消三条路径。
 *
 * 注意：
 * - 比的是 `Thread` 对象身份，不是名字：调试模式下 kotlinx-coroutines 会把线程改名成 `… @coroutine#3`，按名字比恒绿。
 * - 每条先断言 close 真的发生了，否则线程断言是空真。
 * - 证明的是「不在调用方线程」，不是「在 IO」。
 * - 若 `SshTransport` 的实现自己把 close 挪到后台，这几条会恒绿而与 manager 无关；`SshConnection.close()` 是同步的。
 */
class SshConnectionCloseThreadTest {
    private val produced = mutableListOf<FakeTransport>()

    /** 用真默认的 `reconnectDispatcher` 造 manager；在这里注入 dispatcher 会让本文件失去意义。 */
    private fun realDispatcherManager(behavior: suspend FakeTransport.(ConnectionConfig) -> Unit = {}) =
        SshConnectionManager(
            knownHostStore = null,
            transportFactory = { FakeTransport(behavior).also { produced += it } },
        )

    /** 自造的主线程，[onMainThread] 建它时记下，断言按对象身份比。 */
    private var mainThread: Thread? = null

    /** 造一条「主线程」，调用面全跑在它上面。 */
    private fun <T> onMainThread(block: suspend CoroutineScope.() -> T): T {
        val exec =
            Executors.newSingleThreadExecutor { r ->
                Thread(r, MAIN_THREAD_NAME).also { mainThread = it }
            }
        return try {
            runBlocking(exec.asCoroutineDispatcher(), block)
        } finally {
            exec.shutdownNow()
        }
    }

    private fun assertClosedOffTheMainThread(t: FakeTransport) {
        assertTrue("前提：close 必须真的发生过，否则下面的线程断言是空真", t.closed)
        val where = t.closedOnThread
        assertNotNull("前提：要记得到线程，否则分不出没关和关错线程", where)
        val main = mainThread
        assertNotNull("前提：自造的主线程必须真的建起来过", main)
        // 比对象身份，不比名字：调试模式下跑协程的线程会被改名。
        assertNotSame(
            "close() 是网络写，不能落在主线程上（实际落在：${where?.name}）",
            main,
            where,
        )
    }

    /** 断连路径：release 归零后 close 被默认 dispatcher 带离调用方线程。 */
    @Test(timeout = TEST_TIMEOUT_MS)
    fun releaseToZeroClosesOffTheMainThread() {
        val mgr = realDispatcherManager()
        onMainThread {
            mgr.retain("id", "a")
            mgr.connect("id", "h", "L", testCfg("good"))
            mgr.release("id", "a")
            withTimeout(AWAIT_CLOSE_MS) { produced.single().awaitClose() }
        }
        assertClosedOffTheMainThread(produced.single())
    }

    /** 握手失败路径：主线程上连不上一台主机时每次都走。 */
    @Test(timeout = TEST_TIMEOUT_MS)
    fun aFailedConnectClosesOffTheMainThread() {
        val mgr = realDispatcherManager { throw IOException("造出来的握手失败") }
        onMainThread {
            val ex = runCatching { mgr.connect("id", "h", "L", testCfg("bad")) }.exceptionOrNull()
            assertTrue("前提：这次 connect 必须真的失败，否则测的是另一条路径：$ex", ex is IOException)
            withTimeout(AWAIT_CLOSE_MS) { produced.single().awaitClose() }
        }
        assertClosedOffTheMainThread(produced.single())
    }

    /** 建连中途被取消的路径：离屏时协程被取消，连接可能已建到一半，close 是真的网络写。 */
    @Test(timeout = TEST_TIMEOUT_MS)
    fun aCancelledConnectClosesOffTheMainThread() {
        val reachedHandshake = CompletableDeferred<Unit>()
        val mgr =
            realDispatcherManager {
                reachedHandshake.complete(Unit)
                awaitClose() // 卡在握手直到被 close，相当于黑洞对端
            }
        onMainThread {
            val job = launch { runCatching { mgr.connect("id", "h", "L", testCfg("hang")) } }
            reachedHandshake.await() // 前提：确实进到了 connect 里
            job.cancel()
            withTimeout(AWAIT_CLOSE_MS) { produced.single().awaitClose() }
        }
        assertClosedOffTheMainThread(produced.single())
    }
}

/** 自造主线程的名字，只用于可读输出；断言比对象身份。 */
private const val MAIN_THREAD_NAME = "aterm-fake-main"

/** 等后台把 close 做掉的上限。真 IO dispatcher 是毫秒级派发，这个值只是防挂死。 */
private const val AWAIT_CLOSE_MS = 5_000L

/** 整条用例的兜底上限，比 [AWAIT_CLOSE_MS] 宽，让内层超时先报出更具体的信息。 */
private const val TEST_TIMEOUT_MS = 15_000L
