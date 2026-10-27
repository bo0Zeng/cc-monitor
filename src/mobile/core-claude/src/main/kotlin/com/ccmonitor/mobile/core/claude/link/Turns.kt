package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.TurnRow
import com.ccmonitor.mobile.core.wire.gen.HistoryTurns as HistoryTurnsPage

/**
 * `history-turns`：一轮的摘要（过程怎么分段、行上的字都在里面，同桌面 `session-reads.ts::readTurns`）。
 * 只拉了记录后半截 ⇒ 第一问拿已拉到的最上面那一行的字节问（[cover]：从包着它的那一轮起）；之后从还没收尾那一轮的 `at` 续取（[from]）。
 */
object Turns {
    const val COMMAND: String = "history-turns"

    fun cover(
        path: String,
        cover: Long,
    ): Map<String, Any?> = mapOf("path" to path, "cover" to cover)

    fun from(
        path: String,
        from: Long,
    ): Map<String, Any?> = mapOf("path" to path, "from" to from)

    fun of(data: Any?): HistoryTurnsPage? = Wire.bind<HistoryTurnsPage>(data)

    /** 手上那一份往后续取从哪个字节起：最早一轮还没收尾的那一轮；都收尾了 ⇒ 最后一轮（新的一轮从它后面长出来）；手上没有 ⇒ `null`。 */
    fun resumeAt(have: List<TurnRow>): Long? = (have.firstOrNull { !it.done } ?: have.lastOrNull())?.at

    /** 从 [from] 起续取到的那几轮接上手上那一份：开头不早于 [from] 的那几轮换成新的。 */
    fun merge(
        have: List<TurnRow>,
        from: Long,
        got: List<TurnRow>,
    ): List<TurnRow> = have.filter { it.at < from } + got
}
