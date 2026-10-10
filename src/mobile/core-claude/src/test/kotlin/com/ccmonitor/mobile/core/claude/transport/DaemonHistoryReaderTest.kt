package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.catalog.ClaudeSessionCatalog
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 会话血统：一条有界的批量探针命令拿回一批文件的 `forkedFrom`。 */
class DaemonHistoryReaderTest {
    private class FakeChannel(
        private val reply: (String) -> String,
    ) : RemoteCommandChannel {
        val commands = mutableListOf<String>()

        override fun exec(command: String) = flowOf(reply(command).toByteArray()).also { commands += command }
    }

    /** 造一段探针输出：每个文件一个 RS 段，`forks` 里有的再加一行血统片段。 */
    private fun probeReply(
        paths: List<String>,
        forks: Map<String, String>,
    ): String =
        buildString {
            for (p in paths) {
                append(ClaudeSessionCatalog.CHUNK_MARKER).append(p).append('\n')
                // 注意：`grep -a -m 1 -o` 吐的是片段，不带外层花括号；造成完整记录行的话会被当成 JSONL 记录解析。
                forks[p]?.let { append("\"forkedFrom\":{\"sessionId\":\"$it\",\"messageUuid\":\"u\"}").append('\n') }
            }
        }

    /** 一条命令拿回一批。 */
    @Test
    fun oneCommandBringsBackTheWholeBatch() =
        runTest {
            val paths = (1..5).map { "/p/s$it.jsonl" }
            val ch = FakeChannel { probeReply(paths, mapOf("/p/s2.jsonl" to "root", "/p/s5.jsonl" to "root")) }
            val found = DaemonHistoryReader(ch).parentsFor((1..5).associate { "s$it" to "/p/s$it.jsonl" })
            assertEquals("五个会话只该发一次", 1, ch.commands.size)
            assertEquals(mapOf("s2" to "root", "s5" to "root"), found)
        }

    /**
     * 命令形状守三条：`-o` 片段、`cut` 上限、不许出现 `tail`（tail 管线按子串分派命令，会撞）。
     */
    @Test
    fun theCommandUsesFragmentsCapsLinesAndAvoidsTail() =
        runTest {
            val ch = FakeChannel { probeReply(listOf("/p/a.jsonl"), emptyMap()) }
            DaemonHistoryReader(ch).parentsFor(mapOf("a" to "/p/a.jsonl"))
            val cmd = ch.commands.single()
            assertTrue("要抓血统片段：$cmd", cmd.contains("""grep -a -m 1 -o '"forkedFrom":{[^}]*}'"""))
            assertTrue("每行要封上界", cmd.contains("cut -c -${ClaudeSessionCatalog.PROBE_LINE_CAP}"))
            assertFalse("命令里不许出现 tail：tail 管线按子串分派命令，会撞", cmd.contains("tail"))
        }

    /** 单命令探针只在 ≤15 个文件时安全 ⇒ 必须分片。 */
    @Test
    fun theBatchIsChunkedBecauseOneCommandIsOnlySafeForAFewFiles() =
        runTest {
            val n = ClaudeSessionCatalog.HISTORY_TITLE_CHUNK * 2 + 1
            val ch = FakeChannel { cmd -> probeReply(Regex("'(/p/[^']+)'").findAll(cmd).map { it.groupValues[1] }.toList(), emptyMap()) }
            DaemonHistoryReader(ch).parentsFor((1..n).associate { "s$it" to "/p/s$it.jsonl" })
            assertEquals("$n 个文件要切成 3 片", 3, ch.commands.size)
        }

    /**
     * RS 标记行本身就是正向成功证据：段出现了就证明命令跑到了那个文件，
     * 段里没有血统片段 = 真的没有血统。没出现段 = 探针失败，不许缓存成「没有血统」。
     */
    @Test
    fun aFileWithNoSectionIsRetriedWhileOneWithAnEmptySectionIsNot() =
        runTest {
            // 只回 a 的段，b 整个没出现（模拟命令中途失败/被截断）
            val ch = FakeChannel { probeReply(listOf("/p/a.jsonl"), emptyMap()) }
            val reader = DaemonHistoryReader(ch)
            val ask = mapOf("a" to "/p/a.jsonl", "b" to "/p/b.jsonl")
            reader.parentsFor(ask)
            reader.parentsFor(ask)
            val asked = ch.commands.map { c -> Regex("'(/p/[^']+)'").findAll(c).map { it.groupValues[1] }.toList() }
            assertTrue("a 有段 ⇒ 确认没有血统 ⇒ 第二次不该再问它", asked[1].none { it.endsWith("a.jsonl") })
            assertTrue("b 没段 ⇒ 探针失败 ⇒ 第二次必须重问", asked[1].any { it.endsWith("b.jsonl") })
        }

    /** 自指的 `forkedFrom` 是已知缺陷（uuid 不 remap ⇒ 溯源链自指）⇒ 当没有父。 */
    @Test
    fun aSelfReferencingForkedFromIsTreatedAsNoParent() =
        runTest {
            val ch = FakeChannel { probeReply(listOf("/p/me.jsonl"), mapOf("/p/me.jsonl" to "me")) }
            assertTrue(DaemonHistoryReader(ch).parentsFor(mapOf("me" to "/p/me.jsonl")).isEmpty())
        }

    /** 整片失败（exec 抛异常）⇒ 不缓存，下次重试。 */
    @Test
    fun aFailedChunkIsNotCached() =
        runTest {
            var first = true
            val ch =
                FakeChannel {
                    if (first) {
                        first = false
                        throw java.io.IOException("断了")
                    }
                    probeReply(listOf("/p/a.jsonl"), mapOf("/p/a.jsonl" to "root"))
                }
            val reader = DaemonHistoryReader(ch)
            assertTrue(reader.parentsFor(mapOf("a" to "/p/a.jsonl")).isEmpty())
            assertEquals(mapOf("a" to "root"), reader.parentsFor(mapOf("a" to "/p/a.jsonl")))
        }
}
