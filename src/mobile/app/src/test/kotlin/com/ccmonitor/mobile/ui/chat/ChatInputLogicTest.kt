package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.LiveBlock
import com.ccmonitor.mobile.core.claude.link.LiveCardState
import com.ccmonitor.mobile.core.claude.link.wire.CommandScope
import com.ccmonitor.mobile.core.claude.link.wire.NewSessionPick
import com.ccmonitor.mobile.core.claude.link.wire.SessionCommand
import com.ccmonitor.mobile.core.claude.link.wire.SessionCommands
import com.ccmonitor.mobile.core.copy.copyText
import org.junit.Assert.assertEquals
import org.junit.Test

/** 输入那一圈的几处纯判定：chip 写什么 · ［+］面板下面两行在不在 · 点命令填进去的字 · 分组 · 活卡画成的字。 */
class ChatInputLogicTest {
    private fun cmd(
        name: String,
        scope: CommandScope = CommandScope.BUILTIN,
        scopeText: String = "内置",
        confirm: Boolean = false,
    ) = SessionCommand(name, null, scope, scopeText, confirm)

    @Test
    fun `chip：正在换写换成的那一项；读回过写读回的字；都没有写这颗是什么`() {
        assertEquals("模型", chipText(SwitchChip(), "模型"))
        assertEquals("Opus 5.5", chipText(SwitchChip(text = "Opus 5.5"), "模型"))
        assertEquals("Sonnet", chipText(SwitchChip(text = "Opus 5.5", pending = NewSessionPick("sonnet", "Sonnet")), "模型"))
    }

    @Test
    fun `面板下面两行：清单里有那两条内置命令才画，先不先确认照那一条的 confirm；同名的项目命令不算`() {
        val got = plusActions(SessionCommands(listOf(cmd("compact"), cmd("clear", confirm = true), cmd("review")), emptyList(), emptyList()))
        assertEquals(listOf(PlusAction(PlusAction.Kind.COMPACT, "/compact", false), PlusAction(PlusAction.Kind.CLEAR, "/clear", true)), got)
        assertEquals(emptyList<PlusAction>(), plusActions(SessionCommands(listOf(cmd("compact", CommandScope.PROJECT, "本项目")), emptyList(), emptyList())))
    }

    @Test
    fun `点命令填进去的是 斜杠名 加一个空格；分组照那台写的住哪、组序照清单`() {
        assertEquals("/deploy ", commandDraft(cmd("deploy")))
        val groups = groupsOf(listOf(cmd("a"), cmd("d", CommandScope.PROJECT, "本项目"), cmd("b")))
        assertEquals(listOf("内置" to listOf("a", "b"), "本项目" to listOf("d")), groups.map { (h, rows) -> h to rows.map { it.name } })
    }

    @Test
    fun `活卡画成纯文字：头一行照相取字；截过头先说一句；思考 · 工具块各一句；空的文字块不出`() {
        val card =
            LiveCardState(
                1,
                "r",
                awaiting = false,
                clipped = true,
                blocks = listOf(LiveBlock("thinking", "", null), LiveBlock("text", "正文", null), LiveBlock("tool", "", "Bash"), LiveBlock("text", "", null), LiveBlock("other", "", null)),
            )
        val (head, body) = liveCardText(card)
        assertEquals(copyText("liveCard.state.streaming"), head)
        assertEquals(listOf(copyText("liveCard.body.clipped"), copyText("liveCard.block.thinking"), "正文", copyText("liveCard.block.toolUse", "tool" to "Bash")).joinToString("\n"), body)
        assertEquals(copyText("liveCard.state.awaitingRecord"), liveCardText(card.copy(awaiting = true)).first)
    }
}
