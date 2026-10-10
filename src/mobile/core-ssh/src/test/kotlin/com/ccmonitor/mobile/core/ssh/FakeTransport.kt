package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.remote.ExecResult
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.Flow
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

/**
 * [SshTransport] 测试替身。connect 行为由 [connectBehavior] 注入（成功、delay、throw、[awaitClose] 阻塞）；
 * shell 与 sftp 一律抛，manager 的测试不碰它们。
 */
class FakeTransport(
    private val connectBehavior: suspend FakeTransport.(ConnectionConfig) -> Unit = {},
) : SshTransport {
    val connectCalls = AtomicInteger(0)
    val probeCalls = AtomicInteger(0) // 主动探测真正被调用的次数，供断言探测分支确实运行

    @Volatile
    var closed = false
        private set

    /**
     * [close] 落在哪条线程上，供「close 不在主线程」的测试断言。
     *
     * 注意：存的是 `Thread` 对象，不是线程名。kotlinx-coroutines 调试模式会给跑协程的线程改名，按名字比恒绿。
     */
    @Volatile
    var closedOnThread: Thread? = null
        private set

    @Volatile
    private var alive = false

    private val closeSignal = CompletableDeferred<Unit>()

    override val isConnected: Boolean get() = alive && !closed

    /** probeAlive 桩值。null（默认）走接口默认实现（isConnected）；false 模拟 isConnected 仍为 true 的死 TCP。 */
    @Volatile
    var probeAliveResult: Boolean? = null

    /** 非 null 时 probeAlive 先 await 它再出结果，用来拉开探测在飞的窗口、注入并发 disconnect。 */
    @Volatile
    var probeGate: CompletableDeferred<Unit>? = null

    override suspend fun probeAlive(timeoutMs: Long): Boolean {
        probeCalls.incrementAndGet()
        probeGate?.await()
        return probeAliveResult ?: isConnected
    }

    override suspend fun connect(
        configs: List<ConnectionConfig>,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) {
        connectCalls.incrementAndGet()
        // 只代表一次连接，用首地址。
        val config = configs.first()
        // 发合成事件，供断言 onEvent 被转发（带 host:port）。
        onEvent?.invoke(SshConnectEvent(config.host, config.port, SshConnectEvent.Stage.TCP, "fake connecting"))
        connectBehavior(config) // 可 delay / throw / awaitClose
        alive = true
        onEvent?.invoke(SshConnectEvent(config.host, config.port, SshConnectEvent.Stage.ESTABLISHED, "fake established"))
    }

    /** 供 connectBehavior 用：挂起直到 [close] 被调用，相当于阻塞在 connect 的连接被关闭而中止。 */
    suspend fun awaitClose() = closeSignal.await()

    /** 模拟底层连接掉线（不 close）：isConnected 变 false。 */
    fun simulateDrop() {
        alive = false
    }

    override fun close() {
        // 注意：线程要在置 closed 之前记。等在 [awaitClose] 上的测试一被唤醒就会读它，顺序反了会读到 null。
        closedOnThread = Thread.currentThread()
        closed = true
        alive = false
        closeSignal.complete(Unit)
    }

    private fun nope(): Nothing = throw NotImplementedError("FakeTransport 仅供 manager 逻辑测，未实现 shell/sftp")

    override suspend fun openShell(
        term: String,
        cols: Int,
        rows: Int,
    ): ShellChannel = nope()

    override fun execStream(command: String): Flow<ByteArray> = nope()

    override suspend fun execCapture(command: String): ExecResult = nope()

    override suspend fun sftpList(path: String): List<SftpEntry> = nope()

    override suspend fun sftpRealPath(path: String): String = nope()

    override suspend fun sftpDownload(
        remotePath: String,
        local: File,
    ) = nope()

    override suspend fun sftpUpload(
        local: File,
        remotePath: String,
    ) = nope()

    override suspend fun sftpRename(
        from: String,
        to: String,
    ) = nope()

    override suspend fun sftpDelete(
        path: String,
        isDir: Boolean,
    ) = nope()

    override suspend fun sftpMkdir(path: String) = nope()

    override suspend fun sftpReadText(
        path: String,
        maxBytes: Int,
    ): String = nope()

    override suspend fun sftpReadTextForEdit(
        path: String,
        maxBytes: Int,
    ): String? = nope()

    override suspend fun sftpWriteText(
        remotePath: String,
        text: String,
    ) = nope()
}
