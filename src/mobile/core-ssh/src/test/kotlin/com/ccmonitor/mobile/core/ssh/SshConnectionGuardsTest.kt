package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import net.schmizz.sshj.connection.channel.OpenFailException
import net.schmizz.sshj.transport.TransportException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import java.io.IOException
import java.io.InputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicInteger

/**
 * SshConnection 的几条守卫，在 JVM 上直接测守卫逻辑本身（真实的挂死命令与黑洞地址无法在 JVM 上造）：
 * - [readWithDeadline] 让带超时的读真正守约，挂死的远端命令不会把 IO 线程钉死。
 * - 探测异常分型、未连接时探测早返。
 * - 竞速赢家的 close 复查。
 * - `close()` 之后拒绝登记新 client。
 */
class SshConnectionGuardsTest {
    // readWithDeadline

    @Test
    fun readWithDeadlineTimesOutUnblocksViaOnTimeoutAndThrows() {
        runBlocking(Dispatchers.IO) {
            val unblock = CountDownLatch(1)
            val hungStream =
                object : InputStream() {
                    override fun read(): Int {
                        unblock.await() // 相当于挂死的远端命令：阻塞到 onTimeout 关流才返回
                        return -1
                    }
                }
            val start = System.currentTimeMillis()
            val ex =
                runCatching {
                    readWithDeadline(timeoutMs = 200, onTimeout = { unblock.countDown() }) {
                        hungStream.readBytes().toString(Charsets.UTF_8)
                    }
                }.exceptionOrNull()
            assertTrue("到点应抛 IOException 超时（而非把被掐断的部分输出当成功）", ex is IOException)
            assertTrue("错误信息应说明超时：${ex?.message}", ex!!.message!!.contains("超时"))
            assertTrue("确实等到了 deadline（非立即失败）", System.currentTimeMillis() - start >= 180)
        }
    }

    @Test
    fun readWithDeadlineFastPathReturnsResultWithoutWaitingTimeout() {
        runBlocking(Dispatchers.IO) {
            val start = System.currentTimeMillis()
            val out = readWithDeadline(timeoutMs = 60_000, onTimeout = { fail("快路径不应触发超时") }) { "ok" }
            assertEquals("ok", out)
            assertTrue("看门狗被取消，不等满 60s", System.currentTimeMillis() - start < 5_000)
        }
    }

    @Test
    fun readWithDeadlinePropagatesReadErrorWhenNotTimedOut() {
        runBlocking(Dispatchers.IO) {
            val ex =
                runCatching {
                    readWithDeadline<String>(timeoutMs = 60_000, onTimeout = {}) { throw IOException("read boom") }
                }.exceptionOrNull()
            assertEquals("未超时的读错误原样上抛", "read boom", ex?.message)
        }
    }

    // readWithDeadline 的可选 message

    @Test
    fun readWithDeadlineUsesCustomMessageOnTimeout() {
        runBlocking(Dispatchers.IO) {
            val unblock = CountDownLatch(1)
            val ex =
                runCatching {
                    readWithDeadline(timeoutMs = 200, onTimeout = { unblock.countDown() }, message = "打开终端通道超时：连接可能已失效") {
                        unblock.await()
                        "late"
                    }
                }.exceptionOrNull()
            assertTrue("到点抛 IOException", ex is IOException)
            assertEquals("自定义 message 原样作为超时错误文案", "打开终端通道超时：连接可能已失效", ex?.message)
        }
    }

    @Test
    fun readWithDeadlineDefaultMessageWhenNotProvided() {
        runBlocking(Dispatchers.IO) {
            val unblock = CountDownLatch(1)
            val ex =
                runCatching {
                    readWithDeadline(timeoutMs = 200, onTimeout = { unblock.countDown() }) {
                        unblock.await()
                        "late"
                    }
                }.exceptionOrNull()
            assertEquals("message 缺省时用默认文案", "命令执行超时（200ms），已中止", ex?.message)
        }
    }

    // 读完成与看门狗超时互斥：读成功返回时 onTimeout 一定没触发，超时抛错时 onTimeout 恰触发一次。
    // onTimeout 会关整条连接，误触就会关掉活连接。让 read 在 deadline 附近返回，制造交错。
    @Test
    fun readWithDeadlineOnTimeoutAndSuccessAreMutuallyExclusive() {
        runBlocking(Dispatchers.IO) {
            repeat(50) {
                val onTimeoutCount = AtomicInteger(0)
                val result =
                    runCatching {
                        readWithDeadline(timeoutMs = 2, onTimeout = { onTimeoutCount.incrementAndGet() }) {
                            Thread.sleep(2) // 在 deadline 附近返回，与看门狗交错
                            "ok"
                        }
                    }
                if (result.isSuccess) {
                    assertEquals("读成功返回 → onTimeout 必未触发（否则误关整条连接）", 0, onTimeoutCount.get())
                } else {
                    assertTrue("超时应抛 IOException", result.exceptionOrNull() is IOException)
                    assertEquals("超时 → onTimeout 恰触发一次", 1, onTimeoutCount.get())
                }
            }
        }
    }

    // 探测异常分型：通道级拒绝说明链路活，不能关整条连接

    @Test
    fun probeExceptionMeansAliveClassifiesChannelRejectAsAlive() {
        assertTrue(
            "OpenFailException（服务器 CHANNEL_OPEN_FAILURE，如 MaxSessions 打满）→ 链路活",
            probeExceptionMeansAlive(OpenFailException("h", OpenFailException.Reason.ADMINISTRATIVELY_PROHIBITED, "MaxSessions")),
        )
        assertFalse("TransportException（传输层死）→ 连接死", probeExceptionMeansAlive(TransportException("dead")))
        assertFalse("readWithDeadline 超时 IOException → 连接死", probeExceptionMeansAlive(IOException("命令执行超时（5000ms），已中止")))
    }

    // 未连接时探测早返，不触网络

    @Test
    fun probeAliveReturnsFalseWhenNotConnectedWithoutNetwork() =
        runBlocking {
            val conn = SshConnection(null) // 未 connect：早返 false，不发起任何 socket
            assertFalse("未连接 → 探测 false", conn.probeAlive(5_000))
        }

    // 竞速赢家的 close 复查。真实的幻影胜出需要真 SSH 服务器和精确落窗，这里直接测 connect 尾部调用的判定。

    @Test
    fun closeWinnerIfClosedDuringRaceClosesWinnerAndThrowsCe() {
        var closedWinner: String? = null
        val ex =
            runCatching {
                closeWinnerIfClosedDuringRace(winner = "C", closed = true) { closedWinner = it }
            }.exceptionOrNull()
        assertTrue("closed → 抛 CancellationException（对齐取消路径，doConnect 不标 ERROR）", ex is CancellationException)
        assertEquals("赢家必被关（不返回幻影成功 → 杜绝活连接泄漏）", "C", closedWinner)
    }

    @Test
    fun closeWinnerIfClosedDuringRacePassesThroughWhenNotClosed() {
        var closedWinner: String? = null
        closeWinnerIfClosedDuringRace(winner = "C", closed = false) { closedWinner = it } // 不抛
        assertFalse("未 closed → 正常胜出，不关赢家", "C" == closedWinner)
    }

    // close 之后拒绝登记新 client

    @Test
    fun connectAfterCloseFailsFastInsteadOfEscapingWatchdog() =
        runBlocking {
            val conn = SshConnection(null)
            conn.close()
            conn.close() // 幂等：重复 close 无害
            val cfg = ConnectionConfig(host = "192.0.2.1", port = 22, username = "u", auth = AuthMethod.Password("p"))
            val ex = runCatching { conn.connect(listOf(cfg)) }.exceptionOrNull()
            assertTrue("close 后 connect 应快速失败（聚合 IOException）：$ex", ex is IOException)
            assertTrue(
                "失败原因应是登记窗守卫拒绝（连接已关闭），而非真发起网络连接：${ex?.message}",
                ex!!.message!!.contains("连接已关闭"),
            )
        }
}
