package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.wire.gen.SessionDot
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.Wires
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 聊天屏顶栏两行与「会话信息」抽屉：字全是那台写好的那几格（帧上的 `title` · 脸 · 会话事实的 `line` · `usage` · `writersLine` ·
 * `session-account` 的 `accountLine` · `tasks-list` 的 `chip` · `hint` · `statusText`），手机只摆、不拼、不数。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatInfoTest {
    private class Opened(
        val link: FakeCoreLink,
        val session: ChatSession,
    ) {
        val chat: ChatUiState get() = session.state.value
    }

    private fun TestScope.opened(
        face: String = "live-working-plain",
        title: String = "proj · 订单服务重试",
        facts: Map<String, Any?> = Wires.facts("path" to "/p/s1.jsonl"),
        extra: (String, Map<String, Any?>?) -> Reply? = { _, _ -> null },
        records: List<Map<String, Any?>> = listOf(FakeCoreLink.human("u0", "问")),
    ): Opened {
        val link = FakeCoreLink()
        link.report("s1", "/h/proj", Faces.of(face))
        val book = link.table.value
        link.table.value = book.copy(live = book.live.mapValues { (_, v) -> v.copy(title = title) })
        val file = FakeCoreLink.File(records.toMutableList())
        link.answer = { cmd, args -> extra(cmd, args) ?: (if (cmd == "history-facts") Reply.Ok(facts) else null) ?: file.answer(cmd, args) ?: Reply.LinkDown }
        val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope)
        s.open()
        runCurrent()
        return Opened(link, s)
    }

    private fun line(
        activity: String,
        text: String,
    ) = mapOf("for" to activity, "text" to text, "clock" to null, "peek" to null)

    private fun usage(
        text: String,
        tone: String,
    ) = mapOf(
        "promptTokens" to 1L,
        "model" to null,
        "peakPromptTokens" to 1L,
        "limit" to 2L,
        "limitFrom" to "model",
        "percent" to 86,
        "contextText" to text,
        "contextTone" to tone,
        "promptTokensText" to "1",
        "limitText" to "2",
        "limitFromText" to null,
    )

    @Test
    fun `顶栏第一行是帧上那台写好的整行标题`() =
        runTest {
            val o = opened()
            assertEquals("proj · 订单服务重试", o.chat.title)
        }

    @Test
    fun `第二行：点与那一句都照脸；会话事实另带的那一句不拿来比、不用`() =
        runTest {
            val o = opened(facts = Wires.facts("path" to "/p/s1.jsonl", "line" to line("working", "运行中 · 改 3 个文件")))
            val h = headLine(o.chat)!!
            assertEquals(SessionDot.RUNNING, h.dot)
            assertEquals("运行中", h.text)
            assertNull("上下文没到要注意那一档 ⇒ 不接", h.context)
        }

    @Test
    fun `上下文到了那台判的要注意那一档 ⇒ 第二行末尾接那一句`() =
        runTest {
            val o = opened(facts = Wires.facts("path" to "/p/s1.jsonl", "usage" to usage("上下文 86%", "warn")))
            assertEquals("上下文 86%", headLine(o.chat)!!.context)
            val calm = opened(facts = Wires.facts("path" to "/p/s1.jsonl", "usage" to usage("上下文 12%", "plain")))
            assertNull(headLine(calm.chat)!!.context)
        }

    @Test
    fun `新到一批记录 ⇒ 再问一次会话事实（步骤那一态 · 状态一句跟着记录走）`() =
        runTest {
            val o = opened()
            val before = o.link.calls("history-facts").size
            o.link.line("s1", 1, FakeCoreLink.reply("a1", "答"))
            runCurrent()
            assertEquals(before + 1, o.link.calls("history-facts").size)
        }

    @Test
    fun `没配上结果的那一步照会话事实 pending 里那一项；不在里面 ⇒ 没有那一态`() =
        runTest {
            val pending = mapOf("id" to "call-1", "name" to "Bash", "what" to null, "at" to null, "atMs" to null, "state" to "awaiting", "why" to null, "text" to "等批准", "whyText" to null)
            val o = opened(facts = Wires.facts("path" to "/p/s1.jsonl", "pending" to listOf(pending)))
            assertEquals("等批准", o.chat.waitOf("call-1")?.text)
            assertNull(o.chat.waitOf("call-2"))
        }

    @Test
    fun `重试细条与报错卡照会话事实 retries 那一串：没接上 ⇒ 细条收起、那一串的整句进紧跟着的报错卡`() =
        runTest {
            val retry = mapOf("agent" to "claude", "id" to "r1", "t" to "retry", "draw" to "card", "human" to false, "reason" to "unknown", "text" to "rec-line")
            val error =
                mapOf(
                    "agent" to "claude",
                    "id" to "a-err",
                    "t" to "reply",
                    "autoReply" to false,
                    "draw" to "card",
                    "human" to false,
                    "blocks" to listOf(FakeCoreLink.prose("raw")),
                    "error" to mapOf("reason" to "quota", "title" to "err-title"),
                )
            val records = listOf(FakeCoreLink.human("u0", "问"), retry, error)
            val calm = opened(records = records)
            val strip = calm.chat.units.filterIsInstance<RenderUnit.Retry>().single()
            assertEquals("事实里还没有 ⇒ 记录那一行", "rec-line", calm.chat.retryOf(strip)?.text)
            assertNull("事实里还没有 ⇒ 报错卡照通用那一句", calm.chat.errorNextOf("a-err"))
            assertEquals("err-title", calm.chat.units.filterIsInstance<RenderUnit.ApiError>().single().title)
            val run = mapOf("id" to "r1", "outcome" to "failed", "text" to "run-text")
            val o = opened(facts = Wires.facts("path" to "/p/s1.jsonl", "retries" to listOf(run)), records = records)
            assertNull("没接上 ⇒ 细条收起", o.chat.retryOf(strip))
            assertEquals("run-text", o.chat.errorNextOf("a-err"))
        }

    @Test
    fun `信息抽屉：号与额度 · 几个进程在写 · 上下文 · 任务都照那台那几格原样摆`() =
        runTest {
            val account =
                mapOf(
                    "account" to "work",
                    "name" to "work",
                    "when" to "now",
                    "from" to null,
                    "accountLine" to "work · 5 小时 42%",
                    "fromLine" to null,
                    "usage" to null,
                    "stale" to false,
                    "hover" to emptyList<Any>(),
                    "sampled" to null,
                    "differs" to false,
                    "badge" to null,
                    "queryable" to true,
                )
            val task = mapOf("id" to "1", "subject" to "写测试", "status" to "in_progress", "statusText" to "进行中", "blocks" to emptyList<Any>(), "blockedBy" to emptyList<Any>())
            val tasks = mapOf("tasks" to listOf(task), "chip" to "任务 0/1", "hint" to "完成 0 · 进行 1 · 未开始 0")
            val facts = Wires.facts("path" to "/p/s1.jsonl", "writersLine" to "2 个进程在写这份记录", "usage" to usage("上下文 86%", "warn"))
            val o = opened(facts = facts, extra = { cmd, _ -> mapOf("session-account" to Reply.Ok(account), "tasks-list" to Reply.Ok(tasks))[cmd] })
            val info = ChatInfo(o.link, "box", backgroundScope) { "s1" }
            info.open()
            runCurrent()
            val ui = info.state.value!!
            assertEquals(listOf(mapOf("sid" to "s1")), o.link.calls("session-account"))
            assertEquals(listOf(mapOf("sid" to "s1")), o.link.calls("tasks-list"))
            assertEquals("work · 5 小时 42%", ui.account)
            assertEquals("2 个进程在写这份记录", ui.writers)
            assertEquals("上下文 86%", ui.context)
            assertEquals("任务 0/1", ui.tasks?.chip)
            assertEquals("完成 0 · 进行 1 · 未开始 0", ui.tasks?.hint)
            assertEquals(listOf("写测试" to "进行中"), ui.tasks?.tasks?.map { it.subject to it.statusText })
            info.close()
            assertNull(info.state.value)
        }

    @Test
    fun `信息抽屉：问不到的那一格写那台那一句，别的照摆`() =
        runTest {
            val noTasks = Reply.Ok(mapOf("tasks" to emptyList<Any>(), "chip" to "", "hint" to ""))
            val o = opened(extra = { cmd, _ -> mapOf("session-account" to FakeCoreLink.failed("那台不认"), "tasks-list" to noTasks)[cmd] })
            val info = ChatInfo(o.link, "box", backgroundScope) { "s1" }
            info.open()
            runCurrent()
            val ui = info.state.value!!
            assertNull(ui.account)
            assertEquals(listOf("那台不认"), ui.failed)
            assertNull("没有任务 ⇒ 不画那一段", ui.tasks)
        }

    private fun kin(
        sid: String,
        title: String,
    ) = mapOf("sid" to sid, "title" to title, "face" to Faces.raw["live-working-plain"], "hidden" to null)

    private val quiet: (String, Map<String, Any?>?) -> Reply? = { cmd, _ ->
        mapOf("session-account" to FakeCoreLink.failed("x"), "tasks-list" to Reply.Ok(mapOf("tasks" to emptyList<Any>(), "chip" to "", "hint" to "")))[cmd]
    }

    @Test
    fun `信息抽屉：在哪 · 后台在跑 · 血缘照会话事实那几格`() =
        runTest {
            val bg = mapOf("text" to "后台任务运行中 · npm run dev · 18m", "clock" to null, "what" to "npm run dev", "count" to 1, "tone" to "busy")
            val facts =
                Wires.facts(
                    "path" to "/p/s1.jsonl",
                    "whereLine" to "~/work/orders · devbox",
                    "background" to bg,
                    "startedBy" to kin("s0", "发版前整理"),
                    "children" to listOf(kin("c1", "子一"), kin("c2", "子二")),
                    "startedByText" to "由「发版前整理」启动",
                    "childrenText" to "子会话 2",
                )
            val o = opened(facts = facts, extra = quiet)
            val info = ChatInfo(o.link, "box", backgroundScope) { "s1" }
            info.open()
            runCurrent()
            val ui = info.state.value!!
            assertEquals("~/work/orders · devbox", ui.where)
            assertEquals("后台任务运行中 · npm run dev · 18m", ui.background)
            assertEquals("抬头是那台写好的那一句", "由「发版前整理」启动", ui.parent?.head)
            assertEquals(listOf("s0" to "发版前整理"), ui.parent?.kin?.map { it.sid to it.title })
            assertEquals("子会话 2", ui.children?.head)
            assertEquals(listOf("c1" to "子一", "c2" to "子二"), ui.children?.kin?.map { it.sid to it.title })
        }

    @Test
    fun `信息抽屉：没有血缘 · 不在后台 · 不知在哪 ⇒ 那几段不画`() =
        runTest {
            val o = opened(extra = quiet)
            val info = ChatInfo(o.link, "box", backgroundScope) { "s1" }
            info.open()
            runCurrent()
            val ui = info.state.value!!
            assertNull(ui.where)
            assertNull(ui.background)
            assertNull(ui.parent)
            assertNull(ui.children)
        }
}
