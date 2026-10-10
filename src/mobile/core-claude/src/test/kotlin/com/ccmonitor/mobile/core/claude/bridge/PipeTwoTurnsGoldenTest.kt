package com.ccmonitor.mobile.core.claude.bridge

import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 常驻管道两轮对话的行为，用真机录下的素材钉住。
 *
 * 素材 `cli-two-turns.cli.ndjson` 是真跑 `ClaudeInvocation.pipeInvocation` 那条命令
 * 得到的 `out.ndjson`（两轮对话），不是 `claude -p` 单次。
 */
class PipeTwoTurnsGoldenTest {
    private val moshi = Moshi.Builder().build()
    private val anyAdapter = moshi.adapter(Any::class.java)

    private fun vectorsDir(): File =
        File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")

    private fun lines(): List<Map<*, *>> =
        File(vectorsDir(), "cli-two-turns.cli.ndjson")
            .readLines()
            .mapNotNull { anyAdapter.fromJson(it) as? Map<*, *> }
            .filterNot { it.containsKey("__meta__") }

    private fun frames(): List<BridgeFrame> {
        val enc = CliFrameEncoder()
        return lines().flatMap { enc.feed(it) }
    }

    /**
     * 进程常驻：两轮同一个 `session_id` ⇒ 没重启、没重载历史。
     *
     * 这是选常驻管道而不是「每条消息 `claude -p --resume`」的理由（后者在 194MB 历史下首字 21.9 秒）。
     */
    @Test
    fun bothTurnsShareOneSessionSoTheProcessIsTrulyPersistent() {
        val sids = lines().mapNotNull { it["session_id"] as? String }.toSet()
        assertEquals("两轮必须是同一个会话：$sids", 1, sids.size)
        assertEquals("确实是两轮", 2, frames().filterIsInstance<BridgeFrame.Result>().size)
    }

    /**
     * 管道不回显 ⇒ `PipeUplinkSink.echoesBack = false`。真管道两轮里零条 user。
     */
    @Test
    fun thePersistentPipeNeverEchoesTheUserMessage() {
        assertFalse("真管道两轮都没有 user 回显", lines().any { it["type"] == "user" })
        assertTrue("⇒ 上层必须乐观回显", frames().filterIsInstance<BridgeFrame.UserText>().isEmpty())
    }

    /**
     * 第二轮没有 hook 帧，但仍然有 `init`：每轮都发 `init`，
     * 所以断线重连、或从 offset 中途接上，下一轮就能拿回 catalog。
     */
    @Test
    fun everyTurnCarriesItsOwnInitSoCatalogSurvivesReconnect() {
        val inits = frames().filterIsInstance<BridgeFrame.Init>()
        assertEquals("每轮一个 init", 2, inits.size)
        assertTrue("catalog 不许空：命令选择器建在它上面", inits.all { it.tools.isNotEmpty() })
        // 第二轮没有 hook 帧（第一轮有 hook_started/hook_response）
        val hookEvents = frames().filterIsInstance<BridgeFrame.Event>().filter { it.k?.startsWith("system:hook") == true }
        assertEquals("hook 帧只在第一轮", 2, hookEvents.size)
    }

    /** 两轮正文都对，且块下标不跨消息撞键（每轮各自从 0 开始）。 */
    @Test
    fun bothTurnsDecodeTheirOwnTextWithoutKeyCollision() {
        val texts = frames().filterIsInstance<BridgeFrame.AssistantText>()
        assertEquals(listOf("hello", "world"), texts.map { it.x })
        assertEquals("每轮首块都是 0", listOf(0, 0), texts.map { it.i })
        assertEquals("两轮的 message id 必须不同，否则会互相覆盖", 2, texts.mapNotNull { it.m }.toSet().size)
    }

    /**
     * 第二轮开始时 `turn` 必须退回 Streaming：用真两轮素材过一遍装配器。
     *
     * 否则常驻管道从第 2 轮起，界面谎报「这轮结束了」、`streaming` 恒 false ⇒ 停止键不再出现，
     * 而远端其实正在生成。单轮素材看不出来，只有两轮的素材能钉住。
     */
    @Test
    fun theSecondTurnReopensTheTurnStateInsteadOfStayingDone() {
        val fs = frames()
        val firstRes = fs.indexOfFirst { it is BridgeFrame.Result }
        assertTrue("前提：素材里真有两轮", firstRes >= 0 && fs.drop(firstRes + 1).any { it is BridgeFrame.Result })

        val asm = ChatTurnAssembler()
        fs.take(firstRes + 1).forEach { asm.feed(it) }
        assertTrue("第一轮收口后应是 Done", asm.turn is TurnState.Done)

        // 喂到第二轮第一条内容帧为止（不含它的 res）
        val rest = fs.drop(firstRes + 1)
        val secondRes = rest.indexOfFirst { it is BridgeFrame.Result }
        rest.take(secondRes).forEach { asm.feed(it) }
        assertTrue("第二轮一开始 turn 就该退回 Streaming，否则停止键再也不出现", asm.turn is TurnState.Streaming)

        rest.take(secondRes + 1).forEach { asm.feed(it) }
        assertTrue("第二轮也要能正常收口", asm.turn is TurnState.Done)
    }

    /** 脱敏：golden 里不许有真实用户名/家目录。 */
    @Test
    fun theGoldenIsRedacted() {
        val raw = File(vectorsDir(), "cli-two-turns.cli.ndjson").readText()
        val home = System.getProperty("user.home").orEmpty()
        val user = home.substringAfterLast('/')
        assertTrue("不许残留家目录", home.isEmpty() || !raw.contains(home))
        assertTrue("不许残留用户名", user.isEmpty() || !raw.contains(Regex("\\b${Regex.escape(user)}\\b")))
    }
}
