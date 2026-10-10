package com.ccmonitor.mobile.core.claude.transport
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.squareup.moshi.Moshi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/**
 * α 传输：消费 cc-monitor 后端流模式的 exec 输出（换行分隔 JSON 帧，PROTO_VERSION=1），解析为 [JsonlFrame]。
 *
 * 启动形 = `<path> -- --stream [--with-bg] [--tail-only]`，由 [DaemonCommands] 拼；argv 分派规则见那里。
 * 与 β [TailTransport] 对偶，但读结构化帧：后端已经判活、分会话、处理拥塞。
 *
 * - [currentOffset] = 已收 [JsonlFrame.Line] 的 max(byteOffset) = 已结算的 resume baseline
 *   （`byte_offset` = 行末含 `\n` 的累计原始字节）。
 * - resume：另起一条 `--read-session-from-offset <path> <N>` exec 拉 `[N,EOF]`。
 * - [JsonlFrame.Overflow]：消费方另起 offset exec 补齐 `[baseline,当前)`，补齐前不推进 baseline。
 * - resolve 是独立的 `--resolve` 一次性 exec，不走本流。
 *
 * 未知 kind / 坏 JSON → 跳过（后端加新帧不崩老客户端，也不预折叠未知 kind）。
 * Flow 被取消 → [RemoteCommandChannel] 负责杀远端 exec。
 */
class DaemonTransport(
    private val channel: RemoteCommandChannel,
    private val streamCommand: String,
    startOffsetBytes: Long = 0L,
) : JsonlTransport {
    private val framer = LineFramer()

    /** 已收 Line 帧的 max(byteOffset) = resume baseline（调用方持久化 + 重连 --read-session-from-offset 续）。 */
    var currentOffset: Long = startOffsetBytes
        private set

    override fun frames(): Flow<JsonlFrame> =
        flow {
            channel.exec(streamCommand).collect { chunk ->
                for (framed in framer.feedFramed(chunk)) {
                    emitFrame(framed.raw)?.let { emit(it) }
                }
            }
            // 流正常结束：冲残行（帧应恒完整行；残行多为坏 JSON → parseFrame 跳过）。
            framer.flush()?.let { emitFrame(it)?.let { f -> emit(f) } }
        }

    /** 解析一行 JSON 帧；Line 帧顺带推进 [currentOffset]（只增，防乱序/回放倒退）。坏帧/未知 → null（跳过）。 */
    private fun emitFrame(json: String): JsonlFrame? {
        val frame = parseFrame(json) ?: return null
        if (frame is JsonlFrame.Line) currentOffset = maxOf(currentOffset, frame.byteOffset)
        return frame
    }

    private companion object {
        private val moshi = Moshi.Builder().build()
        private val anyAdapter = moshi.adapter(Any::class.java)

        /** wire kind → [JsonlFrame]。字段名按 wire（Line/TurnEnd=`session_id`，SessionAdded/Status/Removed=`sid`）。 */
        fun parseFrame(json: String): JsonlFrame? {
            val map = runCatching { anyAdapter.fromJson(json) }.getOrNull() as? Map<*, *> ?: return null
            return when (map["kind"] as? String) {
                "hello" -> parseHello(map)
                "line" -> parseLine(map)
                "session_added" -> parseSessionAdded(map)
                "session_status" ->
                    (map["sid"] as? String)?.let {
                        JsonlFrame.SessionStatus(
                            it,
                            map["status"] as? String,
                            map["waiting_for"] as? String,
                            map["liveness_confidence"] as? String,
                            // status 与 activity 两格都收、不折叠（见 [JsonlFrame.SessionStatus.activity]）。
                            map["activity"] as? String,
                        )
                    }
                // `cause`：`"superseded"`（/branch、/clear 原地换 sid）≠「会话死了」。缺席 = `"gone"`。
                "session_removed" -> (map["sid"] as? String)?.let { JsonlFrame.SessionRemoved(it, map["cause"] as? String) }
                // `lost` / `lost_truncated` 说丢的是什么。只取 `dropped` 的话，
                // 「某条对话的 status 丢了、上一次说的是 waiting」就永远看不见。
                "overflow" ->
                    JsonlFrame.Overflow(
                        dropped = num(map["dropped"])?.toInt() ?: 0,
                        lost = parseLost(map["lost"]),
                        // 只认真布尔；缺席 = false = 表是全的
                        lostTruncated = map["lost_truncated"] as? Boolean ?: false,
                    )
                "turn_end" -> parseTurnEnd(map)
                else -> null // 未知 kind：跳过（前向兼容，勿崩、勿预折叠）
            }
        }

        private fun parseHello(map: Map<*, *>): JsonlFrame.Hello =
            JsonlFrame.Hello(
                v = num(map["v"])?.toInt() ?: 1,
                buildId = map["build_id"] as? String,
                hostArch = map["host_arch"] as? String,
                claudeDir = map["claude_dir"] as? String,
                capabilities = strList(map["capabilities"]),
                emits = strList(map["emits"]),
                // codex_dir / kinds 缺席即 null / 空。
                codexDir = map["codex_dir"] as? String,
                kinds = strList(map["kinds"]),
            )

        /**
         * Line 必须有 `byte_offset`（resume 锚点）和 `raw`，缺任一整帧丢。
         *
         * `raw` 只在发了 `--with-raw` 时才带，而目前没有生产 `want` 要它（见 [DaemonCommands.WANT_RAW]），
         * 所以够新的后端上每一帧 `line` 都会在这里被丢。`line` 帧目前也没有生产消费方。
         *
         * 仍取「整帧丢」而不是「丢 `raw` 保住 `byte_offset`」：[currentOffset] 是已结算的 baseline，
         * 推进一个正文没交付给消费方的 offset，下次 resume 会从它往后拉，那几条记录就永远丢了。
         * 整帧丢的代价是 resume 从头重下，贵但不丢数据。
         *
         * 要用 `line` 正文时三件一起做：`want` 里加 [DaemonCommands.WANT_RAW]；把 [JsonlFrame.Line.raw]
         * 改成可空并处置它的消费点；重新决定 baseline 怎么推进。只做第一件会得到整面空白且无诊断。
         */
        private fun parseLine(map: Map<*, *>): JsonlFrame.Line? {
            val byteOffset = num(map["byte_offset"])?.toLong() ?: return null
            val raw = map["raw"] as? String ?: return null
            return JsonlFrame.Line(
                sessionId = map["session_id"] as? String,
                path = map["path"] as? String,
                seq = num(map["seq"])?.toLong() ?: 0L,
                raw = raw,
                byteOffset = byteOffset,
            )
        }

        private fun parseSessionAdded(map: Map<*, *>): JsonlFrame.SessionAdded? {
            val sid = map["sid"] as? String ?: return null
            return JsonlFrame.SessionAdded(
                sessionId = sid,
                path = map["path"] as? String,
                sessionKind = map["session_kind"] as? String,
                cwd = map["cwd"] as? String,
                name = map["name"] as? String,
                lines = num(map["lines"])?.toInt(),
                status = map["status"] as? String,
                waitingFor = map["waiting_for"] as? String,
                // agent_kind / liveness_confidence 缺席即 null；映射经 agentKindFromWire / livenessAuthoritative。
                agentKind = map["agent_kind"] as? String,
                livenessConfidence = map["liveness_confidence"] as? String,
                // 只认真布尔：字符串 `"false"` 之类落到 null = 缺席 = 可 attach（缺席即 true 是对端定的默认）。
                attachable = map["attachable"] as? Boolean,
                // 宣告时的活动档。`session_added` 上没有 `status`，只有这一格；漏了它，
                // 「连上之前就在等」的会话永远看不见。
                activity = map["activity"] as? String,
                // 后台会话。对端只在 true 时发 ⇒ 缺席 ≡ false ≡ 交互会话。只认真布尔，字符串 `"true"` 落 false。
                background = map["background"] as? Boolean ?: false,
            )
        }

        /**
         * `overflow.lost` → 丢帧身份表。坏项跳过、不整条丢：
         * 少认出一项只是少作废一条等待态，把整个 `overflow` 丢掉才是真的静默。
         */
        private fun parseLost(v: Any?): List<JsonlFrame.Overflow.Lost> =
            (v as? List<*>).orEmpty().mapNotNull { item ->
                val m = item as? Map<*, *> ?: return@mapNotNull null
                val kind = m["kind"] as? String ?: return@mapNotNull null
                JsonlFrame.Overflow.Lost(kind = kind, subject = m["subject"] as? String)
            }

        private fun parseTurnEnd(map: Map<*, *>): JsonlFrame.TurnEnd? {
            val sid = map["session_id"] as? String ?: return null
            val uuid = map["uuid"] as? String ?: return null
            return JsonlFrame.TurnEnd(sid, uuid)
        }

        private fun num(v: Any?): Number? = v as? Number

        private fun strList(v: Any?): List<String> = (v as? List<*>)?.mapNotNull { it as? String } ?: emptyList()
    }
}
