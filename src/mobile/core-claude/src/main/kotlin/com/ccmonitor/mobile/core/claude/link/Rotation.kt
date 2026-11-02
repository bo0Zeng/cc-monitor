package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.RotTarget
import com.ccmonitor.mobile.core.wire.gen.RotationForNewReq
import com.ccmonitor.mobile.core.wire.gen.RotationReloadReq
import com.ccmonitor.mobile.core.wire.gen.RotationStrategyAddReq
import com.ccmonitor.mobile.core.wire.gen.RotationStrategyCopyReq
import com.ccmonitor.mobile.core.wire.gen.RotationStrategyDeleteReq
import com.ccmonitor.mobile.core.wire.gen.RotationStrategyRenameReq
import com.ccmonitor.mobile.core.wire.gen.RotationStrategyTzReq
import com.ccmonitor.mobile.core.wire.gen.RotationUseNowReq
import com.ccmonitor.mobile.core.wire.gen.RotationUseReq
import com.ccmonitor.mobile.core.wire.gen.RotationViewReply
import com.ccmonitor.mobile.core.wire.gen.RotationViewReq

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
    fun wire(): RotTarget =
        when (this) {
            is Session -> RotTarget.Sid(sid)
            is Strategy -> RotTarget.Strategy(id)
        }
}

object Rotation {
    const val VIEW = Commands.RotationView.NAME
    const val STRATEGIES = Commands.RotationStrategies.NAME
    const val SETTINGS = Commands.RotationSettings.NAME
    const val USE = Commands.RotationUse.NAME
    const val RELOAD = Commands.RotationReload.NAME
    const val FOR_NEW = Commands.RotationForNew.NAME
    const val TEMP = Commands.RotationTemp.NAME
    const val USE_NOW = Commands.RotationUseNow.NAME
    const val ADD = Commands.RotationStrategyAdd.NAME
    const val COPY = Commands.RotationStrategyCopy.NAME
    const val RENAME = Commands.RotationStrategyRename.NAME
    const val DELETE = Commands.RotationStrategyDelete.NAME
    const val TZ = Commands.RotationStrategyTz.NAME

    /** 会话那一形不画推演 ⇒ 不带 [hours]；设置页那一形推 [hours] 小时。 */
    fun view(
        where: RotWhere,
        hours: Int,
    ): Map<String, Any?> = Commands.RotationView.args(RotationViewReq(target = where.wire(), hours = hours.takeIf { where is RotWhere.Strategy }))

    fun ofView(data: Any?): RotationViewReply? = Commands.RotationView.reply(data)

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
    ): Map<String, Any?> = Commands.RotationUse.args(RotationUseReq(sid = sid, strategy = strategy))

    /** 在用这份旧版的活会话都载入新版。 */
    fun reload(strategy: String): Map<String, Any?> = Commands.RotationReload.args(RotationReloadReq(strategy))

    fun forNew(strategy: String?): Map<String, Any?> = Commands.RotationForNew.args(RotationForNewReq(strategy))

    /** 会话顺序里把 [account] 那一行拖到第 [to] 格（第一格 ⇒ 现在用它；拖完那一串由那台认）。 */
    fun move(
        sid: String,
        account: String,
        to: Int,
    ): Map<String, Any?> = mapOf("sid" to sid, "op" to mapOf("move" to mapOf("account" to account, "to" to to)))

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
    ): Map<String, Any?> = Commands.RotationUseNow.args(RotationUseNowReq(sid = sid, account = account))

    /** 从文件添加（[source] ＝ 那份原文）；[source] 缺 ⇒ 新建（照编辑器那一份）。 */
    fun add(
        source: String?,
        name: String,
    ): Map<String, Any?> = Commands.RotationStrategyAdd.args(RotationStrategyAddReq(source = source, name = name))

    /** 加 · 复制 · 改名回的那一份的名字（三条回包同一形）。 */
    fun ofId(data: Any?): String? = Commands.RotationStrategyAdd.reply(data)?.id

    fun copy(
        id: String,
        name: String,
    ): Map<String, Any?> = Commands.RotationStrategyCopy.args(RotationStrategyCopyReq(id = id, name = name))

    fun rename(
        id: String,
        name: String,
        ifRev: String,
    ): Map<String, Any?> = Commands.RotationStrategyRename.args(RotationStrategyRenameReq(id = id, name = name, ifRev = ifRev))

    fun delete(id: String): Map<String, Any?> = Commands.RotationStrategyDelete.args(RotationStrategyDeleteReq(id))

    fun tz(
        id: String,
        tz: String,
        ifRev: String,
    ): Map<String, Any?> = Commands.RotationStrategyTz.args(RotationStrategyTzReq(id = id, tz = tz, ifRev = ifRev))
}

/** 改设置的一个意图（线上 `RotEdit`）：那台照声明改完、校验；`row` ＝ 号列表第几行（缺 ⇒ 总体那一格）。 */
object RotEdit {
    fun set(
        key: String,
        row: Int?,
        v: Any?,
    ): Map<String, Any?> = mapOf("set" to at(row, mapOf("key" to key, "v" to v)))

    /** 恢复默认：删掉这一格自己设的值（号那一行 ⇒ 回到跟总体；总体那一格 ⇒ 回到声明默认）。 */
    fun reset(
        key: String,
        row: Int?,
    ): Map<String, Any?> = mapOf("reset" to at(row, mapOf("key" to key)))

    /** 清掉号列表里这台账号库没有的号。 */
    fun prune(): Map<String, Any?> = mapOf("prune" to emptyMap<String, Any?>())

    /** 分段那一窗（[window]）里：[at] ＋ [seg] ⇒ 第 [at] 段换成它；只 [at] ⇒ 删第 [at] 段；只 [seg] ⇒ 加一段（[RotSegment]）。 */
    fun seg(
        key: String,
        row: Int?,
        window: String,
        at: Int?,
        seg: RotSegment?,
    ): Map<String, Any?> =
        mapOf(
            "seg" to
                at(
                    row,
                    buildMap {
                        put("key", key)
                        put("window", window)
                        at?.let { put("at", it) }
                        seg?.let { put("seg", mapOf("from" to it.from, "to" to it.to, "v" to it.v)) }
                    },
                ),
        )

    /** 分段后面那一窗：[same] ⇒ 同第一窗（删掉自己的）；否则另设（把此刻生效的写成自己的）。 */
    fun same(
        key: String,
        row: Int?,
        window: String,
        same: Boolean,
    ): Map<String, Any?> = mapOf("same" to at(row, mapOf("key" to key, "window" to window, "same" to same)))

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

/** 分段里的一段（线上 `RotSeg`）：`[from, to)` 是一段一行里打的钟点原样的字（认不认得由那台说）· 上限。 */
data class RotSegment(
    val from: String,
    val to: String,
    val v: Double,
)
