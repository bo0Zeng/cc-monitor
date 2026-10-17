package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.BatchEvent
import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Transfer
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 文件屏的读写全问那台后端（`files-*`）：手机不判能不能做、不写出错那句，失败原样带回那台写好的那一句与复制详情。
 * 上传 · 下载的分块那一圈在核心里（交描述符），上传提交同名时那台问、答了同一份暂存件再交；删一项（目录连同里面）是一组里的一项、一问做完；打开一个路径先问那台它是什么。
 */
class FileOpsTest {
    private val dir = RemotePath.of("/w")

    private val staged = mapOf("key" to "k1", "chunks" to 3.0, "bytes" to 9.0, "sha256" to "ab")

    @Test
    fun uploadHandsTheFileToTheCoreThenCommitsWhatItStagedWithoutSayingWhatToDoOnAClash() =
        runTest {
            val link =
                FakeCoreLink().apply {
                    moves = listOf(4L to 9L, 9L to 9L)
                    transferred = { Reply.Ok(staged) }
                    answer = { _, _ -> Reply.Ok(mapOf("path" to "/w/up.bin", "bytes" to 9.0)) }
                }
            val seen = mutableListOf<Long>()
            val r = FileOps(link) { "box" }.upload(dir, "up.bin", { 17 }) { got, _ -> seen += got }.getOrThrow()
            assertEquals(FileOps.Upload.Done, r)
            assertEquals(listOf(Transfer.Stage("up.bin") to 17), link.transfers)
            assertEquals(listOf(4L, 9L), seen)
            assertEquals("Kotlin 不再一块一块问", listOf("files-commit-upload"), link.calls.map { it.first })
            val commit = link.calls("files-commit-upload").single()!!
            assertEquals("同名怎么办不带（＝ 那台问：回 exists）", false, "on_exists" in commit)
            assertEquals(listOf("k1", 3L, 9L, mapOf("sha256" to "ab")), listOf(commit["key"], commit["chunks"], commit["bytes"], commit["expect"]))
        }

    @Test
    fun aClashComesBackAsTheBackendsListAndAnsweringResendsTheSameStagedCopy() =
        runTest {
            val conflict = mapOf("key" to "0", "name" to "up.bin", "kind" to "file", "size" to 9.0, "there" to "file", "there_size" to 3.0, "there_size_text" to "3 字节", "there_mtime_text" to "14:02", "answers" to listOf("overwrite", "keep_both"))
            val link =
                FakeCoreLink().apply {
                    transferred = { Reply.Ok(staged) }
                    answer = { _, a ->
                        if (a!!["on_exists"] == null) {
                            Reply.Failed(CoreFailure("exists", "同名 1 项", "d", mapOf("conflicts" to listOf(conflict))))
                        } else {
                            Reply.Ok(mapOf("path" to "/w/up (2).bin", "bytes" to 9.0))
                        }
                    }
                }
            val ops = FileOps(link) { "box" }
            val c = ops.upload(dir, "up.bin", { 17 }) { _, _ -> }.getOrThrow() as FileOps.Upload.Clash
            assertEquals("同名 1 项", c.said)
            assertEquals(listOf(BatchEvent.Clash("0", "up.bin", "3 字节", listOf("overwrite", "keep_both"))), c.rows)
            assertEquals(FileOps.Upload.Done, ops.commit(dir, c.name, c.staged, "keep_both").getOrThrow())
            assertEquals("块不重送", 1, link.transfers.size)
            val again = link.calls("files-commit-upload").last()!!
            assertEquals(listOf("keep_both", "k1"), listOf(again["on_exists"], again["key"]))
        }

    @Test
    fun aRefusedCommitOrStageCarriesTheBackendsSentenceAndALocalFileThatWontOpenSaysSo() =
        runTest {
            val link =
                FakeCoreLink().apply {
                    transferred = { Reply.Ok(staged) }
                    answer = { _, _ -> Reply.Failed(CoreFailure("io_failed", "那台写的那一句", "详情")) }
                }
            val e = FileOps(link) { "box" }.upload(dir, "up.bin", { 17 }) { _, _ -> }.exceptionOrNull() as FileOps.Failed
            assertEquals(listOf("那台写的那一句", "详情"), listOf(e.problem.text, e.problem.detail))
            val no = FakeCoreLink().apply { transferred = { Reply.Failed(CoreFailure("refused", "暂存区满了", "")) } }
            assertEquals("暂存区满了", (FileOps(no) { "box" }.upload(dir, "up.bin", { 17 }) { _, _ -> }.exceptionOrNull() as FileOps.Failed).problem.text)
            assertEquals("送不成就不提交", emptyList<Any>(), no.calls)
            val gone = FileOps(FakeCoreLink()) { "box" }.upload(dir, "up.bin", { throw java.io.IOException("ENOENT") }) { _, _ -> }.exceptionOrNull() as FileOps.Failed
            assertEquals(copyText("mobFiles.local.readFailed", "name" to "up.bin"), gone.problem.text)
        }

    @Test
    fun downloadHandsTheTargetToTheCoreAndSaysHowManyBytesLanded() =
        runTest {
            val link = FakeCoreLink().apply { transferred = { Reply.Ok(mapOf("bytes" to 3.0)) } }
            assertEquals(3L, FileOps(link) { "box" }.download(RemotePath.of("/w/f"), { 18 }, "/sd/f") { _, _ -> }.getOrThrow())
            assertEquals(listOf(Transfer.Pull(RemotePath.of("/w/f"), "/sd/f") to 18), link.transfers)
            val e = FileOps(link) { "box" }.download(RemotePath.of("/w/f"), { throw java.io.IOException("EROFS") }, "/sd/f") { _, _ -> }.exceptionOrNull() as FileOps.Failed
            assertEquals(copyText("mobFiles.local.writeFailed", "path" to "/sd/f"), e.problem.text)
            assertTrue("本机那一份开不了不是断了", !e.cut)
            val down = FakeCoreLink().apply { transferred = { Reply.LinkDown } }
            assertTrue("通道断了 ⇒ 断了（下载一组停在那里等继续）", (FileOps(down) { "box" }.download(RemotePath.of("/w/f"), { 18 }, "f") { _, _ -> }.exceptionOrNull() as FileOps.Failed).cut)
        }

    @Test
    fun aGroupHandsTheCoreWhatToDoAndTheCoreBuildsTheArgs() =
        runTest {
            val link = FakeCoreLink()
            val d = FilesLs.Entry(RemotePath.of("/w/d"), RemotePath.of("d"), isDir = true, size = null, sizeText = null, mtime = null)
            val f = FilesLs.Entry(RemotePath.of("/w/f"), RemotePath.of("f"), isDir = false, size = 1L, sizeText = "1 字节", mtime = null)
            val ops = FileOps(link) { "box" }
            assertEquals(1L, ops.delete(dir, listOf(d, f)))
            assertEquals(2L, ops.chmod(dir, listOf(f), "644"))
            assertEquals(3L, ops.again(1L, undo = true))
            assertEquals(4L, ops.copy(dir, listOf(d, f), RemotePath.of("/w/sub")))
            assertEquals(
                listOf(
                    mapOf("delete" to mapOf("root" to "/w", "items" to listOf(mapOf("rel" to "d", "name" to "d", "dir" to true), mapOf("rel" to "f", "name" to "f", "dir" to false)))),
                    mapOf("chmod" to mapOf("root" to "/w", "items" to listOf(mapOf("rel" to "f", "name" to "f")), "mode" to "644")),
                    mapOf("undo" to 1L),
                    mapOf("copy" to mapOf("from" to "/w", "names" to listOf("d", "f"), "to" to "/w/sub")),
                ),
                link.wants,
            )
            assertTrue("一问都不经一问一答那一路", link.calls.isEmpty())
        }

    @Test
    fun openingAPathAsksTheBackendWhatItIsDirsAreEnteredFilesOpenInTheirFolder() =
        runTest {
            val kinds = mapOf("/w/src" to ("dir" to "/w"), "/w/src/a.kt" to ("file" to "/w/src"), "C:\\w\\b.kt" to ("file" to "C:\\w"))
            val link =
                FakeCoreLink().apply {
                    answer = { _, a -> kinds[a!!["path"]]?.let { (kind, up) -> Reply.Ok(mapOf("path" to a["path"], "up" to up, "kind" to kind, "size" to 0.0, "readonly" to false, "owner" to null, "link_target" to null)) } ?: Reply.Failed(CoreFailure("not_found", "不在", "")) }
                }
            val ops = FileOps(link) { "box" }
            assertEquals(FileOps.Opening.Dir(RemotePath.of("/w/src")), ops.open("/w/src").getOrThrow())
            assertEquals(FileOps.Opening.File(RemotePath.of("/w/src"), RemotePath.of("/w/src/a.kt")), ops.open("/w/src/a.kt").getOrThrow())
            // 所在的目录照那台切的（Windows 的反斜杠也是它切），手机不切。
            assertEquals(FileOps.Opening.File(RemotePath.of("C:\\w"), RemotePath.of("C:\\w\\b.kt")), ops.open("C:\\w\\b.kt").getOrThrow())
            assertEquals(listOf("files-stat"), link.calls.map { it.first }.distinct())
            assertEquals("不在", (ops.open("/w/nope").exceptionOrNull() as FileOps.Failed).problem.text)
        }

    @Test
    fun aBrokenLinkSaysTheSentenceTheOtherScreensSay() =
        runTest {
            val e = FileOps(FakeCoreLink()) { "box" }.list(dir).exceptionOrNull() as FileOps.Failed
            assertEquals(copyText("front.body.offline", "machine" to "box"), e.problem.text)
        }
}
