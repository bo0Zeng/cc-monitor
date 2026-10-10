package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.transport.LineFramer
import com.ccmonitor.mobile.core.remote.RemoteDuplex
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/** 一条入方向命令的应答（`IPC-PROTOCOL.md` §4）。 */
sealed interface Reply {
    /** `ok: true`；`data` 是命令的返回值（无返回值 ⇒ `null`）。 */
    data class Ok(
        val data: Any?,
    ) : Reply

    /** `ok: false`：核心写好的失败（`message` 上屏，`detail` 进复制详情）。 */
    data class Failed(
        val failure: CoreFailure,
    ) : Reply

    /** 到了客户端给的期限（已发 `cancel` 撤单）。 */
    data object TimedOut : Reply

    /** 后端回 `cancelled`。 */
    data object Cancelled : Reply

    /** 流断了（写不进去 · 读到头 · 关了）。 */
    data object LinkDown : Reply

    /** 应答帧缺必填格（两端契约对不上）：原样带回。 */
    data class Unreadable(
        val raw: String,
    ) : Reply
}

/** 接常驻流的结局。 */
sealed interface AttachOutcome {
    data class Attached(
        val client: FrameClient,
    ) : AttachOutcome

    /**
     * 拒了：小中继连不上常驻后端（`absent` 没人在听 · `unreachable` 别的原因），或常驻后端嫌 attach 行形状不对
     * （`malformed-attach`）。`reason` 原样。
     */
    data class Refused(
        val reason: String?,
    ) : AttachOutcome

    /** 第一行不是 hello / attach 的回话认不出：那头不是 cc-monitor 后端。 */
    data class NotOurs(
        val line: String,
    ) : AttachOutcome

    /** 握手没说完就断了（[stage]：等 hello · 等 attach 回话）。 */
    data class Cut(
        val stage: Stage,
        val stderr: String,
    ) : AttachOutcome

    enum class Stage { HELLO, ATTACH_REPLY }
}

/**
 * R 通道上的帧客户端：一条 `ccm -- --resident-attach` 双向长 exec，上面一问一答（请求 `{id, cmd, args, within_ms}` ⇒
 * `reply`）＋ 出方向的帧。手机所有远端调用只经它一处（`架构.md` D5）。
 *
 * - 先读 `hello` 再写话（`IPC-PROTOCOL.md` §3）；`attach` 行带这条连接要的流旗标（`--tail-only` …）。
 * - `id` 由这里发号（`<连接 nonce>-<单调序号>`），后端只回显。
 * - 超时归客户端：[call] 的期限到了就发 `cancel` 撤单，回 [Reply.TimedOut]。
 * - 帧（`reply` / `cancelled` 以外的每一行）原样进 [frames]，只许一个消费方；它慢 ⇒ 这里停读 ⇒ SSH 窗口停发（不丢帧）。
 */
class FrameClient private constructor(
    private val duplex: RemoteDuplex,
    /** 那台的 hello 帧原样。 */
    val hello: Map<String, Any?>,
    private val lines: ReceiveChannel<String>,
    private val scope: CoroutineScope,
    private val nonce: String,
    /** 看的这一台的时区（IANA 名）：每条请求信封都带（回包里写成字的时刻按它写）。 */
    private val tz: String,
    private val onBreak: (String) -> Unit,
) {
    private val seq = AtomicLong(0)
    private val pending = ConcurrentHashMap<String, CompletableDeferred<Reply>>()
    private val down = AtomicBoolean(false)
    private val frameChannel = Channel<Map<String, Any?>>(FRAME_BUFFER)
    private val writeLock = Mutex()

    /** 撤了单的 id（与撤单那条命令自己的 id）：迟到的应答照收、不当契约对不上。 */
    private val abandoned: MutableSet<String> = ConcurrentHashMap.newKeySet()

    /** 出方向的帧（`hello` · `reply` · `cancelled` 以外的全部），按到达顺序；流断了 ⇒ 关上。 */
    val frames: ReceiveChannel<Map<String, Any?>> get() = frameChannel

    /** 流还通着。 */
    val isUp: Boolean get() = !down.get()

    private fun start(): Job =
        scope.launch {
            try {
                for (raw in lines) dispatch(raw)
            } finally {
                goDown()
            }
        }

    private suspend fun dispatch(raw: String) {
        if (raw.isBlank()) return
        val m = Json.obj(raw)
        if (m == null) {
            onBreak("不是 JSON 对象的一行：${raw.take(BREAK_SNIPPET)}")
            return
        }
        when (m.str("kind")) {
            "reply", "cancelled" -> answer(m, raw)
            null -> onBreak("帧没有 kind：${raw.take(BREAK_SNIPPET)}")
            else -> frameChannel.send(m)
        }
    }

    private fun answer(
        m: Map<String, Any?>,
        raw: String,
    ) {
        val id = m.str("id")
        val waiter = id?.let { pending.remove(it) }
        when {
            waiter != null -> waiter.complete(if (m.str("kind") == "cancelled") Reply.Cancelled else replyOf(m, raw))
            id != null && abandoned.remove(id) -> Unit
            else -> onBreak("应答对不上请求：${raw.take(BREAK_SNIPPET)}")
        }
    }

    private fun goDown() {
        if (!down.compareAndSet(false, true)) return
        frameChannel.close()
        pending.values.forEach { it.complete(Reply.LinkDown) }
        pending.clear()
        duplex.close()
    }

    private suspend fun writeLine(m: Map<String, Any?>) = writeLock.withLock { duplex.write((Json.line(m) + "\n").toByteArray(Charsets.UTF_8)) }

    /**
     * 发一条命令、等它的应答。[withinMs] 同时交给后端（`within_ms`：它据此收紧阻塞档命令的总期限）
     * 与这里自己掐表（到点撤单）。
     */
    suspend fun call(
        cmd: String,
        args: Map<String, Any?>? = null,
        withinMs: Long,
    ): Reply {
        val id = "$nonce-${seq.incrementAndGet()}"
        val answer = CompletableDeferred<Reply>()
        pending[id] = answer
        // 先登记再查：与 [goDown] 交错时，要么它清掉了这一条，要么这里看见它已断。
        if (down.get()) {
            pending.remove(id)
            return Reply.LinkDown
        }
        val request =
            buildMap<String, Any?> {
                put("id", id)
                put("cmd", cmd)
                if (args != null) put("args", args)
                put("within_ms", withinMs)
                put("tz", tz)
            }
        try {
            writeLine(request)
        } catch (_: IOException) {
            pending.remove(id)
            return Reply.LinkDown
        }
        return withTimeoutOrNull(withinMs) { answer.await() } ?: run {
            pending.remove(id)
            abandoned += id
            abandoned += "$id-cancel"
            runCatching { writeLine(mapOf("id" to "$id-cancel", "cmd" to "cancel", "args" to mapOf("target" to id))) }
            Reply.TimedOut
        }
    }

    /** 关流：未答的请求一律 [Reply.LinkDown]。 */
    fun close() {
        goDown()
        scope.cancel()
    }

    companion object {
        private const val LINE_BUFFER = 256
        private const val FRAME_BUFFER = 1024
        private const val BREAK_SNIPPET = 200

        /** 小中继连不上时第一行就是它（`{"attach":"refused","reason":…}`）。 */
        private fun refusal(m: Map<String, Any?>?): String? = if (m?.str("attach") == "refused") m.str("reason") ?: "" else null

        internal fun replyOf(
            m: Map<String, Any?>,
            raw: String,
        ): Reply =
            when (m.bool("ok")) {
                true -> Reply.Ok(m["data"])
                false -> CoreFailure.of(m)?.let { Reply.Failed(it) } ?: Reply.Unreadable(raw)
                null -> Reply.Unreadable(raw)
            }

        /**
         * 读 hello → 写 attach 行 → 等 `{"attach":"ok"}`。[parent] 是这条连接活多久：它取消 ⇒ 流关。
         * [onBreak] 收「两端契约对不上」的那几处（坏行 · 对不上的应答），一种报一次由调用方管。
         */
        suspend fun attach(
            duplex: RemoteDuplex,
            flags: List<String>,
            parent: CoroutineScope,
            handshakeMs: Long,
            nonce: String,
            tz: String,
            onBreak: (String) -> Unit,
        ): AttachOutcome {
            val scope = CoroutineScope(parent.coroutineContext + SupervisorJob(parent.coroutineContext[Job]))
            val lines = Channel<String>(LINE_BUFFER)
            scope.launch {
                val framer = LineFramer()
                try {
                    duplex.stdout.collect { chunk -> for (f in framer.feedFramed(chunk)) lines.send(f.raw) }
                    framer.flush()?.let { lines.send(it) }
                    lines.close()
                } catch (e: IOException) {
                    lines.close(e)
                }
            }

            return try {
                val hello = handshake(duplex, lines, flags, tz, handshakeMs)
                AttachOutcome.Attached(FrameClient(duplex, hello, lines, scope, nonce, tz, onBreak).also { it.start() })
            } catch (stop: Stop) {
                duplex.close()
                scope.cancel()
                stop.outcome
            }
        }

        /** 握手走不通：带着结局跳出 [handshake]（只在 [attach] 里接住）。 */
        private class Stop(
            val outcome: AttachOutcome,
        ) : RuntimeException()

        /** attach 行：流旗标 ＋ 旁边一格 `tz`（不进 `flags`；这条流推出去的钟面按它写）。 */
        private suspend fun sendAttach(
            duplex: RemoteDuplex,
            flags: List<String>,
            tz: String,
        ) {
            try {
                duplex.write((Json.line(mapOf("attach" to true, "flags" to flags, "tz" to tz)) + "\n").toByteArray(Charsets.UTF_8))
            } catch (_: IOException) {
                throw Stop(AttachOutcome.Cut(AttachOutcome.Stage.ATTACH_REPLY, duplex.stderrTail))
            }
        }

        /** 读 hello → 写 attach 行 → 等 `{"attach":"ok"}`；回 hello。走不通 ⇒ 抛 [Stop]。 */
        private suspend fun handshake(
            duplex: RemoteDuplex,
            lines: ReceiveChannel<String>,
            flags: List<String>,
            tz: String,
            handshakeMs: Long,
        ): Map<String, Any?> {
            suspend fun next(stage: AttachOutcome.Stage): Pair<String, Map<String, Any?>?> {
                val line =
                    withTimeoutOrNull(handshakeMs) { lines.receiveCatching().getOrNull() }
                        ?: throw Stop(AttachOutcome.Cut(stage, duplex.stderrTail))
                val m = Json.obj(line)
                refusal(m)?.let { throw Stop(AttachOutcome.Refused(it)) }
                return line to m
            }

            fun ours(
                line: String,
                m: Map<String, Any?>?,
                ok: (Map<String, Any?>) -> Boolean,
            ): Map<String, Any?> = m?.takeIf(ok) ?: throw Stop(AttachOutcome.NotOurs(line))
            val (first, m1) = next(AttachOutcome.Stage.HELLO)
            val hello = ours(first, m1) { it.str("kind") == "hello" }
            sendAttach(duplex, flags, tz)
            val (second, m2) = next(AttachOutcome.Stage.ATTACH_REPLY)
            ours(second, m2) { it.str("attach") == "ok" }
            return hello
        }
    }
}
