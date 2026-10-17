package com.ccmonitor.mobile.ui.files

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.claude.link.BatchAnswer
import com.ccmonitor.mobile.core.claude.link.BatchEvent
import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.FilesReadText
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.PickMenu
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ssh.HostConnector
import com.ccmonitor.mobile.ui.chat.newsession.DirBrowser
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
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
    private val ops = FileOps(feed, ::machine)

    private val _ui = MutableStateFlow<FilesUi>(FilesUi.Connecting)
    val ui: StateFlow<FilesUi> = _ui.asStateFlow()

    private val _cwd = MutableStateFlow<RemotePath?>(null)
    val cwd: StateFlow<RemotePath?> = _cwd.asStateFlow()

    private val _listing = MutableStateFlow(FilesLs.Listing(emptyList(), truncated = false, total = 0))
    val listing: StateFlow<FilesLs.Listing> = _listing.asStateFlow()

    private val _selection = MutableStateFlow(Selection())

    /** 选择模式里选中的那几项（空 ⇒ 不在选择模式）。 */
    val selection: StateFlow<Selection> = _selection.asStateFlow()

    /** 那台此刻通着（断着 ⇒ 选中照留、能做的那几件由核心灰掉，列表顶一条说离线）。 */
    val online: StateFlow<Boolean> = feed.state.map { it is LinkState.Up }.stateIn(viewModelScope, SharingStarted.Eagerly, false)

    /** 选中那几项此刻能做什么（核心照那几行与此刻那一台判；不在选择模式 ⇒ 空）。 */
    val menu: StateFlow<PickMenu> =
        combine(_selection, _listing, feed.state) { sel, l, _ ->
            if (sel.active) feed.picks(sel.of(l.entries).map { it.row }) else PickMenu()
        }.stateIn(viewModelScope, SharingStarted.Eagerly, PickMenu())

    /** 这一屏的那一层（[dir] 换了 ⇒ 选中清空；同一层重列 ⇒ 只留还在的）。 */
    private fun shown(
        dir: RemotePath,
        listed: FilesLs.Listing,
    ) {
        _selection.value = if (dir == _cwd.value) _selection.value.kept(listed.entries) else Selection()
        _listing.value = listed
        _cwd.value = dir
    }

    /** 长按 / 选择模式里点一下：切换这一项。 */
    fun pick(e: FilesLs.Entry) {
        _selection.value = _selection.value.toggle(e)
    }

    fun pickAll() {
        _selection.value = _selection.value.all(_listing.value.entries)
    }

    /** ✕ · 返回键：退出选择模式。 */
    fun unpick() {
        _selection.value = Selection()
    }

    /** 选中的那几行（照列表的先后）。 */
    fun picked(): List<FilesLs.Entry> = _selection.value.of(_listing.value.entries)

    /** 不在选择模式时点一下这一行做什么（核心照表给：`open` · `preview` · `download`；什么都不做 ⇒ `null`）。 */
    fun tapOf(e: FilesLs.Entry): String? = feed.picks(listOf(e.row)).tap

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
        viewModelScope.launch { feed.batches.collect(::onBatch) }
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
                    shown(dir, ops.list(dir).getOrThrow())
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

    /** 别处点名的那一份打开成了预览（名字 · 全文）。 */
    private val _opened = MutableSharedFlow<Pair<String, String>>(extraBufferCapacity = 1)
    val opened: SharedFlow<Pair<String, String>> = _opened.asSharedFlow()

    /**
     * 打开别处点名的一个路径（轮换屏的［看代码］…）：连上以后问那台它是什么，目录 ⇒ 进去；文件 ⇒ 进它所在的目录、开预览。
     * 没成 ⇒ 那台那一句。
     */
    fun open(text: String) {
        viewModelScope.launch {
            _ui.first { it == FilesUi.Ready }
            when (val at = ops.open(text).onFailure(::say).getOrNull() ?: return@launch) {
                is FileOps.Opening.Dir -> enter(at.dir)
                is FileOps.Opening.File -> {
                    // 名字与预览那道门都在它那一行上；不在这一屏（截了）⇒ 门不明，读那一步说那一句。
                    val e = enter(at.dir)?.entries?.firstOrNull { it.path == at.file }
                    if (e != null && e.preview == null) return@launch
                    readText(at.file, e?.preview).onSuccess { _opened.tryEmit((e?.name ?: at.file.shown) to it.text) }
                }
            }
        }
    }

    /** 进一个目录（上级 · 面包屑 · 书签 · 点目录都走它）：列不出 ⇒ 留在原处、说那台那一句。 */
    fun navigateTo(dir: RemotePath) {
        viewModelScope.launch { enter(dir) }
    }

    private suspend fun enter(dir: RemotePath): FilesLs.Listing? {
        _busy.value = true
        val r =
            ops
                .list(dir, _sortBySize.value)
                .onSuccess { shown(dir, it) }
                .onFailure(::say)
        _busy.value = false
        return r.getOrNull()
    }

    private suspend fun refresh() {
        val dir = _cwd.value ?: return
        ops.list(dir, _sortBySize.value).onSuccess { shown(dir, it) }.onFailure(::say)
    }

    /** 名称 ⇄ 大小：那台照它重排一遍。 */
    fun toggleSort() {
        _sortBySize.value = !_sortBySize.value
        viewModelScope.launch { refresh() }
    }

    /** 底部说一句（点了灰着的那一项：核心写好的为什么）。 */
    fun say(said: String) {
        _events.tryEmit(Notice(said))
    }

    /** 这台在屏上的名字。 */
    suspend fun machine(): String = hostRepo.get(hostId)?.label ?: hostId

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
    ) = write { ops.rename(it, entry.rel, typed) }

    private val _batch = MutableStateFlow<BatchEvent?>(null)

    /** 底部浮卡画的那一组（在做 · 在问同名 · 断开 · 收场了还有没成的）；全成了 ⇒ `null`（结局句走 toast）。 */
    val batch: StateFlow<BatchEvent?> = _batch.asStateFlow()

    private val _undos = MutableSharedFlow<Pair<String, Long>>(extraBufferCapacity = 4)

    /** 全成了、撤得了（改权限）：那一句 ＋ 那一组的编号（屏上 toast［撤销］）。 */
    val undos: SharedFlow<Pair<String, Long>> = _undos.asSharedFlow()

    /** 这一屏起的那几组（别处起的不画）。 */
    private val mine = mutableSetOf<Long>()

    /** 编号还没回到这里、核心已经交来的那一格（每组只留最后一格；编号一回来照它画）。 */
    private val early = LinkedHashMap<Long, BatchEvent>()

    private fun onBatch(e: BatchEvent) {
        if (e.id !in mine) {
            early[e.id] = e
            if (early.size > EARLY_KEPT) early.remove(early.keys.first())
            return
        }
        when {
            e.phase == BatchEvent.ENDED && e.failed.isEmpty() -> {
                _batch.value = null
                if (e.undo) _undos.tryEmit(e.said.orEmpty() to e.id) else _events.tryEmit(Notice(e.said.orEmpty()))
            }
            e.phase == BatchEvent.DROPPED -> _batch.value = null
            else -> _batch.value = e
        }
        if (e.over) viewModelScope.launch { refresh() }
    }

    /** 起一组（[start] 回编号；0 ＝ 这台不在 ⇒ 那一句）；起了就退出选择模式。 */
    private fun group(start: suspend (RemotePath) -> Long) {
        val dir = _cwd.value ?: return
        unpick()
        viewModelScope.launch {
            val id = start(dir)
            if (id == 0L) {
                _events.tryEmit(Notice(copyText("front.body.offline", "machine" to machine())))
            } else {
                mine += id
                early.remove(id)?.let(::onBatch)
            }
        }
    }

    fun delete(entries: List<FilesLs.Entry>) = group { ops.delete(it, entries) }

    fun chmod(
        entries: List<FilesLs.Entry>,
        mode: String,
    ) = group { ops.chmod(it, entries, mode) }

    fun copyTo(
        entries: List<FilesLs.Entry>,
        to: RemotePath,
    ) = group { ops.copy(it, entries, to) }

    /** 复制到… 那一屏的目录浏览（同一台、只列文件夹；与新建会话选目录同一个浏览器）。 */
    suspend fun browser(): DirBrowser = DirBrowser(feed, machine(), viewModelScope)

    /** ［重试失败的］（[undo] ⇒ ［撤销］）：照那一组另起。 */
    fun again(
        id: Long,
        undo: Boolean,
    ) = group { ops.again(id, undo) }

    /** 同名那一问的答（`null` ＝ 取消）。 */
    fun answerBatch(
        id: Long,
        answer: BatchAnswer?,
    ) = feed.batchAnswer(id, answer)

    /** 浮卡上的［停］。 */
    fun stopBatch(id: Long) = feed.batchStop(id)

    /** 收场了还留着的那张卡收起。 */
    fun dismissBatch() {
        _batch.value = null
    }

    /** 改权限那一框的起点：此刻的权限位（八进制）；那台没有这一说 · 读不到 ⇒ `null`。 */
    suspend fun modeText(entry: FilesLs.Entry): String? = ops.mode(entry.path).getOrNull()?.toString(OCTAL)

    /** 下载到 [destDir]（Composable 传 App 自己的外部文件目录）：[open] 开本机那一份、交出描述符。 */
    fun download(
        entry: FilesLs.Entry,
        destDir: File?,
        open: (File) -> Int,
    ) {
        viewModelScope.launch {
            _busy.value = true
            val local = File(destDir, entry.name)
            withContext(io) { ops.download(entry.path, { open(local) }, local.absolutePath, ::track) }
                .onSuccess { _events.tryEmit(Notice(copyText("mobFiles.download.done", "path" to local.absolutePath))) }
                .onFailure {
                    local.delete()
                    say(it)
                }
            _progress.value = null
            _busy.value = false
        }
    }

    /** 上传撞了同名、在问（[FileOps.Upload.Clash]：那台的同名清单与那一句；在哪个目录）；没在问 ⇒ `null`。 */
    private val _clash = MutableStateFlow<Pair<RemotePath, FileOps.Upload.Clash>?>(null)
    val clash: StateFlow<Pair<RemotePath, FileOps.Upload.Clash>?> = _clash.asStateFlow()

    /** 上传 [name]（[fd] 每次现开所选那一份、交出描述符）。撞了同名 ⇒ 落进 [clash] 等答。 */
    fun upload(
        name: String,
        fd: () -> Int,
    ) = landed(name) { dir -> withContext(io) { ops.upload(dir, name, fd, ::track) }.also { _progress.value = null } }

    /** 同名那一问的答：取消 ⇒ 不传了（暂存件交那台的孤儿扫）；否则同一份暂存件带这个答再交一次（只一项：整组那一个答）。 */
    fun answer(a: BatchAnswer?) {
        val (dir, c) = _clash.value ?: return
        _clash.value = null
        val onExists = (a as? BatchAnswer.All)?.decide ?: return
        landed(c.name, dir) { ops.commit(it, c.name, c.staged, onExists) }
    }

    /** 上传那一件的一趟：放好了 ⇒ 那一句、重列；撞了同名 ⇒ 去问；没成 ⇒ 那台那一句。 */
    private fun landed(
        name: String,
        dir: RemotePath? = _cwd.value,
        block: suspend (RemotePath) -> Result<FileOps.Upload>,
    ) {
        val at = dir ?: return
        viewModelScope.launch {
            _busy.value = true
            block(at)
                .onSuccess {
                    when (it) {
                        FileOps.Upload.Done -> {
                            _events.tryEmit(Notice(copyText("mobFiles.upload.done", "name" to name)))
                            refresh()
                        }
                        is FileOps.Upload.Clash -> _clash.value = at to it
                    }
                }.onFailure(::say)
            _busy.value = false
        }
    }

    private fun track(
        got: Long,
        total: Long,
    ) {
        _progress.value = if (total > 0) (got.toFloat() / total).coerceIn(0f, 1f) else null
    }

    /** 照那道门读那一份文本（预览 · 编辑）：门不许 ⇒ 说那台写好的那一句、不读；没成 ⇒ 已说那一句。 */
    suspend fun readText(
        path: RemotePath,
        gate: FilesLs.Open?,
    ): Result<FilesReadText.Text> {
        val cap = gate?.cap
        if (cap == null) {
            val said = gate?.said ?: copyText("rsFilewinSource.gates.unlisted")
            _events.tryEmit(Notice(said))
            return Result.failure(IllegalStateException(said))
        }
        _busy.value = true
        return ops.readText(path, cap).also {
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
        val r = ops.writeText(dir, entry.rel, text, sha256)
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

        /** 编号还没回来时先收着几组的那一格。 */
        const val EARLY_KEPT = 16
    }
}
