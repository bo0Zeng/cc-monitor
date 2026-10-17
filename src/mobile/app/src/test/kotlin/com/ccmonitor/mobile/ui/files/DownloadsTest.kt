package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.BatchAnswer
import com.ccmonitor.mobile.core.claude.link.BatchEvent
import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Transfer
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 下载一组在手机这一圈：本机同名先问一次 · 逐项现开本机那一份交核心拉 · 没下成的那一份删掉 · 断开停在那里等［继续］（没下完的整份重下）。
 * 卡上的字全是核心那一格（替身把交进去的逐项结局原样排出来，看得出 Kotlin 交的是哪几格）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class DownloadsTest {
    private fun row(name: String) = FilesLs.Entry(RemotePath.of("/w/$name"), RemotePath.of(name), isDir = false, size = 1L, sizeText = null, mtime = null)

    /** 本机文件夹的替身：已有哪几个名字 · 开过哪几份（名字 ＋ 盖的那一份）· 删过哪几份。 */
    private class Folder(
        val here: List<String> = emptyList(),
    ) : LocalFolder {
        val made = mutableListOf<Pair<String, String?>>()
        val dropped = mutableListOf<String>()

        override fun names() = here

        override fun create(
            name: String,
            over: String?,
        ): LocalFile {
            made += name to over
            return LocalFile(fd = { 40 + made.size }, drop = { dropped += name })
        }
    }

    private class Rig(
        scope: TestScope,
    ) {
        val link = FakeCoreLink()
        val shown = mutableListOf<BatchEvent>()
        val downloads = Downloads(link, FileOps(link) { "box" }, scope, StandardTestDispatcher(scope.testScheduler), { "box" }, { shown += it })
        val last get() = shown.last()

        fun pulled() = link.transfers.map { (it.first as Transfer.Pull).path.shown }
    }

    @Test
    fun aFailedItemIsDroppedOnThePhoneAndTheCardSaysWhatTheCoreSays() =
        runTest {
            val rig = Rig(this)
            rig.link.transferred = { t ->
                if ((t as Transfer.Pull).path.shown == "/w/b") Reply.Failed(CoreFailure("io_failed", "读不出 b", "EIO")) else Reply.Ok(mapOf("bytes" to 1.0))
            }
            val folder = Folder()
            rig.downloads.start(listOf(row("a"), row("b"), row("c")), folder)
            advanceUntilIdle()
            assertEquals(listOf("/w/a", "/w/b", "/w/c"), rig.pulled())
            assertEquals("每项现开一份", listOf(41, 42, 43), rig.link.transfers.map { it.second })
            assertEquals(listOf("b"), folder.dropped)
            assertEquals(BatchEvent.ENDED, rig.last.phase)
            assertEquals("ended:done,failed,done", rig.last.said)
            assertEquals(listOf("b：读不出 b"), rig.last.failed)
            val again = rig.link.downloads.size
            assertTrue(rig.downloads.again(rig.last.id))
            advanceUntilIdle()
            assertEquals("重试只拉没成的那一项", listOf("/w/a", "/w/b", "/w/c", "/w/b"), rig.pulled())
            assertTrue(rig.link.downloads.size > again)
            assertEquals("ended:failed", rig.last.said)
        }

    @Test
    fun aCutStopsTheGroupCountsWhatIsLeftAndResumeRefetchesOnlyThose() =
        runTest {
            val rig = Rig(this)
            rig.link.transferred = { t -> if ((t as Transfer.Pull).path.shown == "/w/b") Reply.LinkDown else Reply.Ok(mapOf("bytes" to 1.0)) }
            val folder = Folder()
            rig.downloads.start(listOf(row("a"), row("b"), row("c")), folder)
            advanceUntilIdle()
            assertEquals("断了不往下拉", listOf("/w/a", "/w/b"), rig.pulled())
            assertEquals(BatchEvent.CUT, rig.last.phase)
            assertEquals("cut:done,cancelled,cancelled", rig.last.line)
            assertTrue(rig.last.resume)
            assertEquals("断在半路的那一份不留", listOf("b"), folder.dropped)
            rig.link.transferred = { Reply.Ok(mapOf("bytes" to 1.0)) }
            assertTrue(rig.downloads.again(rig.last.id))
            advanceUntilIdle()
            assertEquals(listOf("/w/a", "/w/b", "/w/b", "/w/c"), rig.pulled())
            assertEquals("ended:done,done", rig.last.said)
        }

    @Test
    fun namesAlreadyInTheFolderAreAskedOnceAndTheAnswerDecidesEachItem() =
        runTest {
            val rig = Rig(this)
            rig.link.transferred = { Reply.Ok(mapOf("bytes" to 1.0)) }
            val folder = Folder(here = listOf("a", "c"))
            rig.downloads.start(listOf(row("a"), row("b"), row("c")), folder)
            advanceUntilIdle()
            assertEquals(listOf("a", "c"), rig.link.downloads.first()["here"])
            assertEquals(BatchEvent.ASKING, rig.last.phase)
            assertEquals(emptyList<Any>(), rig.link.transfers)
            assertTrue(rig.downloads.answer(rig.last.id, BatchAnswer.Each(mapOf("2" to "overwrite"))))
            advanceUntilIdle()
            assertEquals("没点的那一项跳过、盖的那一项开盘上那一份", listOf("b" to null, "c" to "c"), folder.made)
            assertEquals("ended:skipped,done,done", rig.last.said)

            val drop = Rig(this)
            drop.downloads.start(listOf(row("a")), Folder(here = listOf("a")))
            advanceUntilIdle()
            assertTrue(drop.downloads.answer(drop.last.id, null))
            advanceUntilIdle()
            assertEquals("取消 ⇒ 卡收起、一项不拉", BatchEvent.DROPPED, drop.last.phase)
            assertEquals(emptyList<Any>(), drop.link.transfers)
        }
}
