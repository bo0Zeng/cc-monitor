package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.security.MessageDigest

/**
 * 文件屏的读写全问那台后端（`files-*`）：手机不判能不能做、不写出错那句，失败原样带回那台写好的那一句与复制详情。
 * 上传一块一块进暂存区再提交（不覆盖）；下载按块读回；删一项（目录连同里面）是一组里的一项、一问做完；打开一个路径先问那台它是什么。
 */
class FileOpsTest {
    private val dir = RemotePath.of("/w")

    @Test
    fun uploadStagesEveryChunkThenCommitsWithTheWholeDigestAndNeverOverwrites() =
        runTest {
            val link = FakeCoreLink().apply { answer = { cmd, _ -> if (cmd == "files-stage-chunk") Reply.Ok(mapOf("bytes" to 1.0)) else Reply.Ok(mapOf("path" to "/w/up.bin", "bytes" to 1.0)) } }
            val body = ByteArray(FileOps.CHUNK * 2 + 5) { (it % 251).toByte() }
            val seen = mutableListOf<Long>()
            FileOps(link) { "box" }.upload(dir, "up.bin", { body.inputStream() }) { got, _ -> seen += got }.getOrThrow()
            val chunks = link.calls("files-stage-chunk")
            assertEquals(listOf(0L, 1L, 2L), chunks.map { it!!["seq"] })
            assertEquals("同一份上传同一个键", 1, chunks.map { it!!["key"] }.toSet().size)
            val commit = link.calls("files-commit-upload").single()!!
            assertEquals("同名怎么办不带（＝ 那台问：回 exists）", false, "on_exists" in commit)
            assertEquals(3L, commit["chunks"])
            assertEquals(body.size.toLong(), commit["bytes"])
            assertEquals(chunks.first()!!["key"], commit["key"])
            val sha = MessageDigest.getInstance("SHA-256").digest(body).joinToString("") { "%02x".format(it) }
            assertEquals(mapOf("sha256" to sha), commit["expect"])
            assertEquals(body.size.toLong(), seen.last())
        }

    @Test
    fun aRefusedCommitCarriesTheBackendsSentenceAndDetail() =
        runTest {
            val link =
                FakeCoreLink().apply {
                    answer = { cmd, _ ->
                        if (cmd == "files-commit-upload") Reply.Failed(CoreFailure("io_failed", "那台写的那一句", "详情")) else Reply.Ok(mapOf("bytes" to 1.0))
                    }
                }
            val e = FileOps(link) { "box" }.upload(dir, "up.bin", { byteArrayOf(1).inputStream() }) { _, _ -> }.exceptionOrNull() as FileOps.Failed
            assertEquals("那台写的那一句", e.problem.text)
            assertEquals("详情", e.problem.detail)
        }

    @Test
    fun downloadReadsChunksUntilTheEnd() =
        runTest {
            val link =
                FakeCoreLink().apply {
                    answer = { _, a ->
                        val off = a!!["offset"] as Long
                        Reply.Ok(mapOf("path" to "/w/f", "offset" to off.toDouble(), "size" to 3.0, "eof" to (off > 0), "content" to mapOf("b16" to if (off == 0L) "0102" else "03")))
                    }
                }
            val out = ByteArrayOutputStream()
            assertEquals(3L, FileOps(link) { "box" }.download(RemotePath.of("/w/f"), out) { _, _ -> }.getOrThrow())
            assertArrayEquals(byteArrayOf(1, 2, 3), out.toByteArray())
            assertEquals(listOf(0L, 2L), link.calls("files-read-chunk").map { it!!["offset"] })
        }

    @Test
    fun deletingAFolderIsOneAskAndARefusedItemSaysTheBackendSentence() =
        runTest {
            val ok = FakeCoreLink().apply { answer = { _, _ -> Reply.Ok(mapOf("outcomes" to listOf(mapOf("key" to "0", "state" to "done")), "said" to "已删除")) } }
            FileOps(ok) { "box" }.delete(dir, RemotePath.of("d"), isDir = true).getOrThrow()
            assertEquals(1, ok.calls("files-delete").size)
            assertEquals(listOf(mapOf("rel" to "d", "recursive" to true)), ok.calls("files-delete").single()!!["items"])
            val no = FakeCoreLink().apply { answer = { _, _ -> Reply.Ok(mapOf("outcomes" to listOf(mapOf("key" to "0", "state" to "failed", "detail" to "EACCES")), "said" to "删除失败 ×1")) } }
            val e = FileOps(no) { "box" }.delete(dir, RemotePath.of("d"), isDir = true).exceptionOrNull() as FileOps.Failed
            assertEquals("删除失败 ×1", e.problem.text)
            assertEquals("EACCES", e.problem.detail)
        }

    @Test
    fun openingAPathAsksTheBackendWhatItIsDirsAreEnteredFilesOpenInTheirFolder() =
        runTest {
            val kinds = mapOf("/w/src" to "dir", "/w/src/a.kt" to "file")
            val link =
                FakeCoreLink().apply {
                    answer = { _, a -> kinds[a!!["path"]]?.let { Reply.Ok(mapOf("path" to a["path"], "kind" to it, "size" to 0.0, "readonly" to false, "owner" to null, "link_target" to null)) } ?: Reply.Failed(CoreFailure("not_found", "不在", "")) }
                }
            val ops = FileOps(link) { "box" }
            assertEquals(FileOps.Opening.Dir(RemotePath.of("/w/src")), ops.open("/w/src").getOrThrow())
            assertEquals(FileOps.Opening.File(RemotePath.of("/w/src/a.kt")), ops.open("/w/src/a.kt").getOrThrow())
            assertEquals(listOf("files-stat"), link.calls.map { it.first }.distinct())
            assertEquals("不在", (ops.open("/w/nope").exceptionOrNull() as FileOps.Failed).problem.text)
        }

    @Test
    fun aBrokenLinkSaysTheSentenceTheOtherScreensSay() =
        runTest {
            val e = FileOps(FakeCoreLink()) { "box" }.list(dir).exceptionOrNull() as FileOps.Failed
            assertEquals(copyText("front.body.offline", "machine" to "box"), e.problem.text)
        }

    @Test
    fun theModeIsTypedAsOctalDigits() {
        assertEquals(420L, FileOps.modeOf("644").getOrThrow())
        assertEquals(copyText("rsFilewinWriteops.mode.empty"), FileOps.modeOf(" ").exceptionOrNull()?.message)
        assertEquals(copyText("rsFilewinWriteops.mode.notOctal"), FileOps.modeOf("69").exceptionOrNull()?.message)
        assertEquals(copyText("rsFilewinWriteops.mode.outOfRange"), FileOps.modeOf("17777").exceptionOrNull()?.message)
    }
}
