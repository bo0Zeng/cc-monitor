package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.AccountAsk
import com.ccmonitor.mobile.core.wire.gen.AgentFacts
import com.ccmonitor.mobile.core.wire.gen.ArriveAsk
import com.ccmonitor.mobile.core.wire.gen.ArriveEnd
import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.FactsReply
import com.ccmonitor.mobile.core.wire.gen.Held
import com.ccmonitor.mobile.core.wire.gen.HistoryFactsReq
import com.ccmonitor.mobile.core.wire.gen.HistoryImageReq
import com.ccmonitor.mobile.core.wire.gen.HistoryLinesReq
import com.ccmonitor.mobile.core.wire.gen.HlCode
import com.ccmonitor.mobile.core.wire.gen.ProseHlReq
import com.ccmonitor.mobile.core.wire.gen.SegHl
import com.ccmonitor.mobile.core.wire.gen.HistoryReadReq
import com.ccmonitor.mobile.core.wire.gen.HistoryRunReq
import com.ccmonitor.mobile.core.wire.gen.HistoryTailReq
import com.ccmonitor.mobile.core.wire.gen.JsonValue
import com.ccmonitor.mobile.core.wire.gen.LOCAL
import com.ccmonitor.mobile.core.wire.gen.Landed
import com.ccmonitor.mobile.core.wire.gen.Launch
import com.ccmonitor.mobile.core.wire.gen.Line
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.core.wire.gen.ModeSwitched
import com.ccmonitor.mobile.core.wire.gen.ModelSwitched
import com.ccmonitor.mobile.core.wire.gen.OpenFacts
import com.ccmonitor.mobile.core.wire.gen.OpenWhat
import com.ccmonitor.mobile.core.wire.gen.QueueCancelled
import com.ccmonitor.mobile.core.wire.gen.QueueList
import com.ccmonitor.mobile.core.wire.gen.QueuePut
import com.ccmonitor.mobile.core.wire.gen.Rewind
import com.ccmonitor.mobile.core.wire.gen.RewindOptions
import com.ccmonitor.mobile.core.wire.gen.RotationPick
import com.ccmonitor.mobile.core.wire.gen.SentAct
import com.ccmonitor.mobile.core.wire.gen.SentResult
import com.ccmonitor.mobile.core.wire.gen.SessionCommands
import com.ccmonitor.mobile.core.wire.gen.SessionCommandsReq
import com.ccmonitor.mobile.core.wire.gen.SessionInterruptReq
import com.ccmonitor.mobile.core.wire.gen.SessionModelReq
import com.ccmonitor.mobile.core.wire.gen.SessionOpenReq
import com.ccmonitor.mobile.core.wire.gen.SessionPermissionReq
import com.ccmonitor.mobile.core.wire.gen.SessionQueueCancelReq
import com.ccmonitor.mobile.core.wire.gen.SessionQueuePutReq
import com.ccmonitor.mobile.core.wire.gen.SessionQueueReadReq
import com.ccmonitor.mobile.core.wire.gen.SessionRewindOptionsReq
import com.ccmonitor.mobile.core.wire.gen.HistoryRun as HistoryRunPage
import com.ccmonitor.mobile.core.wire.gen.HistoryTail as HistoryTailReply
import com.ccmonitor.mobile.core.wire.gen.Sent as SentWire

/*
 * 起会话 · 往会话里送字 · 读一条会话的正文：都是那台核心的帧命令，手机只拼入参、解成品（`IPC-COMMANDS.md` 各节）。
 * 命令名 · 入参 · 回包都取生成的那一份（`Commands.<命令>`，照 TypedCommands）：这里只填格。
 * 正文是核心的通用记录（`IPC-PROTOCOL.md`「通用记录」）。
 */

/**
 * `session-open`：新起 · 恢复 · 接回 · 开一个 shell，都是这一问（那台定落在哪、渲好要在终端里敲的那一行）。
 * 手机的终端只是 ssh 那一屏：永远 `local: false`；新起 · 恢复永远起在那台 tmux 里，桌面那一侧看得到、接得上。
 */
object SessionOpen {
    const val COMMAND: String = Commands.SessionOpen.NAME

    /**
     * 新起一条会话（[agent] 是哪一家的线上名，照那台 `session-open-facts` 的 `agents[].kind`）。账号 · 模型 · 权限都不带 ⇒ 跟随那台的默认。
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
            OpenWhat.New(agent = agent, cwd = cwd, prompt = prompt, rotation = rotation?.let { RotationPick(it.strategy) }),
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
    ): Map<String, Any?> = ask(OpenWhat.New(agent = agent, cwd = "~", tmuxName = name), run = true, ticket = ticket)

    /** 恢复 [sid] 那条（终端没了 ⇒ 那台照记录起回来；已在跑 ⇒ 回它所在的那个）。 */
    fun resume(sid: String): Map<String, Any?> = ask(OpenWhat.Resume(sid), run = true)

    /** 接回名叫 [session] 的那个 tmux 会话（不在 ⇒ 那台拒，不建）。 */
    fun look(session: String): Map<String, Any?> = ask(OpenWhat.Look(session = session))

    /**
     * 开一个 shell：给了 [session] ⇒ 那台 tmux 里那个会话（不在就在 [cwd] 里建一个空的）；只给 [cwd] ⇒ 进那个目录那一行（那台的 shell 方言）。
     * [cwd] 只在建的那一刻生效。
     */
    fun shell(
        session: String?,
        cwd: String?,
    ): Map<String, Any?> = ask(OpenWhat.Shell(session = session, cwd = cwd))

    private fun ask(
        what: OpenWhat,
        run: Boolean = false,
        ticket: String? = null,
        account: String? = null,
    ): Map<String, Any?> =
        Commands.SessionOpen.args(
            SessionOpenReq(
                what = what,
                account = account?.let { AccountAsk.Named(it) },
                run = Launch.TMUX.takeIf { run },
                local = false,
                ticket = ticket,
                client = CLIENT,
            ),
        )

    data class Answer(
        val landed: Landed,
        /** 落在哪个 tmux 会话（手机这几形都有）。 */
        val session: String?,
        /** 要在终端里敲的那一行（那台渲好的）。 */
        val line: String?,
        /** 那台先定好的 sid；那一家不认先定 ⇒ `null`（报到之前说不出）。 */
        val sid: String?,
        /** 展开之后的目录；接回 ⇒ `null`。 */
        val cwd: String?,
        /** 那台要附带说的一句；没有 ⇒ `null`。 */
        val note: String?,
        /** 第一句交进去的结局（同 `terminal-input` 的回话）；没给第一句 ⇒ `null`。 */
        val prompt: TerminalInput.Result?,
        /** 起了一家 ⇒ 等它报到那一问（原样交 [SessionArrive]）；没起 ⇒ `null`。 */
        val arrival: ArriveAsk?,
    )

    /** 核心按 `cmd-wire` 那一份解过的回包 ⇒ 这一份（第一句的结局折成 [TerminalInput.Result]）。 */
    fun of(data: Any?): Answer? {
        val r = Commands.SessionOpen.reply(data) ?: return null
        val prompt = r.prompt?.let { TerminalInput.of(it) ?: return null }
        return Answer(r.landed, r.session, r.line, r.sid, r.cwd, r.note, prompt, r.arrival)
    }
}

/**
 * `session-arrive`：等起好的那一条在那台报到（`session-open` 回包里的 `arrival` 原样交回；等多久 ＝ 这一问的期限）。
 * 认法 · 没见到那一句 · 那一屏原话都是那台的，手机只照回包说。
 */
object SessionArrive {
    const val COMMAND: String = Commands.SessionArrive.NAME

    fun args(ask: ArriveAsk): Map<String, Any?> = Commands.SessionArrive.args(ask)

    /** 报到了 ⇒ `sid`；没见到 ⇒ `said`（那台写好的那一句）。 */
    fun of(data: Any?): ArriveEnd? = Commands.SessionArrive.reply(data)
}

/**
 * `session-open-facts`：这台能起哪几家（注册表序；各带写给人看的名字与可选的模型）。哪几家 · 叫什么 · 有哪些模型都问那台，手机一家都不认识。
 */
object SessionOpenFacts {
    const val COMMAND: String = Commands.SessionOpenFacts.NAME

    /** 整份：几家（[OpenFacts.agents]）＋ 最近的目录。 */
    fun of(data: Any?): OpenFacts? = Commands.SessionOpenFacts.reply(data)

    /** 这台存的那一家（线上 kind）；没存 · 那台不认 ⇒ 那台列的第一家；一家都没有 ⇒ `null`。 */
    fun pick(
        agents: List<AgentFacts>,
        stored: String?,
    ): AgentFacts? = agents.firstOrNull { it.kind == stored } ?: agents.firstOrNull()
}

/**
 * `history-facts`：一条会话的事实（聊天屏读 chip 上的两个显示字：此刻的模型 · 权限档 ＋ 项目目录，那台写好的）。
 * 手上有记录路径 ⇒ 按路径问；只有会话 id（会话表里没有、续点里也没有）⇒ 按 sid 问，那台找那份记录、回包的 `path` 就是它。
 */
object HistoryFacts {
    const val COMMAND: String = Commands.HistoryFacts.NAME

    fun args(path: String): Map<String, Any?> = Commands.HistoryFacts.args(HistoryFactsReq.Path(path))

    fun bySid(sid: String): Map<String, Any?> = Commands.HistoryFacts.args(HistoryFactsReq.Sid(sid))

    fun of(data: Any?): FactsReply? = Commands.HistoryFacts.reply(data)
}

/** `session-model`：中途给一条会话换模型（那台送命令、读回来认；在等人不送）。结局照那台写好的那一句说。 */
object SessionModel {
    const val COMMAND: String = Commands.SessionModel.NAME

    fun args(
        sid: String,
        model: String,
    ): Map<String, Any?> = Commands.SessionModel.args(SessionModelReq(sid = sid, model = model, client = CLIENT))

    /** 结局（`done` · `unsure` · `refused`）＋ 读回的显示名 ＋ 那台写好的那一句。 */
    fun of(data: Any?): ModelSwitched? = Commands.SessionModel.reply(data)
}

/** `session-permission`：中途给一条会话切权限档（那台按那一家的循环键、读回到了为止；在等人不按）。 */
object SessionPermission {
    const val COMMAND: String = Commands.SessionPermission.NAME

    /** [mode] 是那台 `session-open-facts` 那一家 `permissionModes` 里的原值。 */
    fun args(
        sid: String,
        mode: String,
    ): Map<String, Any?> = Commands.SessionPermission.args(SessionPermissionReq(sid = sid, mode = mode, client = CLIENT))

    /** 结局 ＋ 读回那一档写给人看的字 ＋ 那台写好的那一句。 */
    fun of(data: Any?): ModeSwitched? = Commands.SessionPermission.reply(data)
}

/** `session-commands`：一条会话能用的斜杠命令与 Skill（那台按那条会话的目录与号现扫，字也是它写的）。 */
object SessionCommandList {
    const val COMMAND: String = Commands.SessionCommands.NAME

    fun args(sid: String): Map<String, Any?> = Commands.SessionCommands.args(SessionCommandsReq(sid))

    fun of(data: Any?): SessionCommands? = Commands.SessionCommands.reply(data)
}

/**
 * `session-queue-put`：往一条会话说一句 ＝ 投进那条会话的输入队（会话那台落盘了才回；那台能送时再送进 agent）。
 * [id] 是这一端待发箱里那一句的编号：没回执就连上重发同一个，那台按它去重、只进一条。
 */
object SessionQueuePut {
    const val COMMAND: String = Commands.SessionQueuePut.NAME

    /**
     * [attachments]：随这一句发的附件编号（[SessionAttachmentCommit] 回的，按草稿里的先后）；没有 ⇒ 不带这一格。
     * [rewind]：编辑已发的那一句（确认那一下）：会话那台投递时先退到 `rewind.to` 之前再说它；普通的一句 ⇒ 不带。
     */
    fun args(
        sid: String,
        id: String,
        text: String,
        atMs: Long,
        attachments: List<String> = emptyList(),
        rewind: Rewind? = null,
    ): Map<String, Any?> =
        Commands.SessionQueuePut.args(
            SessionQueuePutReq(
                machine = LOCAL,
                sid = sid,
                id = id,
                end = CLIENT,
                atMs = atMs,
                text = text,
                attachments = attachments.takeIf { it.isNotEmpty() },
                rewind = rewind,
            ),
        )

    fun of(data: Any?): QueuePut? = Commands.SessionQueuePut.reply(data)
}

/** `session-rewind-options`：编辑已发的一句、发送那一下给哪几项（会话那台照那一家申报的交出，界面照摆）。 */
object SessionRewindOptions {
    const val COMMAND: String = Commands.SessionRewindOptions.NAME

    /** `to`：被编辑那一句的 uuid（哪一项能选按那一句之后的事判）。 */
    fun args(
        sid: String,
        to: String,
    ): Map<String, Any?> = Commands.SessionRewindOptions.args(SessionRewindOptionsReq(machine = LOCAL, sid = sid, to = to))

    fun of(data: Any?): RewindOptions? = Commands.SessionRewindOptions.reply(data)
}

/** `session-queue-cancel`：撤队里一条还没送的（回撤完的整份名单）。 */
object SessionQueueCancel {
    const val COMMAND: String = Commands.SessionQueueCancel.NAME

    fun args(
        sid: String,
        id: String,
    ): Map<String, Any?> = Commands.SessionQueueCancel.args(SessionQueueCancelReq(machine = LOCAL, sid = sid, id = id))

    fun of(data: Any?): QueueCancelled? = Commands.SessionQueueCancel.reply(data)
}

/** `session-queue-read`：一条会话此刻的名单（会话那台那一份；推送 `changed {session_queue}` 没带体时重问它）。 */
object SessionQueueRead {
    const val COMMAND: String = Commands.SessionQueueRead.NAME

    fun args(sid: String): Map<String, Any?> = Commands.SessionQueueRead.args(SessionQueueReadReq(machine = LOCAL, sid = sid))

    fun of(data: Any?): QueueList? = Commands.SessionQueueRead.reply(data)
}

/** `session-interrupt`：中断一条会话这一轮（■）。不进输入队、不靠持有输入；回包同 `terminal-input`（[TerminalInput.of] 解）。 */
object SessionInterrupt {
    const val COMMAND: String = Commands.SessionInterrupt.NAME

    fun args(sid: String): Map<String, Any?> = Commands.SessionInterrupt.args(SessionInterruptReq(sid = sid, client = CLIENT))
}

/** 那台判的送没送到（`terminal-input` 回话那一形；`session-open` 第一句 · `session-interrupt` 的回包同形）。手机不发 `terminal-input`。 */
object TerminalInput {
    sealed interface Result {
        data object Delivered : Result

        /** 不知道送没送到：别重发。 */
        data object Unsure : Result

        /** 没送：[said] 是那台写好的那一句，[acts] 是那台判好的下一步（有 [SentAct.RETRY] ⇒ 给［再发一次］）。 */
        data class Refused(
            val said: String,
            val acts: List<SentAct>,
        ) : Result
    }

    /** 核心按 `cmd-wire` 那一份解过的回包 ⇒ 结局。被拒却没带那一句 ⇒ `null`（没有可上屏的话）。 */
    fun of(data: Any?): Result? = Commands.TerminalInput.reply(data)?.let { of(it) }

    /** 解过的回包（`terminal-input` · `session-open` 里第一句那一格同一形）⇒ 结局。 */
    fun of(sent: SentWire): Result? =
        when (sent.result) {
            SentResult.DELIVERED -> Result.Delivered
            SentResult.UNSURE -> Result.Unsure
            SentResult.REFUSED -> sent.said?.let { Result.Refused(it, sent.acts.orEmpty()) }
        }
}

/**
 * `history-tail`：一份会话最新 [n] 行在哪 ＋ 手上那一份（[held]，上一次回的、之后流上的行推过的）怎么接，连第一页正文一问回来。
 * 怎么接那台判（续得上 · 变短 · 被改写 · 缺得比 [n] 多）；第一页与 `history-read` 同一形（`rows` · `next` · `eof`，[HistoryRead.of] 解）。
 */
object HistoryTail {
    const val COMMAND: String = Commands.HistoryTail.NAME

    fun args(
        path: String,
        n: Int,
        held: Held? = null,
    ): Map<String, Any?> = Commands.HistoryTail.args(HistoryTailReq(path = path, n = n.toLong(), held = held))

    fun of(data: Any?): HistoryTailReply? = Commands.HistoryTail.reply(data)
}

/**
 * `history-run`：一个子运行的记录，一页（父记录 [parent] ＋ 运行表里那一格 [run]）。哪一份记录 · 一页多大 · 读到哪那台定；
 * 收场的从头读（[after] 0）、在跑的从尾巴读（[tail]），往前一页拿上一页的 `from` 当 [before]，往后一页拿 `end` 当 [after] 的起点。
 */
object HistoryRun {
    const val COMMAND: String = Commands.HistoryRun.NAME

    fun after(
        parent: String,
        run: String,
        from: Long,
    ): Map<String, Any?> = Commands.HistoryRun.args(HistoryRunReq(parent = parent, run = run, from = from))

    fun tail(
        parent: String,
        run: String,
    ): Map<String, Any?> = Commands.HistoryRun.args(HistoryRunReq(parent = parent, run = run, tail = true))

    fun before(
        parent: String,
        run: String,
        before: Long,
    ): Map<String, Any?> = Commands.HistoryRun.args(HistoryRunReq(parent = parent, run = run, before = before))

    fun of(data: Any?): HistoryRunPage? = Commands.HistoryRun.reply(data)
}

/** `history-read`：按字节读 `[offset, until)` 那一段，每行一条通用记录（不进界面的行没有记录、不出）。 */
object HistoryRead {
    const val COMMAND: String = Commands.HistoryRead.NAME

    fun args(
        path: String,
        offset: Long,
        until: Long,
    ): Map<String, Any?> = Commands.HistoryRead.args(HistoryReadReq(path = path, offset = offset, until = until))

    data class Page(
        val records: List<LineRecord>,
        /** 每条记录那一行之后的字节（与 [records] 一一对应）。 */
        val ends: List<Long>,
        val next: Long,
        val eof: Boolean,
    )

    fun of(data: Any?): Page? {
        val p = Commands.HistoryRead.reply(data) ?: return null
        val rows = p.rows.mapNotNull { r -> r.record?.let { it to (r.end ?: -1L) } }
        return Page(rows.map { it.first }, rows.map { it.second }, p.next, p.eof)
    }

    /** 回包里的 `rows[]` 原样（缓存存的就是它）；不是那一形 ⇒ `null`。 */
    fun rawRows(data: Any?): List<Any?>? = ((data as? Map<*, *>)?.get("rows") as? List<*>)?.toList()

    /**
     * 缓存里那几格 ⇒ 记录（每格只取 `record`，与 [of] 同一个解码器；不带记录的那一格不进界面）；有一格解不出 ⇒ `null`（整页不认）。
     * 缓存的格有两种来处：`rows[]` 原样（带摘要）· 流上的行（[row]，不带摘要）—— 手机只读记录那一格。
     */
    fun decodeRows(rows: List<Any?>): List<LineRecord>? =
        rows.mapNotNull { r ->
            val m = r as? Map<*, *> ?: return null
            m["record"]?.let { Wire.bind<LineRecord>(it) ?: return null }
        }

    /** 一行记录（流上的 `line` · `history-lines` 的一格）进缓存的那一格：`{end, record}`，[record] 原样（流上的行不带摘要，就不写这一格）。 */
    fun row(
        end: Long?,
        record: Any?,
    ): Map<String, Any?> = mapOf("end" to end, "record" to record)
}

/** `history-image`：按引用取回一张图（记录里的图片块只带引用：哪一行 `at` · 第几张 `n` ＋ 那一条的 `id`）。 */
object HistoryImage {
    const val COMMAND: String = Commands.HistoryImage.NAME

    fun args(
        path: String,
        at: Long,
        n: Int,
        id: String,
    ): Map<String, Any?> = Commands.HistoryImage.args(HistoryImageReq(path = path, at = at, n = n.toLong(), id = id))

    /** 回包里那张图的 base64；解不开 ⇒ `null`。 */
    fun of(data: Any?): String? = Commands.HistoryImage.reply(data)?.data
}

/** `prose-hl`：拉一段正文的代码高亮（交那一段的代码块，先序；回包的 `hl[n]` 对第 n 块）。 */
object ProseHl {
    const val COMMAND: String = Commands.ProseHl.NAME

    fun args(codes: List<HlCode>): Map<String, Any?> = Commands.ProseHl.args(ProseHlReq(codes = codes))

    /** 解不开 ⇒ `null`。 */
    fun of(data: Any?): SegHl? = Commands.ProseHl.reply(data)
}

/** `history-lines`：按行号取 `[from, until)` 那一段（不进界面的行照占号、不出）。 */
object HistoryLines {
    const val COMMAND: String = Commands.HistoryLines.NAME

    fun args(
        path: String,
        from: Long,
        until: Long,
    ): Map<String, Any?> = Commands.HistoryLines.args(HistoryLinesReq(path = path, from = from, until = until))

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
        val p = Commands.HistoryLines.reply(data) ?: return null
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
            if (frame["kind"] != "line") return null
            val f = Wire.bind(Line.adapter(JsonValue), frame) ?: return null
            val record = frame["record"]?.let { Wire.bind<LineRecord>(it) }
            return RecordLine(f.sessionId, f.path, f.seq, f.byteOffset, record, frame["record"])
        }
    }
}

/** `stream-watch` 应答里的一格（核心报名单、交回来）：这个会话刚进名单，这条流从第 `seq` 行起发它；`[已有, seq)` 由看的那一方补。 */
typealias WatchFrom = com.ccmonitor.mobile.core.wire.gen.WatchFrom
