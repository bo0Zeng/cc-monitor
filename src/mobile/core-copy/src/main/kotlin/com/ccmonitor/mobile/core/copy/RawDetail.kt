package com.ccmonitor.mobile.core.copy

/**
 * 「表里的一句 ＋ 原话」：失败时上屏的那句从文案表来，下层（SSH 库、远端）抛出来的原话不当句子，
 * 另起一行放在后面，给会看的人对照（同桌面失败应答的「句子 · 复制详情」两格）。原话空 ⇒ 只有那一句。
 */
fun withRaw(
    sentence: String,
    raw: String?,
): String = if (raw.isNullOrBlank()) sentence else "$sentence\n$raw"
