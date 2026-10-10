package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.command.PipeCommands
import com.ccmonitor.mobile.core.claude.transport.DaemonCommands
import com.ccmonitor.mobile.core.claude.transport.JsonlFrame
import com.ccmonitor.mobile.core.claude.transport.TailTransport
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.squareup.moshi.Moshi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.toList
import java.util.concurrent.atomic.AtomicInteger
import kotlin.random.Random

/**
 * 把常驻管道的两头接起来：下行 `out.ndjson`，上行 `in.ndjson`。
 *
 * 下行复用 [TailTransport]（`tail -c +N -F`），只把每行喂给 [CliFrameEncoder]。
 * 注意：`out.ndjson` 是管道 stdout 的重定向，内容是 CLI stream-json，不是 `~/.claude/projects/` 下的会话 JSONL；
 * 两者只是都用「追加文本行 + 字节 offset」这套传输。
 *
 * 本类不拼远端命令串，命令都由 [PipeCommands] 出。这里只管协议帧的形状：那一行 JSON 长什么样
 * （[userLineJson] / [interruptLineJson]）、针怎么铸（[uplinkIdNeedle]）、远端答什么算哪一档（[appendResultOf]）。
 */
class PipeSession(
    private val channel: RemoteCommandChannel,
    private val sessionId: String,
) {
    /**
     * 下行：管道事件文件的每一行 → 帧。tail 哪个文件由 [PipeCommands.eventsPath] 答。
     */
    fun frames(startOffsetBytes: Long = 0L): Flow<BridgeFrame> =
        flow {
            val enc = CliFrameEncoder()
            TailTransport(channel, PipeCommands.eventsPath(sessionId), startOffsetBytes).frames().collect { fr ->
                // `Hello` 是 `TailTransport` 自己的开场白，不是管道的帧
                val raw = (fr as? JsonlFrame.Line)?.raw ?: return@collect
                // 坏行不弄崩整条流：`tail -F` 在文件截断或轮转时可能给出半行。
                val row = runCatching { ADAPTER.fromJson(raw) as? Map<*, *> }.getOrNull() ?: return@collect
                // 首行是 `{"__meta__":…}`，其后是 `{"t_ns":…,"event":…}`；下游只认这个信封。
                if (row.containsKey("__meta__")) return@collect
                val line = row["event"] as? Map<*, *> ?: return@collect
                enc.feed(line).forEach { emit(it) }
            }
        }
            // JSON 解析与编码不占主线程：收集者跑在 Main 上。
            // 用单 worker 的调度器，[CliFrameEncoder] 的内部状态因此只被一个线程碰。
            .flowOn(PARSE_DISPATCHER)

    /**
     * 上行：同一条远端命令里先查标识在不在，不在才追加。行的形状：
     * `{"type":"user","aterm_id":"aterm-<origin>-local#<n>","message":{"role":"user","content":"…"}}`
     *
     * 命令串由 [PipeCommands.idempotentAppendCommand] 出，本函数只给那一行 JSON 与那根针。
     */
    fun idempotentAppendCommand(
        text: String,
        uplinkId: String,
    ): String =
        PipeCommands.idempotentAppendCommand(
            sessionId = sessionId,
            payload = userLineJson(text, uplinkId),
            needle = uplinkIdNeedle(uplinkId),
        )

    /**
     * 上行的控制面：往 `in.ndjson` 追一行 `control_request`，与 send 走同一个文件、同一条 `tail -f`。
     *
     * ```json
     * {"type":"control_request","request_id":"req_<n>_<8hex>","request":{"subtype":"interrupt"}}
     * ```
     *
     * `requestId` 由调用方铸并保证不重（[newInterruptRequestId]）。不读回 `control_response` 做配对。
     */
    fun interruptCommand(requestId: String): String =
        PipeCommands.appendLineCommand(sessionId, interruptLineJson(requestId))

    companion object {
        private val ADAPTER = Moshi.Builder().build().adapter(Any::class.java)

        /** 下行解析跑在这里。单 worker，见 [frames]。 */
        @OptIn(ExperimentalCoroutinesApi::class)
        internal val PARSE_DISPATCHER: CoroutineDispatcher = Dispatchers.Default.limitedParallelism(1)

        /**
         * 幂等标识在帧上的键名。`claude` 的 stream-json 输入容忍未知键，所以标识直接放在那一行 JSON 里。
         * 带 `aterm_` 前缀，免得撞上 CLI 自己的键而静默互相覆盖。
         */
        const val UPLINK_ID_KEY: String = "aterm_id"

        /**
         * 上行那一行：`{"type":"user","aterm_id":"…","message":{"role":"user","content":"…"}}`。
         *
         * 用 Moshi 序列化，正文含引号、换行、反斜杠也不会坏。
         * [uplinkId] 没有默认值：没有标识的行在远端永远查不到，每次重试都会再追加。
         */
        fun userLineJson(
            text: String,
            uplinkId: String,
        ): String =
            ADAPTER.toJson(
                buildMap {
                    put("type", "user")
                    put(UPLINK_ID_KEY, uplinkId)
                    put("message", mapOf("role" to "user", "content" to text))
                },
            )

        /**
         * 远端 `grep -qF` 用的针：整个键值对 `"aterm_id":"<id>"`，不是裸标识。
         *
         * 裸标识会让正文里恰好写了同一串的消息被判成重复而吞掉；正文经 Moshi 序列化后引号都变成 `\"`，
         * 构造不出这个键值对。注意：针必须是 [userLineJson] 输出的字面子串，否则每次都会追加。
         */
        fun uplinkIdNeedle(uplinkId: String): String = "\"$UPLINK_ID_KEY\":\"$uplinkId\""

        /**
         * 读 [idempotentAppendCommand] 的 stdout，判是哪一档。
         *
         * @param body `DaemonCommands.bodyOrNullIfFailed` 剥掉成功标记之后的正文。
         * @return `null` = 认不出两个哨兵中的任何一个；调用方必须当失败。
         */
        fun appendResultOf(body: String): UplinkAppendOutcome? =
            when (body.trim()) {
                PipeCommands.APPENDED_MARKER -> UplinkAppendOutcome.Appended
                PipeCommands.DUPLICATE_MARKER -> UplinkAppendOutcome.AlreadyThere
                else -> null
            }

        /**
         * 中断那一行，形状同 Claude Agent SDK 发的 `control_request`。
         * 三个键名不能改：改了 CLI 会当普通行忽略，远端照跑而 app 停在「正在让远端停下」。
         */
        fun interruptLineJson(requestId: String): String =
            ADAPTER.toJson(
                mapOf(
                    "type" to "control_request",
                    "request_id" to requestId,
                    "request" to mapOf("subtype" to INTERRUPT_SUBTYPE),
                ),
            )

        /** 控制请求的子类型。 */
        const val INTERRUPT_SUBTYPE = "interrupt"

        /**
         * 铸一个控制请求 id，形状 `req_<seq>_<8hex>`，同 SDK。
         *
         * 不去重也不等答复：连点两次停止会发两条，第二条对端答一条无害的 success。
         * 十六进制不走 `String.format`，免得随系统语言变。
         */
        fun newInterruptRequestId(seq: Int): String {
            val hex =
                Random
                    .nextInt()
                    .toUInt()
                    .toString(HEX_RADIX)
                    .padStart(HEX_WIDTH, '0')
            return "req_${seq}_$hex"
        }

        private const val HEX_RADIX = 16
        private const val HEX_WIDTH = 8
    }
}

/**
 * 「写进 `in.ndjson`」不等于「远端在听」。
 *
 * 管道已经不在时 `printf … >> in.ndjson` 照样成功，只认 shell 成功标记的话，
 * 管道被杀之后每条消息都显示「已送达」而全部丢失。
 *
 * 「远端在不在听」取决于 tmux 会话在不在，而 `core-claude` 不知道 tmux，所以判定由 app 层注入。
 *
 * 局限：app 侧实现是幂等的「在跑就不动、不在就重建」，只保证会话在，不保证里面的命令还活着。
 * 刚重建的那一瞬，里面的 `tail -n 0 -f` 未必已经打开 `in.ndjson`，紧接着追加的那行可能被跳过；
 * 这只影响管道刚死过一次的路径。
 */
fun interface PipeListening {
    /**
     * 确认远端在听；不在就尽力把它弄回来。
     *
     * @return `null` = 在听（或刚重建好）；非 null = 人可读的原因，调用方当成这条没发出去。
     */
    suspend fun ensureListening(): String?
}

/**
 * 常驻管道的上行出口。与 [TmuxSendKeysSink] 是两条并存的路：
 *
 * | | `TmuxSendKeysSink` | 本类 |
 * |---|---|---|
 * | 目标 | tmux 里的官方 TUI | 常驻管道的 `in.ndjson` |
 * | 手段 | `tmux send-keys` 投键 | 追加一行 JSON |
 * | [echoesBack] | true | false |
 *
 * 管道把输入吃进去不再吐回来（录制里没有一条 `user`），所以上层要做乐观回显。
 */
class PipeUplinkSink(
    private val channel: RemoteCommandChannel,
    private val session: PipeSession,
    /** 远端在不在听，见 [PipeListening]。函数型参数放最后，调用方用尾随 lambda。 */
    private val listening: PipeListening,
) : UplinkSink {
    override val echoesBack: Boolean = false

    /** 控制请求的序号，只求不重。 */
    private val controlSeq = AtomicInteger(0)

    /** 本 sink 的幂等标识，一个实例一个 origin。局限见 [UplinkIdentity]。 */
    private val identity = UplinkIdentity()

    override suspend fun send(request: SendRequest): SendOutcome = sendReporting(request).asOutcome()

    /** 带三档结局的发送；[send] 把它折回两档。 */
    suspend fun sendReporting(request: SendRequest): UplinkAppendOutcome {
        // 闸门量的是拼好之后的最终命令串（含查重外壳与针），不是正文。
        val command = DaemonCommands.guarded(session.idempotentAppendCommand(request.text, identity.idFor(request)))
        UplinkLimits.rejectIfTooLong(command, request.text)?.let {
            return UplinkAppendOutcome.Failed(it.reason, it.retryable)
        }
        return appendIdempotent(command)
    }

    /**
     * 把中断投出去，与 [send] 走同一条路。`Accepted` 只表示投出去了，见 [UplinkSink.interrupt]。
     */
    override suspend fun interrupt(): SendOutcome {
        val requestId = PipeSession.newInterruptRequestId(controlSeq.incrementAndGet())
        return appendGuarded(DaemonCommands.guarded(session.interruptCommand(requestId)))
    }

    /** 跑 [PipeSession.idempotentAppendCommand]，读哨兵分「追加了」与「本来就在」。远端只跑一次。 */
    private suspend fun appendIdempotent(command: String): UplinkAppendOutcome =
        when (val r = remoteBody(command)) {
            is RemoteBody.Failed -> UplinkAppendOutcome.Failed(r.reason, r.retryable)
            is RemoteBody.Ok ->
                // 认不出哨兵就当失败：二义的 stdout 不是送达证据。
                PipeSession.appendResultOf(r.body)
                    ?: UplinkAppendOutcome.Failed(r.body.trim().ifBlank { "没写进去，也没说为什么" })
        }

    /**
     * 盲追加一行进 `in.ndjson`。中断帧走这条：它不去重。
     * 先探活再写，免得消息落进没人读的文件却显示「已送达」。
     */
    private suspend fun appendGuarded(command: String): SendOutcome =
        when (val r = remoteBody(command)) {
            is RemoteBody.Failed -> SendOutcome.Rejected(r.reason, r.retryable)
            is RemoteBody.Ok -> SendOutcome.Accepted
        }

    /** [remoteBody] 的两档结果，只在本类内用。 */
    private sealed interface RemoteBody {
        data class Ok(
            val body: String,
        ) : RemoteBody

        data class Failed(
            val reason: String,
            val retryable: Boolean = true,
        ) : RemoteBody
    }

    /** 探活，跑一次 `exec`，取 guarded 正文。整条发送只有这一处 `channel.exec`。 */
    private suspend fun remoteBody(command: String): RemoteBody {
        runCatching { listening.ensureListening() }
            .getOrElse { e ->
                if (e is CancellationException) throw e
                e.message ?: e.javaClass.simpleName
            }?.let { return RemoteBody.Failed(it) }
        val stdout =
            runCatching {
                channel
                    .exec(command)
                    .toList()
                    .fold(ByteArray(0)) { acc, b -> acc + b }
                    .decodeToString()
            }.getOrElse { e ->
                // 取消不是失败：吞掉它会被谎报成可重试的拒绝，也会破坏协程取消语义。
                if (e is CancellationException) throw e
                return RemoteBody.Failed(e.message ?: e.javaClass.simpleName)
            }
        // 要肯定的成功证据：`exec` 拿不到退出码，stdout 空也可能是目录不存在或磁盘满。
        // `DaemonCommands.guarded` 的标记机制是通用的「shell 成功才打标记」。
        val body =
            DaemonCommands.bodyOrNullIfFailed(stdout)
                ?: return RemoteBody.Failed(stdout.trim().ifBlank { "没写进去，也没说为什么" })
        return RemoteBody.Ok(body)
    }

    companion object {
        /** 上行单行上限，转发 [UplinkLimits]。 */
        const val MAX_UPLINK_BYTES = UplinkLimits.MAX_UPLINK_BYTES
    }
}
