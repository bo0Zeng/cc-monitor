package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.Batch
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.FilesChmod
import com.ccmonitor.mobile.core.claude.link.FilesCommitUpload
import com.ccmonitor.mobile.core.claude.link.FilesCreate
import com.ccmonitor.mobile.core.claude.link.FilesDelete
import com.ccmonitor.mobile.core.claude.link.FilesHome
import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.FilesMkdir
import com.ccmonitor.mobile.core.claude.link.FilesReadChunk
import com.ccmonitor.mobile.core.claude.link.FilesReadText
import com.ccmonitor.mobile.core.claude.link.FilesRename
import com.ccmonitor.mobile.core.claude.link.FilesStageChunk
import com.ccmonitor.mobile.core.claude.link.FilesStat
import com.ccmonitor.mobile.core.claude.link.FilesWriteText
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.wire.FileKind
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.security.MessageDigest
import java.util.Locale
import java.util.UUID

/**
 * 文件屏要那台做的几件，全是那台后端的文件命令（`files-*`，桌面文件窗口问的同一族），经这台的常驻流 [feed]。
 * 能不能做 · 出错那一句 · 目标已在都由那台判、那台写：失败原样带回那一句与复制详情（[Failed]），手机不另判。
 * 写的那几问都发生在当前目录里：`root` ＝ 当前目录，`rel` ＝ 那一项的末段（或敲的名字）。
 */
class FileOps(
    private val feed: CoreLink,
    private val machine: suspend () -> String,
) {
    /** 没做成：那台写好的那一句（或走不通时那一句）＋ 复制详情。 */
    class Failed(
        val problem: Problem,
    ) : Exception(problem.text)

    /** 那台这个用户的家（起点）。 */
    suspend fun home(): Result<RemotePath> = ask(FilesHome.COMMAND, emptyMap(), FilesHome::of)

    suspend fun list(dir: RemotePath): Result<FilesLs.Listing> = ask(FilesLs.COMMAND, FilesLs.args(dir), FilesLs::of)

    /** 读一份文本（预览 · 编辑）。不是文本 · 太大 ⇒ 那台那一句。 */
    suspend fun readText(path: RemotePath): Result<FilesReadText.Text> = ask(FilesReadText.COMMAND, FilesReadText.args(path), FilesReadText::of)

    /** 存回（带读时的摘要：盘上被改过 ⇒ 那台拒）。 */
    suspend fun writeText(
        dir: RemotePath,
        name: RemotePath,
        text: String,
        sha256: String,
    ): Result<Unit> = done(FilesWriteText.COMMAND, FilesWriteText.args(dir, name, text, sha256))

    suspend fun mkdir(
        dir: RemotePath,
        typed: String,
    ): Result<Unit> = done(FilesMkdir.COMMAND, FilesMkdir.args(dir, typed))

    suspend fun create(
        dir: RemotePath,
        typed: String,
    ): Result<Unit> = done(FilesCreate.COMMAND, FilesCreate.args(dir, typed))

    suspend fun rename(
        dir: RemotePath,
        from: RemotePath,
        typed: String,
    ): Result<Unit> = done(FilesRename.COMMAND, FilesRename.args(dir, from, typed))

    /** 删一项；目录连同里面全部。 */
    suspend fun delete(
        dir: RemotePath,
        name: RemotePath,
        isDir: Boolean,
    ): Result<Unit> = batch(FilesDelete.COMMAND, FilesDelete.args(dir, name, isDir), TREE_MS)

    /** 打开别处点名的一个路径：那台说是目录 ⇒ 进去；别的 ⇒ 进它所在的目录、开它。 */
    suspend fun open(text: String): Result<Opening> {
        val path = RemotePath.of(text)
        return ask(FilesStat.COMMAND, FilesStat.args(path), FilesStat::kindOf).map { if (it == FileKind.DIR) Opening.Dir(path) else Opening.File(path.parent, path) }
    }

    /** 打开一个路径落在哪。 */
    sealed interface Opening {
        data class Dir(
            val dir: RemotePath,
        ) : Opening

        data class File(
            val dir: RemotePath,
            val file: RemotePath,
        ) : Opening
    }

    /** 此刻的权限位；那台没有这一说 ⇒ `null`。 */
    suspend fun mode(path: RemotePath): Result<Long?> =
        when (val r = feed.call(FilesStat.COMMAND, FilesStat.args(path))) {
            is Reply.Ok -> Result.success(FilesStat.modeOf(r.data))
            else -> Result.failure(Failed(r.problem(machine(), FilesStat.COMMAND) ?: Problem("", "")))
        }

    suspend fun chmod(
        dir: RemotePath,
        name: RemotePath,
        mode: Long,
    ): Result<Unit> = batch(FilesChmod.COMMAND, FilesChmod.args(dir, name, mode))

    /**
     * 上传：本机那份逐块读，一块一块进那台的暂存区（同一个键），边读边算整份摘要；传完交提交（不覆盖，目标在就拒）。
     * [progress] 报已送字节与总数（总数不知道 ⇒ `-1`）。
     */
    suspend fun upload(
        dir: RemotePath,
        name: String,
        open: () -> InputStream,
        total: Long = -1,
        progress: (Long, Long) -> Unit,
    ): Result<Unit> {
        val key = UUID.randomUUID().toString().replace("-", "")
        val sha = MessageDigest.getInstance("SHA-256")
        val buf = ByteArray(CHUNK)
        var seq = 0L
        var got = 0L
        val local = { e: IOException -> Result.failure<Unit>(Failed(Problem(copyText("mobFiles.local.readFailed", "name" to name), e.toString()))) }
        try {
            open().use { input ->
                while (true) {
                    val n = fill(input, buf)
                    if (n <= 0) break
                    sha.update(buf, 0, n)
                    ask(FilesStageChunk.COMMAND, FilesStageChunk.args(key, seq, buf, n), { Unit }, CHUNK_MS).getOrElse { return Result.failure(it) }
                    seq++
                    got += n
                    progress(got, total)
                }
            }
        } catch (e: IOException) {
            return local(e)
        }
        val digest = sha.digest().joinToString("") { "%02x".format(Locale.ROOT, it) }
        return done(FilesCommitUpload.COMMAND, FilesCommitUpload.args(dir, name, key, digest, chunks = seq, bytes = got), CHUNK_MS)
    }

    /** 下载：按块读回、写进 [out]；回总字节。 */
    suspend fun download(
        path: RemotePath,
        out: OutputStream,
        progress: (Long, Long) -> Unit,
    ): Result<Long> {
        var off = 0L
        while (true) {
            val c = ask(FilesReadChunk.COMMAND, FilesReadChunk.args(path, off), FilesReadChunk::of, CHUNK_MS).getOrElse { return Result.failure(it) }
            out.write(c.bytes)
            off += c.bytes.size
            progress(off, c.size)
            if (c.eof || c.bytes.isEmpty()) return Result.success(off)
        }
    }

    /** 读满一块（或读到头）；回读到几字节。 */
    private fun fill(
        input: InputStream,
        buf: ByteArray,
    ): Int {
        var n = 0
        while (n < buf.size) {
            val k = input.read(buf, n, buf.size - n)
            if (k < 0) break
            n += k
        }
        return n
    }

    /** 一组只交一项的那几问：那一项没成 ⇒ 照后端写的那一句报。 */
    private suspend fun batch(
        command: String,
        args: Map<String, Any?>,
        withinMs: Long = WRITE_MS,
    ): Result<Unit> =
        ask(command, args, Batch::fateOf, withinMs).fold(
            onSuccess = {
                when (it) {
                    Batch.Done -> Result.success(Unit)
                    is Batch.Refused -> Result.failure(Failed(Problem(it.said, it.detail)))
                }
            },
            onFailure = { Result.failure(it) },
        )

    private suspend fun done(
        command: String,
        args: Map<String, Any?>,
        withinMs: Long = WRITE_MS,
    ): Result<Unit> = ask(command, args, { }, withinMs)

    private suspend fun <T : Any> ask(
        command: String,
        args: Map<String, Any?>,
        decode: (Any?) -> T?,
        withinMs: Long = WRITE_MS,
    ): Result<T> {
        val r = feed.call(command, args, withinMs)
        val m = machine()
        return when (r) {
            is Reply.Ok -> decode(r.data)?.let { Result.success(it) } ?: Result.failure(Failed(unreadableReply(m, command, r.data.toString())))
            else -> Result.failure(Failed(r.problem(m, command) ?: Problem("", "")))
        }
    }

    companion object {
        /** 上传一块多少原始字节（同桌面块形上传那一块）。 */
        const val CHUNK: Int = 256 * 1024

        /** 写的一问等多久（同桌面写面一趟）。 */
        const val WRITE_MS: Long = 20_000L

        /** 一块（上传 · 下载）等多久。 */
        const val CHUNK_MS: Long = 20_000L

        /** 删一整棵树的一段等多久（同桌面）。 */
        const val TREE_MS: Long = 120_000L

        /** 权限框里敲的八进制（同桌面：空 · 不是八进制 · 超出 7777 各一句）。 */
        fun modeOf(typed: String): Result<Long> {
            val t = typed.trim()
            if (t.isEmpty()) return Result.failure(IllegalArgumentException(copyText("rsFilewinWriteops.mode.empty")))
            val m = t.toLongOrNull(OCTAL) ?: return Result.failure(IllegalArgumentException(copyText("rsFilewinWriteops.mode.notOctal")))
            if (m < 0 || m > MAX_MODE) return Result.failure(IllegalArgumentException(copyText("rsFilewinWriteops.mode.outOfRange")))
            return Result.success(m)
        }

        private const val OCTAL = 8
        private const val MAX_MODE = 0xFFFL
    }
}
