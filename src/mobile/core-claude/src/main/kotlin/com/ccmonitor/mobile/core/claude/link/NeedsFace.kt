package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.NeedsMerged

/**
 * 几台的需手动合成一份：`libmobcore.so` 里那一个口（`core-rs/rust/lib.rs`），合法是 `face_core::needs::merge_needs`
 * （本机后端 `sessions-needs-merge` 合各台那一份）。手机逐台问 `sessions-needs`，答了的 · 没答上的 · 还在读的那几台都原样交进来；
 * 先后 · 总数 · 头那一句 · 读屏那一句 · 数说得准说不准 · 没答上那几台那一句（叫出那台）都在核心写，这里只装运。
 */
object NeedsFace {
    init {
        System.loadLibrary("mobcore")
    }

    /** 交核心的一台：哪台（hostId）· 给人看的称呼 · 那一格（答了 ⇒ 回包原样；没答上 ⇒ [missOf] 那一形）。 */
    data class Asked(
        val origin: String,
        val label: String,
        val reply: Any?,
    )

    /**
     * [answered] · [unanswered]：台序照给的（当前那台在前、别的照机器表先后）；[reading]：还在读的那几台（hostId）。
     * 核心读不动 ＝ 同一份构建里两头的形对不上（出形是生成的）⇒ 照实抛，不画一份假的。
     */
    fun of(
        answered: List<Asked>,
        unanswered: List<Asked>,
        reading: List<String>,
    ): NeedsMerged {
        val one = { a: Asked -> mapOf("origin" to a.origin, "label" to a.label, "reply" to a.reply) }
        val line = merge(Json.line(mapOf("answered" to answered.map(one), "unanswered" to unanswered.map(one), "reading" to reading)))
        return checkNotNull(Wire.read<NeedsMerged>(line)) { "核心交的需手动那一份认不出：${line.take(SNIPPET)}" }
    }

    /**
     * 那一问没答上的结局 ⇒ 交核心的那一形：断着（[down]，那一句是链上那台写好的离线那一句 [offline]）· 期限到 · 撤了 ⇒ 连接层那一句原样；
     * 那台答了一个错 ⇒ 码 · 那一句 · 详情原样；回包解不开 ⇒ 原文。哪一句叫不叫出那台由核心照码定。
     */
    fun missOf(
        r: Reply?,
        offline: String,
        down: Boolean,
    ): Map<String, Any?> =
        if (down) {
            link(offline, "", true)
        } else {
            when (r) {
                is Reply.Failed -> mapOf("kind" to "failed", "code" to r.failure.code, "said" to r.failure.said, "detail" to r.failure.detail)
                is Reply.TimedOut -> link(r.said, r.detail, false)
                is Reply.Cancelled -> link(r.said, r.detail, false)
                is Reply.Unreadable -> mapOf("kind" to "unreadable", "raw" to r.raw)
                Reply.LinkDown, is Reply.Ok, null -> link(offline, "", false)
            }
        }

    private fun link(
        said: String,
        detail: String,
        down: Boolean,
    ) = mapOf("kind" to "link", "said" to said, "detail" to detail, "down" to down)

    /** `{answered, unanswered, reading}`（JSON）⇒ `NeedsMerged`（JSON）；读不动 ⇒ 空串。 */
    external fun merge(data: String): String

    private const val SNIPPET = 200
}
