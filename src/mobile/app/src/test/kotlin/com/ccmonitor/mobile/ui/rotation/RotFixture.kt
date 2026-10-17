package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Reply

/** 合成的轮换回包（号 · 策略名 · 时刻都是编的），形状照 `RotationViewReply` · `RotationStrategiesReply`。 */
internal object RotFixture {
    const val NOW = 1_000_000L

    fun strategies(forNew: String = "spread"): Map<String, Any?> =
        mapOf(
            "strategies" to
                listOf(
                    strategy("spread", inUse = listOf("s1", "s2"), forNew = forNew == "spread"),
                    strategy("night", forNew = forNew == "night"),
                ),
            "forNew" to forNew,
            "oldScripts" to 0.0,
        )

    fun strategy(
        id: String,
        inUse: List<String> = emptyList(),
        forNew: Boolean = false,
        rev: String = "h:$id",
    ): Map<String, Any?> =
        mapOf(
            "id" to id,
            "description" to "$id 第一行\n\n第二段",
            "summary" to "$id 第一行",
            "rev" to rev,
            "inUse" to inUse,
            "forNew" to forNew,
            "wordsBad" to false,
            "tzUnknown" to false,
        )

    private fun win(
        used: Double,
        resets: String? = null,
    ) = buildMap<String, Any?> {
        put("open", true)
        put("used", used)
        resets?.let { put("resetsText", it) }
    }

    fun fact(
        account: String,
        u5: Double,
        u7: Double,
        resets: String? = null,
        blocked: String? = null,
    ): Map<String, Any?> =
        buildMap {
            put("account", account)
            put("kind", "sub")
            put("w5h", win(u5, resets))
            put("w7d", win(u7))
            put("login", "ok")
            put("plate", mapOf("color" to 2.0, "label" to if (account == "work") "Work" else account, "text" to account.take(2)))
            blocked?.let { put("blocked", mapOf("why" to "used", "text" to it)) }
        }

    private fun daySegs(v: Double) = listOf(mapOf("from" to 0.0, "to" to 1440.0, "v" to v, "text" to "夹具·$v", "tip" to "夹具·全天 $v"))

    private fun cap(
        w: String,
        from: String,
    ) = mapOf("key" to "cap", "kind" to "daySegments", "window" to w, "label" to "用量上限 · $w", "v" to null, "from" to from, "segs" to if (w == "5h") daySegs(90.0) else emptyList())

    private fun delay(
        v: Double,
        from: String,
    ) = mapOf("key" to "delay", "kind" to "minutes", "label" to "启用延迟", "help" to "到点后再等这么久", "v" to v, "num" to v, "from" to from, "min" to 0.0, "max" to 600.0)

    private fun row(
        account: String,
        line: String,
        cells: List<Map<String, Any?>>,
    ) = mapOf("account" to account, "line" to line, "hover" to "夹具·$account 悬停", "cells" to cells, "rules" to listOf("夹具·$account 的规则"))

    /** 后端解好的那几格（合成的）：总体两格 ＋ 每号一行；work 跟总体、team 自己设了启用延迟。 */
    val BOARD =
        mapOf(
            "all" to
                listOf(
                    cap("5h", "own"),
                    cap("7d", "default"),
                    mapOf("key" to "tol", "kind" to "percent", "label" to "权重容差", "v" to 20.0, "num" to 20.0, "from" to "own", "nullable" to "不强制切换"),
                ),
            "rows" to
                listOf(
                    row("work", "夹具·work 那一行", listOf(cap("5h", "all"), cap("7d", "all"), delay(0.0, "all"))),
                    row("team", "夹具·team 那一行", listOf(cap("5h", "all"), cap("7d", "all"), delay(30.0, "own"))),
                    row("lab", "夹具·lab 那一行", listOf(cap("5h", "all"), cap("7d", "all"), delay(0.0, "all"))),
                ),
        )

    fun view(
        strategy: String? = "spread",
        hints: List<Map<String, Any?>> = emptyList(),
        temp: List<String> = emptyList(),
        off: List<String> = emptyList(),
    ): Map<String, Any?> =
        mapOf(
            "now" to NOW.toDouble(),
            "nowText" to "14:30",
            "strategy" to strategy?.let { strategy(it) },
            "board" to if (strategy == null) mapOf("all" to emptyList<Any>(), "rows" to emptyList<Any>()) else BOARD,
            "start" to "work",
            "current" to "team",
            "lineup" to listOf("team", "work", "api", "lab"),
            "off" to off,
            "accounts" to listOf(fact("work", 80.4, 10.0, "16:07"), fact("team", 58.0, 30.0, "18:30"), fact("lab", 99.0, 20.0, blocked = "5 小时用量已到上限"), fact("api", 0.0, 0.0)),
            "listed" to 3.0,
            "decision" to if (strategy == null) null else mapOf("order" to emptyList<String>(), "why" to listOf(mapOf("account" to "work", "why" to "夹具·work 的理由")), "fromTemp" to false, "skipped" to emptyList<String>(), "temp" to temp),
            "timeline" to
                if (strategy == null) {
                    null
                } else {
                    mapOf(
                        "switches" to listOf(mapOf("at" to (NOW + 5_000).toDouble(), "atText" to "16:07", "account" to "work")),
                        "uses" to
                            listOf(
                                mapOf("from" to NOW.toDouble(), "to" to (NOW + 5_000).toDouble(), "account" to "team", "fromTemp" to false, "capUnknown" to false, "skipped" to emptyList<String>()),
                                mapOf("from" to (NOW + 5_000).toDouble(), "to" to (NOW + 20_000).toDouble(), "account" to "work", "fromTemp" to false, "capUnknown" to false, "skipped" to listOf("lab")),
                                mapOf("from" to (NOW + 20_000).toDouble(), "to" to (NOW + 43_200).toDouble(), "account" to "lab", "fromTemp" to false, "capUnknown" to false, "skipped" to emptyList<String>()),
                            ),
                        "lanes" to
                            listOf(
                                mapOf(
                                    "account" to "team",
                                    "segs" to
                                        listOf(
                                            mapOf("from" to NOW.toDouble(), "to" to (NOW + 9_000).toDouble(), "sendable" to true, "used" to listOf(58.0, 30.0), "reset" to false, "text" to "夹具·剩 42", "left" to 42.4),
                                            mapOf("from" to (NOW + 9_000).toDouble(), "to" to (NOW + 43_200).toDouble(), "sendable" to true, "used" to listOf(0.0, 30.0), "reset" to true, "text" to "夹具·↻18:30", "left" to 70.0),
                                        ),
                                ),
                            ),
                        "truncated" to false,
                        "ticks" to listOf(mapOf("at" to (NOW + 12_600).toDouble(), "text" to "18:00")),
                    )
                },
            "past" to null,
            "log" to emptyList<Any>(),
            "hints" to hints,
        )

    fun hold(untilIn: Long) = mapOf("hint" to mapOf("k" to "hold", "until" to (NOW + untilIn).toDouble(), "untilText" to "18:30"), "tone" to "warn", "text" to "已停发 · 18:30 team 恢复（还有 {dur}）")

    val TZ = mapOf("hint" to mapOf("k" to "tzUnknown", "zone" to "America/Los_Angeles"), "tone" to "need", "text" to "时区不明 · 分时段上限暂按 UTC 算")

    val ERROR = mapOf("hint" to mapOf("k" to "strategyError", "error" to mapOf("line" to 12.0, "msg" to "除数为 0"), "path" to "/h/strategies/spread.rhai"), "tone" to "fail", "text" to "策略「spread」出错：第 12 行，除数为 0", "detail" to "原话")

    fun failed(
        code: String,
        message: String,
    ) = Reply.Failed(CoreFailure(code, message, "细节"))
}
