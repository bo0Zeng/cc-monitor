package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.SessionState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 会话里找 · 大纲 · 行号换起点 · 结束 · 自动答：入参照协议的格，回包照线上那一形解（结构夹具，手拼）。 */
class SessionFindTest {
    @Test
    fun `history-find：交路径与查询串，别的格不带（那台照缺省：工具结果不搜 · 500 条）`() {
        assertEquals(mapOf("path" to "/p/s.jsonl", "query" to "q"), InSessionFind.args("/p/s.jsonl", "q", user = false))
        assertEquals("只搜用户 ⇒ 带 scope user", mapOf("path" to "/p/s.jsonl", "query" to "q", "scope" to "user"), InSessionFind.args("/p/s.jsonl", "q", user = true))
    }

    @Test
    fun `history-find 回包：命中带行号 · 落在哪次工具调用里；缺必填格 ⇒ 解不出`() {
        val hit = mapOf("uuid" to "u1", "line" to 42L, "kind" to "assistant", "before" to "a ", "matched" to "q", "after" to " b", "turn" to 3L, "tsMs" to 0L, "tsText" to "10:00", "tool" to "c1")
        val got = InSessionFind.of(mapOf("total" to 7L, "hits" to listOf(hit), "head" to "前 1 · 共 7", "widen" to false))!!
        assertEquals("前 1 · 共 7", got.head)
        assertEquals(42L, got.hits.single().line)
        assertEquals("c1", got.hits.single().tool)
        assertNull(InSessionFind.of(mapOf("hits" to emptyList<Any>())))
    }

    @Test
    fun `history-user-inputs：只交路径；回包每一条带字节起点与摘要`() {
        assertEquals(mapOf("path" to "/p/s.jsonl"), SaidList.args("/p/s.jsonl"))
        val row = mapOf("at" to 120L, "line" to 12L, "uuid" to "u1", "timestamp" to "", "excerpt" to "第一句")
        val got = SaidList.of(mapOf("from" to 0L, "end" to 900L, "next" to null, "entries" to listOf(row)))!!
        assertEquals(120L, got.entries.single().at)
        assertEquals("第一句", got.entries.single().excerpt)
    }

    @Test
    fun `行号换起点：问一行都不交的那一段，起点是回包的 end`() {
        assertEquals(mapOf("path" to "/p/s.jsonl", "from" to 7L, "until" to 7L), LineStart.args("/p/s.jsonl", 7L))
        assertEquals(70L, LineStart.of(mapOf("from" to 7L, "next" to 7L, "eof" to false, "end" to 70L, "lines" to emptyList<Any>())))
        assertNull(LineStart.of(mapOf("from" to 7L)))
    }

    @Test
    fun `sessions-stop：交那几个 sid，自报是手机；回包逐条带结局与那台那一句`() {
        assertEquals(mapOf("sids" to listOf("s"), "client" to "mobile"), SessionsStop.args(listOf("s")))
        val one = mapOf("machine" to "local", "sid" to "s", "outcome" to "failed", "why" to "not_found", "said" to "那台那一句", "copyDetail" to "", "title" to "t", "session" to null)
        val got = SessionsStop.of(mapOf("results" to listOf(one)))!!.results.single()
        assertEquals(SessionState.FAILED, got.outcome)
        assertEquals("那台那一句", got.said)
    }

    @Test
    fun `自动答那一格：交 patch_autoAnswer_afterMin（0 ＝ 关），那一句不带`() {
        assertEquals(mapOf("sid" to "s", "patch" to mapOf("autoAnswer" to mapOf("afterMin" to 5))), HistoryAnnotate.autoAnswer("s", 5))
        assertEquals(mapOf("sid" to "s", "patch" to mapOf("autoAnswer" to mapOf("afterMin" to 0))), HistoryAnnotate.autoAnswer("s", 0))
    }
}
