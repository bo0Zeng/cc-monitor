package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.ProfilesBook
import com.ccmonitor.mobile.core.claude.link.wire.ProfilesWriteDone
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/**
 * 那台的别名（配置文件里的各段）：读 · 新建一段，与桌面别名页同两条命令。手机只交意图；
 * 一段能不能用、合下来是什么、名字合不合规都是那台判的。
 */
object Profiles {
    const val READ: String = "profiles-read"
    const val WRITE: String = "profiles-write"

    /** 读回的那一份里手机用的两样：盘上那份的指纹（写的时候交回）· 能用的那几段的名字（按那台的顺序）。 */
    data class Book(
        val fingerprint: String?,
        val usable: List<String>,
    )

    fun of(data: Any?): Book? = Wire.bind<ProfilesBook>(data)?.let { b -> Book(b.fingerprint, b.profiles.filter { it.usable }.map { it.name }) }

    /** 在终端里起名叫 [name] 的那一段：`ccm @名`（与终端里自己敲的同一行）。 */
    fun startLine(name: String): String = "ccm @$name"

    /** 新建一段（[form] 是别名页表单那一形）；[fingerprint] 是刚读回的那一份的，盘上被别处改过 ⇒ 那台回 `stale`。 */
    fun createArgs(
        fingerprint: String?,
        form: Map<String, Any?>,
    ): Map<String, Any?> = mapOf("changes" to listOf(mapOf("op" to "set", "was" to null, "form" to form)), "fingerprint" to fingerprint)

    /** 写完：终端函数那份真改了 ⇒ 「已开的终端要重读」那一句（那台写好的）；否则 `null`。形状对不上 ⇒ 解不出。 */
    fun written(data: Any?): Written? = Wire.bind<ProfilesWriteDone>(data)?.let { Written(it.reload) }

    data class Written(
        val reload: String?,
    )
}
