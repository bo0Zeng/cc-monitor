package com.ccmonitor.mobile.core.claude.link

/**
 * 起会话 · 往会话里送字 · 读一条会话的正文：都是那台核心的帧命令，手机只拼入参、解成品（`IPC-COMMANDS.md` 各节）。
 * 会话起在那台 tmux 里（`place: tmux`），桌面那一侧看得到、接得上；正文是核心的通用记录（`IPC-PROTOCOL.md`「通用记录」）。
 */
object SessionNew {
    const val COMMAND: String = "session-new"

    /**
     * 新起一条会话（[agent] 是哪一家的线上名：`claude` · `codex`）。账号 · 模型 · 权限都不带 ⇒ 跟随那台的默认（中途换模型送 `/model <名>`）。
     * [ticket] 一个草稿一张：期限到了同一张再问，那台认出是同一趟、不起第二个。
     * [prompt] 第一句：随起交（那台作为启动参数交给 agent，结局在回包的 `prompt`）。
     */
    fun args(
        cwd: String,
        ticket: String,
        agent: String,
        prompt: String,
    ): Map<String, Any?> = mapOf("agent" to agent, "cwd" to cwd, "place" to "tmux", "local" to false, "ticket" to ticket, "prompt" to prompt)

    /** 在家目录起 [agent]、起在名叫 [name] 的会话里，不带第一句（终端屏「后台起」那一格）。 */
    fun inTmux(
        agent: String,
        name: String,
        ticket: String,
    ): Map<String, Any?> = mapOf("agent" to agent, "cwd" to "~", "place" to "tmux", "local" to false, "ticket" to ticket, "tmuxName" to name)

    data class Answer(
        /** `started`（tmux 里起好了）· `open`（要开终端窗口：手机不走这一形）。 */
        val outcome: String,
        /** tmux 会话名。 */
        val session: String?,
        /** 那台先定好的 sid；新起的 ⇒ `null`（报到之前说不出）。 */
        val sid: String?,
        /** 展开之后的目录：认报到的会话按它。 */
        val cwd: String,
        /** 第一句交进去的结局（同 `terminal-input` 的回话）；那台没交 ⇒ `null`。 */
        val prompt: TerminalInput.Result?,
    )

    fun of(data: Any?): Answer? =
        decoding {
            val m = data.asObj()
            val prompt = m["prompt"]?.let { TerminalInput.of(it) ?: throw Malformed() }
            Answer(m.str("outcome").need(), m.str("session"), m.str("sid"), m.str("cwd").need(), prompt)
        }

    /** 起好之后报到的那一条：会话表里目录是 [cwd]、起之前（[before]）还没有的。和桌面认法一样（`awaitArrival` 的 `{cwd}`）。 */
    fun arrived(
        table: SessionTable,
        cwd: String,
        before: Set<String>,
    ): LiveSession? = table.sessions.values.firstOrNull { it.cwd == cwd && it.sid !in before }
}

/** `terminal-input`：往一条会话所在的终端送字或送键（那台过身份门、判送没送到）。 */
object TerminalInput {
    const val COMMAND: String = "terminal-input"

    /** 送字（原样送、末尾补回车）。 */
    fun text(
        sid: String,
        text: String,
    ): Map<String, Any?> = mapOf("sid" to sid, "text" to text)

    /** 按名单里的句柄送字（终端屏：那个终端里不一定跑着会话）。 */
    fun textInto(
        terminal: String,
        text: String,
    ): Map<String, Any?> = mapOf("terminal" to terminal, "text" to text)

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
        /** 尾段第一行的行号（与 `line` 帧的 `seq` 同一个空间）。 */
        val tailFrom: Long,
        /** 可计行总数 ＝ 读到 [end] 为止的下一行行号。 */
        val total: Long,
    )

    fun of(data: Any?): Answer? =
        decoding {
            data.asObj().let { Answer(it.num("end").need(), it.num("split_at").need(), it.num("tail_from").need(), it.num("total").need()) }
        }
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

/** `history-lines`：按行号取 `[from, until)` 那一段（不进界面的行照占号、不出）。 */
object HistoryLines {
    const val COMMAND: String = "history-lines"

    fun args(
        path: String,
        from: Long,
        until: Long,
    ): Map<String, Any?> = mapOf("path" to path, "from" to from, "until" to until)

    data class Page(
        val records: List<Map<String, Any?>>,
        /** 每条记录的行号（与 [records] 一一对应）。 */
        val seqs: List<Long>,
        /** 下一段从这一行起。 */
        val next: Long,
        val eof: Boolean,
    )

    fun of(data: Any?): Page? =
        decoding {
            val m = data.asObj()
            val rows = m.objs("lines").need().mapNotNull { l -> l.obj("record")?.let { it to l.num("seq").need() } }
            Page(rows.map { it.first }, rows.map { it.second }, m.num("next").need(), m.bool("eof") == true)
        }
}

/** 流上的 `line` 帧：一条会话记录文件里新写的一行（`record` 缺 ＝ 不进界面、照占号）。 */
data class RecordLine(
    val sid: String,
    val path: String,
    /** 这一行在这份记录里的行号（与 `history-tail.total` · `history-lines` · `stream-watch.from[].seq` 同一个空间）：接续与去重按它。 */
    val seq: Long,
    /** 这一行末尾在文件里的累计字节。 */
    val byteOffset: Long?,
    val record: Map<String, Any?>?,
) {
    companion object {
        fun of(frame: Map<String, Any?>): RecordLine? {
            if (frame.str("kind") != "line") return null
            return decoding {
                RecordLine(frame.str("session_id").need(), frame.str("path").need(), frame.num("seq").need(), frame.num("byte_offset"), frame.obj("record"))
            }
        }
    }
}

/** `stream-watch` 应答里的一格：这个会话刚进名单，这条流从第 [seq] 行起发它；`[已有, seq)` 由看的那一方补。 */
data class WatchFrom(
    val sid: String,
    val path: String,
    val seq: Long,
)

/** `stream-watch`：这条流此刻在看哪几个会话（整份换；从没报过 ＝ 全看）。 */
object StreamWatch {
    const val COMMAND: String = "stream-watch"

    fun args(sids: Set<String>): Map<String, Any?> = mapOf("sids" to sids.sorted())

    fun of(data: Any?): List<WatchFrom>? =
        decoding {
            data
                .asObj()
                .objs("from")
                .need()
                .map { WatchFrom(it.str("sid").need(), it.str("path").need(), it.num("seq").need()) }
        }
}
