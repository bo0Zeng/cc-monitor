package com.ccmonitor.mobile.core.claude.link

/*
 * 答题口（`needs.ask` · `needs-answer`，`IPC-COMMANDS.md`）：一条挂着的待答请求可以怎么答、怎么交回。
 * 选项的字 · 次序 · 语气 · 要不要附话 · 题目全是核心写好的，这里只认格、一个字不拼（桌面 `answer-block.ts` 读同一份）。
 */

/** 一条挂着的请求（`needs.ask`）。 */
data class NeedsAsk(
    /** 后端发的请求 id：答的时候原样交回。 */
    val id: String,
    /** approve · answer · plan。 */
    val kind: String,
    val tool: String,
    /** 子 agent 那一趟；主会话 ⇒ `null`。 */
    val run: NeedsRun?,
    val what: String?,
    /** 要批的全文（命令全文 · 计划原文），不截。 */
    val detail: String,
    /** 能选的几项，核心排好的序。 */
    val choices: List<NeedsChoice>,
    /** 提问那一种的题目；别的种类为空。 */
    val questions: List<NeedsQuestion>,
) {
    companion object {
        fun of(m: Any?): NeedsAsk? = decoding { need(m) }

        internal fun need(m: Any?): NeedsAsk =
            m.asObj().let { a ->
                NeedsAsk(
                    id = a.str("id").need(),
                    kind = a.str("kind").need(),
                    tool = a.str("tool").need(),
                    run = a.obj("run")?.let { NeedsRun(it.str("id").need(), it.str("kind")) },
                    what = a.str("what"),
                    detail = a.str("detail").need(),
                    choices = a.objs("choices").need().map(NeedsChoice::need),
                    questions = a.objs("questions").need().map(NeedsQuestion::need),
                )
            }
    }
}

/** 子运行（子 agent）那一趟：`kind` 是那一家原样（如 `general-purpose`），没说 ⇒ `null`。 */
data class NeedsRun(
    val id: String,
    val kind: String?,
)

/** 要不要附一句话（闭集）。 */
enum class NeedsNote {
    NONE,
    OPTIONAL,
    REQUIRED,
    ;

    companion object {
        fun of(s: String?): NeedsNote? = entries.firstOrNull { it.name.equals(s, ignoreCase = true) }
    }
}

/** 一项选择：[id] 是答的时候交回的那个词（协议里的闭集），[text] 照抄上屏。 */
data class NeedsChoice(
    val id: String,
    val text: String,
    val tone: Tone?,
    val note: NeedsNote,
) {
    companion object {
        internal fun need(m: Map<String, Any?>): NeedsChoice =
            NeedsChoice(
                id = m.str("id").need(),
                text = m.str("text").need(),
                tone = Tone.of(m.str("tone")),
                note = NeedsNote.of(m.str("note")).need(),
            )
    }
}

/** 一道题（`q0` · `q1` …）；每题都许写自由文本。 */
data class NeedsQuestion(
    val id: String,
    val text: String,
    val header: String?,
    val multi: Boolean,
    val options: List<NeedsOption>,
) {
    companion object {
        internal fun need(m: Map<String, Any?>): NeedsQuestion =
            NeedsQuestion(
                id = m.str("id").need(),
                text = m.str("text").need(),
                header = m.str("header"),
                multi = m.bool("multi").need(),
                options = m.objs("options").need().map { NeedsOption(it.str("id").need(), it.str("text").need(), it.str("detail")) },
            )
    }
}

data class NeedsOption(
    val id: String,
    val text: String,
    val detail: String?,
)

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

    /** 入参：[note] · [picks] 只在给了时才交（提问那一种选 `answer` 时每题都交）。 */
    fun args(
        sid: String,
        ask: String,
        choice: String,
        note: String? = null,
        picks: Map<String, NeedsPick>? = null,
    ): Map<String, Any?> =
        buildMap {
            put("sid", sid)
            put("ask", ask)
            put("choice", choice)
            note?.let { put("note", it) }
            picks?.let { p -> put("picks", p.mapValues { (_, v) -> mapOf("options" to v.options.toList(), "text" to v.text.trim()) }) }
        }

    /** 别处先答了 / 那一头撤了（那一句是那台写好的）。 */
    fun gone(code: String): Boolean = code in GONE

    private val GONE = setOf("already_answered", "closed_elsewhere")
}

/** 一帧 `changed`：这台的某样东西变了（[topic] 主题，[key] 哪一个），出口重问那一份。 */
data class Changed(
    val topic: String,
    val key: String?,
) {
    companion object {
        fun of(frame: Map<String, Any?>): Changed? =
            if (frame.str("kind") != "changed") null else frame.str("topic")?.let { Changed(it, frame.str("key")) }

        /** 某条会话挂着的待答请求变了（挂上 · 撤掉 · 答掉）。 */
        const val NEEDS: String = "needs"
    }
}
