package com.ccmonitor.mobile.ui.session

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.data.repo.LauncherRepository
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import com.ccmonitor.mobile.core.ssh.HostKeyChangedException
import com.ccmonitor.mobile.core.ssh.KnownHostStore
import com.ccmonitor.mobile.core.ssh.SessionStatus
import com.ccmonitor.mobile.core.ssh.ShellChannel
import com.ccmonitor.mobile.core.ssh.SshConnectEvent
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.SshTransport
import com.ccmonitor.mobile.core.ssh.commandChannel
import com.ccmonitor.mobile.core.ssh.commandExecutor
import com.ccmonitor.mobile.core.terminal.TerminalSession
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ssh.HostConnector
import com.ccmonitor.mobile.ssh.LaunchSpec
import com.ccmonitor.mobile.ssh.PosixDialect
import com.ccmonitor.mobile.ssh.ShellDialect
import com.ccmonitor.mobile.ssh.dialectFor
import com.ccmonitor.mobile.ssh.initialCommands
import com.ccmonitor.mobile.ssh.resolveTmuxSession
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.UUID
import kotlin.coroutines.coroutineContext

/** 终端屏的连接态。 */
sealed interface SessionUiState {
    data object Connecting : SessionUiState

    /** 主机未绑身份且有多个身份：一步选一个，选中即绑定并连接。 */
    data class NeedIdentity(
        val identities: List<Identity>,
    ) : SessionUiState

    /**
     * 服务器身份变了（TOFU 指纹不符）。这是唯一一个「拒绝连接但可恢复」的态。
     *
     * 不并进 [Error]：一个字符串给不出出路，删掉服务器重加也没用（`KnownHost` 与 `Host` 之间没有外键级联）。
     * 这个态带上新旧指纹，让恢复成为一次有依据的选择。
     */
    data class HostKeyChanged(
        val host: String,
        val port: Int,
        val expectedFingerprint: String,
        val offeredFingerprint: String,
    ) : SessionUiState

    data class Error(
        val message: String,
    ) : SessionUiState

    data class Connected(
        val session: TerminalSession,
        val cwd: String?,
        // 远端 shell 方言，界面按它的能力位开关 tmux 与阅读面。随连接态一起到位，界面不必再读一次 host。
        val dialect: ShellDialect = PosixDialect,
    ) : SessionUiState
}

/**
 * 终端屏的连接编排：连接（竞速）→ openShell → [TerminalSession] → 首发命令或 resume。
 * 暴露 [uiState] / [status] / [channel] / [notices]；[retry] 重连；[onCleared] 关会话并
 * `hostConnector.release`（引用计数，归零才真断）。前台保活服务要 Context，由 Composable 在 Connected 时起。
 *
 * 连接成功路径要建 termlib 原生 [TerminalSession]，纯 JVM 测不了，靠设备上的端到端验。
 */
class SessionViewModel(
    private val hostId: String,
    cdArg: String?,
    launcherIdArg: String?,
    tmuxArg: String?, // 进终端 tab 首连即接回的会话名（敲那台渲出的那一行）；普通 tab 为 null
    private val manager: SshConnectionManager,
    private val hostRepo: HostRepository,
    private val identityRepo: IdentityRepository,
    private val launcherRepo: LauncherRepository,
    private val hostConnector: HostConnector, // 建连编排，retain / release / connection 也经它
    backends: HostBackends, // 这台的常驻流：列会话 · 进终端那一行 · 送字 · 抓屏 · 结束都问那台核心
    private val knownHostStore: KnownHostStore, // 主机钥匙变更的恢复：确认后 forget 再重连
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    private val cd: String? = cdArg?.ifBlank { null } // Koin 参数不传 null，空串当 null
    private val launcherId: String? = launcherIdArg?.ifBlank { null } // 启动配置 id

    private val _uiState = MutableStateFlow<SessionUiState>(SessionUiState.Connecting)
    val uiState: StateFlow<SessionUiState> = _uiState.asStateFlow()

    /** 连接过程的详细事件日志，连接中界面实时滚动；每次连接开始清空。 */
    private val _connectLog = MutableStateFlow<List<SshConnectEvent>>(emptyList())
    val connectLog: StateFlow<List<SshConnectEvent>> = _connectLog.asStateFlow()

    private var session: TerminalSession? = null
    private var connectJob: Job? = null

    /**
     * 当前 shell 开在哪条 [SshTransport] 上（[establishConnectedState] 成功时置），是判断断流原因的基准。
     * io 线程写、收集器读，所以 @Volatile。
     */
    @Volatile
    private var shellOpenedOn: SshTransport? = null

    /** 本次连接选定但还没持久化的身份。认证成功才写 host.authRef，失败可重选，不会卡在错的身份上。 */
    private var pendingIdentityId: String? = null

    /**
     * 该连接的流式命令通道，每次 exec 重新解析连接，所以跟随重连。供 tmux 管理与阅读面用。
     * 无活连接时抛 ConnectionDeadException，绝不给空流；契约见 [commandChannel]。
     */
    val channel: RemoteCommandChannel = manager.commandChannel(hostId)

    /**
     * 同一条连接的一次性执行（带退出码与 stderr），与 [channel] 并列。
     *
     * 流式通道拿不到退出码，「stdout 空」分不清是成功还是命令根本没跑。要判成败的地方（探测文件大小、
     * 写操作）走这条；`tail -F` 那种长跑仍走 [channel]。
     */
    val executor: RemoteExecutor = manager.commandExecutor(hostId)

    /**
     * 只取本主机的会话状态（distinctUntilChanged，别的会话或文件屏变动不触发整屏重组）。
     * 初值取构造时的真实快照：新 tab 为 null（终端屏对 null 不显示断线条幅）；复用已连主机的 tab 为 CONNECTED；
     * 构造时已 ERROR 则条幅立刻显示。写死 CONNECTING 会让每次 Connected 首帧误闪「连接已断开」。
     */
    val status: StateFlow<SessionStatus?> =
        manager.sessions
            .map { it[hostId]?.status }
            .distinctUntilChanged()
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), manager.sessions.value[hostId]?.status)

    /**
     * 当前终端 shell 流是否已断，跟随当前 [SessionUiState.Connected] 会话的 streamClosed。
     * 依赖 termlib 的 TerminalSession，所以留在 VM；断因判定与重挂决策在 [SessionReattachDetector]。
     * 新连接到位时自动复位为 false。
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val terminalDropped: StateFlow<Boolean> =
        uiState
            .flatMapLatest { st -> if (st is SessionUiState.Connected) st.session.streamClosed else flowOf(false) }
            .distinctUntilChanged()
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /**
     * 断因判定与重挂决策。输入 [terminalDropped]、活连接与 shell 所在连接的 provider；
     * 新 shell 建成时经 [SessionReattachDetector.notifyNewShellOpened] 复位。
     */
    private val reattachDetector =
        SessionReattachDetector(viewModelScope, terminalDropped, status, { hostConnector.connection(hostId) }) { shellOpenedOn }

    /** 是否需要自动重挂交互 shell；终端屏在前台时据此重挂。 */
    val reattachNeeded: StateFlow<Boolean> = reattachDetector.reattachNeeded

    /** 进终端 tab 要接回的会话名（首连与重连都敲那台渲出的那一行）；普通 tab 为 null。 */
    private val attachTo: String? = tmuxArg?.ifBlank { null }

    // 当前所在的 tmux 会话名，每个 tab 各记各的（同主机多 tab 不互相覆盖），跨重连保留。
    // 切模型 · 抓屏按它找那台名单里的终端。进终端 tab 以 [attachTo] 初始化。
    private val _tmuxSession = MutableStateFlow(attachTo)
    val tmuxSession: StateFlow<String?> = _tmuxSession.asStateFlow()

    /** 要那台核心做的终端那几件（tmux 管理窗 · 切模型 · 抓屏 · 进终端那一行）。 */
    val terminals = TerminalOps(backends.of(hostId), { hostRepo.get(hostId)?.label ?: hostId }, { hostRepo.get(hostId)?.agentKind })

    /** 可推送公钥的身份列表（命令面板「推送公钥」用）。 */
    val identities: StateFlow<List<Identity>> =
        identityRepo.observeAll().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * 一次性提示（进终端那一行要不到等）。用缓冲 Channel：connect 早于界面订阅时发的也不丢；
     * 只有一个收集方（终端屏收了弹 Toast）。
     */
    private val _notices = Channel<String>(Channel.BUFFERED)
    val notices: Flow<String> = _notices.receiveAsFlow()

    // 本 tab 的持有者 token。retain / release 按 token 记账，强制断开清空持有者后重连可幂等地重新 retain。
    private val holderToken = UUID.randomUUID().toString()

    init {
        connect()
    }

    private fun connect() {
        // 每次连接都登记持有者（首连与重连都走这里），幂等：强制断开清空持有者后，重连能恢复本 tab 的持有。
        // 记下 retain 时的纪元传给 connect：强制断开若落在 retain 之后、建连之前，connect 见纪元已变就弃掉这次连接，
        // 不会装进一条没人持有的连接（否则前台保活服务永远停不下来）。
        val epoch = hostConnector.retain(hostId, holderToken)
        connectJob?.cancel() // 重连时取消在途的连接

        connectJob =
            viewModelScope.launch {
                _uiState.value = SessionUiState.Connecting
                _connectLog.value = emptyList()
                if (!ensureIdentityBound()) return@launch // 无身份的主机先解决身份（自动绑 / 引导 / 快选）
                _uiState.value = withContext(io) { establishConnectedState(epoch) }
            }
    }

    // 连接可能抛各种异常（IO / 认证 / 超时…），统一兜成 Error 态；CancellationException 先重抛。
    @Suppress("TooGenericExceptionCaught")
    private suspend fun establishConnectedState(expectedGen: Int): SessionUiState {
        var created: TerminalSession? = null // 取消时要关掉它，否则 reader / writeExecutor 线程泄漏
        return try {
            val host = hostRepo.get(hostId) ?: error(copyText("mobSession.host.missing", "id" to hostId))
            // 认证、跳板、竞速都在 HostConnector 里；pendingIdentityId 是本次选定的身份。
            val conn =
                hostConnector.connect(
                    host,
                    pendingIdentityId,
                    onEvent = { ev -> _connectLog.update { it + ev } },
                    expectedGen = expectedGen, // 纪元守门，见 connect()
                )
            val shell = conn.openShell("xterm-256color", 80, 24)
            val ts = terminalSessionFor(shell)
            created = ts
            // 这次连接若已被取代（重连取消了本协程）或已离屏，抛 CancellationException，由下方 catch 关掉刚建的 ts。
            // 阻塞 IO 不及时响应取消，不显式守门的话，旧的在途连接会覆盖 session / uiState。
            coroutineContext.ensureActive()
            session = ts
            shellOpenedOn = conn // 断因判定的基准
            reattachDetector.notifyNewShellOpened() // 清掉上一个 shell 的断因
            persistPickedIdentity(host) // 认证成功后才持久化选定身份（密码认证跳过）
            sendStartupCommands(host, ts)
            SessionUiState.Connected(ts, host.defaultWorkingDir, dialectFor(host.os))
        } catch (ce: CancellationException) {
            runCatching { created?.close() } // 关掉本次（可能已建）的孤儿会话
            throw ce // 离屏或重连取消属正常，不标 Error
        } catch (e: HostKeyChangedException) {
            // 服务器身份变更不是普通连接失败：可恢复，但恢复动作有安全后果，要单独成态、显式确认。
            runCatching { created?.close() }
            SessionUiState.HostKeyChanged(
                host = e.host,
                port = e.port,
                expectedFingerprint = e.expectedFingerprint,
                offeredFingerprint = e.offeredFingerprint,
            )
        } catch (e: Exception) {
            runCatching { created?.close() }
            SessionUiState.Error(e.message ?: e.toString())
        }
    }

    /**
     * 确认主机钥匙是自己换的：忘掉旧钥匙再重连，重连时按首见接受并钉住新钥匙。
     *
     * 注意：只能在 [SessionUiState.HostKeyChanged] 页面显式二次确认后调。它放弃了对这台服务器已有的
     * 信任锚点，绝不能出现在任何自动重试路径里。
     */
    fun trustNewHostKey(
        host: String,
        port: Int,
    ) {
        viewModelScope.launch {
            knownHostStore.forget(host, port)
            retry()
        }
    }

    /**
     * 连接成功后的首发命令。进终端 tab：敲那台渲出的那一行（与桌面「命令已复制」同一条；找哪个窗格、怎么附着都在那台），
     * 要不到 ⇒ 留在普通 shell、说一声。其余照「用户在 ssh 里自己敲」：启动配置 / 主机设置的工作目录 · tmux 会话 · 命令。
     */
    private suspend fun sendStartupCommands(
        host: Host,
        ts: TerminalSession,
    ) {
        attachTo?.let { name ->
            terminals
                .attachLine(name)
                .onSuccess { ts.send("$it\n".toByteArray()) }
                .onFailure { _notices.trySend(it.message.orEmpty()) }
            return
        }
        // 有 launcherId 就用它的 LaunchSpec（工作目录 / tmux 会话 / 命令），否则只带 cd。
        val launcher = launcherId?.let { launcherRepo.get(it) }
        val baseSpec =
            launcher?.let { LaunchSpec(it.workingDir, it.tmuxSession, it.command) }
                ?: LaunchSpec(workingDir = cd)
        // 重连时回到本 tab 上次所在的 tmux 会话，优先于 launcher 与默认；首连时为 null。
        val lastTmux = _tmuxSession.value
        val spec = if (lastTmux != null) baseSpec.copy(tmuxSession = lastTmux) else baseSpec
        host.initialCommands(spec).forEach { ts.send("$it\n".toByteArray()) }
        // 记下实际的 tmux 会话名（launcher 指定 > 自动 tmux > 无），与 initialCommands 同源。
        host.resolveTmuxSession(spec)?.let { _tmuxSession.value = it }
    }

    /**
     * 建终端会话并把 [shell] 交给它持有（`remoteChannel`），会话 close 时在后台关掉通道。
     * 只关 input 不够：输入流 close 只是本地 EOF，不关通道，重连或关 tab 会留下
     * 远端 shell 和僵尸 tmux attach client。
     */
    private fun terminalSessionFor(shell: ShellChannel): TerminalSession =
        TerminalSession(
            input = shell.input,
            output = shell.output,
            onResizeRemote = { cols, rows -> shell.resize(cols, rows) },
            rows = 24,
            cols = 80,
            remoteChannel = shell,
        )

    /**
     * 连接前确保主机已绑身份。返回 false 表示已置终态（Error / NeedIdentity），connect 应中止。
     * 无身份的主机（导入的常见）：恰好一个身份就自动用它；没有就引导去建；多个就进 NeedIdentity 选。
     */
    private suspend fun ensureIdentityBound(): Boolean {
        val host = withContext(io) { hostRepo.get(hostId) } ?: return true // 交给主流程报「未找到主机」
        if (host.authRef != null || host.passwordEnc != null) return true // 已绑身份或用密码认证
        val ids = withContext(io) { identityRepo.observeAll().first() }
        return when {
            ids.isEmpty() -> {
                _uiState.value = SessionUiState.Error(copyText("mobSession.identity.none"))
                false
            }
            ids.size == 1 -> {
                pendingIdentityId = ids.first().id // 认证成功后才持久化
                true
            }
            else -> {
                _uiState.value = SessionUiState.NeedIdentity(ids)
                false
            }
        }
    }

    /** 在 NeedIdentity 里选了身份：本次用它连，认证成功后才持久化，失败可重选。 */
    fun pickIdentity(identityId: String) {
        pendingIdentityId = identityId
        connect()
    }

    /** 认证成功后把本次选定的身份写进 host.authRef；失败不写，重试可重选。 */
    private suspend fun persistPickedIdentity(host: Host) {
        // 只在「用身份认证、且这次是新选的身份」时持久化；密码认证不涉及身份。
        if (host.passwordEnc == null && host.authRef == null && pendingIdentityId != null) {
            runCatching { hostRepo.save(host.copy(authRef = pendingIdentityId)) }
            pendingIdentityId = null
        }
    }

    /** 重连：关掉旧会话，重建 shell。 */
    fun retry() {
        runCatching { session?.close() }
        session = null
        connect()
    }

    override fun onCleared() {
        // 关 tab 时关掉 shell 并释放连接引用（app 用 configChanges，旋屏不触发 onCleared）。
        // release 而不是 disconnect：同主机还有别的 tab 持有时不断。
        runCatching { session?.close() }
        hostConnector.release(hostId, holderToken)
    }
}
