package com.ccmonitor.mobile.testing

import java.io.File

/**
 * 读源码字面量的词表判据用：手机界面上的字收进了文案表（`src/shared/copy/table.json`），源码里 `Text(copyText("键"))`
 * 只剩一个键。判据扫到的字面量是表里的键 ⇒ 换成表里那句 `zh` 再比词表；不是键 ⇒ 原样（日志 · 命令 · 内部诊断）。
 *
 * 只按表的排版读（`"键": {` 之后依次 `kind` · `role` · `zh`，排版由 prettier 定、`copy-table.vitest.ts` 判形状），不另带 JSON 库：
 * 读不出一条 ⇒ [zhOf] 全部落空、词表判据静默空转 —— 所以 [entries] 读到的条数由 `CopyTableForTestsTest` 钉下限。
 */
object CopyTableForTests {
    private val ENTRY =
        Regex(""""([A-Za-z][A-Za-z0-9]*(?:\.[A-Za-z][A-Za-z0-9]*){2})": \{\s*"kind": "[^"]*",\s*"role": "[^"]*",\s*"zh": "((?:\\.|[^"\\])*)"""")

    /** 表里全部条目：键 → zh（已解开 JSON 转义）。 */
    val entries: Map<String, String> by lazy {
        val table =
            generateSequence(File("").absoluteFile) { it.parentFile }
                .map { File(it, "src/shared/copy/table.json") }
                .first { it.isFile }
        ENTRY.findAll(table.readText()).associate { it.groupValues[1] to unescape(it.groupValues[2]) }
    }

    /** [literal] 是表里的键 ⇒ 那句 zh；否则 null。 */
    fun zhOf(literal: String): String? = entries[literal]

    /** [literal] 是键就换成那句，不是就原样。 */
    fun shown(literal: String): String = zhOf(literal) ?: literal

    private fun unescape(s: String): String =
        Regex("""\\(u[0-9a-fA-F]{4}|.)""").replace(s) { m ->
            val e = m.groupValues[1]
            when {
                e.startsWith("u") && e.length == 5 ->
                    e
                        .substring(1)
                        .toInt(16)
                        .toChar()
                        .toString()
                e == "n" -> "\n"
                e == "t" -> "\t"
                else -> e
            }
        }
}
