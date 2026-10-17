package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.LiveBlock
import com.ccmonitor.mobile.core.claude.link.LiveCardState
import com.ccmonitor.mobile.core.claude.link.wire.AgentPick
import com.ccmonitor.mobile.core.claude.link.wire.CommandScope
import com.ccmonitor.mobile.core.claude.link.wire.SessionAct
import com.ccmonitor.mobile.core.claude.link.wire.SessionCommand
import com.ccmonitor.mobile.core.claude.link.wire.SessionCommands
import com.ccmonitor.mobile.core.copy.copyText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 输入那一圈的几处纯判定：chip 写什么 · ［+］面板下面两行在不在 · 点命令填进去的字 · 分组 · 活卡画成的字。 */
class ChatInputLogicTest {
    private fun cmd(
        name: String,
        scope: CommandScope = CommandScope.BUILTIN,
        scopeText: String = "内置",
        needsArgs: String? = null,
        confirm: String? = null,
    ) = SessionCommand(name, null, scope, scopeText, needsArgs, confirm)

    @Test
    fun `chip：正在换写换成的那一项；读回过写读回的字；都没有写这颗是什么`() {
        assertEquals("模型", chipText(SwitchChip(), "模型"))
        assertEquals("Opus 5.5", chipText(SwitchChip(text = "Opus 5.5"), "模型"))
        assertEquals("Sonnet", chipText(SwitchChip(text = "Opus 5.5", pending = AgentPick("sonnet", "Sonnet")), "模型"))
    }

    @Test
    fun `面板下面两行：那份清单给了哪一个画哪一个，说的那一句与先不先确认照它；命令行尾照那台写好的标`() {
        val compact = SessionAct("/compact", false)
        val clear = SessionAct("/clear", true)
        assertEquals(listOf(copyText("mobPlus.row.compact") to compact, copyText("mobPlus.row.clear") to clear), oneShots(SessionCommands(emptyList(), emptyList(), emptyList(), compact, clear)))
        assertEquals("那一家不给 ⇒ 不画，同名命令在清单里也不算", emptyList<Pair<String, SessionAct>>(), oneShots(SessionCommands(listOf(cmd("compact"), cmd("clear")), emptyList(), emptyList(), null, null)))
        assertEquals(listOf("带参数", "先确认"), commandMarks(cmd("x", needsArgs = "带参数", confirm = "先确认")))
        assertEquals(emptyList<String>(), commandMarks(cmd("y")))
    }

    @Test
    fun `chip 上方那一行：没成的那一句（琥珀）先于不能换的那一句（灰）；同一个原因只写一遍`() {
        assertNull(chipLine(ChatUiState()))
        val waiting = ChatUiState(model = SwitchChip(blocked = "先答完需手动"), permission = SwitchChip(blocked = "先答完需手动"))
        assertEquals("先答完需手动" to false, chipLine(waiting))
        assertEquals("本轮结束后可换模型" to false, chipLine(ChatUiState(model = SwitchChip(blocked = "本轮结束后可换模型"))))
        assertEquals("只有权限档不能换也写", "x" to false, chipLine(ChatUiState(permission = SwitchChip(blocked = "x"))))
        assertEquals("换模型结果未知" to true, chipLine(waiting.copy(note = "换模型结果未知")))
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
