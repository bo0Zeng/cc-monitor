package com.ccmonitor.mobile.core.copy

/**
 * 「表里的一句 ＋ 原话」拼成一段：给连接日志（本来就是给人看原文的那一栏）和复制详情 / 异常说明用。
 * 不当上屏的句子用 —— 失败时屏上只有表里那一句，原话进复制详情（app 的 `CopyDetailButton` · core-ui 的 `Notice`），
 * 同桌面失败应答的「句子 · 复制详情」。原话空 ⇒ 只有那一句。
 */
fun withRaw(
    sentence: String,
    raw: String?,
): String = if (raw.isNullOrBlank()) sentence else "$sentence\n$raw"
