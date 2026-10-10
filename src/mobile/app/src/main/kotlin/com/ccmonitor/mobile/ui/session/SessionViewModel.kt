package com.ccmonitor.mobile.ui.session

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.agent.agentKindOrDefault
import com.ccmonitor.mobile.core.claude.agent.AgentDirSettingKey
import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.Launcher
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.data.repo.LauncherRepository
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
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
import com.ccmonitor.mobile.ssh.HostConnector
import com.ccmonitor.mobile.ssh.LaunchSpec
import com.ccmonitor.mobile.ssh.PosixDialect
import com.ccmonitor.mobile.ssh.ResumePlan
import com.ccmonitor.mobile.ssh.ResumeSpec
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
import java.util.concurrent.atomic.AtomicBoolean
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
    tmuxArg: String?, // attach tab 首连即附着的 tmux 会话名；普通 tab 为 null
    resumeArg: String?, // resume tab 的目标 Claude sessionId；普通 tab 为 null
    resumeCommandArg: String?, // 历史页长按选定的 resume 命令；走默认候选链时为 null
    private val manager: SshConnectionManager,
    private val hostRepo: HostRepository,
    private val identityRepo: IdentityRepository,
    private val launcherRepo: LauncherRepository,
    private val settingsRepo: SettingsRepository,
    private val hostConnector: HostConnector, // 建连编排，retain / release / connection 也经它
    private val tabManager: SessionTabManager, // resume 失败重跑句柄的注册处（init 注册、onCleared 注销）
    private val knownHostStore: KnownHostStore, // 主机钥匙变更的恢复：确认后 forget 再重连
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    // cd / resumeCommand 只在 resume 失败后被重跑时改写（acceptResumeRedrive，UI 线程），随后 retry() 的
    // 连接协程读新值。launch 已建立 happens-before，@Volatile 只是兜底。
    @Volatile
    private var cd: String? = cdArg?.ifBlank { null } // Koin 参数不传 null，空串当 null
    private val launcherId: String? = launcherIdArg?.ifBlank { null } // 启动配置 id
    private val resumeSessionId: String? = resumeArg?.ifBlank { null } // 非空即 resume tab

    @Volatile
    private var resumeCommand: String? = resumeCommandArg?.ifBlank { null } // 候选链首位（选定 > launcher > 主机 > claude）

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
     * 只取本主机的会话状态（distinctUntilChanged，别的会话或 SFTP 变动不触发整屏重组）。
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

    // 当前所在的 tmux 会话名，每个 tab 各记各的（同主机多 tab 不互相覆盖），跨重连保留。
    // 供对话框判断 attach 是否要 switch-client。以 [tmuxArg] 初始化：attach tab 首连即 `tmux new -A -s` 到它。
    private val _tmuxSession = MutableStateFlow(tmuxArg?.ifBlank { null })
    val tmuxSession: StateFlow<String?> = _tmuxSession.asStateFlow()

    /**
     * 本主机解析后的 agent 配置目录（host.claudeDir > 应用默认 > 内置默认），阅读面据此定位 JSONL。
     * 含义随 [agentKind]：ClaudeCode 是 ~/.claude，Codex 是 ~/.codex（字段名仍叫 claudeDir）。
     */
    private val _claudeDir = MutableStateFlow<String?>(null)
    val claudeDir: StateFlow<String?> = _claudeDir.asStateFlow()

    /** 本主机的 agent 种类（缺省 `AgentProfile.DEFAULT`）。定位、解析、回合结束、用量、resume 都按它取档案。 */
    private val _agentKind = MutableStateFlow(AgentProfile.DEFAULT.kind)
    val agentKind: StateFlow<AgentKind> = _agentKind.asStateFlow()

    /** 可推送公钥的身份列表（命令面板「推送公钥」用）。 */
    val identities: StateFlow<List<Identity>> =
        identityRepo.observeAll().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** 该主机的 launcher 列表，即历史页长按菜单里的命令选项。 */
    val hostLaunchers: StateFlow<List<Launcher>> =
        launcherRepo.observeForHost(hostId).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** 历史页「自定义命令…」上次的输入（全局一份），弹窗预填用。 */
    val lastResumeCustomCommand: StateFlow<String> =
        settingsRepo
            .resumeCustomCommand()
            .map { it.orEmpty() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), "")

    /** 记住本次自定义 resume 命令（原样输入）。 */
    fun setLastResumeCustomCommand(value: String) {
        viewModelScope.launch { settingsRepo.setResumeCustomCommand(value) }
    }

    /**
     * 一次性提示（resume 失败、自定义命令被替换等）。用缓冲 Channel：connect 早于界面订阅时发的也不丢；
     * 只有一个收集方（终端屏收了弹 Toast）。
     */
    private val _notices = Channel<String>(Channel.BUFFERED)
    val notices: Flow<String> = _notices.receiveAsFlow()

    // 本 tab 的持有者 token。retain / release 按 token 记账，强制断开清空持有者后重连可幂等地重新 retain。
    private val holderToken = UUID.randomUUID().toString()

    // resume 失败探测的 job：发出 resume 后启动，connect() 起点取消，重连不留旧探测。
    private var resumeWatchdogJob: Job? = null

    // 上次 resume 是否已被判失败（FAILED / ABSENT 时置位；connect() 起点与发送 resume 前都清零）。
    // 只有这个状态下，open() 命中本 tab 才允许重跑（acceptResumeRedrive）。io 协程写、UI 线程 CAS。
    // connect 起点就清零，重连在途时的并发重跑会被 CAS 拒掉，不会双发。
    private val resumeFailed = AtomicBoolean(false)

    init {
        registerResumeRedriver() // 先注册再连：resume tab 一建成就能接重跑（普通 tab 什么都不做）
        connect()
    }

    private fun connect() {
        // 每次连接都登记持有者（首连与重连都走这里），幂等：强制断开清空持有者后，重连能恢复本 tab 的持有。
        // 记下 retain 时的纪元传给 connect：强制断开若落在 retain 之后、建连之前，connect 见纪元已变就弃掉这次连接，
        // 不会装进一条没人持有的连接（否则前台保活服务永远停不下来）。
        val epoch = hostConnector.retain(hostId, holderToken)
        connectJob?.cancel() // 重连时取消在途的连接
        resumeWatchdogJob?.cancel() // 旧探测作废：重连会重发 resume 并重新探测
        resumeFailed.set(false) // 旧失败结论作废；在途期间的重跑请求因此被拒，不会双发

        connectJob =
            viewModelScope.launch {
                _uiState.value = SessionUiState.Connecting
                _connectLog.value = emptyList()
                if (!ensureIdentityBound()) return@launch // 无身份的主机先解决身份（自动绑 / 引导 / 快选）
                _uiState.value = withContext(io) { establishConnectedState(epoch) }
            }
    }

    /**
     * 这一档的 agent 目录认不认「设置」屏里的应用级默认：问档案，不问种类。
     *
     * `when` 故意不带 `else`：[AgentDirSettingKey] 加了值，这里当场编译不过，而不是静默走缺省分支。
     */
    private suspend fun appDefaultAgentDir(profile: AgentProfile): String? =
        when (profile.appDefaultDirSetting) {
            AgentDirSettingKey.AppDefaultClaudeDir -> settingsRepo.getDefaultClaudeDir()
            null -> null // 这一档没有应用级默认，只认主机覆盖与内置默认
        }

    // 连接可能抛各种异常（IO / 认证 / 超时…），统一兜成 Error 态；CancellationException 先重抛。
    @Suppress("TooGenericExceptionCaught")
    private suspend fun establishConnectedState(expectedGen: Int): SessionUiState {
        var created: TerminalSession? = null // 取消时要关掉它，否则 reader / writeExecutor 线程泄漏
        return try {
            val host = hostRepo.get(hostId) ?: error("未找到主机: $hostId")
            // 按 host.agentKind 选定位器解析 agent 目录。Codex 没有应用级默认，回退内置 ${CODEX_HOME:-$HOME/.codex}。
            val kind = host.agentKindOrDefault()
            _agentKind.value = kind
            val profile = AgentProfile.of(kind)
            val appDefaultDir = appDefaultAgentDir(profile)
            _claudeDir.value = profile.sessionLocator.resolveAgentDir(host.claudeDir, appDefaultDir)
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
            // sshj 的阻塞 IO 不及时响应取消，不显式守门的话，旧的在途连接会覆盖 session / uiState。
            coroutineContext.ensureActive()
            session = ts
            shellOpenedOn = conn // 断因判定的基准
            reattachDetector.notifyNewShellOpened() // 清掉上一个 shell 的断因
            persistPickedIdentity(host) // 认证成功后才持久化选定身份（密码认证跳过）
            val resumed = sendStartupCommands(host, ts)
            // resume tab 的阅读面 cwd 指向被 resume 的项目，会话目录会把这个活动会话排在下拉首位。
            val readerCwd = if (resumed) cd ?: host.defaultWorkingDir else host.defaultWorkingDir
            SessionUiState.Connected(ts, readerCwd, dialectFor(host.os))
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

    /** 连接成功后的首发命令：resume tab 发 resume 命令，否则 initialCommands。返回是否走了 resume 路径。 */
    private suspend fun sendStartupCommands(
        host: Host,
        ts: TerminalSession,
    ): Boolean {
        // 有 launcherId 就用它的 LaunchSpec（工作目录 / tmux 会话 / 命令），否则只带 cd。
        val launcher = launcherId?.let { launcherRepo.get(it) }
        // resume tab：fresh shell 直接跑 resume 命令，替代 initialCommands。远端按 sid 找权威 cwd、守卫 cwd、
        // 起 detached tmux `cc-<sid8>`、send-keys resume 命令、attach。重连重发也安全：tmux 已在则只 attach；
        // tmux 没了（主机重启）就重建重 resume。sid 非法或非 POSIX 时返回 false，落回普通 shell。
        if (trySendResume(host, ts, launcher?.command)) return true
        val baseSpec =
            launcher?.let { LaunchSpec(it.workingDir, it.tmuxSession, it.command) }
                ?: LaunchSpec(workingDir = cd)
        // 重连时回到本 tab 上次所在的 tmux 会话（switch 过的也算），优先于 launcher 与默认；首连时为 null。
        val lastTmux = _tmuxSession.value
        val spec = if (lastTmux != null) baseSpec.copy(tmuxSession = lastTmux) else baseSpec
        // 按序发初始命令（tmux / cd / 命令）。移动端触摸与状态栏设置已拼在 tmux 创建序列里。
        host.initialCommands(spec).forEach { ts.send("$it\n".toByteArray()) }
        // 记下实际的 tmux 会话名（launcher 指定 > 自动 tmux > 无），与 initialCommands 同源。
        host.resolveTmuxSession(spec)?.let { _tmuxSession.value = it }
        return false
    }

    /**
     * 构造并发送 resume 命令、启动失败探测。首连与失败后重跑走同一条路径（都经 [sendStartupCommands]）。
     * 返回是否走了 resume（false = 非 resume tab 或前提不满足，调用方落回 initialCommands）。
     * 发送前再清一次 [resumeFailed]：新尝试一上路，旧失败结论就作废，成败由这次探测重判。
     */
    private suspend fun trySendResume(
        host: Host,
        ts: TerminalSession,
        preferredCommand: String?,
    ): Boolean {
        val resume = buildResumeCommand(host, preferredCommand) ?: return false
        resumeFailed.set(false)
        ts.send((resume.command + "\n").toByteArray())
        _tmuxSession.value = resume.sessionName // 先乐观记名，探测失败再清
        startResumeWatchdog(resume.sessionName, resume.launchLabel) // 失败要看得见，空会话要清掉
        return true
    }

    /**
     * resume 失败探测：发出后 exec 两次探 `pane_current_command`，三种结果见 [ResumeOutcome]。
     * [tmuxName] 是执行前乐观记下的名；FAILED / ABSENT 都要清掉它，否则命令面板 send-keys 落到不存在的会话
     * 还报「已发送」。[connect] 起点取消。
     */
    private fun startResumeWatchdog(
        tmuxName: String,
        launchCommand: String,
    ) {
        resumeWatchdogJob?.cancel()
        resumeWatchdogJob =
            viewModelScope.launch(io) {
                when (ResumeWatchdog(channel).watch(tmuxName)) {
                    ResumeOutcome.SUCCESS -> Unit // Claude 起来了（或像是在包装脚本下跑），保留会话名
                    ResumeOutcome.FAILED -> {
                        _notices.trySend("resume 失败：Claude 未能启动（检查『$launchCommand』是否存在）")
                        _tmuxSession.value = null // 空会话已被杀，本 tab 不再声称附着于它
                        resumeFailed.set(true) // 失败态：同 sid 再次打开可换命令重跑
                    }
                    // 会话不存在（cwd 守卫拦下或没建成）：清掉乐观记的名；pty 里已有守卫的输出，不再弹 Toast。
                    ResumeOutcome.ABSENT -> {
                        _tmuxSession.value = null
                        resumeFailed.set(true) // 没建成也算失败态，换 cwd 或命令重跑可解
                    }
                }
            }
    }

    /**
     * resume tab 把重跑句柄注册进 [tabManager]（key = hostId＋sid，不需要 tab key）；[onCleared] 注销。
     */
    private fun registerResumeRedriver() {
        val sid = resumeSessionId ?: return
        tabManager.registerResumeRedriver(hostId, sid) { command, cwd -> acceptResumeRedrive(command, cwd) }
    }

    /**
     * open() 命中本 tab 时决定要不要重跑（UI 线程，经句柄同步调入）。
     * 只有上次 resume 已被判失败（[resumeFailed]）才接受：CAS 消费失败态，用新的 [command] / [cwd] 更新
     * 候选链首位与兜底 cwd，再 [retry]，返回 true。运行中、在途、连接中一律拒绝，绝不往运行中的 Claude
     * 打第二条命令。用 CAS：一次失败最多换一次重跑，快速双击不双发。[cwd] 为 null 不降级已有值。
     *
     * 走 [retry] 而不直接往 pty 发：失败后外层 pty 是活的 login shell，里面可能正跑着 vim 或别的程序，
     * 直接发会被前台程序当输入吃掉，远端守卫也就不起作用。重连拿到一个闲置 login shell 的新 pty，
     * resume 命令一定作为 shell 命令执行；连接复用，只新开 shell 通道。
     */
    private fun acceptResumeRedrive(
        command: String?,
        cwd: String?,
    ): Boolean {
        if (!resumeFailed.compareAndSet(true, false)) return false
        resumeCommand = command
        if (cwd != null) cd = cwd
        retry() // 新 pty 是闲置 login shell，resume 命令作为 shell 命令执行
        return true
    }

    /**
     * 建终端会话并把 [shell] 交给它持有（`remoteChannel`），会话 close 时在后台关掉通道。
     * 只关 input 不够：sshj 的 ChannelInputStream.close 只是本地 EOF，不关通道，重连或关 tab 会留下
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
     * 构造 resume tab 的首发命令；非 resume tab 或前提不满足时返回 null（落回普通 initialCommands）。
     * 前提：有 sid、远端支持 tmux（POSIX）、sid 过白名单（sid 来自远端文件名，防注入）。
     * cwd 不是前提：远端按 sid 定位 jsonl 读权威 cwd，[cd] 只作兜底；定位不到时远端守卫给出可见失败、不建会话。
     * 命令候选链：选定的 [resumeCommand] > 发起 tab 的 launcher 命令 > 该主机各 launcher 命令 > `claude`。
     * 载荷里先 `unset` 嵌套的 Claude 环境标记，否则 resume 的会话不落盘。
     */
    private suspend fun buildResumeCommand(
        host: Host,
        preferredCommand: String?,
    ): ResumePlan? {
        val sid = resumeSessionId ?: return null
        // 候选解析、净化、tmux 包裹都在 backend.resume 里。先查前提再读库，非法 sid 不必白跑一趟 launcher 查询。
        val backend = dialectFor(host.os).sessionBackend
        if (backend == null || !ClaudeInvocation.isValidSessionId(sid)) return null // 各种类共用一条 sid 白名单
        val hostCommands = launcherRepo.observeForHost(hostId).first().map { it.command }
        // agent 目录与会话目录同源（establishConnectedState 已置 _claudeDir）；兜底按种类解析。
        val kind = host.agentKindOrDefault()
        val profile = AgentProfile.of(kind)
        val agentDir =
            _claudeDir.value
                ?: profile.sessionLocator.resolveAgentDir(host.claudeDir, appDefaultAgentDir(profile))
        val plan =
            backend.resume(
                ResumeSpec(
                    sessionId = sid,
                    launchCandidates = listOf(resumeCommand, preferredCommand) + hostCommands,
                    claudeDir = agentDir,
                    fallbackCwd = cd,
                    alreadyInTmux = false,
                    agentKind = kind,
                ),
            ) ?: return null
        // 配置的命令含不安全字符被替换时，明说一声，免得静默落空。
        plan.substitutedFrom?.let { _notices.trySend("自定义命令『$it』含不安全字符，改用 ${plan.launchLabel} 续跑") }
        return plan
    }

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
                _uiState.value = SessionUiState.Error("该主机未关联身份，请先在身份管理创建或导入一个身份")
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
        // 注销重跑句柄，免得已死的 VM 被 open() 调到。
        resumeSessionId?.let { tabManager.unregisterResumeRedriver(hostId, it) }
        // 关 tab 时关掉 shell 并释放连接引用（app 用 configChanges，旋屏不触发 onCleared）。
        // release 而不是 disconnect：同主机还有别的 tab 持有时不断。
        runCatching { session?.close() }
        hostConnector.release(hostId, holderToken)
    }
}
