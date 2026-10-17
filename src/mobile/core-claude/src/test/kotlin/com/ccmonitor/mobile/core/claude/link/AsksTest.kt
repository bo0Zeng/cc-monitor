package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.AnsweredRow
import com.ccmonitor.mobile.core.claude.link.wire.AskKind
import com.ccmonitor.mobile.core.claude.link.wire.NeedsAsk
import com.ccmonitor.mobile.core.claude.link.wire.NeedsNote
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 答题口的成品（`needs.ask`）与答那一条（`needs-answer`）：手机装进线上类型（`wire/WireTypes.kt`，对着 `cmd-wire` 导出的 TS 核），
 * 一个字不拼、不另写解码。读的是跨语言金样 `needs-ask.golden.json`（后端真序列化器写的，桌面同读一份）。
 */
class AsksTest {
    private val golden = Fixtures.json("needs-ask.golden.json")

    private fun ask(name: String) = Wire.bind<NeedsAsk>(golden[name])!!

    @Test
    fun `四种请求原样收下：选项的字 · 次序 · 附话要求 · 题目都照核心`() {
        val approve = ask("approve")
        assertEquals(AskKind.APPROVE, approve.kind)
        assertEquals("touch a.txt\necho ok", approve.detail)
        assertEquals(listOf("allow", "always-0", "always-1", "deny", "deny-note", "deny-stop"), approve.choices.map { it.id })
        assertEquals("允许，且允许访问 /w（本次会话）", approve.choices[1].text)
        assertEquals(NeedsNote.REQUIRED, approve.choices.single { it.id == "deny-note" }.note)
        assertNull(approve.run)

        val plan = ask("plan")
        assertEquals(AskKind.PLAN, plan.kind)
        assertEquals(NeedsNote.OPTIONAL, plan.choices.last().note)

        val q = ask("question")
        assertEquals(AskKind.QUESTION, q.kind)
        assertEquals(listOf("q0", "q1"), q.questions.map { it.id })
        assertEquals(false, q.questions[0].multi)
        assertEquals(true, q.questions[1].multi)
        assertEquals("warm", q.questions[0].options[0].detail)
        assertEquals("Color", q.questions[0].header)

        val sub = ask("subAgent")
        assertTrue("子 agent 那一趟带 run", sub.run != null)
        assertEquals("release", sub.choices.last().id)
    }

    @Test
    fun `缺必填格 ⇒ 整份解不出`() {
        val bad = (golden["approve"] as Map<*, *>).toMutableMap().apply { remove("choices") }
        assertNull(Wire.bind<NeedsAsk>(bad))
        val badChoice =
            (golden["approve"] as Map<*, *>).toMutableMap().apply {
                put("choices", listOf(mapOf("id" to "allow", "text" to "允许", "tone" to "plain", "note" to "sometimes")))
            }
        assertNull("附话要求是闭集：认不出 ⇒ 解不出", Wire.bind<NeedsAsk>(badChoice))
    }

    @Test
    fun `sessions-needs 一条带 ask · head · headCode；ask 为空时带 manualText`() {
        val data =
            mapOf(
                "answered" to listOf(mapOf("sid" to "s9", "said" to "10 分钟没人答，已自动答：继续")),
                "waiting" to
                    listOf(
                        mapOf(
                            "sid" to "s1",
                            "needs" to
                                mapOf(
                                    "kind" to "approve",
                                    "text" to "等批准",
                                    "head" to "等批准 · Bash",
                                    "headCode" to "touch a.txt",
                                    "tone" to "need",
                                    "rank" to 0.0,
                                    "waitedMs" to 12_000.0,
                                    "waitedText" to "12s",
                                    "ask" to golden["approve"],
                                    "manualText" to null,
                                ),
                        ),
                        mapOf(
                            "sid" to "s2",
                            "needs" to
                                mapOf(
                                    "kind" to "unknown",
                                    "text" to "需手动",
                                    "head" to "需手动",
                                    "headCode" to null,
                                    "tone" to "need",
                                    "rank" to 1.0,
                                    "ask" to null,
                                    "manualText" to "只能在终端里答",
                                ),
                        ),
                    ),
            )
        val rows = SessionsNeeds.of(data)!!
        assertEquals(listOf(AnsweredRow("s9", "10 分钟没人答，已自动答：继续")), SessionsNeeds.list(data)!!.answered)
        assertEquals("等批准 · Bash", rows[0].needs.head)
        assertEquals("touch a.txt", rows[0].needs.headCode)
        assertEquals("ask-1", rows[0].needs.ask?.id)
        assertNull(rows[0].needs.manualText)
        assertNull(rows[1].needs.ask)
        assertEquals("只能在终端里答", rows[1].needs.manualText)
    }

    @Test
    fun `needs-answer 的入参：附话与 picks 只在给了时才交`() {
        assertEquals(mapOf("sid" to "s1", "ask" to "ask-1", "choice" to "allow"), NeedsAnswer.args("s1", "ask-1", "allow"))
        assertEquals(
            mapOf("sid" to "s1", "ask" to "ask-1", "choice" to "deny-note", "note" to "别动 build/"),
            NeedsAnswer.args("s1", "ask-1", "deny-note", note = "别动 build/"),
        )
        assertEquals(
            mapOf(
                "sid" to "s1",
                "ask" to "ask-4",
                "choice" to "answer",
                "picks" to mapOf("q0" to mapOf("options" to listOf("q0o0"), "text" to ""), "q1" to mapOf("options" to emptyList<String>(), "text" to "Kiwi")),
            ),
            NeedsAnswer.args("s1", "ask-4", "answer", picks = mapOf("q0" to NeedsPick(setOf("q0o0"), ""), "q1" to NeedsPick(emptySet(), " Kiwi "))),
        )
    }

    @Test
    fun `别处先答了的两个码（核心那一句照抄）；别的码不算`() {
        assertTrue(NeedsAnswer.gone("already_answered"))
        assertTrue(NeedsAnswer.gone("closed_elsewhere"))
        assertEquals(false, NeedsAnswer.gone("bad_choice"))
    }

    /**
     * 需手动那一族（`sessions-needs` 一行 · `needs.ask` · `needs-answer` 的入参与回包）在手机里只有线上类型那一份：
     * 别处不再声明同名的类，答题口那份源里也没有按键取格的手写解码。
     */
    @Test
    fun `需手动那一族只在线上类型里声明一次，没有手写解码`() {
        val sources =
            File(Fixtures.repoRoot, "src/mobile")
                .walk()
                .filter { it.isFile && it.extension == "kt" && "/src/main/" in it.path && "/build/" !in it.path }
                .toList()
        assertTrue("一份源都没找到 —— 本条在空转", sources.size > 20)
        val decl = Regex("""\bclass (${FAMILY.joinToString("|")})\b""")
        val where = sources.flatMap { f -> decl.findAll(f.readText()).map { it.groupValues[1] to f.name }.toList() }
        val outside = where.filter { it.second != "WireTypes.kt" }
        assertTrue("线上类型之外又声明了：$outside", outside.isEmpty())
        assertEquals("每个都在线上类型里", FAMILY.toSet(), where.map { it.first }.toSet())
        val asks = sources.single { it.name == "Asks.kt" }.readText()
        val hand = Regex("""\.(str|num|bool|obj|objs)\(""").findAll(asks).map { it.value }.toList()
        assertTrue("答题口那份源里有按键取格的手写解码：$hand", hand.isEmpty())
    }

    private companion object {
        val FAMILY =
            listOf("NeedsRow", "Needs", "NeedsAsk", "NeedsAskRun", "NeedsChoice", "NeedsNote", "NeedsQuestion", "NeedsQuestionOption", "AnswerReq", "AskPick", "Answered")
    }
}
