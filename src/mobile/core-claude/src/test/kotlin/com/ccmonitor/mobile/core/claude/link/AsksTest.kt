package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 答题口的成品（`needs.ask`）与答那一条（`needs-answer`）：手机只照抄核心那一份，一个字不拼。
 * 读的是跨语言金样 `needs-ask.golden.json`（后端真序列化器写的，桌面同读一份）。
 */
class AsksTest {
    private val golden = Fixtures.json("needs-ask.golden.json")

    private fun ask(name: String) = NeedsAsk.of(golden[name])!!

    @Test
    fun `四种请求原样收下：选项的字 · 次序 · 附话要求 · 题目都照核心`() {
        val approve = ask("approve")
        assertEquals("approve", approve.kind)
        assertEquals("touch a.txt\necho ok", approve.detail)
        assertEquals(listOf("allow", "always-0", "always-1", "deny", "deny-note", "deny-stop"), approve.choices.map { it.id })
        assertEquals("允许，且允许访问 /w（本次会话）", approve.choices[1].text)
        assertEquals(NeedsNote.REQUIRED, approve.choices.single { it.id == "deny-note" }.note)
        assertNull(approve.run)

        val plan = ask("plan")
        assertEquals(NeedsNote.OPTIONAL, plan.choices.last().note)

        val q = ask("question")
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
        assertNull(NeedsAsk.of(bad))
        val badChoice =
            (golden["approve"] as Map<*, *>).toMutableMap().apply {
                put("choices", listOf(mapOf("id" to "allow", "text" to "允许", "tone" to "plain", "note" to "sometimes")))
            }
        assertNull("附话要求是闭集：认不出 ⇒ 解不出", NeedsAsk.of(badChoice))
    }

    @Test
    fun `sessions-needs 一条带 ask · head · headCode；ask 为空时带 manualText`() {
        val data =
            mapOf(
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
            NeedsAnswer.args("s1", "ask-4", "answer", picks = mapOf("q0" to NeedsPick(setOf("q0o0"), ""), "q1" to NeedsPick(emptySet(), "Kiwi"))),
        )
    }

    @Test
    fun `别处先答了的两个码（核心那一句照抄）；别的码不算`() {
        assertTrue(NeedsAnswer.gone("already_answered"))
        assertTrue(NeedsAnswer.gone("closed_elsewhere"))
        assertEquals(false, NeedsAnswer.gone("bad_choice"))
    }

    @Test
    fun `changed 帧：主题与键；别的帧不是`() {
        assertEquals(Changed("needs", "s1"), Changed.of(mapOf("kind" to "changed", "topic" to "needs", "key" to "s1")))
        assertEquals(Changed("manual", null), Changed.of(mapOf("kind" to "changed", "topic" to "manual")))
        assertNull(Changed.of(mapOf("kind" to "session_status", "topic" to "needs")))
        assertNull("缺主题 ⇒ 不是", Changed.of(mapOf("kind" to "changed")))
    }
}
