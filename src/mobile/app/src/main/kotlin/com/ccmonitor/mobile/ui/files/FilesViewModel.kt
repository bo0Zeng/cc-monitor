package com.ccmonitor.mobile.ui.files

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.FilesReadText
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ssh.HostConnector
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.util.UUID

/** 文件屏此刻（在接 / 那台说打不开 / 列出来了）。 */
sealed interface FilesUi {
    data object Connecting : FilesUi

    data class Error(
        val problem: Problem,
    ) : FilesUi

    data object Ready : FilesUi
}

/**
 * 文件屏的浏览状态与那几件写操作。读写都问那台后端（[FileOps]，经这台的常驻流）；SSH 那层只为让常驻流通着：
 * 与终端共用一条主机连接，[hostConnector] 的 retain/release 按持有者记账，关本屏只 release。
 * 放在 VM 里，是为了在每个 tab 自己的 ViewModelStore 里保活：切走再回来不丢当前目录。
 */
class FilesViewModel(
    private val hostId: String,
    private val hostRepo: HostRepository,
    private val hostConnector: HostConnector,
    backends: HostBackends,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    private val feed = backends.of(hostId)
    private val ops = FileOps(feed) { hostRepo.get(hostId)?.label ?: hostId }

    private val _ui = MutableStateFlow<FilesUi>(FilesUi.Connecting)
    val ui: StateFlow<FilesUi> = _ui.asStateFlow()

    private val _cwd = MutableStateFlow<RemotePath?>(null)
    val cwd: StateFlow<RemotePath?> = _cwd.asStateFlow()

    private val _listing = MutableStateFlow(FilesLs.Listing(emptyList(), truncated = false, total = 0))
    val listing: StateFlow<FilesLs.Listing> = _listing.asStateFlow()

    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy.asStateFlow()

    /** 上传 · 下载在走时的进度（0..1）；不知道总数 · 不在传 ⇒ `null`。 */
    private val _progress = MutableStateFlow<Float?>(null)
    val progress: StateFlow<Float?> = _progress.asStateFlow()

    private val _sortBySize = MutableStateFlow(false)
    val sortBySize: StateFlow<Boolean> = _sortBySize.asStateFlow()

    /** 一次性提示：做成的一句，或那台写好的没做成的那一句（带复制详情）。 */
    private val _events = MutableSharedFlow<Notice>(extraBufferCapacity = 8)
    val events: SharedFlow<Notice> = _events.asSharedFlow()

    private val holderToken = UUID.randomUUID().toString()

    init {
        hostConnector.retain(hostId, holderToken)
        connect()
        // 换了一条流（`epoch` 变了）：之前在路上的问答作废了 ⇒ 打不开的那一屏再问一次，列着的那一屏重列。
        viewModelScope.launch {
            feed.epoch.drop(1).collect {
                when (_ui.value) {
                    is FilesUi.Error -> connect()
                    FilesUi.Ready -> refresh()
                    FilesUi.Connecting -> {}
                }
            }
        }
    }

    /** SSH 那层通着（常驻流在它上面）；没有 · 死了 ⇒ 当场（重）连。 */
    private suspend fun ensureSsh() {
        val epoch = hostConnector.retain(hostId, holderToken)
        if (hostConnector.connection(hostId)?.isConnected == true) return
        val host = hostRepo.get(hostId) ?: error(copyText("mobSession.host.missing", "id" to hostId))
        hostConnector.connect(host, expectedGen = epoch)
    }

    /** 初次与「重试」共用：起点是那台给的家；重试留在当前目录。 */
    @Suppress("TooGenericExceptionCaught") // 建连可抛各类异常（IO · 认证 · 超时 …）⇒ 统一兜成 Error 态（取消先重抛）
    fun connect() {
        viewModelScope.launch {
            _ui.value = FilesUi.Connecting
            _ui.value =
                try {
                    withContext(io) { ensureSsh() }
                    val dir = _cwd.value ?: ops.home().getOrThrow()
                    _listing.value = ops.list(dir).getOrThrow()
                    _cwd.value = dir
                    FilesUi.Ready
                } catch (ce: CancellationException) {
                    throw ce
                } catch (e: FileOps.Failed) {
                    FilesUi.Error(e.problem)
                } catch (e: Exception) {
                    FilesUi.Error(Problem(e.message ?: e.toString(), ""))
                }
        }
    }

    /** 进一个目录（上级 · 面包屑 · 书签 · 点目录都走它）：列不出 ⇒ 留在原处、说那台那一句。 */
    fun navigateTo(dir: RemotePath) {
        viewModelScope.launch {
            _busy.value = true
            ops
                .list(dir)
                .onSuccess {
                    _listing.value = it
                    _cwd.value = dir
                }.onFailure(::say)
            _busy.value = false
        }
    }

    private suspend fun refresh() {
        val dir = _cwd.value ?: return
        ops.list(dir).onSuccess { _listing.value = it }.onFailure(::say)
    }

    fun toggleSort() {
        _sortBySize.value = !_sortBySize.value
    }

    private fun say(e: Throwable) {
        if (e is CancellationException) throw e
        val p = (e as? FileOps.Failed)?.problem ?: Problem(e.message ?: e.toString(), "")
        _events.tryEmit(Notice(p.text, p.detail.ifBlank { null }))
    }

    /** 当前目录里一件写操作：做成 ⇒ [done]（有的话）、重列；没成 ⇒ 那台那一句。 */
    private fun write(
        done: String? = null,
        block: suspend (RemotePath) -> Result<Unit>,
    ) {
        val dir = _cwd.value ?: return
        viewModelScope.launch {
            _busy.value = true
            block(dir)
                .onSuccess {
                    done?.let { said -> _events.tryEmit(Notice(said)) }
                    refresh()
                }.onFailure(::say)
            _busy.value = false
        }
    }

    fun mkdir(typed: String) = write { ops.mkdir(it, typed) }

    fun create(typed: String) = write { ops.create(it, typed) }

    fun rename(
        entry: FilesLs.Entry,
        typed: String,
    ) = write { ops.rename(it, entry.path.name, typed) }

    fun delete(entry: FilesLs.Entry) = write { ops.delete(it, entry.path.name, entry.isDir) }

    fun chmod(
        entry: FilesLs.Entry,
        mode: Long,
    ) = write { ops.chmod(it, entry.path.name, mode) }

    /** 改权限那一框的起点：此刻的权限位（八进制）；那台没有这一说 · 读不到 ⇒ `null`。 */
    suspend fun modeText(entry: FilesLs.Entry): String? = ops.mode(entry.path).getOrNull()?.toString(OCTAL)

    /** 下载到 [destDir]（Composable 传 App 自己的外部文件目录）。 */
    fun download(
        entry: FilesLs.Entry,
        destDir: File?,
    ) {
        viewModelScope.launch {
            _busy.value = true
            val local = File(destDir, entry.name)
            withContext(io) {
                try {
                    local.outputStream().use { out -> ops.download(entry.path, out, ::track) }
                } catch (e: IOException) {
                    Result.failure(FileOps.Failed(Problem(copyText("mobFiles.local.writeFailed", "path" to local.absolutePath), e.toString())))
                }
            }.onSuccess { _events.tryEmit(Notice(copyText("mobFiles.download.done", "path" to local.absolutePath))) }
                .onFailure {
                    local.delete()
                    say(it)
                }
            _progress.value = null
            _busy.value = false
        }
    }

    /** 上传 [name]（[open] 每次给一条从头读的流；[size] 不知道 ⇒ `-1`）。 */
    fun upload(
        name: String,
        size: Long,
        open: () -> InputStream,
    ) = write(copyText("mobFiles.upload.done", "name" to name)) { dir ->
        withContext(io) { ops.upload(dir, name, open, size, ::track) }.also { _progress.value = null }
    }

    private fun track(
        got: Long,
        total: Long,
    ) {
        _progress.value = if (total > 0) (got.toFloat() / total).coerceIn(0f, 1f) else null
    }

    /** 预览：读那一份文本。没成 ⇒ 已说那一句。 */
    suspend fun readText(entry: FilesLs.Entry): Result<FilesReadText.Text> {
        _busy.value = true
        return ops.readText(entry.path).also {
            _busy.value = false
            it.onFailure(::say)
        }
    }

    /** 存回编辑过的文本（带读时的摘要）。做成才关编辑框；没成 ⇒ 那台那一句，编辑框留着。 */
    suspend fun saveText(
        entry: FilesLs.Entry,
        text: String,
        sha256: String,
    ): Result<Unit> {
        val dir = _cwd.value ?: return Result.failure(IllegalStateException("not listed yet"))
        _busy.value = true
        val r = ops.writeText(dir, entry.path.name, text, sha256)
        r
            .onSuccess {
                _events.tryEmit(Notice(copyText("mobFiles.save.done")))
                refresh()
            }.onFailure(::say)
        _busy.value = false
        return r
    }

    override fun onCleared() {
        hostConnector.release(hostId, holderToken)
    }

    private companion object {
        const val OCTAL = 8
    }
}
