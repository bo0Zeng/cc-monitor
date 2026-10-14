package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.RotSeg
import com.ccmonitor.mobile.core.claude.link.wire.RotationDayPaintReply
import com.ccmonitor.mobile.core.claude.link.wire.RotationViewReply
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/*
 * 轮换（第五版）那几条命令（`IPC-COMMANDS.md` 各节，桌面 `rot-mount.ts` 同一套）：手机只拼入参、解成品。
 * 判（能不能用 · 跳不跳 · 先后 · 停到几点）· 算时刻 · 写句子全在那台；这里一条判定都没有。
 */

/** 看的是哪一处：一个会话（改动先成它的草稿，草稿在那台）· 一份策略（设置页：草稿在手上，随读交给那台）。 */
sealed interface RotWhere {
    data class Session(
        val sid: String,
    ) : RotWhere

    data class Strategy(
        val id: String,
    ) : RotWhere

    /** 线上 `RotWhere`（写那几条的 `target`）。 */
    fun wire(): Map<String, Any?> =
        when (this) {
            is Session -> mapOf("sid" to sid)
            is Strategy -> mapOf("strategy" to id)
        }
}

object Rotation {
    const val VIEW = "rotation-view"
    const val STRATEGIES = "rotation-strategies"
    const val SETTINGS = "rotation-settings"
    const val SAVE = "rotation-save"
    const val REVERT = "rotation-revert"
    const val USE = "rotation-use"
    const val FOR_NEW = "rotation-for-new"
    const val TEMP = "rotation-temp"
    const val USE_NOW = "rotation-use-now"
    const val ADD = "rotation-strategy-add"
    const val COPY = "rotation-strategy-copy"
    const val RENAME = "rotation-strategy-rename"
    const val DELETE = "rotation-strategy-delete"
    const val TZ = "rotation-strategy-tz"
    const val DAY_PAINT = "rotation-day-paint"

    /** 推送主题：这个会话的轮换（`key` ＝ sid）· 这台的策略目录。 */
    const val TOPIC = "rotation"
    const val TOPIC_STRATEGIES = "rotation_strategies"

    /**
     * `rotation-view` 的目标：会话 ⇒ `{sid}`；策略 ⇒ `{strategy}`，手上有草稿 ⇒ 连同 `settings` 与它照哪个版本改的（`rev`）一起交
     * （跟文件此刻的不同 ⇒ 那台出「别处被改了」那一条提示）。
     */
    fun view(
        where: RotWhere,
        hours: Int,
        draft: Any? = null,
        draftRev: String? = null,
    ): Map<String, Any?> {
        val target =
            when (where) {
                is RotWhere.Session -> where.wire()
                is RotWhere.Strategy ->
                    buildMap {
                        put("strategy", where.id)
                        if (draft != null) {
                            put("settings", draft)
                            draftRev?.let { put("rev", it) }
                        }
                    }
            }
        return mapOf("target" to target, "hours" to hours)
    }

    fun ofView(data: Any?): RotationViewReply? = Wire.bind<RotationViewReply>(data)

    fun settings(
        where: RotWhere,
        settings: Any?,
        ifRev: String,
    ): Map<String, Any?> = mapOf("target" to where.wire(), "settings" to settings, "ifRev" to ifRev)

    /** 存：会话 ⇒ 存它的草稿（不带 `settings`）；策略 ⇒ 手上那份草稿。 */
    fun save(
        where: RotWhere,
        ifRev: String,
        draft: Any? = null,
    ): Map<String, Any?> =
        buildMap {
            put("target", where.wire())
            put("ifRev", ifRev)
            if (where is RotWhere.Strategy) put("settings", draft)
        }

    fun revert(sid: String): Map<String, Any?> = mapOf("sid" to sid)

    /** 会话换一份策略（`null` ＝ 不轮换）；[discard] ＝ 有没存的改动也换。 */
    fun use(
        sid: String,
        strategy: String?,
        discard: Boolean,
    ): Map<String, Any?> = mapOf("sid" to sid, "strategy" to strategy, "discard" to discard)

    fun forNew(strategy: String?): Map<String, Any?> = mapOf("strategy" to strategy)

    /** 临时条目：把 [account] 排到第 [at] 段（界面把松手的横坐标换成第几段，只是几何）。 */
    fun place(
        sid: String,
        account: String,
        at: Int,
    ): Map<String, Any?> = mapOf("sid" to sid, "op" to mapOf("place" to mapOf("account" to account, "at" to at)))

    fun clearTemp(sid: String): Map<String, Any?> = mapOf("sid" to sid, "op" to "clear")

    fun useNow(
        sid: String,
        account: String,
    ): Map<String, Any?> = mapOf("sid" to sid, "account" to account)

    fun add(
        source: String,
        name: String,
    ): Map<String, Any?> = mapOf("source" to source, "name" to name)

    fun copy(
        id: String,
        name: String,
    ): Map<String, Any?> = mapOf("id" to id, "name" to name)

    fun rename(
        id: String,
        name: String,
        ifRev: String,
    ): Map<String, Any?> = mapOf("id" to id, "name" to name, "ifRev" to ifRev)

    fun delete(id: String): Map<String, Any?> = mapOf("id" to id)

    fun tz(
        id: String,
        tz: String,
        ifRev: String,
    ): Map<String, Any?> = mapOf("id" to id, "tz" to tz, "ifRev" to ifRev)

    /** 24 小时条上刷一段（一天里的分钟；`to ≤ from` 跨午夜、相等 ＝ 一整天）；[v] 为 `null` ＝ 删这一截。 */
    fun dayPaint(
        segs: List<RotSeg>,
        from: Long,
        to: Long,
        v: Double?,
    ): Map<String, Any?> =
        mapOf(
            "segs" to segs.map { mapOf("from" to it.from, "to" to it.to, "v" to it.v) },
            "from" to from,
            "to" to to,
            "v" to v,
        )

    fun ofDayPaint(data: Any?): List<RotSeg>? = Wire.bind<RotationDayPaintReply>(data)?.segs

    /** 策略写的字（`StrategyText`：一句，或 `{zh, en?}`）。 */
    fun text(t: Any?): String =
        when (t) {
            null -> ""
            is String -> t
            is Map<*, *> -> t["zh"] as? String ?: ""
            else -> ""
        }
}

/** 流上一帧 `changed`：哪一样变了（`topic`）· 哪一个（`key`）。 */
data class Changed(
    val topic: String,
    val key: String?,
) {
    companion object {
        const val KIND = "changed"

        fun of(frame: Map<String, Any?>): Changed? = if (frame.str("kind") == KIND) frame.str("topic")?.let { Changed(it, frame.str("key")) } else null
    }
}
