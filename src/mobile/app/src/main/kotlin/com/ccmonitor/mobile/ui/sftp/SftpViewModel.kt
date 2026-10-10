package com.ccmonitor.mobile.ui.sftp

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ssh.SftpEntry
import com.ccmonitor.mobile.core.ssh.SshTransport
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
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.File
import java.util.UUID

// 复用连接前的探活超时，与会话侧 FASTPATH_PROBE_TIMEOUT_MS 同值：健康连接亚秒往返，5s 判死。
private const val SFTP_PROBE_TIMEOUT_MS = 5_000L

/** SFTP 屏状态（连接中 / 错误 / 就绪）。 */
sealed interface SftpUi {
    data object Connecting : SftpUi

    data class Error(
        val message: String,
    ) : SftpUi

    data object Ready : SftpUi
}

/**
 * SFTP 的连接编排与浏览状态。放在 VM 里，是为了在每个 tab 自己的 ViewModelStore 里保活：切走再回来不断连、不丢导航。
 *
 * SFTP 与终端共享一条主机连接（id=hostId，sshj 多路复用 shell 与 SFTP 子系统）。
 * [hostConnector] 的 retain/release 是引用计数：关本屏只 release，归零才真断。
 */
class SftpViewModel(
    private val hostId: String,
    private val hostRepo: HostRepository,
    private val hostConnector: HostConnector, // 建连、持有、查句柄都经它
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    private val _ui = MutableStateFlow<SftpUi>(SftpUi.Connecting)
    val ui: StateFlow<SftpUi> = _ui.asStateFlow()

    private val _path = MutableStateFlow(".")
    val path: StateFlow<String> = _path.asStateFlow()

    private val _entries = MutableStateFlow<List<SftpEntry>>(emptyList())
    val entries: StateFlow<List<SftpEntry>> = _entries.asStateFlow()

    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy.asStateFlow()

    private val _sortBySize = MutableStateFlow(false) // false=名称(目录在前), true=大小
    val sortBySize: StateFlow<Boolean> = _sortBySize.asStateFlow()

    // 远端 OS（「终端」按钮 sftpCwdToShell 归一用）。
    private val _hostOs = MutableStateFlow<String?>(null)
    val hostOs: StateFlow<String?> = _hostOs.asStateFlow()

    // 一次性提示（Toast）：上传/下载/读写成功或失败。
    private val _events = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val events: SharedFlow<String> = _events.asSharedFlow()

    // 串行化按需连接：并发 op 同时 ensureConn 时，首个连成后其余复用。连接管理器另有 single-flight 兜底，
    // 这把锁保证本 VM 内的 op 不各自触发 connect。
    private val connMutex = Mutex()

    // 本 VM 的唯一持有者 token。retain/release 按 token 记账，强制断开清集后重连时幂等重 retain。
    private val holderToken = UUID.randomUUID().toString()

    init {
        hostConnector.retain(hostId, holderToken) // 登记为共享连接持有者（onCleared release，无持有者才真断）
        viewModelScope.launch { _hostOs.value = hostRepo.get(hostId)?.os }
        connect()
    }

    // 确保共享主机连接（与终端 tab 同一条）。不存在或已死则即时（重）连；活则复用。
    private suspend fun ensureConn(): SshTransport =
        connMutex.withLock {
            // 每次幂等重 retain：覆盖所有重连点，强制断开清集后恢复本屏持有。
            // retain 的纪元交给 connect 守门：强制断开落在 retain 与建连之间时弃用，不成孤儿。
            val epoch = hostConnector.retain(hostId, holderToken)
            // 只看 isConnected 不够：对端掉电的死 TCP（无 FIN）约 75s 内仍报 true，复用它会让 sftpList 卡死。
            // 所以复用前主动探活（一次往返、5s 超时）；探活败就走真连接重建。没有现成连接时不空探。
            hostConnector.connection(hostId)?.takeIf { it.isConnected && it.probeAliveOrFalse() } ?: run {
                val host = hostRepo.get(hostId) ?: error("未找到主机: $hostId")
                // 认证、跳板、竞速都在 HostConnector 里。SFTP 没有选身份流（pendingIdentity=null）。
                hostConnector.connect(host, expectedGen = epoch)
            }
        }

    /** 探活：活=true；失败或抛错=false（视为死连接，走真连接重建）。 */
    private suspend fun SshTransport.probeAliveOrFalse(): Boolean = runCatching { probeAlive(SFTP_PROBE_TIMEOUT_MS) }.getOrDefault(false)

    /**
     * 初次连接与 Error 态「重试」共用（ensureConn 会重连死连接）。首次(path=".")解析 home，重试保留当前目录。
     * `@Suppress`：连接可抛各类异常（IO/认证/超时/…）→ 统一兜成 Error 态（CancellationException 已先重抛）。
     */
    @Suppress("TooGenericExceptionCaught")
    fun connect() {
        viewModelScope.launch {
            _ui.value = SftpUi.Connecting
            _ui.value =
                withContext(io) {
                    try {
                        val c = ensureConn()
                        _path.value = if (_path.value == ".") c.sftpRealPath(".") else _path.value
                        _entries.value = c.sftpList(_path.value)
                        SftpUi.Ready
                    } catch (ce: CancellationException) {
                        throw ce // 离屏/导航取消 → 正常取消，不展示为错误
                    } catch (e: Exception) {
                        SftpUi.Error(e.message ?: e.toString())
                    }
                }
        }
    }

    private suspend fun refresh() {
        _busy.value = true
        runCatching { _entries.value = ensureConn().sftpList(_path.value) }
            .onFailure {
                if (it is CancellationException) throw it // 离屏取消（viewModelScope）须传播，不折成 Error 态
                _ui.value = SftpUi.Error(it.message ?: it.toString())
            }
        _busy.value = false
    }

    /** 统一导航入口（上级/进目录/书签/面包屑都走它）。 */
    fun navigateTo(p: String) {
        _path.value = p
        viewModelScope.launch { refresh() }
    }

    fun toggleSort() {
        _sortBySize.value = !_sortBySize.value
    }

    // 当前目录下一次写操作（busy + 错误事件 + 成功刷新）。
    private fun runOp(
        label: String,
        block: suspend (SshTransport) -> Unit,
    ) {
        viewModelScope.launch {
            _busy.value = true
            runCatching {
                val c = ensureConn()
                withContext(io) { block(c) }
            }.onSuccess { refresh() } // refresh 重置 busy
                .onFailure {
                    if (it is CancellationException) throw it // 取消须传播，不折成失败事件
                    _events.tryEmit("$label 失败: ${it.message}")
                    _busy.value = false
                }
        }
    }

    fun mkdir(name: String) = runOp("新建目录") { it.sftpMkdir(joinPath(_path.value, name)) }

    fun rename(
        fromPath: String,
        newName: String,
    ) = runOp("重命名") { it.sftpRename(fromPath, joinPath(_path.value, newName)) }

    fun delete(entry: SftpEntry) = runOp("删除") { it.sftpDelete(entry.path, entry.isDir) }

    /** 下载到 [destDir]（Composable 传 getExternalFilesDir）。 */
    fun download(
        entry: SftpEntry,
        destDir: File?,
    ) {
        viewModelScope.launch {
            _busy.value = true
            runCatching {
                val local = File(destDir, entry.name)
                withContext(io) { ensureConn().sftpDownload(entry.path, local) }
                local.absolutePath
            }.onSuccess { _events.tryEmit("已下载到 $it") }
                .onFailure {
                    if (it is CancellationException) throw it // 取消须传播
                    _events.tryEmit("下载失败: ${it.message}")
                }
            _busy.value = false
        }
    }

    /** 上传 Composable 从 URI 备好的临时文件（上传后删临时文件）。 */
    fun upload(
        localTmp: File,
        remoteName: String,
    ) {
        viewModelScope.launch {
            _busy.value = true
            runCatching {
                withContext(io) {
                    ensureConn().sftpUpload(localTmp, joinPath(_path.value, remoteName))
                    localTmp.delete()
                }
            }.onSuccess {
                _events.tryEmit("已上传 $remoteName")
                refresh()
            }.onFailure {
                if (it is CancellationException) throw it // 取消须传播
                _events.tryEmit("上传失败: ${it.message}")
                _busy.value = false
            }
        }
    }

    /** 预览：读全文（Composable 拿到后开预览对话框）。失败已 emit 事件。 */
    suspend fun readPreview(path: String): Result<String> = readOp { it.sftpReadText(path) }

    /** 读供编辑的全文；null=过大/二进制（不可编辑）。失败已 emit 事件。 */
    suspend fun readForEdit(path: String): Result<String?> = readOp { it.sftpReadTextForEdit(path) }

    private suspend fun <T> readOp(block: suspend (SshTransport) -> T): Result<T> {
        _busy.value = true
        return runCatching { ensureConn().let { c -> withContext(io) { block(c) } } }
            .also { _busy.value = false }
            .onFailure {
                if (it is CancellationException) throw it // 取消须传播（不折成读取失败事件 + 不返回 failure Result）
                _events.tryEmit("读取失败: ${it.message}")
            }
    }

    /**
     * 以 UTF-8 覆写远端文件，成功后刷新。返回 Result 供 Composable 决定成功才关对话框：
     * 失败保留 ConfirmSave 态，可「返回编辑」保住已改全文。
     */
    suspend fun saveText(
        path: String,
        text: String,
    ): Result<Unit> {
        _busy.value = true
        val r = runCatching { withContext(io) { ensureConn().sftpWriteText(path, text) } }
        r
            .onSuccess {
                _events.tryEmit("已保存")
                refresh() // refresh 复位 busy
            }.onFailure {
                if (it is CancellationException) throw it // 取消须传播
                _events.tryEmit("保存失败: ${it.message}")
                _busy.value = false
            }
        return r
    }

    override fun onCleared() {
        // 关本屏：release 共享连接持有。无持有者才真断，同主机终端 tab 还在用就不断。
        hostConnector.release(hostId, holderToken)
    }
}
