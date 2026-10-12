package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.LiveSession
import com.ccmonitor.mobile.core.claude.link.RecordLine
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.SessionWatch
import com.ccmonitor.mobile.core.claude.link.Tone
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.WatchFrom
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

/** 一台核心的替身：会话表 · 记录行 · 一问一答都由测试摆；每一问记下来（命令名 ＋ 入参）。 */
internal class FakeCoreLink :
    CoreLink,
    SessionWatch {
    override val table = MutableStateFlow(SessionTable())
    private val lineFlow = MutableSharedFlow<RecordLine>(extraBufferCapacity = 64)
    override val lines: SharedFlow<RecordLine> = lineFlow.asSharedFlow()
    override val epoch = MutableStateFlow(0L)
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
    ) = lineFlow.emit(RecordLine(sid, "/p/$sid.jsonl", seq, null, record))

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
        tone: Tone = Tone.PLAIN,
        path: String? = "/p/$sid.jsonl",
    ) {
        table.value = table.value.copy(sessions = table.value.sessions + (sid to LiveSession(sid, path = path, cwd = cwd, activity = Toned("空闲", tone))))
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
            mapOf("agent" to "claude", "id" to id, "t" to "reply", "autoReply" to false, "endsTurn" to true, "blocks" to listOf(mapOf("type" to "text", "text" to text)))

        fun failed(message: String) = Reply.Failed(CoreFailure("start_failed", message, "detail"))
    }

    /**
     * 一份会话记录的替身：第 i 行（行号 i）占字节 `[10i, 10i+10)`、每行都进界面。照它答 history-tail · history-read · history-lines。
     */
    class File(
        val records: MutableList<Map<String, Any?>> = mutableListOf(),
    ) {
        fun answer(
            cmd: String,
            args: Map<String, Any?>?,
        ): Reply? {
            val n = records.size.toLong()
            return when (cmd) {
                "history-tail" -> {
                    val from = (n - (args!!["n"] as Long)).coerceAtLeast(0)
                    Reply.Ok(mapOf("end" to n * W, "split_at" to from * W, "tail_from" to from, "total" to n))
                }
                "history-read" -> {
                    val a = (args!!["offset"] as Long) / W
                    val b = (args["until"] as Long) / W
                    Reply.Ok(mapOf("rows" to (a until b).map { mapOf("end" to (it + 1) * W, "record" to records[it.toInt()]) }, "next" to b * W, "eof" to true))
                }
                "history-lines" -> {
                    val a = args!!["from"] as Long
                    val b = minOf(args["until"] as Long, n)
                    Reply.Ok(mapOf("lines" to (a until b).map { mapOf("seq" to it, "record" to records[it.toInt()]) }, "next" to b, "eof" to (b >= n)))
                }
                else -> null
            }
        }

        private companion object {
            const val W = 10L
        }
    }
}
