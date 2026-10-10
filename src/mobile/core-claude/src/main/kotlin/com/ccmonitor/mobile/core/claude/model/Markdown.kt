package com.ccmonitor.mobile.core.claude.model

/**
 * 从 markdown 提取 fenced ``` 代码块内容（按出现顺序，去掉围栏行与语言标记、尾随换行）。
 * 仅匹配成对闭合的围栏；未闭合的代码块不提取（不完整，避免复制半截）。供阅读面「复制代码块」用。
 */
private val FENCED_CODE = Regex("```[^\\n]*\\n([\\s\\S]*?)```")

fun extractCodeBlocks(markdown: String): List<String> =
    FENCED_CODE.findAll(markdown).map { it.groupValues[1].trimEnd('\n') }.toList()
