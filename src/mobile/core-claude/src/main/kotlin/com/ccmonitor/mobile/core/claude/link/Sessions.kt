package com.ccmonitor.mobile.core.claude.link

/**
 * 起会话 · 往会话里送字 · 读一条会话的正文：都是那台核心的帧命令，手机只拼入参、解成品（`IPC-COMMANDS.md` 各节）。
 * 会话起在那台 tmux 里（`place: tmux`），桌面那一侧看得到、接得上；正文是核心的通用记录（`IPC-PROTOCOL.md`「通用记录」）。
 */
object SessionNew {
    const val COMMAND: String = "session-new"

    /**
     * 新起一条 Claude 会话。账号 · 模型 · 权限都不带 ⇒ 跟随那台的默认（中途换模型送 `/model <名>`）。
     * [ticket] 一个草稿一张：期限到了同一张再问，那台认出是同一趟、不起第二个。
     */
    fun args(
        cwd: String,
        ticket: String,
    ): Map<String, Any?> = mapOf("agent" to AGENT, "cwd" to cwd, "place" to "tmux", "local" to false, "ticket" to ticket)

    data class Answer(
        /** `started`（tmux 里起好了）· `open`（要开终端窗口：手机不走这一形）。 */
        val outcome: String,
        /** tmux 会话名。 */
        val session: String?,
        /** 那台先定好的 sid；新起的 ⇒ `null`（报到之前说不出）。 */
        val sid: String?,
        /** 展开之后的目录：认报到的会话按它。 */
        val cwd: String,
    )

    fun of(data: Any?): Answer? =
        decoding {
            val m = data.asObj()
            Answer(m.str("outcome").need(), m.str("session"), m.str("sid"), m.str("cwd").need())
        }

    /** 起好之后报到的那一条：会话表里目录是 [cwd]、起之前（[before]）还没有的。和桌面认法一样（`awaitArrival` 的 `{cwd}`）。 */
    fun arrived(
        table: SessionTable,
        cwd: String,
        before: Set<String>,
    ): LiveSession? = table.sessions.values.firstOrNull { it.cwd == cwd && it.sid !in before }

    private const val AGENT = "claude"
}

/** `terminal-input`：往一条会话所在的终端送字或送键（那台过身份门、判送没送到）。 */
object TerminalInput {
    const val COMMAND: String = "terminal-input"

    /** 送字（原样送、末尾补回车）。 */
    fun text(
        sid: String,
        text: String,
    ): Map<String, Any?> = mapOf("sid" to sid, "text" to text)

    /** 送键（`esc` 停这一轮 …）。 */
    fun key(
        sid: String,
        key: String,
    ): Map<String, Any?> = mapOf("sid" to sid, "key" to key)

    sealed interface Result {
        data object Delivered : Result

        /** 不知道送没送到：别重发。 */
        data object Unsure : Result

        /** 没送：[said] 是那台写好的那一句，[why] 是原因码（`screen_changed` · `ended` …）。 */
        data class Refused(
            val said: String,
            val why: String?,
        ) : Result
    }

    fun of(data: Any?): Result? =
        decoding {
            val m = data.asObj()
            when (m.str("result").need()) {
                "delivered" -> Result.Delivered
                "unsure" -> Result.Unsure
                "refused" -> Result.Refused(m.str("said").need(), m.str("why"))
                else -> throw Malformed()
            }
        }
}

/** `history-tail`：一份会话最新 [n] 行从哪个字节起。 */
object HistoryTail {
    const val COMMAND: String = "history-tail"

    fun args(
        path: String,
        n: Int,
    ): Map<String, Any?> = mapOf("path" to path, "n" to n.toLong())

    data class Answer(
        /** 最后一个完整行之后的字节。 */
        val end: Long,
        /** 尾段第一行的字节起点。 */
        val splitAt: Long,
    )

    fun of(data: Any?): Answer? = decoding { data.asObj().let { Answer(it.num("end").need(), it.num("split_at").need()) } }
}

/** `history-read`：按字节读 `[offset, until)` 那一段，每行一条通用记录（不进界面的行没有记录、不出）。 */
object HistoryRead {
    const val COMMAND: String = "history-read"

    fun args(
        path: String,
        offset: Long,
        until: Long,
    ): Map<String, Any?> = mapOf("path" to path, "offset" to offset, "until" to until)

    data class Page(
        val records: List<Map<String, Any?>>,
        /** 每条记录那一行之后的字节（与 [records] 一一对应）。 */
        val ends: List<Long>,
        val next: Long,
        val eof: Boolean,
    )

    fun of(data: Any?): Page? =
        decoding {
            val m = data.asObj()
            val rows = m.objs("rows").need().mapNotNull { r -> r.obj("record")?.let { it to (r.num("end") ?: -1L) } }
            Page(rows.map { it.first }, rows.map { it.second }, m.num("next").need(), m.bool("eof") == true)
        }
}

/** 流上的 `line` 帧：一条会话记录文件里新写的一行（`record` 缺 ＝ 不进界面、照占号）。 */
data class RecordLine(
    val sid: String,
    val path: String,
    /** 这一行末尾在文件里的累计字节：续读按它。 */
    val byteOffset: Long,
    val record: Map<String, Any?>?,
) {
    companion object {
        fun of(frame: Map<String, Any?>): RecordLine? {
            if (frame.str("kind") != "line") return null
            return decoding { RecordLine(frame.str("session_id").need(), frame.str("path").need(), frame.num("byte_offset").need(), frame.obj("record")) }
        }
    }
}
