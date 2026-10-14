package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.CommandScope
import com.ccmonitor.mobile.core.claude.link.wire.ModeSwitchResult
import com.ccmonitor.mobile.core.claude.link.wire.NewSessionPick
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 会话中途那几问（换模型 · 切档 · 命令清单 · 说一句）：入参照协议的格，回包照 `IPC-COMMANDS.md` 那几形解。 */
class SessionsMidTest {
    @Test
    fun `session-model：回包带读回的显示名`() {
        val r = SessionModel.of(mapOf("result" to "done", "model" to "Opus 5.5 (1M context)", "said" to "已换成 Opus 5.5"))
        assertEquals(ModeSwitchResult.DONE, r?.result)
        assertEquals("Opus 5.5 (1M context)", r?.model)
        assertNull("没读到 ⇒ null", SessionModel.of(mapOf("result" to "unsure", "model" to null))?.model)
    }

    @Test
    fun `session-permission：入参 sid · mode · client；回包照 ModeSwitched 解`() {
        assertEquals(mapOf("sid" to "s1", "mode" to "plan", "client" to "mobile"), SessionPermission.args("s1", "plan"))
        val r = SessionPermission.of(mapOf("result" to "done", "mode" to "plan", "modeText" to "规划模式"))
        assertEquals(ModeSwitchResult.DONE, r?.result)
        assertEquals("规划模式", r?.modeText)
        assertEquals("先答完需手动", SessionPermission.of(mapOf("result" to "refused", "mode" to null, "modeText" to null, "why" to "waiting", "said" to "先答完需手动"))?.said)
        assertNull("结局认不出 ⇒ 解不出", SessionPermission.of(mapOf("result" to "maybe", "mode" to null, "modeText" to null)))
    }

    @Test
    fun `session-commands：入参只有 sid；命令 · Skill · 读不出的那几份照原样解`() {
        assertEquals(mapOf("sid" to "s1"), SessionCommandList.args("s1"))
        val r =
            SessionCommandList.of(
                mapOf(
                    "commands" to
                        listOf(
                            mapOf("name" to "clear", "text" to "清空", "scope" to "builtin", "scopeText" to "内置", "needsArgs" to false, "confirm" to true),
                            mapOf("name" to "deploy", "text" to null, "scope" to "project", "scopeText" to "本项目", "needsArgs" to true, "confirm" to false),
                        ),
                    "skills" to listOf(mapOf("name" to "pdf", "text" to null, "say" to "用 pdf 这个 Skill")),
                    "problems" to listOf("读不出 x"),
                ),
            )!!
        assertEquals(listOf(CommandScope.BUILTIN, CommandScope.PROJECT), r.commands.map { it.scope })
        assertEquals(true, r.commands.first().confirm)
        assertEquals("用 pdf 这个 Skill", r.skills.single().say)
        assertEquals(listOf("读不出 x"), r.problems)
        assertNull("scope 认不出 ⇒ 解不出", SessionCommandList.of(mapOf("commands" to listOf(mapOf("name" to "x", "text" to null, "scope" to "team", "scopeText" to "", "needsArgs" to false, "confirm" to false)), "skills" to emptyList<Any>(), "problems" to emptyList<Any>())))
    }

    @Test
    fun `session-say：入参 sid · text · client；结局同送字那一形`() {
        assertEquals(mapOf("sid" to "s1", "text" to "/compact", "client" to "mobile"), SessionSay.args("s1", "/compact"))
        assertEquals(TerminalInput.Result.Delivered, SessionSay.of(mapOf("result" to "delivered")))
        assertEquals(TerminalInput.Result.Refused("先答完需手动", "waiting"), SessionSay.of(mapOf("result" to "refused", "why" to "waiting", "said" to "先答完需手动")))
    }

    @Test
    fun `session-new-facts：那一家的权限档清单照原样解`() {
        val agents =
            SessionNewFacts.of(
                mapOf(
                    "recent" to emptyList<Any>(),
                    "agents" to
                        listOf(
                            mapOf("kind" to "claude", "text" to "Claude Code", "models" to emptyList<Any>(), "permissionModes" to listOf(mapOf("value" to "plan", "text" to "规划模式"))),
                        ),
                ),
            )
        assertEquals(listOf(NewSessionPick("plan", "规划模式")), agents?.single()?.permissionModes)
    }
}
