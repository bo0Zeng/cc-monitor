package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.BatchEvent
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.FilesCommitUpload
import com.ccmonitor.mobile.core.claude.link.FilesCreate
import com.ccmonitor.mobile.core.claude.link.FilesHome
import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.FilesMkdir
import com.ccmonitor.mobile.core.claude.link.FilesReadText
import com.ccmonitor.mobile.core.claude.link.FilesRename
import com.ccmonitor.mobile.core.claude.link.FilesStat
import com.ccmonitor.mobile.core.claude.link.FilesWriteText
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Transfer
import com.ccmonitor.mobile.core.claude.link.wire.FileKind
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import java.io.IOException

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

    /** 列一层（那台排好；[bySize] ⇒ 按大小）。 */
    suspend fun list(
        dir: RemotePath,
        bySize: Boolean = false,
    ): Result<FilesLs.Listing> = ask(FilesLs.COMMAND, FilesLs.args(dir, bySize), FilesLs::of)

    /** 读一份文本（预览 · 编辑）。不是文本 · 太大 ⇒ 那台那一句。 */
    suspend fun readText(
        path: RemotePath,
        cap: Long,
    ): Result<FilesReadText.Text> = ask(FilesReadText.COMMAND, FilesReadText.args(path, cap), FilesReadText::of)

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

    /** 删当前目录里这几项（目录连同里面全部）：一组，全过程在核心里。回编号（这台不在 ⇒ 0），走到哪经 [CoreLink.batches]。 */
    suspend fun delete(
        dir: RemotePath,
        entries: List<FilesLs.Entry>,
    ): Long = feed.batch(mapOf("delete" to mapOf("root" to dir.wire, "items" to entries.map { mapOf("rel" to it.rel.wire, "name" to it.name, "dir" to it.isDir) })))

    /** 打开别处点名的一个路径：那台说是目录 ⇒ 进去；别的 ⇒ 进那台说的它所在的目录、开它。 */
    suspend fun open(text: String): Result<Opening> {
        val path = RemotePath.of(text)
        return ask(FilesStat.COMMAND, FilesStat.args(path), { d ->
            FilesStat.placeOf(d)?.let { p -> if (p.kind == FileKind.DIR) Opening.Dir(path) else p.up?.let { Opening.File(it, path) } }
        })
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

    /** 改当前目录里这几项的权限位（八进制串）：同 [delete]。 */
    suspend fun chmod(
        dir: RemotePath,
        entries: List<FilesLs.Entry>,
        mode: String,
    ): Long = feed.batch(mapOf("chmod" to mapOf("root" to dir.wire, "items" to entries.map { mapOf("rel" to it.rel.wire, "name" to it.name) }, "mode" to mode)))

    /** 把当前目录里这几项复制到 [to]（同一台；公共根 · 入参由核心拼，同名那台问）：同 [delete]。 */
    suspend fun copy(
        dir: RemotePath,
        entries: List<FilesLs.Entry>,
        to: RemotePath,
    ): Long = feed.batch(mapOf("copy" to mapOf("from" to dir.wire, "names" to entries.map { it.rel.wire }, "to" to to.wire)))

    /** 那一组没成的那几项再做一次（[undo] ⇒ 撤销那一组改的权限）：核心照它记着的那一组另起。 */
    suspend fun again(
        id: Long,
        undo: Boolean,
    ): Long = feed.batch(mapOf((if (undo) "undo" else "retry") to id))

    /** 上传那一件提交的结局：放好了 · 撞了同名（[Clash.staged] 原样留着：答了再交一次，块不重送）。 */
    sealed interface Upload {
        data object Done : Upload

        data class Clash(
            val name: String,
            val staged: FilesCommitUpload.Staged,
            /** 那台写的那一句（「同名 1 项」）。 */
            val said: String,
            /** 抽屉那几行（核心写好的）。 */
            val rows: List<BatchEvent.Clash>,
        ) : Upload
    }

    /**
     * 上传：[fd] 现开本机那一份交给核心，核心逐块送进那台的暂存区（分块那一圈是两端同一份）；送完交提交（同名那台问）。
     * [progress] 报已送字节与总数（总数不知道 ⇒ `-1`）。
     */
    suspend fun upload(
        dir: RemotePath,
        name: String,
        fd: () -> Int,
        progress: (Long, Long) -> Unit,
    ): Result<Upload> {
        val staged =
            transfer(Transfer.Stage(name), fd, progress, { FilesCommitUpload.Staged.of(it) }) {
                copyText("mobFiles.local.readFailed", "name" to name)
            }.getOrElse { return Result.failure(it) }
        return commit(dir, name, staged, onExists = null)
    }

    /** 提交（同名照 [onExists]：不带 ⇒ 那台问）。撞了同名 ⇒ [Upload.Clash]，带那台的同名清单与那一句。 */
    suspend fun commit(
        dir: RemotePath,
        name: String,
        staged: FilesCommitUpload.Staged,
        onExists: String?,
    ): Result<Upload> {
        val r = feed.call(FilesCommitUpload.COMMAND, FilesCommitUpload.args(dir, name, staged, onExists), CHUNK_MS)
        val clash = (r as? Reply.Failed)?.failure?.takeIf { it.code == EXISTS }
        val rows = clash?.let { feed.clashes(it.data) }.orEmpty()
        if (clash != null && rows.isNotEmpty()) return Result.success(Upload.Clash(name, staged, clash.message, rows))
        return when (r) {
            is Reply.Ok -> Result.success(Upload.Done)
            else -> Result.failure(Failed(r.problem(machine(), FilesCommitUpload.COMMAND) ?: Problem("", "")))
        }
    }

    /** 下载：核心按块读回、写进 [fd] 现开的本机那一份（[shown] 是它在句子里的名字）；回总字节。 */
    suspend fun download(
        path: RemotePath,
        fd: () -> Int,
        shown: String,
        progress: (Long, Long) -> Unit,
    ): Result<Long> =
        transfer(Transfer.Pull(path, shown), fd, progress, { d -> ((d as? Map<*, *>)?.get("bytes") as? Number)?.toLong() }) {
            copyText("mobFiles.local.writeFailed", "path" to shown)
        }

    /** 分块传一趟：本机那一份开不了 ⇒ [local] 那一句（系统原话进详情）；别的同一问一答。 */
    private suspend fun <T : Any> transfer(
        how: Transfer,
        fd: () -> Int,
        progress: (Long, Long) -> Unit,
        decode: (Any?) -> T?,
        local: () -> String,
    ): Result<T> {
        val r =
            try {
                feed.transfer(how, fd, CHUNK_MS, progress)
            } catch (e: IOException) {
                return Result.failure(Failed(Problem(local(), e.toString())))
            }
        return reply(r, if (how is Transfer.Stage) STAGE else PULL, decode)
    }

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
    ): Result<T> = reply(feed.call(command, args, withinMs), command, decode)

    private suspend fun <T : Any> reply(
        r: Reply,
        command: String,
        decode: (Any?) -> T?,
    ): Result<T> {
        val m = machine()
        return when (r) {
            is Reply.Ok -> decode(r.data)?.let { Result.success(it) } ?: Result.failure(Failed(unreadableReply(m, command, r.data.toString())))
            else -> Result.failure(Failed(r.problem(m, command) ?: Problem("", "")))
        }
    }

    companion object {
        /** 写的一问等多久（同桌面写面一趟）。 */
        const val WRITE_MS: Long = 20_000L

        /** 一块（上传 · 下载）· 提交等多久。 */
        const val CHUNK_MS: Long = 20_000L

        /** 提交撞了同名的码。 */
        private const val EXISTS = "exists"

        /** 分块传两半出错时详情里点名的命令。 */
        private const val STAGE = "files-stage-chunk"
        private const val PULL = "files-read-chunk"
    }
}
