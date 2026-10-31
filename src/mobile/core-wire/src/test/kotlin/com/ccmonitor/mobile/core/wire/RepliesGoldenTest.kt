package com.ccmonitor.mobile.core.wire

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.QueuePutOutcome
import com.ccmonitor.mobile.core.wire.gen.SessionQueuePutReq
import com.squareup.moshi.JsonAdapter
import com.squareup.moshi.JsonReader
import com.squareup.moshi.JsonWriter
import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

/**
 * 手机读的每条回包：Rust 照类型写的那一行（跨语言金样 `wire-replies.golden.json` 的 `rust` 格，`cmd-wire` 那边现打）
 * 经生成的 Kotlin 类解、再写回，与它逐字节同 —— 生成的类与 Rust 那一份对不上（少一格 · 型不对 · 顺序不同 · null 与缺席分不清）就红在这里。
 */
class RepliesGoldenTest {
    @Test
    fun everyPhoneReplyRoundTripsThroughItsGeneratedTypeByteForByte() {
        val cases = rustLines(File(repoRoot(), "tests/__fixtures__/wire-replies.golden.json").readText())
        assertEquals("命令表与金样里的命令不是同一组", cases.keys.toSortedSet(), Commands.REPLIES.keys.toSortedSet())
        for (cmd in Commands.REPLIES.keys.sorted()) {
            val rust = cases.getValue(cmd)

            @Suppress("UNCHECKED_CAST")
            val ad = Commands.REPLIES.getValue(cmd) as JsonAdapter<Any?>
            val typed = ad.fromJson(JsonReader.of(Buffer().writeUtf8(rust)))
            val out = Buffer()
            ad.toJson(JsonWriter.of(out), typed)
            assertEquals("$cmd 经生成的类写回与 Rust 那一行不同", rust, out.readUtf8())
        }
    }

    /** 生成的命令表与桌面那一份（`TypedCommands.ts`）是同一组命令：桌面有的，手机都有名字 ＋ 入参类型 ＋ 回包类型。 */
    @Test
    fun everyDesktopCommandHasAGeneratedOne() {
        val desktop =
            Regex("""^\s*"([a-z0-9-]+)": \{ req:""", RegexOption.MULTILINE)
                .findAll(File(repoRoot(), "src/frontend/ui/generated/TypedCommands.ts").readText())
                .map { it.groupValues[1] }
                .toSortedSet()
        assertEquals("生成的命令表与 TypedCommands 不是同一组", desktop, Commands.ALL.keys.toSortedSet())
        for ((name, c) in Commands.ALL) assertEquals("$name 登记的名字", name, c.name)
    }

    /** 入参照生成的类出：可缺的格缺着就不交；回包照生成的类解，缺一格 ⇒ `null`。 */
    @Test
    fun queuePutArgsAndReplyComeFromTheGeneratedTypes() {
        val c = Commands.SessionQueuePut
        assertEquals("session-queue-put", Commands.SessionQueuePut.NAME)
        val bare = c.args(SessionQueuePutReq(machine = "<local>", sid = "s1", id = "o1", end = "mobile", atMs = 7, text = "hi"))
        assertEquals(mapOf("machine" to "<local>", "sid" to "s1", "id" to "o1", "end" to "mobile", "atMs" to 7L, "text" to "hi"), bare)
        val attached = c.args(SessionQueuePutReq(machine = "<local>", sid = "s1", id = "o1", end = "mobile", atMs = 7, text = "hi", attachments = listOf("a")))
        assertEquals(listOf("a"), attached["attachments"])
        assertEquals(QueuePutOutcome.QUEUED, c.reply(mapOf("outcome" to "queued", "said" to ""))?.outcome)
        assertEquals(null, c.reply(mapOf("outcome" to "queued")))
    }

    /** 金样里每条命令的 `rust` 那一行（`sample` 不看）。 */
    private fun rustLines(text: String): Map<String, String> {
        val out = mutableMapOf<String, String>()
        val r = JsonReader.of(Buffer().writeUtf8(text))
        r.beginObject()
        while (r.hasNext()) {
            if (r.nextName() != "cases") {
                r.skipValue()
                continue
            }
            r.beginObject()
            while (r.hasNext()) {
                val cmd = r.nextName()
                r.beginObject()
                while (r.hasNext()) {
                    if (r.nextName() == "rust") out[cmd] = r.nextString() else r.skipValue()
                }
                r.endObject()
            }
            r.endObject()
        }
        r.endObject()
        return out
    }

    private fun repoRoot(): File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
}
