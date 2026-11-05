package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.StreamItem
import com.ccmonitor.mobile.core.wire.gen.BranchRun
import com.ccmonitor.mobile.core.wire.gen.SessionBranch
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.reply
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * 聊天屏的回退框接线：记录路径知道了 ⇒ `history-branch` 照 `next` 一页页读完；流上这份记录的 `session_branch` 一帧换一截。
 * 框头是那台写好的 `said`，手机不分段、不数数；开合记在内存。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatBranchTest {
    private fun TestScope.existing(link: FakeCoreLink) = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }

    private fun row(
        end: Long,
        record: Map<String, Any?>,
    ) = mapOf("end" to end, "hash" to 0, "record" to record)

    private fun run(
        at: Int,
        n: Int,
        said: String,
    ) = mapOf("at" to at, "n" to n, "said" to said)

    /** 尾段 u1 · a1 · u2 · a2 · u3 · a3；a1 · u2 是一段回退掉的，a2 是另一段。清单分两页给。 */
    private fun link(): FakeCoreLink {
        val link = FakeCoreLink()
        link.report("s1", "/w", Faces.of("live-working-plain"))
        link.answer = { cmd, args ->
            when (cmd) {
                "history-tail" ->
                    Reply.Ok(
                        FakeCoreLink.tail(
                            600,
                            100,
                            6,
                            listOf(
                                row(150, human("u1", "问")),
                                row(200, reply("a1", "一")),
                                row(300, human("u2", "再问")),
                                row(400, reply("a2", "二")),
                                row(500, human("u3", "又问")),
                                row(600, reply("a3", "答")),
                            ),
                        ),
                    )
                "history-branch" ->
                    when (args?.get("from")) {
                        0L -> Reply.Ok(mapOf("off" to listOf("a1", "u2"), "runs" to listOf(run(0, 2, "已被 Esc 回退 · 消息 ×2")), "end" to 600, "next" to 2))
                        else -> Reply.Ok(mapOf("off" to listOf("a2"), "runs" to listOf(run(2, 1, "已被 Esc 回退 · 消息 ×1")), "end" to 600, "next" to null))
                    }
                else -> Reply.LinkDown
            }
        }
        return link
    }

    private fun ChatSession.shape() =
        state.value.stream.map {
            when (it) {
                is StreamItem.Card -> it.unit.key + (it.branch?.let { b -> "@$b" } ?: "")
                is StreamItem.Process -> it.key
                is StreamItem.Branch -> "[${it.said}${if (it.open) " open" else ""}]"
            }
        }

    @Test
    fun `冷读照 next 读完两页 ⇒ 两个框、框头是那两句，段里的卡收着`() =
        runTest {
            val link = link()
            val s = existing(link)
            runCurrent()
            assertEquals(listOf(0L, 2L), link.calls("history-branch").map { it?.get("from") })
            assertEquals(listOf("u1", "[已被 Esc 回退 · 消息 ×2]", "[已被 Esc 回退 · 消息 ×1]", "u3", "a3#0"), s.shape())
        }

    @Test
    fun `点开一个框 ⇒ 段里的卡接在框头后面、标着那一段；再点收起`() =
        runTest {
            val link = link()
            val s = existing(link)
            runCurrent()
            s.toggleBranch("a1")
            runCurrent()
            assertEquals(listOf("u1", "[已被 Esc 回退 · 消息 ×2 open]", "a1#0@a1", "u2@a1", "[已被 Esc 回退 · 消息 ×1]", "u3", "a3#0"), s.shape())
            s.toggleBranch("a1")
            runCurrent()
            assertEquals(listOf("u1", "[已被 Esc 回退 · 消息 ×2]", "[已被 Esc 回退 · 消息 ×1]", "u3", "a3#0"), s.shape())
        }

    @Test
    fun `流上一帧 at 落在第二段段首 ⇒ 第一段不动、只换第二段；别的记录的帧不收`() =
        runTest {
            val link = link()
            val s = existing(link)
            runCurrent()
            link.branch(SessionBranch("s1", "/p/other.jsonl", 0, emptyList(), emptyList(), "x"))
            link.branch(SessionBranch("s1", "/p/s1.jsonl", 2, listOf("a2", "u3"), listOf(BranchRun(2, 2, "已被 Esc 回退 · 消息 ×2 · 新")), "y"))
            runCurrent()
            assertEquals(listOf("u1", "[已被 Esc 回退 · 消息 ×2]", "[已被 Esc 回退 · 消息 ×2 · 新]", "a3#0"), s.shape())
        }
}
