package com.ccmonitor.mobile.core.ssh

import net.schmizz.sshj.connection.channel.direct.Session
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.OutputStream
import java.lang.reflect.Proxy
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * 关常驻流那条双向 exec 不卡调用方：sshj 关通道要等对端回 CLOSE，死连接（黑洞）上会一直等。
 * 常驻流换一条时旧流在 BackendFeed 的收尾里关，卡住就是「换一条要多等」（net2 台子：刚断过 · 复用多等 1 秒）。
 */
class SshDuplexCloseTest {
    private val peerGone = CountDownLatch(1) // 对端不回话：close 一直等到这里放行

    private inline fun <reified T> fake(crossinline on: (String) -> Any?): T =
        Proxy.newProxyInstance(T::class.java.classLoader, arrayOf(T::class.java)) { _, m, _ ->
            on(m.name)
        } as T

    private fun blockingClose(name: String): Any? =
        when (name) {
            "close" -> peerGone.await().let { null }
            "getInputStream", "getErrorStream" -> ByteArrayInputStream(ByteArray(0))
            "getOutputStream" -> OutputStream.nullOutputStream()
            "isOpen" -> false
            "hashCode" -> 0
            "toString" -> "fake"
            else -> null
        }

    @Test
    fun `关流不等对端回话：调用方当场返回`() {
        val session = fake<Session> { blockingClose(it) }
        val cmd = fake<Session.Command> { blockingClose(it) }
        val d = SshDuplex(session, cmd)
        val returned = CountDownLatch(1)
        Thread { d.close().also { returned.countDown() } }.start()
        try {
            assertTrue("对端一直不回话，close 也得返回", returned.await(10, TimeUnit.SECONDS))
        } finally {
            peerGone.countDown()
        }
    }
}
