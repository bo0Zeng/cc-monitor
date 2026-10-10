package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 识别非真实用户输入：内部噪声、实体转义、compact 摘要、斜杠命令、bash 输入输出。 */
class ClaudeMessageNoiseTest {
    // --- stripInternalNoise ---
    @Test
    fun stripsInterruptMarkerToEmpty() {
        // CLI 把中断标记作为整条 user message 唯一文本（无前导空白），按 `^\[…\]\s*$`（非多行）匹配。
        assertEquals("", stripInternalNoise("[Request interrupted by user]"))
        assertEquals("", stripInternalNoise("[Request interrupted by user for tool use]"))
        assertEquals("", stripInternalNoise("[Request interrupted by user]\n")) // 尾换行/空白由 \s*$ + trim 兜住
    }

    @Test
    fun stripsXmlNoiseBlocks() {
        assertEquals("", stripInternalNoise("<local-command-stdout>Set model to Fable 5</local-command-stdout>"))
        assertEquals("", stripInternalNoise("<system-reminder>be nice</system-reminder>"))
        assertEquals("", stripInternalNoise("<local-command-caveat>Caveat: DO NOT respond</local-command-caveat>"))
        assertEquals("", stripInternalNoise("<task-notification>done</task-notification>"))
    }

    @Test
    fun stripsBoilerplateLines() {
        assertEquals("", stripInternalNoise("Continue from where you left off."))
        assertEquals("", stripInternalNoise("No response requested."))
        assertEquals("", stripInternalNoise("continue from where you left off")) // 大小写 + 无句点
    }

    @Test
    fun keepsRealTextAndStripsSurroundingNoise() {
        // /compact 后跟 stdout：剥 stdout 留命令 XML（下游 parseSlashCommand 再消费）。
        val t = "<command-name>/model</command-name><local-command-stdout>Set model</local-command-stdout>"
        assertEquals("<command-name>/model</command-name>", stripInternalNoise(t))
        // 真实 prompt 原样保留。
        assertEquals("全面理解审计这个项目", stripInternalNoise("全面理解审计这个项目"))
    }

    @Test
    fun interruptRegexDoesNotEatLegitMultilineText() {
        // 非多行：合法正文里偶然有一行以该模式开头，不该被吞（整条非纯中断标记）。
        val t = "看这个:\n[Request interrupted by user] 是什么意思"
        assertEquals(t, stripInternalNoise(t))
    }

    // --- unescapeEntities ---
    @Test
    fun unescapesHtmlEntitiesSinglePass() {
        assertEquals("a < b && c > d \"q\" 'a'", unescapeEntities("a &lt; b &amp;&amp; c &gt; d &quot;q&quot; &#39;a&#39;"))
        assertEquals("&lt;", unescapeEntities("&amp;lt;")) // 一次过：amp 同批解码，不二次解 lt
    }

    // --- compact ---
    @Test
    fun detectsCompactSummary() {
        assertTrue(isCompactSummary("This session is being continued from a previous conversation…"))
        assertTrue(isCompactSummary("  This session is being continued from a previous conversation"))
        assertFalse(isCompactSummary("This session is great"))
    }

    // --- slash ---
    @Test
    fun parsesSlashCommand() {
        val c = parseSlashCommand("<command-message>demo-skill</command-message><command-name>/demo-skill</command-name><command-args>全面理解</command-args>")
        assertEquals("/demo-skill", c?.name)
        assertEquals("全面理解", c?.args)
    }

    @Test
    fun parsesSlashCommandNoArgs() {
        val c = parseSlashCommand("<command-name>/compact</command-name><command-args></command-args>")
        assertEquals("/compact", c?.name)
        assertEquals("", c?.args)
    }

    @Test
    fun slashRejectsWhenLeftoverNonBlank() {
        // 正文恰好含标签但有额外文字 → 非命令，回退。
        assertNull(parseSlashCommand("看 <command-name>/x</command-name> 这个命令"))
        assertNull(parseSlashCommand("no tags here"))
        assertNull(parseSlashCommand("<command-name></command-name>")) // 空 name
    }

    // --- bash ---
    @Test
    fun parsesBashInput() {
        assertEquals("npm install", parseBashInput("<bash-input>npm install</bash-input>")?.command)
        assertEquals("a && b", parseBashInput("<bash-input>a &amp;&amp; b</bash-input>")?.command) // unescape
        assertNull(parseBashInput("<bash-input></bash-input>")) // 空
        assertNull(parseBashInput("prefix <bash-input>x</bash-input>")) // 非整段
    }

    @Test
    fun parsesBashOutput() {
        val o = parseBashOutput("<bash-stdout>ok</bash-stdout><bash-stderr>warn</bash-stderr>")
        assertEquals("ok", o?.stdout)
        assertEquals("warn", o?.stderr)
        // 缺 stderr 段宽容。
        assertEquals("only out", parseBashOutput("<bash-stdout>only out</bash-stdout>")?.stdout)
        assertEquals("", parseBashOutput("<bash-stdout>only out</bash-stdout>")?.stderr)
        // 残余非这两类 → 回退。
        assertNull(parseBashOutput("<bash-stdout>x</bash-stdout>junk"))
        assertNull(parseBashOutput("plain text"))
    }
}
