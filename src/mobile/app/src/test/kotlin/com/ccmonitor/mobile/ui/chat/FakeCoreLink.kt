package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.EMPTY_BOOK
import com.ccmonitor.mobile.core.claude.link.RecordLine
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionWatch
import com.ccmonitor.mobile.core.claude.link.Transfer
import com.ccmonitor.mobile.core.claude.link.WatchFrom
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.BatchAnswer
import com.ccmonitor.mobile.core.wire.gen.BatchLine
import com.ccmonitor.mobile.core.wire.gen.ClashChoice
import com.ccmonitor.mobile.core.wire.gen.ClashRow
import com.ccmonitor.mobile.core.wire.gen.Decide
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.LiveCards
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.core.wire.gen.MachineState
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.core.wire.gen.PhoneMenu
import com.ccmonitor.mobile.core.wire.gen.SessionAdded
import com.ccmonitor.mobile.core.wire.gen.SessionFace
import com.ccmonitor.mobile.core.wire.gen.SessionLeft
import com.ccmonitor.mobile.ui.Faces
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

/** 那台的状态成品（测试摆，只填画到的那几格；机器名 · 那几句是结构夹具）。 */
internal fun machineOf(
    state: MachineStateKind,
    said: String = "",
    detail: String = "",
    fixes: List<MachineFixButton> = emptyList(),
) = MachineState(
    name = "m",
    state = state,
    offline = false,
    reason = null,
    stage = null,
    version = null,
    versionRelation = null,
    os = null,
    word = "",
    said = said,
    offlineSaid = FakeCoreLink.OFFLINE,
    fixes = fixes,
    seenHostKey = null,
    expectedHostKey = null,
    detail = detail.takeIf { it.isNotEmpty() },
)

/** 核心交的那一行：通着。 */
internal fun linkUp(
    said: String = "",
    detail: String = "",
    fixes: List<MachineFixButton> = emptyList(),
    state: MachineStateKind = MachineStateKind.UP,
) = LinkLine.Up(0L, machineOf(state, said, detail, fixes))

/** 核心交的那一行：断着。 */
internal fun linkDown(
    said: String,
    detail: String = "",
    fixes: List<MachineFixButton> = emptyList(),
    state: MachineStateKind = MachineStateKind.DOWN,
) = LinkLine.Down(machineOf(state, said, detail, fixes))

/** 核心交的那一行：正在连。 */
internal fun linkConnecting() = LinkLine.Connecting(machineOf(MachineStateKind.CONNECTING))

/** 一台核心的替身：会话表 · 记录行 · 一问一答都由测试摆；每一问记下来（命令名 ＋ 入参）。 */
internal class FakeCoreLink :
    CoreLink,
    SessionWatch {
    override val state = MutableStateFlow<LinkLine?>(linkUp())

    /** 那台成品写的离线那一句（假的那台的成品一开始就到了）。 */
    override val offline = MutableStateFlow<String?>(OFFLINE)
    override val table = MutableStateFlow(EMPTY_BOOK)
    private val lineFlow = MutableSharedFlow<RecordLine>(extraBufferCapacity = 64)
    override val lines: SharedFlow<RecordLine> = lineFlow.asSharedFlow()
    override val epoch = MutableStateFlow(0L)
    private val liveFlow = MutableSharedFlow<LiveCards>(extraBufferCapacity = 64)
    override val live: SharedFlow<LiveCards> = liveFlow.asSharedFlow()

    /** 核心交来一处整份的活卡（照线上那一形，经生成的 [LiveCards] 解）。 */
    suspend fun live(cards: Map<String, Any?>) = liveFlow.emit(Wire.bind<LiveCards>(cards)!!)

    val calls = mutableListOf<Pair<String, Map<String, Any?>?>>()

    /** ［重试］叫了几回整份重接。 */
    var reconnects = 0

    override fun reconnect() {
        reconnects++
    }

    var answer: (String, Map<String, Any?>?) -> Reply = { _, _ -> Reply.LinkDown }

    /** `session-arrive` 那一问挂着、直到判据说报到了（[arrive]）或没见到（[miss]）。 */
    private val arrival = kotlinx.coroutines.CompletableDeferred<Reply>()

    fun arrive(sid: String) = arrival.complete(Reply.Ok(mapOf("kind" to "arrived", "sid" to sid)))

    fun miss(said: String) = arrival.complete(Reply.Ok(mapOf("kind" to "missed", "said" to said, "terminal" to null)))

    override suspend fun call(
        cmd: String,
        args: Map<String, Any?>?,
        withinMs: Long,
    ): Reply {
        calls += cmd to args
        if (cmd == "session-arrive") return arrival.await()
        return answer(cmd, args)
    }

    fun calls(cmd: String) = calls.filter { it.first == cmd }.map { it.second }

    /** 分块传每一趟：哪一半 ＋ 交出来的描述符。 */
    val transfers = mutableListOf<Pair<Transfer, Int>>()

    /** 分块传怎么答：先报几下走到哪（[moved]），再回结局。 */
    var moves: List<Pair<Long, Long>> = emptyList()
    var transferred: (Transfer) -> Reply = { Reply.LinkDown }

    override suspend fun transfer(
        how: Transfer,
        fd: () -> Int,
        withinMs: Long,
        moved: (Long, Long) -> Unit,
    ): Reply {
        transfers += how to fd()
        moves.forEach { (d, t) -> moved(d, t) }
        return transferred(how)
    }

    private val batchFlow = MutableSharedFlow<BatchLine>(extraBufferCapacity = 64)
    override val batches: SharedFlow<BatchLine> = batchFlow.asSharedFlow()

    /** 核心里有活在走（测试照它翻）。 */
    override val working = MutableStateFlow(false)

    override fun resumed(): List<BatchLine> = emptyList()

    /** 起过的每一组（要做什么）· 每一答 · 每一停。 */
    val wants = mutableListOf<Map<String, Any?>>()
    val answered = mutableListOf<Pair<Long, BatchAnswer?>>()
    val stopped = mutableListOf<Long>()

    override suspend fun batch(want: Map<String, Any?>): Long {
        wants += want
        return wants.size.toLong()
    }

    override fun batchAnswer(
        id: Long,
        answer: BatchAnswer?,
    ) {
        answered += id to answer
    }

    override fun batchStop(id: Long) {
        stopped += id
    }

    /** 「能做什么」那一问交进来的原行（每一问一份）；回 [menu]。 */
    val picked = mutableListOf<List<Any?>>()
    var menu: (List<Any?>) -> PhoneMenu? = { null }

    /** 同名清单 ⇒ 抽屉那几行（核心那一份的替身：名字照抄、说明写成那台给的大小、每种答的字写成它的线上名，先后照那台给的）。 */
    override fun clashes(data: Any?): List<ClashRow> =
        ((data as? Map<*, *>)?.get("conflicts") as? List<*>).orEmpty().map { c ->
            val m = c as Map<*, *>
            val choices =
                (m["answers"] as List<*>).map { a ->
                    ClashChoice(Decide.entries.first { it.wire == a }, a as String, a)
                }
            ClashRow(m["key"] as String, m["name"] as String, m["there_size_text"] as String, choices, null)
        }

    /** 交核心认的同名那几答（键 · 答）；回 [decide] 给的（核心那一份的替身，规矩在核心里判）。 */
    val decisions = mutableListOf<Pair<List<String>, BatchAnswer?>>()
    var decide: (List<String>, BatchAnswer?) -> Map<String, Decide>? = { _, _ -> null }

    override fun decided(
        keys: List<String>,
        answer: BatchAnswer?,
    ): Map<String, Decide>? {
        decisions += keys to answer
        return decide(keys, answer)
    }

    override fun picks(rows: List<Any?>): PhoneMenu? {
        picked.add(rows)
        return menu(rows)
    }

    /** 核心交来一格（照核心那一行，经生成的 [BatchLine] 解：线上必有的格都得在）。 */
    suspend fun batchEvent(m: Map<String, Any?>) = batchFlow.emit(Wire.bind<BatchLine>(m) ?: error("核心那一行装不进 BatchLine：$m"))

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

    /** 那台报一条会话；[path] 缺 ⇒ 记录还没落盘（那台宣告时就不带路径）。进程目录 [cwd] 与项目目录 [projectDir] 缺省同一个。 */
    fun report(
        sid: String,
        cwd: String,
        face: SessionFace = Faces.of("live-idle"),
        path: String? = "/p/$sid.jsonl",
        projectDir: String? = cwd,
    ) {
        table.value = table.value.copy(live = table.value.live + (sid to liveSeat(sid, face, path = path, cwd = cwd, projectDir = projectDir)))
    }

    /** 那台说这一条走了（去向那一张脸）。 */
    fun left(
        sid: String,
        face: SessionFace,
    ) {
        table.value = table.value.copy(live = table.value.live - sid, left = table.value.left + (sid to leftSeat(sid, face)))
    }

    companion object {
        /** 假的那台成品里的离线那一句（测试照它比，不取字）。 */
        const val OFFLINE = "成品写的那台离线那一句"

        fun human(
            id: String,
            text: String,
        ): Map<String, Any?> = mapOf("agent" to "claude", "id" to id, "t" to "said", "draw" to "card", "human" to true, "who" to mapOf("speaker" to mapOf("kind" to "human"), "text" to text), "blocks" to emptyList<Any>())

        fun reply(
            id: String,
            text: String,
        ): Map<String, Any?> = mapOf("agent" to "claude", "id" to id, "t" to "reply", "autoReply" to false, "draw" to "card", "human" to false, "blocks" to listOf(prose(text)))

        /** 助手正文块：后端解析好的那一形（一段字的树；只一段 ⇒ 「展开」那一格留空）。 */
        fun prose(text: String): Map<String, Any?> =
            mapOf("type" to "prose", "doc" to mapOf("blocks" to listOf(mapOf("t" to "p", "k" to listOf(text))), "segs" to emptyList<Any>()), "more" to "")

        fun failed(message: String) = Reply.Failed(Failed("start_failed", message, "detail"))

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
        /** `history-read` 一页至多几行。 */
        var pageLines: Long = Long.MAX_VALUE / W,
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
                            // 变短 · 被改写 ⇒ 那台写好的那一句（字是替身的，不是文案表的）。
                            "said" to SAID[how],
                            "from" to from,
                            "rows" to rows(from, n),
                            "next" to n * W,
                            "eof" to true,
                            "held" to mapOf("path" to args["path"], "next" to n, "next_byte" to n * W, "mark" to id),
                        ),
                    )
                }
                "history-read" -> {
                    // 右端缺 · 过了文件尾 ⇒ 到文件尾；一页至多 [pageLines] 行（同那台按字节分页）。
                    val a = (args!!["offset"] as Long) / W
                    val limit = minOf((args["until"] as? Long)?.div(W) ?: n, n)
                    val b = minOf(limit, a + pageLines)
                    Reply.Ok(mapOf("rows" to rows(a, b), "next" to b * W, "eof" to (b >= limit)))
                }
                "history-lines" -> {
                    val a = args!!["from"] as Long
                    val b = minOf(args["until"] as Long, n)
                    Reply.Ok(
                        mapOf(
                            "lines" to (a until b).map { mapOf("session_id" to "s1", "path" to "/p/s1.jsonl", "seq" to it, "end" to (it + 1) * W, "cwd" to null, "record" to records[it.toInt()]) },
                            "from" to a,
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
        ) = (a until b).map { mapOf("end" to (it + 1) * W, "hash" to it, "record" to records[it.toInt()]) }

        companion object {
            private const val W = 10L

            /** 变短 · 被改写时那台说的那一句（替身的字）。 */
            val SAID = mapOf("truncated" to "tail-said-truncated", "rewritten" to "tail-said-rewritten")
        }
    }
}

/** 簿里活着的一条（宣告格只摆测试要的那几格；整份由核心折，测试直接摆成品）。 */
internal fun liveSeat(
    sid: String,
    face: SessionFace,
    path: String? = null,
    cwd: String? = null,
    projectDir: String? = cwd,
) = SessionAdded(sid = sid, agentKind = "claude", cwd = cwd, projectDir = projectDir, path = path, face = face, pid = 0, label = sid, title = sid)

/** 簿里走了的一条（可重连那一种；测试只看脸）。 */
internal fun leftSeat(
    sid: String,
    face: SessionFace,
) = SessionLeft(sid, face)
