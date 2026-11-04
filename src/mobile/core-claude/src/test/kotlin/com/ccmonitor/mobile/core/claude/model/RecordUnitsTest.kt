package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.link.Json
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.Block
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import com.ccmonitor.mobile.core.wire.gen.RetryOutcome
import com.ccmonitor.mobile.core.wire.gen.RetryRun
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 核心的通用记录 ⇒ 渲染单元：只按 `t` · `speaker.kind` · 块的 `type` 排，不认任何一家的盘上格式。
 * 样本是后端真序列化器写的 `tests/__fixtures__/record.golden.jsonl`。
 */
class RecordUnitsTest {
    private val cases: Map<String, LineRecord> =
        File(repoRoot(), "tests/__fixtures__/record.golden.jsonl")
            .readLines()
            .mapNotNull(Json::obj)
            .associate { (it["case"] as String) to requireNotNull(Wire.bind<LineRecord>(it["record"])) { "金样装不进线上类：$it" } }

    private fun units(vararg names: String) = RecordUnits.of(names.map { cases.getValue(it) })

    @Test
    fun `人说的画成气泡；正文照 who_text`() {
        assertEquals(listOf(RenderUnit.UserText("u-min", "hello", "u-min")), units("said-min"))
    }

    @Test
    fun `人粘贴的块：不折的只露正文那一截（标记不上屏），整句并成一段`() {
        val u = units("said-pasted").single() as RenderUnit.UserText
        assertEquals(listOf(SaidPart.Plain("see A\nB end")), u.parts)
    }

    @Test
    fun `人粘贴的块：那台说要折 ⇒ 折成一段，收着那一行是那台写好的 foldText，点开是正文那一截`() {
        val u = units("said-pasted-fold").single() as RenderUnit.UserText
        assertEquals(listOf(SaidPart.Plain("see "), SaidPart.Folded("粘贴的内容 · 13 行", (1..13).joinToString("\n"))), u.parts)
    }

    @Test
    fun `重试记录画成重试细条：字是那一条写好的 retry_text；相邻的几条并成一条，身份是首条的`() {
        assertEquals(listOf(RenderUnit.Retry("r-full", "服务器过载 · 重试 2/10 · 01:30 起", "r-full")), units("retry-full"))
        assertEquals("相邻的并成一条", listOf("r-full"), units("retry-full", "retry-min").map { it.key })
        assertEquals("中间隔着别的 ⇒ 各一条", listOf("r-full", "a-min#0", "@4096"), units("retry-full", "reply-min", "retry-min").map { it.key })
    }

    @Test
    fun `带 error 的回复画成报错卡：卡头是那台写好的 error_title，正文进原文`() {
        assertEquals(listOf(RenderUnit.ApiError("a-err", "本轮中断 · 额度满", "01:30", "API Error: 429 rate limited", "a-err")), units("reply-error"))
    }

    @Test
    fun `细条照会话事实那一串：在 ⇒ 那一串的整句与结局；不在 ⇒ 记录那一行；没接上 ⇒ 收起、整句进紧跟着的报错卡`() {
        val us = units("retry-full", "retry-min", "reply-error")
        val strip = us[0] as RenderUnit.Retry
        assertEquals("事实还没到 ⇒ 记录那一行、在重试", RetryStrip("服务器过载 · 重试 2/10 · 01:30 起", faded = false), RetryLook.strip(strip, emptyList()))
        val ok = RetryRun("r-full", RetryOutcome.RECOVERED, "重试 3 次后接上")
        assertEquals("接上了 ⇒ 那一串的整句、变淡", RetryStrip("重试 3 次后接上", faded = true), RetryLook.strip(strip, listOf(ok)))
        assertEquals("接上了 ⇒ 报错卡照通用那一句", null, RetryLook.next(us, "a-err", listOf(ok)))
        val failed = RetryRun("r-full", RetryOutcome.FAILED, "重试 10 次仍未接上")
        assertEquals("没接上 ⇒ 细条收起", null, RetryLook.strip(strip, listOf(failed)))
        assertEquals("没接上 ⇒ 整句进紧跟着的报错卡", "重试 10 次仍未接上", RetryLook.next(us, "a-err", listOf(failed)))
        assertEquals("报错卡前面不是细条 ⇒ 通用那一句", null, RetryLook.next(units("reply-error"), "a-err", listOf(failed)))
    }

    @Test
    fun `工具头那一行照核心写好的：左边是这一步的参数（认不出的那一家照核心那句），右边是结果那一句`() {
        val calls = units("reply-full", "said-read").filterIsInstance<RenderUnit.ToolCall>().associateBy { it.toolUseId }
        assertEquals("认得的那一步：参数", "/w/a.txt", calls.getValue("call-3").summary)
        assertEquals("认不出的那一步：核心那一句，不另拼 k=v", "未识别结果 · 原文", calls.getValue("call-4").summary)
        assertEquals("结果那一句", "3 行", calls.getValue("call-4").resultLine)
        assertEquals("还没结果 ⇒ 没有那一句", null, calls.getValue("call-3").resultLine)
        val bash = units("reply-step-note").filterIsInstance<RenderUnit.ToolCall>().single()
        assertEquals("ls", bash.summary)
    }

    @Test
    fun `回复：思考 · 正文 · 工具调用各一单元，按块的先后；工具的结果从后来那条人侧记录里配上`() {
        val u = units("reply-full")
        assertEquals(listOf("a-full#0", "a-full#1", "a-full#2", "a-full#3", "a-full#4"), u.map { it.key })
        assertEquals(RenderUnit.Thinking("a-full#0", "plan it", "a-full"), u[0])
        // 正文照记录里那棵树原样带上（后端解析好的），手机不再解析；「展开 {大小}」那一格是后端写好的，原样带到卡上。
        val prose = (cases.getValue("reply-full") as LineRecord.Reply).blocks[1] as Block.Prose
        assertTrue("「展开」那一格后端写好了", prose.more.isNotEmpty())
        assertEquals(RenderUnit.AssistantMarkdown("a-full#1", Doc(listOf(MdBlock.P(listOf(Inl.Text("done")))), emptyList()), prose.more, "a-full"), u[1])
        assertEquals("done", u[1].searchableText())
        val edit = u[3] as RenderUnit.ToolCall
        assertEquals("Edit", edit.name)
        assertTrue("没有结果 ⇒ 还没配上结果（是什么态由会话事实说）", edit.pending)
        assertEquals("认不出的工具也是一张调用卡", "mcp__x__y", (u[4] as RenderUnit.ToolCall).name)
    }

    @Test
    fun `工具结果：正文 · 出错 · 核心算好的行差原样带上`() {
        val reply = mapOf("agent" to "claude", "id" to "a", "t" to "reply", "autoReply" to false, "draw" to "steps", "human" to false, "blocks" to listOf(mapOf("type" to "tool_use", "id" to "call-1", "name" to "Edit", "input" to mapOf("file_path" to "/w/a.txt"))))
        val call = RecordUnits.of(listOf(requireNotNull(Wire.bind<LineRecord>(reply)), cases.getValue("said-full"))).first() as RenderUnit.ToolCall
        assertEquals("line a\nline b", call.resultText)
        assertEquals(false, call.pending)
        assertEquals(false, call.isError)
        // 行差照核心算好的原样带上（行 · 行号 · 「+N −M」），不另判行。
        val diff = requireNotNull(call.diff) { "结果带着行差" }
        assertEquals(listOf(Triple("a", 1, null), Triple("b", null, 1), Triple("c", null, 2)), diff.rows.map { Triple(it.text, it.oldNo, it.newNo) })
        assertEquals("+2 −1", diff.stat)
        assertEquals("/w/a.txt", call.patchFilePath)
    }

    @Test
    fun `交互卡照核心判的卡型（reply_cards）认，不认工具名`() {
        fun call(
            cards: Map<String, String>?,
            draw: String,
        ) =
            RecordUnits
                .of(listOf(requireNotNull(Wire.bind<LineRecord>(mapOf("agent" to "claude", "id" to "a", "t" to "reply", "autoReply" to false, "draw" to draw, "human" to false, "cards" to cards, "blocks" to listOf(mapOf("type" to "tool_use", "id" to "c", "name" to "Tool", "input" to emptyMap<String, Any>())))))))
                .single() as RenderUnit.ToolCall
        assertTrue(call(mapOf("c" to "interactive"), "card").interactive)
        assertEquals(false, call(mapOf("c" to "diff"), "steps").interactive)
        assertEquals(false, call(null, "steps").interactive)
    }

    @Test
    fun `自动应答不建卡；斜杠命令 · bash 入出 · 摘要 · 中断各画各的`() {
        assertEquals(emptyList<RenderUnit>(), units("reply-auto"))
        assertEquals(RenderUnit.SlashCommand("u-said-slash", "/review", "x", "u-said-slash"), units("said-slash").single())
        assertTrue(units("said-bash-input").single() is RenderUnit.BashInput)
        assertEquals("o", (units("said-bash-output").single() as RenderUnit.BashOutput).stdout)
        assertTrue(units("said-compact-summary").single() is RenderUnit.CompactSummary)
        assertTrue(units("said-interrupt").single() is RenderUnit.Interrupt)
    }

    @Test
    fun `没定画法的那几类（交回 · 系统 · 标题）不上屏`() {
        val silent =
            listOf("said-system", "said-agent-message", "said-coordinator", "said-command-output", "title-agent", "said-read")
        assertEquals(emptyList<RenderUnit>(), units(*silent.toTypedArray()))
    }

    @Test
    fun `另一会话来话画成来话条：抬头是那台写好的 title · 钟面 · 正文是那台解析好的树`() {
        assertEquals(
            listOf(RenderUnit.Peer("u-said-peer-session", "会话「s2」", "01:31", Doc(listOf(MdBlock.P(listOf(Inl.Text("hi")))), emptyList()), "u-said-peer-session")),
            units("said-peer-session"),
        )
    }

    @Suppress("UNCHECKED_CAST")
    @Test
    fun `后台任务收场通知画一行：字是那台写好的 row，失败照 end`() {
        assertEquals(listOf(RenderUnit.Notice("u-said-task-notification", "后台任务 finished 完成 · 01:31", false, "u-said-task-notification")), units("said-task-notification"))
        val r = Json.obj(File(repoRoot(), "tests/__fixtures__/record.golden.jsonl").readLines().first { it.contains("\"case\":\"said-task-notification\"") })!!
        val rec = (r["record"] as Map<String, Any?>).toMutableMap()
        val who = (rec["who"] as Map<String, Any?>).toMutableMap()
        who["speaker"] = (who["speaker"] as Map<String, Any?>) + ("end" to "failed")
        rec["who"] = who
        val failed = RecordUnits.of(listOf(requireNotNull(Wire.bind<LineRecord>(rec)))).single() as RenderUnit.Notice
        assertTrue(failed.failed)
    }

    @Test
    fun `认不出的那一条（后端认不出 · 手机这边解不开，核心都写成 unread）画成一张卡：字 · 钟面 · 摘录照核心写好的`() {
        assertEquals(
            listOf(RenderUnit.Unread("n-1", "无法识别的记录 · brand-new-kind", "01:31", """{"type":"brand-new-kind","uuid":"n-1","timestamp":"2026-10-09T01:31:30.000Z","x":1}""", "n-1")),
            units("unread-unknown"),
        )
        assertEquals("没有时刻 ⇒ 没有钟面", null, (units("unread-parse-failed").single() as RenderUnit.Unread).timeText)
    }

    @Test
    fun `增量排与整份排一样：一条一条接 · 先接后半再往前接前半，结果先到后到都配得上`() {
        val all = cases.values.toList()
        val whole = RecordUnits.of(all)
        val one = RecordUnits()
        all.forEach { one.append(listOf(it)) }
        assertEquals(whole, one.units())
        for (k in all.indices) {
            val split = RecordUnits()
            split.append(all.drop(k))
            split.prepend(all.take(k))
            assertEquals("从第 $k 条切开", whole, split.units())
        }
        val reset = RecordUnits().also { it.append(all) }
        reset.reset(all.take(2))
        assertEquals(RecordUnits.of(all.take(2)), reset.units())
    }

    @Test
    fun `丢最前面几条 ⇒ 同只排剩下那几条；结果那一条还在 ⇒ 往上翻读回调用那一条时照样配上`() {
        val reply = mapOf("agent" to "claude", "id" to "a", "t" to "reply", "autoReply" to false, "draw" to "steps", "human" to false, "blocks" to listOf(mapOf("type" to "tool_use", "id" to "call-1", "name" to "Edit", "input" to mapOf("file_path" to "/w/a.txt"))))
        val call = requireNotNull(Wire.bind<LineRecord>(reply))
        val rest = listOf(cases.getValue("said-full"), cases.getValue("said-min"))
        val body = RecordUnits().also { it.append(listOf(call) + rest) }
        body.dropFirst(1)
        assertEquals(RecordUnits.of(rest), body.units())
        body.prepend(listOf(call))
        assertEquals("读回的调用卡配上了还在手上的结果", false, (body.units().first() as RenderUnit.ToolCall).pending)
        assertEquals(RecordUnits.of(listOf(call) + rest), body.units())
        body.dropFirst(3)
        assertTrue(body.isEmpty())
    }

    @Test
    fun `没变就交上一份（同一个实例）；接一行只排这一行，旧单元原样留着`() {
        val body = RecordUnits().also { it.append(cases.values.toList()) }
        val first = body.units()
        assertTrue("没变 ⇒ 同一份", first === body.units())
        body.append(listOf(cases.getValue("said-min")))
        val next = body.units()
        assertTrue("变了 ⇒ 另一份", first !== next)
        assertEquals(first.size + 1, next.size)
        first.forEachIndexed { i, u -> assertTrue("第 $i 个单元原样留着", u === next[i]) }
    }

    @Test
    fun `人说的那一句带的图片与附件照收进单元（画在气泡外）；只有附件没有字也成一个单元`() {
        // 图片块只带引用（类型 · 字节数 · 哪一行 · 第几张）：单元里记下它与那一条的身份，字节画到那一格时另取。
        val image = mapOf("type" to "image", "mediaType" to "image/png", "bytes" to 3L, "at" to 120L, "n" to 0)
        val file = mapOf("name" to "login.css", "rel" to ".ccm-attach/1023-1529-7c01/login.css", "class" to "code", "classText" to "代码", "sizeText" to "1.3 KB")
        val gone = mapOf("name" to "old.pdf", "rel" to ".ccm-attach/1016-0900-aaaa/old.pdf", "class" to "pdf", "classText" to "PDF", "gone" to true)

        fun said(
            id: String,
            text: String,
        ) = requireNotNull(
            Wire.bind<LineRecord>(
                mapOf(
                    "agent" to "claude",
                    "id" to id,
                    "t" to "said",
                    "draw" to "card",
                    "human" to true,
                    "who" to mapOf("speaker" to mapOf("kind" to "human"), "text" to text),
                    "blocks" to listOf(image),
                    "attachments" to listOf(file, gone),
                ),
            ),
        )
        val u = RecordUnits.of(listOf(said("u-a", "截图在这"), said("u-b", ""))).map { it as RenderUnit.UserText }
        assertEquals(listOf(SaidImage("image/png", 120L, 0, "u-a")), u[0].images)
        assertEquals(listOf("login.css", "old.pdf"), u[0].attachments.map { it.name })
        assertEquals(listOf(null, true), u[0].attachments.map { it.gone })
        assertEquals("只有附件也成单元", "", u[1].text)
        assertEquals(2, u[1].attachments.size)
    }

    @Test
    fun `子运行记录里派活的那一句（agentTask）排成一张派活卡：字与时刻照记录`() {
        assertEquals(listOf(RenderUnit.Brief("u-said-agent-task", "go scan", "01:31", "u-said-agent-task")), units("said-agent-task"))
    }

    @Test
    fun `一轮跑着时插进去的那句也是人说的`() {
        assertTrue(units("queued").single() is RenderUnit.UserText)
    }

    private fun repoRoot(): File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
}
