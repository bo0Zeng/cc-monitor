package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.PrefsReply
import com.ccmonitor.mobile.core.claude.link.wire.QuotaReadReply
import com.ccmonitor.mobile.core.claude.link.wire.TabPref
import com.ccmonitor.mobile.core.claude.link.wire.Usage
import com.ccmonitor.mobile.core.claude.link.wire.Wire

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
    const val READ: String = "prefs-read"
    const val SET: String = "prefs-set"

    fun of(data: Any?): PrefsReply? = Wire.bind<PrefsReply>(data)

    fun tab(
        kind: TabKind,
        value: TabPref,
    ): Map<String, Any?> = mapOf(kind.wire to if (value == TabPref.SHOW) "show" else "hide")

    /** 一个号的默认模型；[model] 为 `null` ⇒ 删掉那一条（跟随那一家的默认）。 */
    fun model(
        account: String,
        model: String?,
    ): Map<String, Any?> = mapOf("models" to mapOf(account to model))
}

/** `quota-read`：每号用量那一格（号名 ⇒ 那一格；出过数的与没出过数的合一张）。 */
object QuotaRead {
    const val COMMAND: String = "quota-read"

    fun usage(data: Any?): Map<String, Usage>? =
        Wire.bind<QuotaReadReply>(data)?.let { q ->
            q.unseen.associate { it.account to Usage(it.usage.text, it.usage.tone) } + q.accounts.associate { it.account to it.usage }
        }
}
