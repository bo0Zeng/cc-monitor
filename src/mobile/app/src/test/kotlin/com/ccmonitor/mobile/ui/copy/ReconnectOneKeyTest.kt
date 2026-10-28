package com.ccmonitor.mobile.ui.copy

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 「重新连接」一个义项一个键（R2 · R17）：手机各屏那台离线时的那一颗按钮由机器条画、字是核心点名的修法（成品的 `fixes[].label`），
 * 手机代码里原文是「重新连接」的键至多剩一个（终端屏那一颗，归终端那一路收）。
 */
class ReconnectOneKeyTest {
    private val root = generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }.first { File(it, "settings.gradle.kts").isFile }

    @Test
    fun `手机代码里原文是「重新连接」的文案键至多一个`() {
        val table = File(root, "../shared/copy/table.json").readText()
        // 原文从表里取（测试不照抄原文）：离线那一颗今天借的那个键的字。
        val word = copyText("newSession.machine.reconnect")
        val keys = Regex(""""([A-Za-z0-9_.]+)":\s*\{[^{}]*?"zh":\s*"${Regex.escape(word)}"""").findAll(table).map { it.groupValues[1] }.toSet()
        assertTrue("前提：表里得找得到这几个键（实得 $keys）", keys.size >= 2)
        val used =
            File(root, "app/src/main/kotlin")
                .walkTopDown()
                .filter { it.isFile && it.extension == "kt" }
                .flatMap { f ->
                    val code = KotlinSourceScanner.codeOnlyKeepingLiterals(f.readText())
                    keys.filter { code.contains("\"$it\"") }.map { it to f.name }
                }.toList()
        assertTrue("原文是「重新连接」的键在手机代码里不止一个：$used", used.map { it.first }.toSet().size <= 1)
    }
}
