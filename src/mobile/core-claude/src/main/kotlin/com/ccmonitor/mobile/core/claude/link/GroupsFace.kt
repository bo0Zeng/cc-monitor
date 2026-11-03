package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.GroupsRead

/**
 * 勾上的那几台的组合成一份：`libmobcore.so` 里那一个口（`core-rs/rust/lib.rs`），合法是 `face_core::groups::merge`
 * （本机后端 `groups-read` 合各台那一份）。手机逐台问 `groups-here`、各台那一片原样交进来，这里只装运。
 */
object GroupsFace {
    init {
        System.loadLibrary("mobcore")
    }

    /**
     * [answered]：每台 `(hostId, 它 groups-here 回包原样)`，照机器表先后。核心读不动 ＝ 同一份构建里两头的形对不上
     * （入出两形都是生成的）⇒ 照实抛，不画一份假的。
     */
    fun of(answered: List<Pair<String, Any?>>): GroupsRead {
        val line = merge(Json.line(mapOf("answered" to answered.map { (origin, reply) -> mapOf("origin" to origin, "reply" to reply) })))
        return checkNotNull(Wire.read<GroupsRead>(line)) { "核心交的组那一份认不出：${line.take(SNIPPET)}" }
    }

    /** `{answered: [{origin, reply}]}`（JSON）⇒ `GroupsRead`（JSON）；读不动 ⇒ 空串。 */
    external fun merge(data: String): String

    private const val SNIPPET = 200
}
