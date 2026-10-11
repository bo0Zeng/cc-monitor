package com.ccmonitor.mobile.core.ssh

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * 保活间隔必须在 `connect` 之前就在 client 上。
 *
 * sshj 0.39 的契约：保活线程只在 `onConnect` 那一刻、`keepAliveInterval` 已经 > 0 时才起；连上之后再设，线程不会起来，
 * 保活等于关着（mobcore 实测：对端不说话，鉴权后才设的那一版约 190 s 都没判死；连接前设 15 s 的那一版 90.1 s 判死）。
 * 所以量的是「造出来、还没连的那个 client」上的间隔：生产里直连与经跳板都从 [newSshClient] 造 client。
 */
class KeepAliveBeforeConnectTest {
    @Test
    fun `造出来还没连的 client 上已经装好保活间隔`() {
        val config = ConnectionConfig(host = "example.com", username = "u", auth = AuthMethod.Password("x"), keepAliveIntervalSec = 15)
        assertEquals(15, newSshClient(config).connection.keepAlive.keepAliveInterval)
    }
}
