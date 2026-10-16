package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.RotationSettingsReply
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

    /** 改设置交一个意图（[RotEdit]）：会话 ⇒ 带 `ifRev`；设置页 ⇒ 带手上那份草稿（上一次回的，没有 ⇒ 不带）。 */
    fun settings(
        where: RotWhere,
        edit: Any,
        ifRev: String? = null,
        draft: Any? = null,
    ): Map<String, Any?> =
        buildMap {
            put("target", where.wire())
            put("edit", edit)
            ifRev?.let { put("ifRev", it) }
            draft?.let { put("settings", it) }
        }

    /** 改完的那一份（设置页当新的草稿存着）。 */
    fun ofSettings(data: Any?): RotationSettingsReply? = Wire.bind<RotationSettingsReply>(data)

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

}

/** 改设置的一个意图（线上 `RotEdit`）：那台照声明改完、校验；`row` ＝ 号列表第几行（缺 ⇒ 总体那一格）。 */
object RotEdit {
    const val KEEP = "keep"

    fun set(
        key: String,
        row: Int?,
        v: Any?,
    ): Map<String, Any?> = mapOf("set" to at(row, mapOf("key" to key, "v" to v)))

    /** 24 小时条那一窗上把 `[from, to)` 刷成 [v]（`null` ＝ 删这一截；一天里的分钟，`to ≤ from` 跨午夜、相等 ＝ 一整天）。 */
    fun day(
        key: String,
        row: Int?,
        window: String,
        from: Long,
        to: Long,
        v: Double?,
    ): Map<String, Any?> = mapOf("day" to at(row, mapOf("key" to key, "window" to window, "from" to from, "to" to to, "v" to v)))

    fun move(
        from: Int,
        to: Int,
    ): Map<String, Any?> = mapOf("move" to mapOf("from" to from, "to" to to))

    fun add(account: String): Map<String, Any?> = mapOf("add" to mapOf("account" to account))

    fun remove(row: Int): Map<String, Any?> = mapOf("remove" to mapOf("row" to row))

    private fun at(
        row: Int?,
        m: Map<String, Any?>,
    ): Map<String, Any?> = if (row == null) m else m + ("row" to row)
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
