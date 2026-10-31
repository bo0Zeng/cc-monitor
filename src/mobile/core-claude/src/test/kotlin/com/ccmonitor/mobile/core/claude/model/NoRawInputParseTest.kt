package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.link.Fixtures
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 核心外不按某一家的原文形状解（R4）：`core-claude` 产品代码里不把一团原样 JSON 解成无类型的值
 * （`readJsonValue`）。线上来的东西走生成的线上类型，原样的那一格只照抄、不解。
 */
class NoRawInputParseTest {
    @Test fun mainSourcesNeverReadUntypedJson() {
        val main = File(Fixtures.repoRoot, "src/mobile/core-claude/src/main/kotlin")
        val files = main.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
        assertTrue("core-claude/src/main 一份 .kt 都没扫到 —— 本条在空转", files.size > 20)
        val hits =
            files.flatMap { f ->
                KotlinSourceScanner.codeOnlyDroppingLiterals(f.readText()).lines().mapIndexedNotNull { i, line ->
                    if ("readJsonValue" in line) "${f.relativeTo(main)}:${i + 1}" else null
                }
            }
        assertEquals("这几处把原样 JSON 解成无类型的值", emptyList<String>(), hits)
    }
}
