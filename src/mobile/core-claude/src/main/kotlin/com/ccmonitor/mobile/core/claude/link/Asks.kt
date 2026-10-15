package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.AnswerReq
import com.ccmonitor.mobile.core.claude.link.wire.AskPick
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/*
 * 答题口（`needs.ask` · `needs-answer`）：挂着的请求是 `sessions-needs` 那一份里的 `NeedsAsk`（线上类型，`wire/WireTypes.kt`）；
 * 这里只有答那一条：入参照线上的 `AnswerReq` 出，与桌面 · 后端同一形。
 */

/** 一道题此刻答成什么样：选了哪几项 ＋ 自由文本。 */
data class NeedsPick(
    val options: Set<String>,
    val text: String,
) {
    /** 答了（选了一项或写了字）。 */
    val answered: Boolean get() = options.isNotEmpty() || text.isNotBlank()
}

/** `needs-answer`：答一条挂着的请求，第一个答案落下。 */
object NeedsAnswer {
    const val COMMAND: String = "needs-answer"

    /** 推送 `changed` 的主题：某条会话挂着的待答请求变了（挂上 · 撤掉 · 答掉），键是 sid。 */
    const val TOPIC: String = "needs"

    /** 入参：[note] · [picks] 只在给了时才交（提问那一种选 `answer` 时每题都交）。 */
    fun args(
        sid: String,
        ask: String,
        choice: String,
        note: String? = null,
        picks: Map<String, NeedsPick>? = null,
    ): Map<String, Any?> =
        Wire.args(
            AnswerReq(
                sid = sid,
                ask = ask,
                choice = choice,
                note = note,
                picks = picks?.mapValues { (_, v) -> AskPick(v.options.toList(), v.text.trim()) },
            ),
        )

    /** 别处先答了 / 那一头撤了（那一句是那台写好的）。 */
    fun gone(code: String): Boolean = code in GONE

    private val GONE = setOf("already_answered", "closed_elsewhere")
}
