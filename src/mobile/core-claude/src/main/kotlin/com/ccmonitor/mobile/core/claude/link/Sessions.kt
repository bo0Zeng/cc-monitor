package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.AgentFacts
import com.ccmonitor.mobile.core.claude.link.wire.FirstSay
import com.ccmonitor.mobile.core.claude.link.wire.HistoryLinesPage
import com.ccmonitor.mobile.core.claude.link.wire.HistoryReadPage
import com.ccmonitor.mobile.core.claude.link.wire.Landed
import com.ccmonitor.mobile.core.claude.link.wire.LineRecord
import com.ccmonitor.mobile.core.claude.link.wire.ModeSwitched
import com.ccmonitor.mobile.core.claude.link.wire.ModelSwitched
import com.ccmonitor.mobile.core.claude.link.wire.NewFacts
import com.ccmonitor.mobile.core.claude.link.wire.Opened
import com.ccmonitor.mobile.core.claude.link.wire.ReadRow
import com.ccmonitor.mobile.core.claude.link.wire.SentResult
import com.ccmonitor.mobile.core.claude.link.wire.SessionCommands
import com.ccmonitor.mobile.core.claude.link.wire.SessionFacts
import com.ccmonitor.mobile.core.claude.link.wire.TailPlan
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import com.ccmonitor.mobile.core.claude.link.wire.Sent as SentWire

/*
 * 起会话 · 往会话里送字 · 读一条会话的正文：都是那台核心的帧命令，手机只拼入参、解成品（`IPC-COMMANDS.md` 各节）。
 * 正文是核心的通用记录（`IPC-PROTOCOL.md`「通用记录」）。
 */

/**
 * `session-open`：新起 · 恢复 · 接回 · 开一个 shell，都是这一问（那台定落在哪、渲好要在终端里敲的那一行）。
 * 手机的终端只是 ssh 那一屏：永远 `local: false`；新起 · 恢复永远起在那台 tmux 里，桌面那一侧看得到、接得上。
 */
object SessionOpen {
    const val COMMAND: String = "session-open"

    /**
     * 新起一条会话（[agent] 是哪一家的线上名，照那台 `session-new-facts` 的 `agents[].kind`）。账号 · 模型 · 权限都不带 ⇒ 跟随那台的默认。
     * [ticket] 一个草稿一张：期限到了同一张再问，那台认出是同一趟、不起第二个。
     * [prompt] 第一句：随起交（那台作为启动参数交给 agent，结局在回包的 `prompt`）。
     */
    fun newIn(
        cwd: String,
        ticket: String,
        agent: String,
        prompt: String,
        /** 改过账号 ⇒ 那个号名（`account: {kind: named}`）；没改 ⇒ `null`、不交（那台按它的默认号）。 */
        account: String? = null,
        /** 改过轮换 ⇒ 那一份（不轮换 ＝ `strategy: null`）；没改 ⇒ `null`、不交（那台按「新会话用」那份）。 */
        rotation: Rotation? = null,
    ): Map<String, Any?> =
        ask(
            buildMap {
                putAll(mapOf("kind" to "new", "agent" to agent, "cwd" to cwd, "prompt" to prompt))
                rotation?.let { put("rotation", mapOf("strategy" to it.strategy)) }
            },
            run = true,
            ticket = ticket,
            account = account,
        )

    /** 这一条新会话用哪份轮换策略（`what.rotation`）：[strategy] 为 `null` ＝ 不轮换。 */
    data class Rotation(
        val strategy: String?,
    )

    /** 在家目录起 [agent]、起在名叫 [name] 的会话里，不带第一句（tmux 管理窗「后台起」那一格）。 */
    fun newInTmux(
        agent: String,
        name: String,
        ticket: String,
    ): Map<String, Any?> = ask(mapOf("kind" to "new", "agent" to agent, "cwd" to "~", "tmuxName" to name), run = true, ticket = ticket)

    /** 恢复 [sid] 那条（终端没了 ⇒ 那台照记录起回来；已在跑 ⇒ 回它所在的那个）。 */
    fun resume(sid: String): Map<String, Any?> = ask(mapOf("kind" to "resume", "sid" to sid), run = true)

    /** 接回名叫 [session] 的那个 tmux 会话（不在 ⇒ 那台拒，不建）。 */
    fun look(session: String): Map<String, Any?> = ask(mapOf("kind" to "look", "session" to session))

    /**
     * 开一个 shell：给了 [session] ⇒ 那台 tmux 里那个会话（不在就在 [cwd] 里建一个空的）；只给 [cwd] ⇒ 进那个目录那一行（那台的 shell 方言）。
     * [cwd] 只在建的那一刻生效。
     */
    fun shell(
        session: String?,
        cwd: String?,
    ): Map<String, Any?> =
        ask(
            buildMap {
                put("kind", "shell")
                session?.let { put("session", it) }
                cwd?.let { put("cwd", it) }
            },
        )

    private fun ask(
        what: Map<String, Any?>,
        run: Boolean = false,
        ticket: String? = null,
        account: String? = null,
    ): Map<String, Any?> =
        buildMap {
            put("what", what)
            account?.let { put("account", mapOf("kind" to "named", "name" to it)) }
            if (run) put("run", "tmux")
            put("local", false)
            ticket?.let { put("ticket", it) }
            put("client", CLIENT)
        }

    data class Answer(
        val landed: Landed,
        /** 落在哪个 tmux 会话（手机这几形都有）。 */
        val session: String?,
        /** 要在终端里敲的那一行（那台渲好的）。 */
        val line: String?,
        /** 那台先定好的 sid；那一家不认先定 ⇒ `null`（报到之前说不出）。 */
        val sid: String?,
        /** 展开之后的目录：认报到的会话按它；接回 ⇒ `null`。 */
        val cwd: String?,
        /** 那台要附带说的一句；没有 ⇒ `null`。 */
        val note: String?,
        /** 第一句交进去的结局（同 `terminal-input` 的回话）；没给第一句 ⇒ `null`。 */
        val prompt: TerminalInput.Result?,
    )

    /** 核心按 `cmd-wire` 那一份解过的回包 ⇒ 这一份（第一句的结局折成 [TerminalInput.Result]）。 */
    fun of(data: Any?): Answer? {
        val r = Wire.bind<Opened>(data) ?: return null
        val prompt = r.prompt?.let { TerminalInput.of(it) ?: return null }
        return Answer(r.landed, r.session, r.line, r.sid, r.cwd, r.note, prompt)
    }

    /** 起好之后报到的那一条：会话表里目录是 [cwd]、起之前（[before]）还没有的。和桌面认法一样（`awaitArrival` 的 `{cwd}`）。 */
    fun arrived(
        table: SessionTable,
        cwd: String,
        before: Set<String>,
    ): LiveSession? = table.sessions.values.firstOrNull { it.cwd == cwd && it.sid !in before }
}

/**
 * `session-new-facts`：这台能起哪几家（注册表序；各带写给人看的名字与可选的模型）。哪几家 · 叫什么 · 有哪些模型都问那台，手机一家都不认识。
 */
object SessionNewFacts {
    const val COMMAND: String = "session-new-facts"

    /** 整份：几家（[NewFacts.agents]）＋ 最近的目录。 */
    fun of(data: Any?): NewFacts? = Wire.bind<NewFacts>(data)

    /** 这台存的那一家（线上 kind）；没存 · 那台不认 ⇒ 那台列的第一家；一家都没有 ⇒ `null`。 */
    fun pick(
        agents: List<AgentFacts>,
        stored: String?,
    ): AgentFacts? = agents.firstOrNull { it.kind == stored } ?: agents.firstOrNull()
}

/** `history-facts`：一条会话的事实（聊天屏只读 chip 上的两个显示字：此刻的模型 · 权限档，那台写好的）。 */
object HistoryFacts {
    const val COMMAND: String = "history-facts"

    fun args(path: String): Map<String, Any?> = mapOf("path" to path)

    fun of(data: Any?): SessionFacts? = Wire.bind<SessionFacts>(data)
}

/** `session-model`：中途给一条会话换模型（那台送命令、读回来认；在等人不送）。结局照那台写好的那一句说。 */
object SessionModel {
    const val COMMAND: String = "session-model"

    fun args(
        sid: String,
        model: String,
    ): Map<String, Any?> = mapOf("sid" to sid, "model" to model, "client" to CLIENT)

    /** 结局（`done` · `unsure` · `refused`）＋ 读回的显示名 ＋ 那台写好的那一句。 */
    fun of(data: Any?): ModelSwitched? = Wire.bind<ModelSwitched>(data)
}

/** `session-permission`：中途给一条会话切权限档（那台按那一家的循环键、读回到了为止；在等人不按）。 */
object SessionPermission {
    const val COMMAND: String = "session-permission"

    /** [mode] 是那台 `session-new-facts` 那一家 `permissionModes` 里的原值。 */
    fun args(
        sid: String,
        mode: String,
    ): Map<String, Any?> = mapOf("sid" to sid, "mode" to mode, "client" to CLIENT)

    /** 结局 ＋ 读回那一档写给人看的字 ＋ 那台写好的那一句。 */
    fun of(data: Any?): ModeSwitched? = Wire.bind<ModeSwitched>(data)
}

/** `session-commands`：一条会话能用的斜杠命令与 Skill（那台按那条会话的目录与号现扫，字也是它写的）。 */
object SessionCommandList {
    const val COMMAND: String = "session-commands"

    fun args(sid: String): Map<String, Any?> = mapOf("sid" to sid)

    fun of(data: Any?): SessionCommands? = Wire.bind<SessionCommands>(data)
}

/** `session-say`：往一条会话说一句（那台判：在等人不送 · 终端没了按记下的号恢复再交）。结局同 [TerminalInput]。 */
object SessionSay {
    const val COMMAND: String = "session-say"

    fun args(
        sid: String,
        text: String,
    ): Map<String, Any?> = mapOf("sid" to sid, "text" to text, "client" to CLIENT)

    fun of(data: Any?): TerminalInput.Result? = TerminalInput.of(data)
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

    /** 核心按 `cmd-wire` 那一份解过的回包 ⇒ 结局。被拒却没带那一句 ⇒ `null`（没有可上屏的话）。 */
    fun of(data: Any?): Result? = Wire.bind<SentWire>(data)?.let { of(it.result, it.why, it.said) }

    /** 起会话时第一句的结局（同三格，`result` 是字：照 [SentResult] 的线上字认）。 */
    fun of(first: FirstSay): Result? = Wire.bind<SentResult>(first.result)?.let { of(it, first.why, first.said) }

    private fun of(
        result: SentResult,
        why: String?,
        said: String?,
    ): Result? =
        when (result) {
            SentResult.DELIVERED -> Result.Delivered
            SentResult.UNSURE -> Result.Unsure
            SentResult.REFUSED -> said?.let { Result.Refused(it, why) }
        }
}

/** `history-tail`：一份会话最新 [n] 行从哪个字节起。 */
object HistoryTail {
    const val COMMAND: String = "history-tail"

    fun args(
        path: String,
        n: Int,
    ): Map<String, Any?> = mapOf("path" to path, "n" to n.toLong())

    /**
     * 回包就是 `cmd-wire` 的 `TailPlan`（字节 `end` · 尾段起点 `splitAt` · 尾段第一行行号 `tailFrom`，与 `line` 帧的 `seq` 同一个空间 · 可计行总数 `total` ·
     * 文件身份 `file`：同一路径换了一份就变）。
     */
    fun of(data: Any?): TailPlan? = Wire.bind<TailPlan>(data)
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
        val records: List<LineRecord>,
        /** 每条记录那一行之后的字节（与 [records] 一一对应）。 */
        val ends: List<Long>,
        val next: Long,
        val eof: Boolean,
    )

    fun of(data: Any?): Page? {
        val p = Wire.bind<HistoryReadPage>(data) ?: return null
        val rows = p.rows.mapNotNull { r -> r.record?.let { it to (r.end ?: -1L) } }
        return Page(rows.map { it.first }, rows.map { it.second }, p.next, p.eof)
    }

    /** 回包里的 `rows[]` 原样（缓存存的就是它）；不是那一形 ⇒ `null`。 */
    fun rawRows(data: Any?): List<Any?>? = ((data as? Map<*, *>)?.get("rows") as? List<*>)?.toList()

    /** 缓存里那几格（`rows[]` 原样）⇒ 记录，与 [of] 同一个解码器；有一格解不出 ⇒ `null`（整页不认）。 */
    fun decodeRows(rows: List<Any?>): List<LineRecord>? =
        rows.map { Wire.bind<ReadRow>(it) ?: return null }.mapNotNull { it.record }

    /** 一行记录（流上的 `line` · `history-lines` 的一格）照 `rows[]` 那一格的形装起来：`{end, record}`，[record] 原样。 */
    fun row(
        end: Long?,
        record: Any?,
    ): Map<String, Any?> = mapOf("end" to end, "record" to record)
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
        val records: List<LineRecord>,
        /** 每条记录的行号（与 [records] 一一对应）。 */
        val seqs: List<Long>,
        /** 下一段从这一行起。 */
        val next: Long,
        val eof: Boolean,
        /** 这一段最后一个可计行之后的字节：续点 ＝（路径 · 它 · [next]）。 */
        val end: Long,
        /** 每条记录那一行之后的字节（与 [records] 一一对应）。 */
        val ends: List<Long?> = emptyList(),
        /** 每条记录原样（与 [records] 一一对应；缓存存它）。 */
        val raw: List<Any?> = emptyList(),
    )

    fun of(data: Any?): Page? {
        val p = Wire.bind<HistoryLinesPage>(data) ?: return null
        val raw = ((data as? Map<*, *>)?.get("lines") as? List<*>)?.map { (it as? Map<*, *>)?.get("record") }.orEmpty()
        return Page(p.lines.map { it.record }, p.lines.map { it.seq }, p.next, p.eof, p.end, p.lines.map { it.end }, raw)
    }
}

/** 流上的 `line` 帧：一条会话记录文件里新写的一行（`record` 缺 ＝ 不进界面、照占号；形状对不上的那一条核心那一层已换成认不出那一条〔`unread` 卡〕，同按页读的那两条）。 */
data class RecordLine(
    val sid: String,
    val path: String,
    /** 这一行在这份记录里的行号（与 `history-tail.total` · `history-lines` · `stream-watch.from[].seq` 同一个空间）：接续与去重按它。 */
    val seq: Long,
    /** 这一行末尾在文件里的累计字节。 */
    val byteOffset: Long?,
    val record: LineRecord?,
    /** [record] 原样（缓存存它）。 */
    val raw: Any? = null,
) {
    companion object {
        fun of(frame: Map<String, Any?>): RecordLine? {
            if (frame.str("kind") != "line") return null
            val record = frame["record"]?.let { Wire.bind<LineRecord>(it) }
            return decoding {
                RecordLine(frame.str("session_id").need(), frame.str("path").need(), frame.num("seq").need(), frame.num("byte_offset"), record, frame["record"])
            }
        }
    }
}

/** `stream-watch` 应答里的一格（核心报名单、交回来）：这个会话刚进名单，这条流从第 [seq] 行起发它；`[已有, seq)` 由看的那一方补。 */
data class WatchFrom(
    val sid: String,
    val path: String,
    val seq: Long,
)
