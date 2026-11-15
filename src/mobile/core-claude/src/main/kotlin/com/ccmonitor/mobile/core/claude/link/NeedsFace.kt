package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.NeedsMerged

/**
 * 几台的需手动问一圈、合成一份：逐台问 `sessions-needs` 与合（`face_core::needs::merge_needs`，本机后端 `sessions-needs-merge` 那一份合法）
 * 都在嵌的核心（`needs_all`），经交进来的那一口（生产上是 app 的 `CoreResident.needsAll`）。这里只交「问哪几台」、读回合好的那一份；
 * 答了没答上 · 没答上那一句（叫出那台）· 先后 · 总数 · 头那一句 · 数说得准说不准都在核心写。搜内容（[SearchMerge]）交台也是 [Host] 这一形。
 */
object NeedsFace {
    /** 交核心的一台：那条常驻流的编号（[ResidentFeed.handle]）· 哪台（hostId，合好的每一项照它认）。称呼由核心照那条流取。 */
    data class Host(
        val handle: Long,
        val origin: String,
    )

    /** 交核心的那一行（`{hosts: [{handle, origin}], withinMs}`；台序照给的）。 */
    fun input(
        hosts: List<Host>,
        withinMs: Long,
    ): String = Json.line(mapOf("hosts" to hosts.map(::host), "withinMs" to withinMs))

    /** 交核心的一台（`{handle, origin}`）。 */
    internal fun host(h: Host): Map<String, Any?> = mapOf("handle" to h.handle, "origin" to h.origin)

    /** 核心合好的那一份；读不动 ＝ 同一份构建里两头的形对不上（出形是生成的）⇒ 照实抛，不画一份假的。 */
    fun of(line: String): NeedsMerged = checkNotNull(Wire.read<NeedsMerged>(line)) { "核心交的需手动那一份认不出：${line.take(SNIPPET)}" }

    private const val SNIPPET = 200
}
