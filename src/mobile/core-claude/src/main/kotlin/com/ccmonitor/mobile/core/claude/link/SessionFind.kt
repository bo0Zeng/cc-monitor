package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Batched
import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.HistoryFindReq
import com.ccmonitor.mobile.core.wire.gen.HistoryLinesReq
import com.ccmonitor.mobile.core.wire.gen.SearchScope
import com.ccmonitor.mobile.core.wire.gen.SidsReq
import com.ccmonitor.mobile.core.wire.gen.UserInputs
import com.ccmonitor.mobile.core.wire.gen.UserInputsReq
import com.ccmonitor.mobile.core.wire.gen.HistoryFind as FoundWire

/*
 * 一条会话里找 · 大纲 · 结束：都是那台核心的帧命令，手机只拼入参、解成品（`IPC-COMMANDS.md` 各节）。
 * 命中带的是行号（`line`，与实时行的 `seq` 同一个空间）；聊天屏的跳是按那一行的字节起点读（[LineStart]）。
 */

/** `history-find`：在一份记录里找一个词（只搜流里画的那几张卡；`include_tools` 缺 ⇒ 工具结果不搜）。 */
object InSessionFind {
    const val COMMAND: String = Commands.HistoryFind.NAME

    /** [user] ⇒ 只搜人说的（`scope: user`）；否则不带范围（全部）。 */
    fun args(
        path: String,
        query: String,
        user: Boolean = false,
    ): Map<String, Any?> = Commands.HistoryFind.args(HistoryFindReq(path = path, query = query, scope = SearchScope.USER.takeIf { user }))

    /** 这一页的命中（文件序）＋ 结果头那一句（`head`）＋ 能不能放宽到全部（`widen`）。 */
    fun of(data: Any?): FoundWire? = Commands.HistoryFind.reply(data)
}

/** `history-user-inputs`：「人说过的话」清单（大纲），对话序；每一条带那一行的字节起点 `at`。 */
object SaidList {
    const val COMMAND: String = Commands.HistoryUserInputs.NAME

    fun args(path: String): Map<String, Any?> = Commands.HistoryUserInputs.args(UserInputsReq(path = path))

    fun of(data: Any?): UserInputs? = Commands.HistoryUserInputs.reply(data)
}

/**
 * 行号 ⇒ 那一行的字节起点：问 `history-lines {from: L, until: L}`（一行都不交），回包的 `end` 是第 L 行之前最后一个可计行之后那个字节。
 * 第 0 行 ⇒ 0。
 */
object LineStart {
    const val COMMAND: String = Commands.HistoryLines.NAME

    fun args(
        path: String,
        line: Long,
    ): Map<String, Any?> = Commands.HistoryLines.args(HistoryLinesReq(path = path, from = line, until = line))

    fun of(data: Any?): Long? = Commands.HistoryLines.reply(data)?.end
}

/** `sessions-stop`：结束这几条会话（逐个答，一个不成不挡下一个）；不成的那一项带那台写好的那一句。 */
object SessionsStop {
    const val COMMAND: String = Commands.SessionsStop.NAME

    fun args(sids: List<String>): Map<String, Any?> = Commands.SessionsStop.args(SidsReq(sids = sids, client = CLIENT))

    fun of(data: Any?): Batched? = Commands.SessionsStop.reply(data)
}
