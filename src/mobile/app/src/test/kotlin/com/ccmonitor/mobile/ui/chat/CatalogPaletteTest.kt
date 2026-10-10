package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.bridge.BridgeFrame
import com.ccmonitor.mobile.core.claude.bridge.CommandCatalog
import com.ccmonitor.mobile.core.claude.bridge.ReplayVectors
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** catalog → `PaletteAction` 的映射。同样喂真 golden。 */
class CatalogPaletteTest {
    private fun catalog(case: String): CommandCatalog {
        val dir = File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")
        val init =
            ReplayVectors
                .parse(File(dir, "$case.frames.ndjson").readLines().asSequence())
                .map { it.frame }
                .filterIsInstance<BridgeFrame.Init>()
                .first()
        return CommandCatalog.from(init)
    }

    /**
     * 只读条目连 lambda 都没有 —— 结构上不可能「点了插进去一个空东西」。
     *
     * 不用 `enabled: Boolean` + 一个永远 no-op 的 `run`：两个字段表达同一件事，迟早分叉。
     * 可空 `run` 让终端面的调用方一个新概念都不用学。
     */
    @Test
    fun readOnlyEntriesCarryNoActionAtAll() {
        val actions = catalog("long-reply").toPaletteActions { }
        assertTrue("工具整组只读", actions.filter { it.group == GROUP_TOOL }.all { it.run == null })
        assertTrue("MCP 整组只读 —— 服务器名插进草稿不是可调用的东西", actions.filter { it.group == GROUP_MCP }.all { it.run == null })
        assertTrue(
            "命令/skill/agent 可点",
            actions.filter { it.group in setOf(GROUP_SKILL, GROUP_COMMAND, GROUP_AGENT) }.all { it.run != null },
        )
    }

    /** 可点的那些要真的插对东西 —— 尤其 agent 的形式是 `@agent-<名>` 而不是裸名。 */
    @Test
    fun clickingInsertsTheFormThatActuallyWorks() {
        var inserted: String? = null
        val actions = catalog("long-reply").toPaletteActions { inserted = it }
        actions.first { it.group == GROUP_SKILL }.run?.invoke()
        assertEquals("/demo-skill-a ", inserted)
        actions.first { it.group == GROUP_AGENT && it.label == "Explore" }.run?.invoke()
        assertEquals("agent 的唯一生效形式（CLI 的 mention 正则）", "@agent-Explore ", inserted)
    }

    /** 分区互斥要一路保持到 UI 层 —— 同一个名字不许在两个组里各出现一次。 */
    @Test
    fun noEntryAppearsInTwoGroups() {
        val actions = catalog("long-reply").toPaletteActions { }
        val skillNames = actions.filter { it.group == GROUP_SKILL }.map { it.label }.toSet()
        val cmdNames = actions.filter { it.group == GROUP_COMMAND }.map { it.label }.toSet()
        assertTrue("前提：两组都非空", skillNames.isNotEmpty() && cmdNames.isNotEmpty())
        assertTrue("交集必须为空：${skillNames intersect cmdNames}", (skillNames intersect cmdNames).isEmpty())
    }

    @Test
    fun anEmptyCatalogProducesNoActionsAtAll() {
        assertTrue(CommandCatalog().toPaletteActions { }.isEmpty())
    }

    // ---- appendToken ---------------------------------------------------------

    @Test
    fun appendingKeepsWhatTheUserAlreadyTyped() {
        assertEquals("草稿空 ⇒ 就是它", "/clear ", "".appendToken("/clear "))
        assertEquals("已经以空白结尾 ⇒ 直接接", "帮我看看 /clear ", "帮我看看 ".appendToken("/clear "))
        assertEquals("否则补一个空格", "帮我看看 /clear ", "帮我看看".appendToken("/clear "))
        // 不 trim 用户的草稿、不去重 —— 那是替他做决定
        assertEquals("  x /clear ", "  x".appendToken("/clear "))
        assertEquals("连点两次不该被吞掉", "/a /a ", "".appendToken("/a ").appendToken("/a "))
        // agent mention 必须在行首或空白之后（CLI 的正则 `(^|[\s。、？！])@agent-…`）——
        //   `appendToken` 恰好保证这一点，所以这条也是 agent 那个形式能生效的依据
        assertTrue("插进去之后 @ 前面必须是空白或行首", "帮我看看".appendToken("@agent-Explore ").contains(" @agent-"))
    }
}
