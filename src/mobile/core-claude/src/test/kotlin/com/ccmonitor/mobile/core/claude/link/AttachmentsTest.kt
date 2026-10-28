package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.AttachRefused
import com.ccmonitor.mobile.core.wire.gen.FileClass
import com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReq
import com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReq
import com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReq
import com.ccmonitor.mobile.core.wire.gen.SessionQueuePutReq
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

/** 附件那三条的入参照生成的那几形（后端 `deny_unknown_fields`，多一格就拒）；回包经生成的类解；`session-queue-put` 有编号才带那一格。 */
class AttachmentsTest {
    private val staged = FilesCommitUpload.Staged("k", 3L, 9L, "ab".repeat(32))

    /** 解回生成的那一形、再出成线上那一份：与拼出来的逐格相等 ⇒ 不多一格、不少一格。 */
    private inline fun <reified T : Any> roundTrips(args: Map<String, Any?>) {
        val typed = Wire.bind<T>(args) ?: error("解不回 ${T::class.simpleName}: $args")
        assertEquals(args, Wire.args(typed))
    }

    @Test
    fun the_three_requests_have_exactly_the_generated_shape() {
        val check = SessionAttachmentCheck.args("s", listOf(AttachAsk("a.png", 5L), AttachAsk("b.txt", 7L)))
        roundTrips<SessionAttachmentCheckReq>(check)
        assertEquals(listOf("a.png", "b.txt"), (check["items"] as List<*>).map { (it as Map<*, *>)["name"] })

        val commit = SessionAttachmentCommit.args("s", "a.png", staged)
        roundTrips<SessionAttachmentCommitReq>(commit)
        assertEquals(3L, commit["chunks"])
        assertEquals(mapOf("sha256" to "ab".repeat(32)), commit["expect"])

        roundTrips<SessionAttachmentDropReq>(SessionAttachmentDrop.args("s", "1027-1200-ab12"))
    }

    @Test
    fun put_carries_ids_only_when_there_are_some() {
        assertFalse("attachments" in SessionQueuePut.args("s", "m", "hi", 1L))
        val with = SessionQueuePut.args("s", "m", "hi", 1L, listOf("x", "y"))
        assertEquals(listOf("x", "y"), with["attachments"])
        assertEquals(listOf("x", "y"), Wire.bind<SessionQueuePutReq>(with)?.attachments)
    }

    /** 发出去的那一行（同跨语言金样那一形，[Json.args]）：格序照生成的类，缺省的格不交。 */
    @Test
    fun the_lines_on_the_wire() {
        assertEquals("""{"sid":"s","items":[{"name":"a.png","bytes":5}]}""", Json.args(SessionAttachmentCheck.args("s", listOf(AttachAsk("a.png", 5L)))))
        assertEquals(
            """{"sid":"s","key":"k","name":"a.png","bytes":9,"chunks":3,"expect":{"sha256":"${"ab".repeat(32)}"}}""",
            Json.args(SessionAttachmentCommit.args("s", "a.png", staged)),
        )
        assertEquals("""{"sid":"s","id":"i"}""", Json.args(SessionAttachmentDrop.args("s", "i")))
        assertEquals(
            """{"machine":"<local>","sid":"s","id":"m","end":"mobile","atMs":1,"text":"hi","attachments":["i"]}""",
            Json.args(SessionQueuePut.args("s", "m", "hi", 1L, listOf("i"))),
        )
    }

    @Test
    fun replies_decode_through_the_generated_types() {
        val verdicts =
            SessionAttachmentCheck.of(
                mapOf(
                    "items" to
                        listOf(
                            mapOf("ok" to true, "class" to "image", "class_text" to "图"),
                            mapOf("ok" to false, "class" to "other", "class_text" to "文件", "why" to "too_big", "said" to "超上限"),
                        ),
                ),
            )!!
        assertEquals(listOf(true, false), verdicts.map { it.ok })
        assertEquals(AttachRefused.TOO_BIG, verdicts[1].why)

        val landed =
            SessionAttachmentCommit.of(
                mapOf("id" to "i", "name" to "a.png", "path" to "/w/.ccm-attach/i/a.png", "rel" to ".ccm-attach/i/a.png", "bytes" to 9.0, "sha256" to "ab", "class" to "image"),
            )!!
        assertEquals("i" to FileClass.IMAGE, landed.id to landed.`class`)
        assertNull(SessionAttachmentCommit.of(mapOf("id" to "i")))
    }
}
