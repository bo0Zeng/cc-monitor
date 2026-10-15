package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.CommandScope
import com.ccmonitor.mobile.core.claude.link.wire.ModeSwitchResult
import com.ccmonitor.mobile.core.claude.link.wire.NewSessionPick
import com.ccmonitor.mobile.core.claude.link.wire.SessionAct
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
                            mapOf("name" to "clear", "text" to "清空", "scope" to "builtin", "scopeText" to "内置", "needsArgs" to false, "confirm" to true, "needsArgsText" to null, "confirmText" to "先确认"),
                            mapOf("name" to "deploy", "text" to null, "scope" to "project", "scopeText" to "本项目", "needsArgs" to true, "confirm" to false, "needsArgsText" to "带参数", "confirmText" to null),
                        ),
                    "skills" to listOf(mapOf("name" to "pdf", "text" to null, "say" to "用 pdf 这个 Skill")),
                    "problems" to listOf("读不出 x"),
                    "compact" to mapOf("say" to "/compact", "confirm" to false),
                    "clear" to null,
                ),
            )!!
        assertEquals(listOf(CommandScope.BUILTIN, CommandScope.PROJECT), r.commands.map { it.scope })
        assertEquals(listOf("先确认", null), r.commands.map { it.confirmText })
        assertEquals(listOf(null, "带参数"), r.commands.map { it.needsArgsText })
        assertEquals(SessionAct("/compact", false), r.compact)
        assertNull("那一家不支持清空 ⇒ 没有那一项", r.clear)
        assertEquals("用 pdf 这个 Skill", r.skills.single().say)
        assertEquals(listOf("读不出 x"), r.problems)
        assertNull("scope 认不出 ⇒ 解不出", SessionCommandList.of(mapOf("commands" to listOf(mapOf("name" to "x", "text" to null, "scope" to "team", "scopeText" to "", "needsArgs" to false, "confirm" to false, "needsArgsText" to null, "confirmText" to null)), "skills" to emptyList<Any>(), "problems" to emptyList<Any>(), "compact" to null, "clear" to null)))
    }

    @Test
    fun `session-say：入参 sid · text · client；结局同送字那一形`() {
        assertEquals(mapOf("sid" to "s1", "text" to "/compact", "client" to "mobile"), SessionSay.args("s1", "/compact"))
        assertEquals(TerminalInput.Result.Delivered, SessionSay.of(mapOf("result" to "delivered")))
        assertEquals(TerminalInput.Result.Refused("先答完需手动", "waiting"), SessionSay.of(mapOf("result" to "refused", "why" to "waiting", "said" to "先答完需手动")))
    }

    @Test
    fun `history-facts：入参只有路径；chip 上的两个显示字照抄，没有就是没有`() {
        assertEquals(mapOf("path" to "/p/s.jsonl"), HistoryFacts.args("/p/s.jsonl"))
        val f = HistoryFacts.of(mapOf("modelText" to "Opus 4.5", "permissionModeText" to null, "end" to 9.0))!!
        assertEquals("Opus 4.5", f.modelText)
        assertNull(f.permissionModeText)
    }

    @Test
    fun `session-new-facts：那一家的权限档清单照原样解`() {
        val agents =
            SessionNewFacts.of(
                mapOf(
                    "recent" to emptyList<Any>(),
                    "agents" to
                        listOf(
                            mapOf(
                                "kind" to "claude",
                                "text" to "Claude Code",
                                "models" to emptyList<Any>(),
                                "modelDefault" to mapOf("value" to "default", "text" to "默认"),
                                "modelNote" to "换模型也改此号以后新会话的默认",
                                "permissionModes" to listOf(mapOf("value" to "plan", "text" to "规划模式")),
                            ),
                        ),
                ),
            )
        val one = agents?.agents?.single()
        assertEquals(listOf(NewSessionPick("plan", "规划模式")), one?.permissionModes)
        assertEquals(NewSessionPick("default", "默认"), one?.modelDefault)
        assertEquals("换模型也改此号以后新会话的默认", one?.modelNote)
    }
}
