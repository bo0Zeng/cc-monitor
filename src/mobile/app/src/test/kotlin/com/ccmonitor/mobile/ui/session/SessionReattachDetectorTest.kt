package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.remote.ExecResult
import com.ccmonitor.mobile.core.ssh.ConnectionConfig
import com.ccmonitor.mobile.core.ssh.SessionStatus
import com.ccmonitor.mobile.core.ssh.SftpEntry
import com.ccmonitor.mobile.core.ssh.ShellChannel
import com.ccmonitor.mobile.core.ssh.SshConnectEvent
import com.ccmonitor.mobile.core.ssh.SshTransport
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * [SessionReattachDetector] 的断链恢复场景。断挂检测只依赖 flow 与 lambda，所以能在纯 JVM 上确定性驱动。
 *
 * 覆盖三条真实 StateFlow 接线：断因闩收集器（钉在断流上升沿、之后不漂移）与 reattachNeeded combine。
 * ShouldReattachTest 已覆纯判定；本测覆接线——断因随连接身份/活性正确计算，且 exit 后连接独立重建不诈尸。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SessionReattachDetectorTest {
    /** 极简 [SshTransport] 假体——本测只关心 [isConnected] + 对象恒等（===）；其余方法不触及。 */
    private class FakeConn(
        override val isConnected: Boolean = true,
    ) : SshTransport {
        override fun close() = Unit

        override suspend fun connect(
            configs: List<ConnectionConfig>,
            onEvent: ((SshConnectEvent) -> Unit)?,
        ) = error("unused")

        override suspend fun openShell(
            term: String,
            cols: Int,
            rows: Int,
        ): ShellChannel = error("unused")

        override fun execStream(command: String): Flow<ByteArray> = error("unused")

        override suspend fun execCapture(command: String): ExecResult = error("unused")

        override suspend fun execDuplex(command: String): com.ccmonitor.mobile.core.remote.RemoteDuplex = error("unused")

        override suspend fun sftpList(path: String): List<SftpEntry> = error("unused")

        override suspend fun sftpRealPath(path: String): String = error("unused")

        override suspend fun sftpDownload(
            remotePath: String,
            local: File,
        ) = error("unused")

        override suspend fun sftpUpload(
            local: File,
            remotePath: String,
        ) = error("unused")

        override suspend fun sftpRename(
            from: String,
            to: String,
        ) = error("unused")

        override suspend fun sftpDelete(
            path: String,
            isDir: Boolean,
        ) = error("unused")

        override suspend fun sftpMkdir(path: String) = error("unused")

        override suspend fun sftpReadText(
            path: String,
            maxBytes: Int,
        ): String = error("unused")

        override suspend fun sftpReadTextForEdit(
            path: String,
            maxBytes: Int,
        ): String? = error("unused")

        override suspend fun sftpWriteText(
            remotePath: String,
            text: String,
        ) = error("unused")
    }

    /** 接线脚手架：可变 live/opened + 输入流 + detector；后台常订 reattachNeeded 保 WhileSubscribed 计算。 */
    private class Harness(
        scope: TestScope,
    ) {
        val terminalDropped = MutableStateFlow(false)
        val status = MutableStateFlow<SessionStatus?>(SessionStatus.CONNECTED)
        var live: SshTransport? = null
        var opened: SshTransport? = null
        val detector =
            SessionReattachDetector(scope.backgroundScope, terminalDropped, status, { live }) { opened }

        init {
            // reattachNeeded 是 WhileSubscribed(5s) → 需活跃订阅方才计算。挂后台常订（backgroundScope 不被 runTest 等待）。
            scope.backgroundScope.launch { detector.reattachNeeded.collect {} }
        }
    }

    // 断在当前活连接同一对象（exit 只关通道、transport 未死）→ 判用户 exit → 不重挂；
    // 且断因闩钉死后，那条连接独立死掉+重建（live 换新对象、status 抖动）仍不重挂（不诈尸）。
    @Test
    fun userExitDropIsNotReattachedEvenAfterConnectionRebuilt() =
        runTest(UnconfinedTestDispatcher()) {
            val h = Harness(this)
            val connA = FakeConn(isConnected = true)
            h.opened = connA
            h.live = connA // 断的那一刻，shell 连接仍是当前活着的同一对象
            h.detector.notifyNewShellOpened() // 新 shell：断因闩清零
            advanceUntilIdle()

            h.terminalDropped.value = true // 断流上升沿 → 收集器判 exit（opened===live 且 isConnected）
            advanceUntilIdle()
            assertFalse("用户 exit（断在同一活连接）→ 不重挂", h.detector.reattachNeeded.value)

            // connA 之后独立死掉、后台重建出 connB（live 换新对象），status 抖动驱动 combine 重算。
            h.live = FakeConn(isConnected = true) // connB
            h.status.value = SessionStatus.RECONNECTING
            advanceUntilIdle()
            h.status.value = SessionStatus.CONNECTED
            advanceUntilIdle()
            assertFalse("不诈尸：exit 后连接独立重建，断因闩不漂移，仍不重挂", h.detector.reattachNeeded.value)
        }

    // 断在非当前活连接（原连接已死、后台重建出新 live 连接）→ 判连接死 → 有 live 连接可挂 → 重挂。
    @Test
    fun connectionDeathDropWithLiveConnReattaches() =
        runTest(UnconfinedTestDispatcher()) {
            val h = Harness(this)
            val connA = FakeConn(isConnected = true)
            h.opened = connA // shell 开在 connA
            h.detector.notifyNewShellOpened()
            advanceUntilIdle()

            h.live = FakeConn(isConnected = true) // 后台已重建 connB（≠ connA）
            h.terminalDropped.value = true // 断的那一刻 opened(connA) !== live(connB) → 非 exit
            advanceUntilIdle()
            assertTrue("连接死（断在非当前活连接）+ 有 live 连接 → 重挂", h.detector.reattachNeeded.value)
        }

    // 断因非 exit 但尚无 live 连接（后台重连未成）→ 暂不重挂（liveConnActive=false）；重连成功后再挂。
    @Test
    fun connectionDeathDropWithoutLiveConnWaits() =
        runTest(UnconfinedTestDispatcher()) {
            val h = Harness(this)
            val connA = FakeConn(isConnected = true)
            h.opened = connA
            h.detector.notifyNewShellOpened()
            advanceUntilIdle()

            h.live = null // 后台重连尚未成
            h.status.value = SessionStatus.RECONNECTING // 掉线后台重连中
            h.terminalDropped.value = true
            advanceUntilIdle()
            assertFalse("无 live 连接（后台重连未成）→ 暂不重挂", h.detector.reattachNeeded.value)

            // 重连成功：live 出现（≠ 已死的 connA）+ status RECONNECTING→CONNECTED（真变化，驱动 combine 重查 live）→ 重挂。
            h.live = FakeConn(isConnected = true)
            h.status.value = SessionStatus.CONNECTED
            advanceUntilIdle()
            assertTrue("后台重连成功出 live 连接 → 重挂", h.detector.reattachNeeded.value)
        }

    // 健康路径：未断（terminalDropped=false）→ 恒不重挂（无论 live 连接如何）。
    @Test
    fun notDroppedNeverReattaches() =
        runTest(UnconfinedTestDispatcher()) {
            val h = Harness(this)
            h.opened = FakeConn()
            h.live = FakeConn(isConnected = true)
            h.detector.notifyNewShellOpened()
            advanceUntilIdle()
            assertFalse("未断 → 不重挂", h.detector.reattachNeeded.value)
        }
}
