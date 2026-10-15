package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CanSwitch
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Said
import com.ccmonitor.mobile.core.claude.link.SessionFate
import com.ccmonitor.mobile.core.claude.link.Switches
import com.ccmonitor.mobile.core.claude.link.wire.NewSessionPick
import com.ccmonitor.mobile.core.copy.copyText
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 聊天屏输入那一圈的会话中途四样 ＋ 逐字流：
 * 换模型 · 切权限档读回才换字、没读回回原字 ＋ 一句；［+］面板的清单 · 动作都问那台；活卡照那台的帧长、撤。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatSessionMidTest {
    private val file = FakeCoreLink.File()

    private fun TestScope.existing(
        link: FakeCoreLink,
        answer: (String, Map<String, Any?>?) -> Reply?,
    ): ChatSession {
        link.report("s1", "/h/proj")
        link.answer = { cmd, args -> answer(cmd, args) ?: file.answer(cmd, args) ?: Reply.LinkDown }
        return ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also {
            it.open()
            it.visible(true)
        }
    }

    private val opus = NewSessionPick("opus", "Opus")
    private val plan = NewSessionPick("plan", "规划模式")

    @Test
    fun `换模型：转圈期间两颗都在换、读回 done 才换成读回的字；勾上换成的那一项`() =
        runTest {
            val link = FakeCoreLink()
            val s = existing(link) { cmd, _ -> if (cmd == "session-model") Reply.Ok(mapOf("result" to "done", "model" to "Opus 5.5 (1M context)", "said" to "已换成")) else null }
            runCurrent()
            s.switchModel(opus)
            assertEquals("点了当场转圈、写换成的那一项", opus, s.state.value.model.pending)
            s.switchPermission(plan)
            assertNull("正在换模型时不起切档那一趟", s.state.value.permission.pending)
            runCurrent()
            val chip = s.state.value.model
            assertNull(chip.pending)
            assertEquals("读回的字，不是抽屉里那一项的字", "Opus 5.5 (1M context)", chip.text)
            assertEquals("opus", chip.value)
            assertNull(s.state.value.note)
            assertEquals(listOf(mapOf("sid" to "s1", "model" to "opus", "client" to "mobile")), link.calls("session-model"))
            assertTrue(link.calls("session-permission").isEmpty())
        }

    @Test
    fun `换模型没读回（unsure · 超时）⇒ 回原字 ＋ 一句；那台带了那一句就照它`() =
        runTest {
            val link = FakeCoreLink()
            val replies = ArrayDeque(listOf<Reply>(Reply.Ok(mapOf("result" to "done", "model" to "Sonnet", "said" to null)), Reply.TimedOut, Reply.Ok(mapOf("result" to "unsure", "model" to null, "said" to "结果未知，看一眼终端"))))
            val s = existing(link) { cmd, _ -> if (cmd == "session-model") replies.removeFirst() else null }
            runCurrent()
            s.switchModel(NewSessionPick("sonnet", "Sonnet"))
            runCurrent()
            assertEquals("Sonnet", s.state.value.model.text)
            s.switchModel(opus)
            runCurrent()
            assertEquals("超时 ⇒ 回原字", "Sonnet", s.state.value.model.text)
            assertEquals("sonnet", s.state.value.model.value)
            assertEquals(copyText("session.model.unsure"), s.state.value.note)
            s.switchModel(opus)
            runCurrent()
            assertEquals("Sonnet", s.state.value.model.text)
            assertEquals("结果未知，看一眼终端", s.state.value.note)
        }

    @Test
    fun `切权限档：done ⇒ 换成那一档写好的字；那台拒（在等人）⇒ 回原字 ＋ 它那一句`() =
        runTest {
            val link = FakeCoreLink()
            val replies =
                ArrayDeque(
                    listOf(
                        mapOf("result" to "done", "mode" to "plan", "modeText" to "规划模式"),
                        mapOf("result" to "refused", "mode" to null, "modeText" to null, "why" to "waiting", "said" to "先答完需手动"),
                    ),
                )
            val s = existing(link) { cmd, _ -> if (cmd == "session-permission") Reply.Ok(replies.removeFirst()) else null }
            runCurrent()
            s.switchPermission(plan)
            runCurrent()
            assertEquals("规划模式", s.state.value.permission.text)
            assertEquals(listOf(mapOf("sid" to "s1", "mode" to "plan", "client" to "mobile")), link.calls("session-permission"))
            s.switchPermission(NewSessionPick("default", "逐个确认"))
            runCurrent()
            assertEquals("规划模式", s.state.value.permission.text)
            assertEquals("先答完需手动", s.state.value.note)
            assertEquals("plan", s.state.value.permission.value)
        }

    @Test
    fun `chip 的字：打开时照 history-facts 那台写好的两个字；换成过就以读回的为准`() =
        runTest {
            val link = FakeCoreLink()
            val s =
                existing(link) { cmd, _ ->
                    when (cmd) {
                        "history-facts" -> Reply.Ok(mapOf("modelText" to "Opus 4.5", "permissionModeText" to "自动接受编辑"))
                        else -> null
                    }
                }
            runCurrent()
            assertEquals(listOf(mapOf("path" to "/p/s1.jsonl")), link.calls("history-facts"))
            assertEquals("Opus 4.5", s.state.value.model.text)
            assertEquals("自动接受编辑", s.state.value.permission.text)
        }

    @Test
    fun `能不能换照会话帧的 switch；离开活了照那一帧带的；那台离线 ⇒ 两颗都灰、写离线那一句`() =
        runTest {
            val link = FakeCoreLink()
            val s = existing(link) { _, _ -> null }
            link.report("s1", "/h/proj", switch = Switches(CanSwitch("本轮结束后可换模型"), CanSwitch(null)))
            runCurrent()
            assertEquals("本轮结束后可换模型", s.state.value.model.blocked)
            assertNull(s.state.value.permission.blocked)
            link.table.value = link.table.value.copy(sessions = emptyMap(), fates = mapOf("s1" to SessionFate(SessionFate.State.ENDED, "已结束", Switches(CanSwitch("会话已退出"), CanSwitch("会话已退出")))))
            runCurrent()
            assertEquals("会话已退出", s.state.value.permission.blocked)
            link.state.value = LinkState.Down(Said("", ""), needsHand = false)
            runCurrent()
            val off = copyText("newSession.machine.offline", "machine" to "box")
            assertEquals(off to off, s.state.value.model.blocked to s.state.value.permission.blocked)
        }

    @Test
    fun `被顶替（清空之后另起一份）⇒ 跟到顶替它的那一条、从新记录读`() =
        runTest {
            val link = FakeCoreLink()
            file.records += FakeCoreLink.human("u1", "旧的那句")
            val s = existing(link) { _, _ -> null }
            runCurrent()
            assertEquals(1, s.state.value.units.size)
            file.records.clear()
            link.report("s2", "/h/proj")
            link.table.value = link.table.value.copy(sessions = link.table.value.sessions - "s1", successors = mapOf("s1" to "s2"))
            runCurrent()
            assertEquals("s2", s.state.value.sid)
            assertEquals("旧记录那几条不留", 0, s.state.value.units.size)
            assertEquals("/p/s2.jsonl", link.calls("history-tail").last()?.get("path"))
            assertEquals(setOf("s2"), link.watching())
        }

    @Test
    fun `抽屉里列的是那台给这条会话那一家的模型与权限档`() =
        runTest {
            val link = FakeCoreLink()
            val facts =
                mapOf(
                    "recent" to emptyList<Any>(),
                    "agents" to
                        listOf(
                            mapOf(
                                "kind" to "claude",
                                "text" to "Claude Code",
                                "models" to listOf(mapOf("value" to "opus", "text" to "Opus")),
                                "modelDefault" to mapOf("value" to "default", "text" to "默认"),
                                "modelNote" to "换模型也改此号以后新会话的默认",
                                "permissionModes" to listOf(mapOf("value" to "plan", "text" to "规划模式")),
                            ),
                        ),
                )
            val s = existing(link) { cmd, _ -> if (cmd == "session-new-facts") Reply.Ok(facts) else null }
            runCurrent()
            s.loadPicks()
            runCurrent()
            assertEquals(Picks(listOf(opus), listOf(plan), NewSessionPick("default", "默认"), "换模型也改此号以后新会话的默认"), s.state.value.picks)
            s.loadPicks()
            runCurrent()
            assertEquals("问到过就不再问", 1, link.calls("session-new-facts").size)
        }

    @Test
    fun `［+］面板：清单问那台这条会话的；读不出 ⇒ 那一句`() =
        runTest {
            val link = FakeCoreLink()
            val list = mapOf("commands" to emptyList<Any>(), "skills" to listOf(mapOf("name" to "pdf", "text" to null, "say" to "用 pdf")), "problems" to emptyList<Any>())
            var fail = false
            val s = existing(link) { cmd, _ -> if (cmd == "session-commands") (if (fail) FakeCoreLink.failed("读不出命令") else Reply.Ok(list)) else null }
            runCurrent()
            s.loadCommands()
            assertEquals(CommandsView.Loading, s.state.value.commands)
            runCurrent()
            val ready = s.state.value.commands as CommandsView.Ready
            val skill = ready.list.skills.single()
            assertEquals("用 pdf", skill.say)
            assertEquals(listOf(mapOf("sid" to "s1")), link.calls("session-commands"))
            fail = true
            s.loadCommands()
            runCurrent()
            assertEquals(CommandsView.Failed("读不出命令"), s.state.value.commands)
        }

    @Test
    fun `面板里的动作经 session-say 说那一句；那台不送 ⇒ chip 上方那一句`() =
        runTest {
            val link = FakeCoreLink()
            val replies = ArrayDeque(listOf(mapOf("result" to "delivered"), mapOf("result" to "refused", "why" to "waiting", "said" to "先答完需手动")))
            val s = existing(link) { cmd, _ -> if (cmd == "session-say") Reply.Ok(replies.removeFirst()) else null }
            runCurrent()
            s.say("/compact")
            runCurrent()
            assertNull(s.state.value.note)
            s.say("/clear")
            runCurrent()
            assertEquals(copyText("terminal.input.failed", "why" to "先答完需手动"), s.state.value.note)
            assertEquals(listOf("/compact", "/clear"), link.calls("session-say").map { it?.get("text") })
        }

    private fun open(
        card: Long,
        text: String,
        sid: String = "s1",
        run: String? = null,
    ) = mapOf("sid" to sid, "card" to card.toDouble(), "n" to 0.0, "op" to "open", "rid" to "r$card", "phase" to "streaming", "clipped" to false, "blocks" to listOf(mapOf("kind" to "text", "text" to text))) +
        (run?.let { mapOf("run" to it) } ?: emptyMap())

    @Test
    fun `逐字流：这条会话主运行的活卡照帧长出来；别的会话 · 子运行的不画；记录到了那台撤卡`() =
        runTest {
            val link = FakeCoreLink()
            val s = existing(link) { _, _ -> null }
            runCurrent()
            link.live(open(1, "改"))
            link.live(open(2, "别的会话", sid = "s2"))
            link.live(open(3, "子运行", run = "a1"))
            link.live(mapOf("sid" to "s1", "card" to 1.0, "n" to 1.0, "op" to "text", "i" to 0.0, "s" to "好了"))
            runCurrent()
            val cards = s.state.value.live
            val texts = cards.map { it.blocks.single().text }
            assertEquals(listOf("改好了"), texts)
            link.live(mapOf("sid" to "s1", "card" to 1.0, "n" to 2.0, "op" to "close", "why" to "record"))
            runCurrent()
            assertEquals(emptyList<Any>(), s.state.value.live)
        }

    @Test
    fun `逐字流：换了一条流 ⇒ 卡全撤，等那台在新的那条上补整卡`() =
        runTest {
            val link = FakeCoreLink()
            val s = existing(link) { _, _ -> null }
            runCurrent()
            link.live(open(1, "一半"))
            runCurrent()
            assertEquals(1, s.state.value.live.size)
            link.epoch.value = 1
            runCurrent()
            assertEquals(emptyList<Any>(), s.state.value.live)
        }
}
