package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.claude.link.TerminalsList
import com.ccmonitor.mobile.core.copy.copyText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * tmux 管理窗的排法照那台的名单：带 `session.sid` 的进「会话所在的终端」，别的进「终端会话」；
 * 能不能结束照每行的 `can.end`（不能 ⇒ 不给［结束］、第二行写那台那一句），手机不自己判。
 * 起会话只走新建会话那一处：管理窗里没有「后台起 agent」。
 */
class TerminalManagerPlanTest {
    private fun terminal(
        name: String,
        sid: String?,
        end: Any = true,
    ) = mapOf(
        "terminal" to "h-$name",
        "host" to "tmux",
        "name" to name,
        "title" to name,
        "program" to "bash",
        "cwd" to "/w",
        "session" to sid?.let { mapOf("sid" to it, "agent" to "claude") },
        "started_by" to mapOf("client" to null, "mine" to true),
        "clients" to emptyList<Any>(),
        "input" to mapOf("kind" to "shared"),
        "state" to "running",
        "last_activity" to 0,
        "can" to mapOf("input" to true, "end" to end),
    )

    private fun rows(vararg t: Map<String, Any?>) = TerminalsList.of(mapOf("terminals" to t.toList(), "complete" to true))!!

    @Test
    fun `那台说不能结束的那一行没有结束，第二行写那台那一句`() {
        val refused = mapOf("no" to "not_yours", "said" to "S-not-ours")
        val plan = terminalPlan(rows(terminal("cc", "s1"), terminal("docs", "s2", refused), terminal("scratch", null)))
        val docs = plan.flatMap { it.lines }.single { it.row.name == "docs" }
        assertFalse("can.end 说不能 ⇒ 不画［结束］", docs.endable)
        assertEquals("S-not-ours", docs.note)
        val cc = plan.flatMap { it.lines }.single { it.row.name == "cc" }
        assertTrue(cc.endable)
        assertNull(cc.note)
    }

    @Test
    fun `照 session_sid 分两组：会话所在的终端在前，终端会话在后；空的那组不画`() {
        val plan = terminalPlan(rows(terminal("scratch", null), terminal("cc", "s1"), terminal("docs", "s2")))
        assertEquals(listOf(copyText("mobTerms.group.session", "n" to 2), copyText("mobTerms.group.plain", "n" to 1)), plan.map { it.head })
        assertEquals(listOf(listOf("cc", "docs"), listOf("scratch")), plan.map { g -> g.lines.map { it.row.name } })
        assertEquals(listOf(copyText("mobTerms.group.plain", "n" to 1)), terminalPlan(rows(terminal("scratch", null))).map { it.head })
        assertTrue(terminalPlan(emptyList()).isEmpty())
    }

    @Test
    fun `后台起 agent 那一格在手机源码里零命中`() {
        val root = listOf(File(".."), File(".")).first { File(it, "core-claude").isDirectory && File(it, "app").isDirectory }
        val hits =
            root
                .listFiles()!!
                .map { File(it, "src/main") }
                .filter { it.isDirectory }
                .flatMap { d -> d.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
                .filter { f -> listOf("detachedCc", "newInTmux", "startAgent").any { f.readText().contains(it) } }
                .map { it.path }
        assertTrue("前提：得扫到手机源码", File(root, "app/src/main").isDirectory)
        assertEquals(emptyList<String>(), hits)
    }
}
