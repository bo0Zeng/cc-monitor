package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.LiveFrame
import com.ccmonitor.mobile.core.claude.link.LiveSession
import com.ccmonitor.mobile.core.claude.link.RecordLine
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.SessionWatch
import com.ccmonitor.mobile.core.claude.link.WatchFrom
import com.ccmonitor.mobile.core.claude.link.wire.LineRecord
import com.ccmonitor.mobile.core.claude.link.wire.SessionFace
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import com.ccmonitor.mobile.ui.Faces
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

/** 一台核心的替身：会话表 · 记录行 · 一问一答都由测试摆；每一问记下来（命令名 ＋ 入参）。 */
internal class FakeCoreLink :
    CoreLink,
    SessionWatch {
    override val state = MutableStateFlow<LinkState>(LinkState.Up())
    override val table = MutableStateFlow(SessionTable())
    private val lineFlow = MutableSharedFlow<RecordLine>(extraBufferCapacity = 64)
    override val lines: SharedFlow<RecordLine> = lineFlow.asSharedFlow()
    override val epoch = MutableStateFlow(0L)
    private val liveFlow = MutableSharedFlow<LiveFrame>(extraBufferCapacity = 64)
    override val live: SharedFlow<LiveFrame> = liveFlow.asSharedFlow()

    /** 流上来一帧活卡（照线上那一形，经 [LiveFrame.of] 解）。 */
    suspend fun live(frame: Map<String, Any?>) = liveFlow.emit(LiveFrame.of(frame + ("kind" to LiveFrame.KIND))!!)

    val calls = mutableListOf<Pair<String, Map<String, Any?>?>>()

    var answer: (String, Map<String, Any?>?) -> Reply = { _, _ -> Reply.LinkDown }

    override suspend fun call(
        cmd: String,
        args: Map<String, Any?>?,
        withinMs: Long,
    ): Reply {
        calls += cmd to args
        return answer(cmd, args)
    }

    fun calls(cmd: String) = calls.filter { it.first == cmd }.map { it.second }

    suspend fun line(
        sid: String,
        seq: Long,
        record: Map<String, Any?>?,
        byteOffset: Long? = null,
    ) = lineFlow.emit(RecordLine(sid, "/p/$sid.jsonl", seq, byteOffset, record?.let { Wire.bind<LineRecord>(it) }, record))

    /** 此刻有人在看的会话（[hold] 的计数）。 */
    val held = mutableMapOf<String, Int>()
    private val fromFlow = MutableSharedFlow<WatchFrom>(extraBufferCapacity = 64)
    override val from: SharedFlow<WatchFrom> = fromFlow.asSharedFlow()

    override fun hold(sid: String): AutoCloseable {
        held[sid] = (held[sid] ?: 0) + 1
        var open = true
        return AutoCloseable {
            if (open) held[sid] = held.getValue(sid) - 1
            open = false
        }
    }

    fun watching(): Set<String> = held.filterValues { it > 0 }.keys

    suspend fun from(
        sid: String,
        seq: Long,
    ) = fromFlow.emit(WatchFrom(sid, "/p/$sid.jsonl", seq))

    /** 那台报一条会话；[path] 缺 ⇒ 记录还没落盘（那台宣告时就不带路径）。 */
    fun report(
        sid: String,
        cwd: String,
        face: SessionFace = Faces.of("live-idle"),
        path: String? = "/p/$sid.jsonl",
    ) {
        table.value =
            table.value.copy(
                sessions = table.value.sessions + (sid to LiveSession(sid, path = path, cwd = cwd)),
                faces = table.value.faces + (sid to face),
            )
    }

    companion object {
        fun human(
            id: String,
            text: String,
        ): Map<String, Any?> = mapOf("agent" to "claude", "id" to id, "t" to "said", "who" to mapOf("speaker" to mapOf("kind" to "human"), "text" to text))

        fun reply(
            id: String,
            text: String,
        ): Map<String, Any?> =
            mapOf("agent" to "claude", "id" to id, "t" to "reply", "autoReply" to false, "blocks" to listOf(mapOf("type" to "text", "text" to text)))

        fun failed(message: String) = Reply.Failed(CoreFailure("start_failed", message, "detail"))

        /** `history-tail` 的一份回包：尾段 `[splitAt, end)` 一页读完（[rows] 是 `rows[]` 原样）、手上那一份停在 [end]。 */
        fun tail(
            end: Long,
            splitAt: Long,
            total: Long,
            rows: List<Map<String, Any?>> = emptyList(),
        ): Map<String, Any?> =
            mapOf(
                "end" to end,
                "split_at" to splitAt,
                "tail_from" to total - rows.size,
                "total" to total,
                "how" to "tail",
                "from" to total - rows.size,
                "rows" to rows,
                "next" to end,
                "eof" to true,
                "held" to mapOf("path" to "/p/s1.jsonl", "next" to total, "next_byte" to end, "mark" to "m"),
            )
    }

    /**
     * 一份会话记录的替身：第 i 行（行号 i）占字节 `[10i, 10i+10)`、每行都进界面；那台给的记号是 [id]（换了一份 ⇒ 换 [id]）。
     * 照它答 history-tail（怎么接照那台的规矩判：没交 · 换了路径 ⇒ 尾段；比交来的短 ⇒ 变短；记号对不上 ⇒ 被改写；缺得比 n 多 ⇒ 不读那一截）·
     * history-read · history-lines。
     */
    class File(
        val records: MutableList<Map<String, Any?>> = mutableListOf(),
        var id: String = "f1",
    ) {
        fun answer(
            cmd: String,
            args: Map<String, Any?>?,
        ): Reply? {
            val n = records.size.toLong()
            return when (cmd) {
                "history-tail" -> {
                    val want = args!!["n"] as Long
                    val tailFrom = (n - want).coerceAtLeast(0)
                    val held = args["held"] as? Map<*, *>
                    val next = held?.get("next") as? Long
                    val how =
                        when {
                            held == null || held["path"] != args["path"] -> "tail"
                            next!! > n || (held["next_byte"] as Long) > n * W -> "truncated"
                            held["mark"] != id -> "rewritten"
                            n - next > want -> "handoff"
                            else -> "resume"
                        }
                    val from =
                        when (how) {
                            "resume" -> next!!
                            "handoff" -> n
                            else -> tailFrom
                        }
                    Reply.Ok(
                        mapOf(
                            "end" to n * W,
                            "split_at" to tailFrom * W,
                            "tail_from" to tailFrom,
                            "total" to n,
                            "how" to how,
                            "from" to from,
                            "rows" to rows(from, n),
                            "next" to n * W,
                            "eof" to true,
                            "held" to mapOf("path" to args["path"], "next" to n, "next_byte" to n * W, "mark" to id),
                        ),
                    )
                }
                "history-read" -> {
                    val a = (args!!["offset"] as Long) / W
                    val b = (args["until"] as Long) / W
                    Reply.Ok(mapOf("rows" to rows(a, b), "next" to b * W, "eof" to true))
                }
                "history-lines" -> {
                    val a = args!!["from"] as Long
                    val b = minOf(args["until"] as Long, n)
                    Reply.Ok(
                        mapOf(
                            "lines" to (a until b).map { mapOf("seq" to it, "end" to (it + 1) * W, "record" to records[it.toInt()]) },
                            "next" to b,
                            "eof" to (b >= n),
                            "end" to b * W,
                        ),
                    )
                }
                else -> null
            }
        }

        /** `[a, b)` 那几行的 `rows[]`（`{end, record}`）。 */
        private fun rows(
            a: Long,
            b: Long,
        ) = (a until b).map { mapOf("end" to (it + 1) * W, "record" to records[it.toInt()]) }

        private companion object {
            const val W = 10L
        }
    }
}
