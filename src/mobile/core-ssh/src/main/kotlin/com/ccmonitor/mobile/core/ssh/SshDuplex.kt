package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.remote.RemoteDuplex
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.withContext
import net.schmizz.sshj.connection.channel.direct.Session
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.atomic.AtomicBoolean

/**
 * sshj 上的 [RemoteDuplex]：一条 exec（不分配 PTY），stdin 写得进、stdout 读得出、stderr 排空留尾。
 *
 * 取消 / [close] 关 session：解开阻塞的 socket read、杀远端命令。
 */
internal class SshDuplex(
    private val session: Session,
    private val cmd: Session.Command,
) : RemoteDuplex {
    private val closed = AtomicBoolean(false)
    private val collected = AtomicBoolean(false)
    private val err = StringBuilder()
    private val writeLock = Any()

    init {
        // stderr 与 stdout 共用通道窗口：不读就把 stdout 一起拖停。
        Thread({ drain(cmd.errorStream) }, "ssh-duplex-stderr").apply {
            isDaemon = true
            start()
        }
    }

    private fun drain(input: InputStream) {
        runCatching {
            val buf = ByteArray(STDERR_CHUNK)
            while (true) {
                val n = input.read(buf)
                if (n < 0) break
                synchronized(err) {
                    err.append(String(buf, 0, n, Charsets.UTF_8))
                    if (err.length > STDERR_TAIL_CHARS) err.delete(0, err.length - STDERR_TAIL_CHARS)
                }
            }
        }
    }

    override val stdout: Flow<ByteArray> =
        callbackFlow {
            check(collected.compareAndSet(false, true)) { "RemoteDuplex.stdout 只许收集一次" }
            val reader =
                Thread({
                    try {
                        pumpStreamToChannel(cmd.inputStream, this@callbackFlow, READ_CHUNK)
                        close()
                    } catch (t: IOException) {
                        close(t)
                    }
                }, "ssh-duplex-stdout").apply {
                    isDaemon = true
                    start()
                }
            awaitClose {
                shut()
                runCatching { reader.join(JOIN_MS) }
            }
        }.buffer(BUFFER_CHUNKS).flowOn(Dispatchers.IO)

    private val stdin: OutputStream get() = cmd.outputStream

    override suspend fun write(bytes: ByteArray) {
        if (closed.get()) throw IOException("双向 exec 已关")
        withContext(Dispatchers.IO) {
            synchronized(writeLock) {
                stdin.write(bytes)
                stdin.flush()
            }
        }
    }

    override val exitStatus: Int? get() = cmd.exitStatus

    override val stderrTail: String get() = synchronized(err) { err.toString() }

    override fun close() = shut()

    private fun shut() {
        if (!closed.compareAndSet(false, true)) return
        runCatching { cmd.close() }
        runCatching { session.close() }
    }

    private companion object {
        const val READ_CHUNK = 8 * 1024
        const val STDERR_CHUNK = 1024
        const val STDERR_TAIL_CHARS = 4 * 1024
        const val BUFFER_CHUNKS = 64
        const val JOIN_MS = 500L
    }
}
