package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.ChangedFrame
import com.ccmonitor.mobile.core.claude.link.wire.RotationViewReply
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/*
 * 轮换（第五版）那几条命令（`IPC-COMMANDS.md` 各节，桌面 `rot-mount.ts` 同一套）：手机只拼入参、解成品。
 * 判（能不能用 · 跳不跳 · 先后 · 停到几点）· 算时刻 · 写句子全在那台；这里一条判定都没有。
 */

/** 看的是哪一处：一个会话（照它载入的副本）· 一份策略（设置页：照文件此刻的）。 */
sealed interface RotWhere {
    data class Session(
        val sid: String,
    ) : RotWhere

    data class Strategy(
        val id: String,
    ) : RotWhere

    /** 线上 `RotTarget`（`rotation-view` 的 `target`）。 */
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
    const val USE = "rotation-use"
    const val RELOAD = "rotation-reload"
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

    fun view(
        where: RotWhere,
        hours: Int,
    ): Map<String, Any?> = mapOf("target" to where.wire(), "hours" to hours)

    fun ofView(data: Any?): RotationViewReply? = Wire.bind<RotationViewReply>(data)

    /** 设置页改一格：一个意图（[RotEdit]）＋ 读到的那一版；那台当场存成新版。 */
    fun settings(
        strategy: String,
        edit: Any,
        ifRev: String,
    ): Map<String, Any?> = mapOf("strategy" to strategy, "edit" to edit, "ifRev" to ifRev)

    /** 会话载入一份此刻的版本（`null` ＝ 不轮换；选它在用的那份 ＝ 载入新版）。 */
    fun use(
        sid: String,
        strategy: String?,
    ): Map<String, Any?> = mapOf("sid" to sid, "strategy" to strategy)

    /** 在用这份旧版的活会话都载入新版。 */
    fun reload(strategy: String): Map<String, Any?> = mapOf("strategy" to strategy)

    fun forNew(strategy: String?): Map<String, Any?> = mapOf("strategy" to strategy)

    /** 临时条目：把 [account] 排到第 [at] 段（界面把松手的横坐标换成第几段，只是几何）。 */
    fun place(
        sid: String,
        account: String,
        at: Int,
    ): Map<String, Any?> = mapOf("sid" to sid, "op" to mapOf("place" to mapOf("account" to account, "at" to at)))

    fun clearTemp(sid: String): Map<String, Any?> = mapOf("sid" to sid, "op" to "clear")

    /** 这个会话先不用（[off]）/ 再用 [account]。 */
    fun off(
        sid: String,
        account: String,
        off: Boolean,
    ): Map<String, Any?> = mapOf("sid" to sid, "op" to mapOf("off" to mapOf("account" to account, "off" to off)))

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

        fun of(frame: Map<String, Any?>): Changed? = if (frame.str("kind") == KIND) Wire.bind<ChangedFrame>(frame)?.let { Changed(it.topic, it.key) } else null
    }
}
