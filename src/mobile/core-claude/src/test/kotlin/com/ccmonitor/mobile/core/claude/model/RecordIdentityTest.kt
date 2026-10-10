package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** [withUuid] 为 Codex 无 uuid 记录补合成 id（unitKey 唯一），逐子型返副本、Title 原样。 */
class RecordIdentityTest {
    @Test fun userAndAssistantGetUuidPreservingOtherFields() {
        val u = JsonlRecord.User(null, null, listOf(ContentBlock.Text("hi")), isMeta = false, timestamp = "t", sessionId = "s", cwd = "/w")
        val u2 = u.withUuid("cx-0") as JsonlRecord.User
        assertEquals("cx-0", u2.uuid)
        assertEquals("其它字段不变", "/w", u2.cwd)
        assertEquals(listOf(ContentBlock.Text("hi")), u2.blocks)

        val a = JsonlRecord.Assistant("orig", null, emptyList(), "m", "t", "s", false, "end_turn")
        assertEquals("已有 uuid 也能覆盖（消费方只对 null 调）", "cx-1", a.withUuid("cx-1").uuid)
    }

    @Test fun unknownGetsUuidKeepingRawJsonAndType() {
        val unk = JsonlRecord.Unknown("event_msg", timestamp = "t", rawJson = "{}")
        val got = unk.withUuid("cx-2") as JsonlRecord.Unknown
        assertEquals("cx-2", got.uuid)
        assertEquals("event_msg", got.type)
        assertEquals("{}", got.rawJson)
    }

    @Test fun titleUnchangedBecauseUuidIsNullGetter() {
        // Title 的 uuid 是恒 null 的 getter（无字段）→ 无法赋值、原样返回（它不参与渲染键）。
        val t = JsonlRecord.Title("标题", "s")
        val got = t.withUuid("cx-3")
        assertEquals(t, got)
        assertNull(got.uuid)
    }
}
