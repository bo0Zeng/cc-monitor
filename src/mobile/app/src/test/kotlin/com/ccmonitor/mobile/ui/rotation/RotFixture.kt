package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Reply

/** 合成的轮换回包（号 · 策略名 · 时刻都是编的），形状照 `RotationViewReply` · `RotationStrategiesReply`。 */
internal object RotFixture {
    const val NOW = 1_000_000L

    fun strategies(
        forNew: String = "spread",
        extra: List<String> = emptyList(),
    ): Map<String, Any?> =
        mapOf(
            "strategies" to
                listOf(
                    strategy("spread", inUse = listOf("s1", "s2"), forNew = forNew == "spread"),
                    strategy("night", forNew = forNew == "night", hand = true),
                ) + extra.map { strategy(it) },
            "forNew" to forNew,
            "oldScripts" to 0.0,
        )

    fun strategy(
        id: String,
        inUse: List<String> = emptyList(),
        forNew: Boolean = false,
        rev: String = "h:$id",
        hand: Boolean = false,
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
            "hand" to hand,
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

    /** 分段一窗：5h 自己的一段；7d 同第一窗（带「同 5 小时」那颗钮的字）。 */
    private fun caps(w: String) =
        buildMap<String, Any?> {
            put("key", "caps")
            put("kind", "daySegments")
            put("window", w)
            put("label", "夹具·上限 · $w")
            put("v", null)
            put("from", if (w == "5h") "own" else "same")
            put("min", 0.0)
            put("max", 100.0)
            put("segs", daySegs(90.0))
            if (w == "7d") put("same", "夹具·同 5h")
        }

    private fun choice(
        key: String,
        v: String,
        vararg choices: String,
    ) = mapOf("key" to key, "kind" to "choice", "label" to "夹具·$key", "help" to "夹具·$key 的说明", "v" to v, "from" to "own", "choices" to choices.map { mapOf("v" to it, "label" to "夹具·$it") })

    private fun wait(v: Double) = mapOf("key" to "wait", "kind" to "minutes", "label" to "夹具·延迟", "v" to v, "num" to v, "from" to "own", "min" to 0.0, "max" to 300.0, "under" to "in")

    private val outTotal = mapOf("key" to "outTotal", "kind" to "percent", "label" to "夹具·切出条件", "lead" to "夹具·合计达到", "v" to null, "num" to 400.0, "from" to "default", "nullable" to "夹具·一有额度", "under" to "out")

    private fun row(
        account: String,
        line: String,
        cells: List<Map<String, Any?>>,
        marks: List<Map<String, Any?>> = emptyList(),
    ) = mapOf("account" to account, "line" to line, "hover" to "夹具·$account 悬停", "cells" to cells, "rules" to listOf("夹具·$account 的规则"), "marks" to marks)

    /**
     * 后端解好的那几格（合成的，格名照编辑器那份声明）：总体一个开关 ＋ 每号一行。只回此刻该出的格：
     * work 默认切入 · 默认切出（没有挂着的格）；team 延迟切入（带「延迟」）；lab 强制切入 · 强制切出（带「切出条件」）。
     */
    val BOARD =
        mapOf(
            "all" to listOf(mapOf("key" to "smart", "kind" to "toggle", "label" to "夹具·智能切换", "help" to "夹具·智能切换的说明", "v" to true, "from" to "own")),
            "rows" to
                listOf(
                    row("work", "夹具·work 那一行", listOf(caps("5h"), caps("7d"), choice("in", "default", "default", "force", "delay"), choice("out", "default", "default", "force"))),
                    row(
                        "team",
                        "夹具·team 那一行",
                        listOf(caps("5h"), caps("7d"), choice("in", "delay", "default", "force", "delay"), wait(30.0), choice("out", "default", "default", "force")),
                        listOf(mapOf("dashed" to true, "tip" to "夹具·延迟切入")),
                    ),
                    row(
                        "lab",
                        "夹具·lab 那一行",
                        listOf(caps("5h"), caps("7d"), choice("in", "force", "default", "force", "delay"), choice("out", "force", "default", "force"), outTotal),
                        listOf(mapOf("glyph" to "↺", "dashed" to false, "tip" to "夹具·强制切入"), mapOf("glyph" to "⇥", "dashed" to false, "tip" to "夹具·强制切出")),
                    ),
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
