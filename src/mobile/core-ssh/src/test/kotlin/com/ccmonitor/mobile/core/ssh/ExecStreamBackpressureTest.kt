package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.util.concurrent.CountDownLatch
import kotlin.concurrent.thread

/**
 * execStream 有界缓冲与背压，直接测读线程的泵 [pumpStreamToChannel]（真 execStream 需要活的 sshj session）：
 * 1. 慢消费者 + 满缓冲：阻塞等待，不丢块不乱序（丢一块就是 JSONL 断行）。
 * 2. 通道关闭或取消会释放阻塞中的泵，否则 awaitClose 里的 `reader.join` 会泄漏线程。
 * 3. EOF 正常收尾；读错误原样上抛。
 */
class ExecStreamBackpressureTest {
    @Test
    fun slowConsumerWithBoundedChannelReceivesEveryByteInOrder() {
        runBlocking {
            // 100 多块（末块不满，覆盖部分块路径），容量只有 4，泵会反复满了阻塞
            val data = ByteArray(100 * 64 + 37) { (it % 251).toByte() }
            val channel = Channel<ByteArray>(capacity = 4)
            val pump =
                thread(name = "h6-pump") {
                    pumpStreamToChannel(ByteArrayInputStream(data), channel, chunkSize = 64)
                    channel.close()
                }
            val received = ByteArrayOutputStream()
            for (chunk in channel) {
                received.write(chunk)
                delay(1) // 故意慢消费，制造持续背压
            }
            pump.join(10_000)
            assertFalse("EOF 后泵线程应退出", pump.isAlive)
            assertArrayEquals("慢消费者 + 有界缓冲：全部字节到达、顺序不变、一块不丢", data, received.toByteArray())
        }
    }

    @Test
    fun cancelledChannelReleasesBlockedPumpWithoutSpin() {
        runBlocking {
            val endless =
                object : InputStream() {
                    override fun read(): Int = 42

                    override fun read(
                        b: ByteArray,
                        off: Int,
                        len: Int,
                    ): Int {
                        b.fill(42, off, off + len)
                        return len
                    }
                }
            val channel = Channel<ByteArray>(capacity = 1)
            val pump = thread(name = "h6-pump") { pumpStreamToChannel(endless, channel, chunkSize = 16) }
            channel.receive() // 泵已开始产出；无限源加容量 1，随即满了阻塞
            channel.cancel() // 相当于下游取消
            pump.join(5_000)
            assertFalse("channel 取消须唤醒阻塞在 trySendBlocking 的泵并使其退出（不自旋）——awaitClose join 依赖此", pump.isAlive)
        }
    }

    @Test
    fun readErrorPropagatesToCaller() {
        val boom =
            object : InputStream() {
                override fun read(): Int = throw IOException("session closed")

                override fun read(
                    b: ByteArray,
                    off: Int,
                    len: Int,
                ): Int = throw IOException("session closed")
            }
        val channel = Channel<ByteArray>(capacity = 4)
        val thrown = CountDownLatch(1)
        val pump =
            thread(name = "h6-pump") {
                runCatching { pumpStreamToChannel(boom, channel, chunkSize = 16) }
                    .onFailure { if (it is IOException) thrown.countDown() }
            }
        pump.join(5_000)
        assertTrue("读错误应原样上抛（execStream reader 以 close(t) 收尾流）", thrown.count == 0L)
    }
}
