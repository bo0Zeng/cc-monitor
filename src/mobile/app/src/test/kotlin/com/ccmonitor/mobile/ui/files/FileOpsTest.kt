package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 文件屏的读写全问那台后端（`files-*`）：手机不判能不能做、不写出错那句，失败原样带回那台写好的那一句与复制详情。
 * 上传 · 下载是一组、全过程在核心里（上传交现开的描述符）；删一项（目录连同里面）是一组里的一项、一问做完；打开一个路径先问那台它是什么。
 */
class FileOpsTest {
    private val dir = RemotePath.of("/w")

    /** 上传是一组：交核心的只有「要上传什么」（目录 · 落到哪在屏上那一串 · 名字 · 现开的描述符），送块 · 提交 · 同名全在核心。 */
    @Test
    fun uploadHandsTheCoreOneGroupAndAsksNothingItself() =
        runTest {
            val link = FakeCoreLink()
            val at = RemotePath.of("/w/sub")
            assertEquals(1L, FileOps(link) { "box" }.upload(at, "up.bin", { 17 }).getOrThrow())
            assertEquals(
                listOf(mapOf("upload" to mapOf("root" to "/w/sub", "shown" to "/w/sub", "items" to listOf(mapOf("name" to "up.bin", "fd" to 17))))),
                link.wants,
            )
            assertTrue("Kotlin 一问都不发", link.calls.isEmpty())
            assertTrue("Kotlin 不起分块传", link.transfers.isEmpty())
        }

    /** 本机那一份开不了 ⇒ 手机那一句（系统原话进详情），不起组。 */
    @Test
    fun aLocalFileThatWontOpenSaysSoAndStartsNothing() =
        runTest {
            val link = FakeCoreLink()
            val gone = FileOps(link) { "box" }.upload(dir, "up.bin", { throw java.io.IOException("ENOENT") }).exceptionOrNull() as FileOps.Failed
            assertEquals(copyText("mobFiles.local.readFailed", "name" to "up.bin"), gone.problem.text)
            assertTrue(gone.problem.detail.contains("ENOENT"))
            assertTrue(link.wants.isEmpty())
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
            assertEquals(5L, ops.download(listOf(f), "content://tree/x", listOf("F")))
            assertEquals(
                listOf(
                    mapOf("delete" to mapOf("root" to "/w", "items" to listOf(mapOf("rel" to "d", "name" to "d", "dir" to true), mapOf("rel" to "f", "name" to "f", "dir" to false)))),
                    mapOf("chmod" to mapOf("root" to "/w", "items" to listOf(mapOf("rel" to "f", "name" to "f")), "mode" to "644")),
                    mapOf("undo" to 1L),
                    mapOf("copy" to mapOf("from" to "/w", "names" to listOf("d", "f"), "to" to "/w/sub")),
                    mapOf("download" to mapOf("folder" to "content://tree/x", "here" to listOf("F"), "items" to listOf(mapOf("path" to "/w/f", "name" to "f")))),
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
                    answer = { _, a -> kinds[a!!["path"]]?.let { (kind, up) -> Reply.Ok(mapOf("path" to a["path"], "up" to up, "kind" to kind, "size" to 0.0, "readonly" to false, "owner" to null, "link_target" to null)) } ?: Reply.Failed(Failed("not_found", "不在", "")) }
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
            // 断着：那一句照链上交的离线那一句（核心恒给）。
            val e = FileOps(FakeCoreLink()) { "box" }.list(dir).exceptionOrNull() as FileOps.Failed
            assertEquals(FakeCoreLink.OFFLINE, e.problem.text)
        }
}
