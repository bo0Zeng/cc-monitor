package com.ccmonitor.mobile.core.terminal

import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.Closeable
import java.io.IOException
import java.util.concurrent.CountDownLatch

private const val JOIN_TIMEOUT_MS = 5_000L

/**
 * [closeRemoteChannelInBackground] 直测：远端通道的 close 是网络写，须在后台守护线程执行
 * （TerminalSession.close 在 onCleared/retry 主线程被调，同步关会抛 NetworkOnMainThreadException）。
 * [TerminalSession] 本体含 termlib 原生 emulator（JNI + Looper），纯 JVM 不可构造，所以直测这个 helper；
 * close()→helper 的连线与断开后远端无残留要在设备上验。
 */
class RemoteChannelCloseTest {
    @Test
    fun closesChannelOnBackgroundDaemonThread() {
        var closedOn: Thread? = null
        val t = closeRemoteChannelInBackground(Closeable { closedOn = Thread.currentThread() })
        assertNotNull("有通道应派发关闭线程", t)
        t!!.join(JOIN_TIMEOUT_MS)
        assertNotNull("close 应已执行", closedOn) // join 建立 happens-before，读安全
        assertNotSame("close 不得在调用线程执行", Thread.currentThread(), closedOn)
        assertTrue("守护线程（不阻进程退出）", t.isDaemon)
    }

    @Test
    fun nullChannelIsNoopWithoutThread() {
        assertNull("无通道 → 不起线程", closeRemoteChannelInBackground(null))
    }

    @Test
    fun closeExceptionIsSwallowed() {
        var uncaught: Throwable? = null
        val gate = CountDownLatch(1) // 先挂住 close，装好 uncaughtExceptionHandler 再放行（免竞态）
        val t =
            closeRemoteChannelInBackground(
                Closeable {
                    gate.await()
                    throw IOException("boom")
                },
            )!!
        t.setUncaughtExceptionHandler { _, e -> uncaught = e }
        gate.countDown()
        t.join(JOIN_TIMEOUT_MS)
        assertNull("close 抛异常应被吞（不冒到线程未捕获处理器）", uncaught)
    }
}
