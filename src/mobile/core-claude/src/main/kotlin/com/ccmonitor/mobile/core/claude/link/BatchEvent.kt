package com.ccmonitor.mobile.core.claude.link

/**
 * 一组一条命令走到哪（核心交的 `batch` 那一行；字全由核心写好，这里不拼一个字）。
 * [phase]：`running`（[current] 手上那一项 · [frac] 几成）· `asking`（[conflicts] 同名那几项）· `cut`（[line] 断开那一行）·
 * `ended`（[said] 结局句 · [failed] 没成的那几行（[failedTitle] 那一屏的标题）· [details] 复制详情 · [retry] / [undo] 给不给）· `failed`（[said] · [details]）· `dropped`（取消了）。
 */
data class BatchEvent(
    val id: Long,
    val phase: String,
    val title: String,
    val current: String? = null,
    val frac: Float? = null,
    val line: String? = null,
    val said: String? = null,
    val failed: List<String> = emptyList(),
    /** 看失败的那一屏的标题。 */
    val failedTitle: String? = null,
    val details: String? = null,
    val retry: Boolean = false,
    val undo: Boolean = false,
    val conflicts: List<Clash> = emptyList(),
) {
    /** 同名的一项：名字 · 那一行的说明（核心写好）· 是不是目录 · 目录撞目录（不合并，覆盖灰）。 */
    data class Clash(
        val key: String,
        val name: String,
        val meta: String,
        val dir: Boolean,
        val dirInto: Boolean,
    )

    /** 收场了（结局 · 没成 · 取消）。 */
    val over: Boolean get() = phase == ENDED || phase == FAILED || phase == DROPPED

    companion object {
        const val RUNNING = "running"
        const val ASKING = "asking"
        const val CUT = "cut"
        const val ENDED = "ended"
        const val FAILED = "failed"
        const val DROPPED = "dropped"

        /** 核心交的那一行 ⇒ 一格；认不出 ⇒ `null`。 */
        fun of(m: Map<String, Any?>): BatchEvent? =
            BatchEvent(
                id = m.num("id") ?: return null,
                phase = m.str("phase") ?: return null,
                title = m.str("title") ?: return null,
                current = m.str("current"),
                frac = (m["frac"] as? Number)?.toFloat(),
                line = m.str("line"),
                said = m.str("said"),
                failed = m.strs("failed").orEmpty(),
                failedTitle = m.str("failed_title"),
                details = m.str("details"),
                retry = m.bool("retry") == true,
                undo = m.bool("undo") == true,
                conflicts =
                    m.objs("conflicts").orEmpty().map { c ->
                        Clash(
                            key = c.str("key") ?: return null,
                            name = c.str("name") ?: return null,
                            meta = c.str("meta").orEmpty(),
                            dir = c.bool("dir") == true,
                            dirInto = c.bool("dir_into") == true,
                        )
                    },
            )
    }
}

/** 同名那一问怎么答：整组一个答（`skip` · `keep_both` · `overwrite`）或逐项（键 ⇒ 答，没点的那几项核心照跳过）。 */
sealed interface BatchAnswer {
    data class All(
        val decide: String,
    ) : BatchAnswer

    data class Each(
        val picks: Map<String, String>,
    ) : BatchAnswer

    /** 交核心的那一形（`null` ＝ 取消）。 */
    companion object {
        fun wire(a: BatchAnswer?): Map<String, Any?>? =
            when (a) {
                is All -> mapOf("all" to a.decide)
                is Each -> mapOf("each" to a.picks)
                null -> null
            }
    }
}
