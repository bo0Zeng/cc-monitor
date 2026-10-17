package com.ccmonitor.mobile.core.claude.link

/**
 * 不带记号那一问的 `files-ls` 回包（整份，带键集合那一形）：键照那台（末段名是 UTF-8 ⇒ 它本身，不是 ⇒ `\u0000` ＋ 十六进制），
 * `order` 是给的先后，`head.info` 是那一问的头。
 */
internal fun lsWhole(
    entries: List<Map<String, Any?>>,
    info: Map<String, Any?>,
): Map<String, Any?> {
    val keys =
        entries.mapIndexed { i, e ->
            when (val n = e["name"]) {
                is String -> n
                is Map<*, *> -> "\u0000" + n["b16"]
                else -> "#$i"
            }
        }
    return mapOf(
        "held" to "h1",
        "whole" to true,
        "same" to false,
        "order" to keys,
        "up" to keys.zip(entries).toMap(),
        "head" to mapOf("total" to entries.size.toDouble(), "info" to info),
    )
}
