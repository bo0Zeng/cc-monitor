package com.ccmonitor.mobile.core.claude.catalog

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [CodexSessionCatalog] 端到端（fake channel）——日期分区发现 + 末尾 UUID sessionId + mtime 兜底（无 pidfile）
 * + 标题=首条真实用户消息（跳注入）。fixtures 按真 ~/.codex rollout 路径与 envelope 形状。
 */
class CodexSessionCatalogTest {
    private val rs = ClaudeSessionCatalog.CHUNK_MARKER
    private val p1 = "/h/.codex/sessions/2026/07/18/rollout-2026-07-18T22-45-32-019f78e8-84dd-7ac0-b479-e9c1b9caec67.jsonl"
    private val p2 = "/h/.codex/sessions/2026/07/17/rollout-2026-07-17T10-00-00-019f7000-1111-7000-a000-000000000000.jsonl"

    private fun userLine(text: String) =
        """{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"$text"}]}}"""

    @Test fun scanDiscoversRolloutsWithTrailingUuidNoPidfile() =
        runTest {
            var lsCmd: String? = null
            val channel =
                RemoteCommandChannel { cmd ->
                    if (cmd.startsWith("ls -t")) {
                        lsCmd = cmd
                        flowOf("$p1\n$p2\n".toByteArray())
                    } else {
                        emptyFlow()
                    }
                }
            val scan = CodexSessionCatalog(channel, "/h/.codex").scanLiveSessions(cwd = null)
            assertTrue("发现命令走 sessions 日期 glob：$lsCmd", lsCmd!!.contains("/sessions/*/*/*/rollout-*.jsonl*"))
            assertEquals(2, scan.refs.size)
            assertEquals("sessionId=末尾 UUID", "019f78e8-84dd-7ac0-b479-e9c1b9caec67", scan.refs[0].sessionId)
            assertEquals("Codex 无 pidfile → live=false", false, scan.refs[0].live)
            assertEquals("最新在前（ls -t 序保持）", p1, scan.refs[0].path)
            assertTrue("无 pidfile 探测可降级 → probeOk", scan.probeOk)
        }

    @Test fun labeledTitleIsFirstRealUserSkippingInjection() =
        runTest {
            val probe =
                "$rs$p1\n" +
                    userLine("<environment_context> cwd=/x </environment_context>") + "\n" + // 注入 → isMeta 跳过
                    userLine("帮我修一个登录 bug") + "\n" // 首条真实
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.startsWith("ls -t") -> flowOf("$p1\n".toByteArray())
                        cmd.startsWith("for f in") -> flowOf(probe.toByteArray())
                        else -> emptyFlow()
                    }
                }
            val cat = CodexSessionCatalog(channel, "/h/.codex")
            val labeled = cat.labeledSessions(cat.liveSessionRefs(null))
            assertEquals(1, labeled.size)
            assertEquals("标题=首条真实用户消息（跳过注入上下文）", "帮我修一个登录 bug", labeled[0].title)
        }

    @Test fun labeledFallsBackToSid8WhenProbeEmpty() =
        runTest {
            val channel =
                RemoteCommandChannel { cmd ->
                    if (cmd.startsWith("ls -t")) flowOf("$p1\n".toByteArray()) else emptyFlow() // 探针无输出
                }
            val cat = CodexSessionCatalog(channel, "/h/.codex")
            val labeled = cat.labeledSessions(cat.liveSessionRefs(null))
            assertEquals("无真实用户消息 → sid8 兜底", "019f78e8", labeled[0].title)
        }

    @Test fun labeledDegradesOnProbeFailureButRethrowsCancellation() =
        runTest {
            // 探针（非 CE）异常 → 该片降级 sid8、不上抛（labeledSessions 仍返回）。CE 重抛由 try/catch 结构保证（与 Claude 侧同）。
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.startsWith("ls -t") -> flowOf("$p1\n".toByteArray())
                        else -> flow<ByteArray> { throw java.io.IOException("探针断") }
                    }
                }
            val cat = CodexSessionCatalog(channel, "/h/.codex")
            val labeled = cat.labeledSessions(cat.liveSessionRefs(null))
            assertEquals("探针失败 → sid8 兜底（不上抛）", "019f78e8", labeled[0].title)
        }
}
