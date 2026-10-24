package com.ccmonitor.mobile.core.claude.link.wire

import com.ccmonitor.mobile.core.claude.link.wire.gen.Commands
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
