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
    /** 拨号身份：[again] 出来的那一条与它同一个（拨号请求一字不差）。 */
    private val identity: Int = NEXT_IDENTITY.incrementAndGet(),
) : SshTransport {
    val connectCalls = AtomicInteger(0)

    /** 经 [again] 从这一条派生出去的那几条（SSH 那层跟着共用循环换连接时走它）。 */
    val derived = mutableListOf<FakeTransport>()

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

    private val drops = mutableListOf<() -> Unit>()

    override val dialRequest: String get() = "{\"identity\":$identity}"

    override fun again(): SshTransport = FakeTransport(connectBehavior, identity).also { synchronized(derived) { derived += it } }

    override fun onDrop(f: () -> Unit) {
        synchronized(drops) { drops += f }
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

    /** 模拟底层连接掉线（不 close）：isConnected 变 false，[onDrop] 登记的那几个各叫一次。 */
    fun simulateDrop() {
        alive = false
        synchronized(drops) { drops.toList() }.forEach { it() }
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

    private companion object {
        val NEXT_IDENTITY = AtomicInteger(0)
    }
}
