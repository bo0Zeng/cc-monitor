package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.PrefsReply
import com.ccmonitor.mobile.core.wire.gen.PrefsSet
import com.ccmonitor.mobile.core.wire.gen.Tab
import com.ccmonitor.mobile.core.wire.gen.QuotaRead as QuotaReadReply

/*
 * 设置屏问那台的几条（`IPC-COMMANDS.md` 各节，桌面「这台上的 cc-monitor」读写同一份）：手机只拼入参、解成品。
 */

/** 那一种会话进不进标签栏（`prefs-set` 里的 `childTab` · `bgTab`）。 */
enum class TabKind(
    val wire: String,
) {
    CHILD("childTab"),
    BG("bgTab"),
}

/** `prefs-read` · `prefs-set`：这台的偏好；改只交点到的那一格，那台写完读回。 */
object MachinePrefs {
    const val READ: String = Commands.PrefsRead.NAME
    const val SET: String = Commands.PrefsSet.NAME

    fun of(data: Any?): PrefsReply? = Commands.PrefsRead.reply(data)

    fun tab(
        kind: TabKind,
        value: Tab,
    ): Map<String, Any?> =
        Commands.PrefsSet.args(
            when (kind) {
                TabKind.CHILD -> PrefsSet(childTab = value)
                TabKind.BG -> PrefsSet(bgTab = value)
            },
        )

    /** 一个号的默认模型；[model] 为 `null` ⇒ 删掉那一条（跟随那一家的默认）。 */
    fun model(
        account: String,
        model: String?,
    ): Map<String, Any?> = Commands.PrefsSet.args(PrefsSet(models = mapOf(account to model)))
}

/** `quota-read`：这台的额度账（设置屏账号卡 · 账号一览 · 用量屏同读这一份）。 */
object QuotaRead {
    const val COMMAND: String = Commands.QuotaRead.NAME

    fun of(data: Any?): QuotaReadReply? = Commands.QuotaRead.reply(data)
}
