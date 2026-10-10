package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.ShellWord
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 打开一条已有对话时把历史读出来。
 *
 * 素材照真实记录的形状造：磁盘上的会话记录（`<配置目录>/projects/<编码后的 cwd>/<编号>.jsonl`）有十来种 `type`
 * （`assistant` / `user` / `mode` / `permission-mode` / `ai-title` / `attachment` / `file-history-delta` /
 * `queue-operation` / `system` / `last-prompt` …），其中只有 `assistant`/`user` 带 `message`，
 * 形如 `{id, role, content:[{type:text|thinking|tool_use…}]}`，与 CLI 行同形。
 *
 * 仓里那份 `sample.jsonl` 是 gitignored 的本地素材（CI 上必然 skip），判据不建在它上面。
 */
class ChatHistorySourceTest {
    /** 假通道不跑命令，路径只要是个词就行。真 shell 下读不读得到，见 `ChatHistoryRealShellTest`。 */
    private val recordWord = ShellWord.literal("/rec.jsonl")

    private class FakeChannel(
        private val replies: MutableList<String>,
    ) : RemoteCommandChannel {
        val commands = mutableListOf<String>()

        override fun exec(command: String) =
            flowOf((if (replies.isEmpty()) "" else replies.removeAt(0)).toByteArray()).also { commands += command }
    }

    /** 骨架扫描的输出形状：`offset\tuuid\tparentUuid\ttype\tts\tinterrupt`，末行是 EOF 总字节。 */
    private fun skeleton(vararg rows: Triple<Long, String, String>) =
        rows.joinToString("\n") { (off, uuid, type) -> "$off\t$uuid\tp-$uuid\t$type\t2026-08-02T00:00:00Z\t0" }

    private fun assistantRecord(
        id: String,
        text: String,
    ) = """{"type":"assistant","message":{"id":"$id","role":"assistant","content":[{"type":"text","text":"$text"}]},""" +
        """"sessionId":"s","uuid":"u-$id","parentUuid":null,"isSidechain":false}"""

    private fun userRecord(text: String) =
        """{"type":"user","message":{"role":"user","content":"$text"},"sessionId":"s","uuid":"u-$text"}"""

    /**
     * 历史真的能读出来。
     *
     * `--resume` 不回放历史：resume 上去只有 `system`(hook) 与 `result`。
     * 所以点开一条老对话看到之前说过什么只能靠读会话记录；读不出来就是一个空白的老对话
     * （而 Claude 其实记得，更让人困惑）。
     */
    @Test
    fun theHistoryOfAnExistingConversationIsActuallyReadBack() =
        runTest {
            val ch =
                FakeChannel(
                    mutableListOf(
                        skeleton(Triple(0L, "a", "user"), Triple(120L, "b", "assistant")) + "\n240",
                        userRecord("早先说过的话") + "\n" + assistantRecord("m1", "早先的回答"),
                    ),
                )
            val page = ChatHistorySource(ch, recordWord).loadOlder()

            assertNotNull("必须读出内容来", page)
            val text = page!!.filterIsInstance<BridgeFrame.AssistantText>().joinToString("") { it.x }
            assertEquals("助手正文要还原", "早先的回答", text)
            assertTrue(
                "用户那句也要在：${page.map { it.t }}",
                page.any { it is BridgeFrame.UserText && it.x.contains("早先说过的话") },
            )
        }

    /**
     * 只读一次，绝不 tail。
     *
     * 这份记录同时正被这条对话本身写（管道里那个 Claude 在写它）。
     * 一旦 tail 它，同一句话就会来两遍（一遍从管道、一遍从记录），界面上直接双份。
     *
     * 判据落在发出去的命令上：不许出现 `tail -f`。
     * （`tail -c +N` 是取字节偏移，与 `-f` 是两回事，所以判据必须精确到 `-f`。）
     */
    @Test
    fun itReadsOnceAndNeverFollowsTheRecordItIsAlsoWriting() =
        runTest {
            val ch =
                FakeChannel(
                    mutableListOf(
                        skeleton(Triple(0L, "a", "assistant")) + "\n80",
                        assistantRecord("m1", "x"),
                    ),
                )
            ChatHistorySource(ch, recordWord).loadOlder()

            assertTrue("前提：确实发了命令", ch.commands.isNotEmpty())
            for (c in ch.commands) {
                assertFalse("绝不许 tail 这份记录：$c", c.contains("tail -f"))
            }
            assertTrue("取的是有界字节区间", ch.commands.any { it.contains("tail -c +") && it.contains("head -c ") })
        }

    /**
     * 到顶要说得出来：返回 null 让界面显示「已是最早」而不是无限转圈。
     *
     * 判据同时钉住「到顶之后不再发命令」：再问一次不该又去打一次远端。
     */
    @Test
    fun reachingTheOldestIsReportedAndStopsAskingTheRemote() =
        runTest {
            val ch = FakeChannel(mutableListOf(skeleton(Triple(0L, "a", "assistant")) + "\n80", assistantRecord("m1", "x")))
            val src = ChatHistorySource(ch, recordWord)

            assertNotNull("前提：第一页要有东西", src.loadOlder())
            val afterFirst = ch.commands.size
            assertNull("没有更早的了 ⇒ 返回 null", src.loadOlder())
            assertNull("再问还是 null", src.loadOlder())
            assertEquals("到顶之后不该再打远端", afterFirst, ch.commands.size)
        }

    /**
     * 骨架只扫一次：翻页不重扫，这正是相对「每次全量重下」省下来的东西。
     *
     * 判据落在「第二页没有再发一次扫描命令」上。
     */
    @Test
    fun theSkeletonIsScannedOnceAndPagingDoesNotRescanIt() =
        runTest {
            val ch =
                FakeChannel(
                    mutableListOf(
                        skeleton(Triple(0L, "a", "assistant"), Triple(100L, "b", "assistant")) + "\n200",
                        assistantRecord("m2", "第二页"),
                        assistantRecord("m1", "第一页"),
                    ),
                )
            val src = ChatHistorySource(ch, recordWord, pageRecords = 1)
            src.loadOlder()
            val scans = { ch.commands.count { it.contains("awk") } }
            assertEquals("前提：首次要扫一次骨架", 1, scans())
            src.loadOlder()
            assertEquals("翻页不许重扫骨架", 1, scans())
        }

    /**
     * 一页往前插的渲染单元不许超上限。
     *
     * 一条 record 会展开成多个 unit，工具密集的一页 40 条 record 完全可能超过 100 个 unit，
     * 所以页大小要按 unit 数封顶，不能只按 record 数。超了的话列表的 key 锚定窗口锚不住，
     * 往前插之后位置乱跳。
     *
     * 造一页远超上限的记录（每条一个独立 message ⇒ 一条一个 unit），断言真正交出去的帧
     * 最多产出上限那么多单元；数单元用真的 assembler。
     */
    @Test
    fun aPageNeverHandsOverMoreRenderUnitsThanTheAnchorWindowCanHold() =
        runTest {
            val n = ChatHistorySource.MAX_PREPEND_UNITS * 2
            val bones = (0 until n).map { Triple(it * 100L, "r$it", "assistant") }
            val body = (0 until n).joinToString("\n") { assistantRecord("m$it", "第 $it 段") }
            val ch = FakeChannel(mutableListOf(skeleton(*bones.toTypedArray()) + "\n${n * 100}", body))

            val page = ChatHistorySource(ch, recordWord, pageRecords = n).loadOlder()
            assertNotNull(page)
            // 前提：素材本身确实超了，否则这条测的是一个不存在的情形
            assertTrue("前提：这一页的记录条数确实远超上限", n > ChatHistorySource.MAX_PREPEND_UNITS)

            val probe = ChatTurnAssembler()
            page!!.forEach { probe.feed(it) }
            assertTrue(
                "实得 ${probe.units().size} 个单元，超过上限 ${ChatHistorySource.MAX_PREPEND_UNITS}",
                probe.units().size <= ChatHistorySource.MAX_PREPEND_UNITS,
            )
            assertTrue("但也不能裁成空的：那等于历史读不出来", probe.units().isNotEmpty())
        }

    /**
     * 「只读一次」得落在字节上。
     *
     * `-f` 只是造成双份的一种写法。一个每页都把已经读过的那一段再读一遍的翻页器，形式上没有 `-f`，
     * 效果上就是一条按上滑手势驱动的 tail。而且这种坏法在界面上看不出来：`ChatTurnAssembler.prependFrames`
     * 按 key 去重，重复的那一段会被折叠掉（`added == 0`）。所以判据量机制（发出去的字节区间），不量症状。
     *
     * 连翻三页，把每条取内容命令里的 `tail -c +N … | head -c LEN` 解回 `[N-1, N-1+LEN)`，
     * 断言后一页整段落在前一页起点之前（左闭右开，允许接缝相等、不许交叠）。
     * 先断言三条区间都解出来了：空列表的 `zipWithNext` 什么都不比，会恒绿。
     *
     * 只看这一个 [ChatHistorySource] 实例自己的翻页序列；同一条对话被造出两个 source 的双份它看不见。
     */
    @Test
    fun everyPageReadsAStrictlyEarlierByteRangeAndNeverRepeatsOne() =
        runTest {
            val bones = (0 until 4).map { Triple(it * 100L, "r$it", "assistant") }
            val ch =
                FakeChannel(
                    mutableListOf(
                        skeleton(*bones.toTypedArray()) + "\n400",
                        assistantRecord("m3", "第四段"),
                        assistantRecord("m2", "第三段"),
                        assistantRecord("m1", "第二段"),
                    ),
                )
            val src = ChatHistorySource(ch, recordWord, pageRecords = 1)
            repeat(3) { src.loadOlder() }

            val ranges = ch.commands.mapNotNull(::byteRangeOf)
            assertEquals("前提：三次翻页各该发一条取内容的命令，实际发了 ${ch.commands}", 3, ranges.size)
            ranges.zipWithNext { newer, older ->
                assertTrue(
                    "翻页取的字节区间不许压在已经读过的那一段上：" +
                        "这一页 [${older.first}, ${older.second}) 与上一页 [${newer.first}, ${newer.second}) 交叠",
                    older.second <= newer.first,
                )
            }
        }

    /** 把 `tail -c +N … | head -c LEN` 解回左闭右开的字节区间；不是取内容的命令则 null。 */
    private fun byteRangeOf(cmd: String): Pair<Long, Long>? {
        val start = firstLongGroup(cmd, """tail -c \+(\d+)""") ?: return null
        val len = firstLongGroup(cmd, """head -c (\d+)""") ?: return null
        return (start - 1) to (start - 1 + len)
    }

    private fun firstLongGroup(
        text: String,
        pattern: String,
    ): Long? =
        Regex(pattern)
            .find(text)
            ?.groupValues
            ?.get(1)
            ?.toLongOrNull()

    /**
     * 一行坏掉不许拖垮整页。
     *
     * 记录是被别的进程边写边追加的，读到半行（或写坏的一行）是正常情形。
     * 整页丢的表现是「历史无缘无故少一段」，而且没有任何地方说得出为什么。
     */
    @Test
    fun oneBrokenLineDoesNotDiscardTheWholePage() =
        runTest {
            val ch =
                FakeChannel(
                    mutableListOf(
                        skeleton(Triple(0L, "a", "assistant")) + "\n200",
                        "{\"type\":\"assistant\",\"message\":{\"id\":  <<坏掉的半行\n" + assistantRecord("m1", "好的那行"),
                    ),
                )
            val page = ChatHistorySource(ch, recordWord).loadOlder()
            assertNotNull(page)
            assertEquals(
                "好的那行必须还在",
                "好的那行",
                page!!.filterIsInstance<BridgeFrame.AssistantText>().joinToString("") { it.x },
            )
        }
}
