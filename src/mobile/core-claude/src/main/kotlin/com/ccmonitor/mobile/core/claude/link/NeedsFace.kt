package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.NeedsMerged

/**
 * 几台的需手动合成一份：`libmobcore.so` 里那一个口（`core-rs/rust/lib.rs`），合法是 `face_core::needs::merge_needs`
 * （本机后端 `sessions-needs-merge` 合各台那一份）。手机逐台问 `sessions-needs`、各台那一份原样交进来，先后与总数都在核心算，这里只装运。
 */
object NeedsFace {
    init {
        System.loadLibrary("mobcore")
    }

    /**
     * [answered]：每台 `(hostId, 它 sessions-needs 回包原样)`，当前那台在前、别的照机器表先后。核心读不动 ＝ 同一份构建里两头的形对不上
     * （入出两形都是生成的）⇒ 照实抛，不画一份假的。
     */
    fun of(answered: List<Pair<String, Any?>>): NeedsMerged {
        val line = merge(Json.line(mapOf("answered" to answered.map { (origin, reply) -> mapOf("origin" to origin, "reply" to reply) })))
        return checkNotNull(Wire.read<NeedsMerged>(line)) { "核心交的需手动那一份认不出：${line.take(SNIPPET)}" }
    }

    /** `{answered: [{origin, reply}]}`（JSON）⇒ `NeedsMerged`（JSON）；读不动 ⇒ 空串。 */
    external fun merge(data: String): String

    private const val SNIPPET = 200
}
