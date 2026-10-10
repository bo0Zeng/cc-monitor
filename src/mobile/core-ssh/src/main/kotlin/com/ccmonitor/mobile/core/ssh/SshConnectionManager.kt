package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean

/** 指数退避 `baseDelay shl n` 的 n 上限，防 Long 溢出（2^20×base 早已超过 maxDelay）。 */
private const val MAX_BACKOFF_SHIFT = 20

/** 复用已有连接前活性探测的超时：健康连接一次 session 开关是亚秒级。 */
private const val FASTPATH_PROBE_TIMEOUT_MS = 5_000L

/**
 * 多 SSH 会话管理。按 id 持有 [SshConnection]，对外暴露 [sessions]；负责注册、状态、连断与重连。
 * 所有会话都计入 activeCount 并参与后台重连。
 *
 * 注意：连接的生死只该经 app 层的 HostConnector 网关进出，别处不要直接调本类的
 * [connect] / [retain] / [release] / [disconnect] / [disconnectAll] / [connection]。
 */
class SshConnectionManager(
    private val knownHostStore: KnownHostStore? = null,
    // 传输工厂；测试注入假实现。
    private val transportFactory: (KnownHostStore?) -> SshTransport = { SshConnection(it) },
    // 重连协程的 dispatcher；测试注入 TestDispatcher。
    reconnectDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    private val _sessions = MutableStateFlow<Map<String, SessionState>>(emptyMap())
    val sessions: StateFlow<Map<String, SessionState>> = _sessions.asStateFlow()

    // 每个 id 一份 [HostConnectionState]（锁、在飞连接、连接、配置、代号、持有者、去重标）。
    // computeIfAbsent 原子创建；条目永不移除：锁与代号必须跨 disconnect 稳定，disconnect 只清字段。
    private val states = ConcurrentHashMap<String, HostConnectionState>()

    /** 写路径与临界区用：原子取或建。只读查询用 `states[id]?.x`，别在陌生 id 上白建条目。 */
    private fun stateFor(id: String): HostConnectionState = states.computeIfAbsent(id) { HostConnectionState(it) }

    // 两把锁：synchronized 短临界区锁串行化 connect 注册与 disconnect 清理；suspend 锁让同 id 的首连单飞。
    // disconnect 只用 [lockFor]，不等 [connectLockFor]，所以作废在飞 connect 不会被它阻塞。
    private fun lockFor(id: String): Any = stateFor(id).lock

    private fun connectLockFor(id: String): Mutex = stateFor(id).connectLock

    private fun put(state: SessionState) = _sessions.update { it + (state.id to state) }

    /** 单地址连接（委托给多地址版；竞速本身在 SshConnection 内）。 */
    suspend fun connect(
        id: String,
        hostId: String,
        label: String,
        config: ConnectionConfig,
        onEvent: ((SshConnectEvent) -> Unit)? = null,
    ): SshTransport =
        connect(id, hostId, label, listOf(config), onEvent)

    /**
     * 连接一个会话：造一个 transport，多地址竞速与跳板都在 [SshConnection] 内。全失败置 ERROR 并抛出。
     * [onEvent] 透传连接阶段事件。
     * [expectedGen] 非空时守重连纪元：进入首个临界区时代号已变（期间被 disconnect）就弃用，不写回任何状态。
     */
    suspend fun connect(
        id: String,
        hostId: String,
        label: String,
        addressConfigs: List<ConnectionConfig>,
        onEvent: ((SshConnectEvent) -> Unit)? = null,
        expectedGen: Int? = null,
    ): SshTransport {
        require(addressConfigs.isNotEmpty()) { "no address to connect" }
        // 合成事件（复用、等待、失效提示）用第一个地址的 host/port，直接发给 [onEvent]，
        // 不参与握手看门狗的进度判定。
        val first = addressConfigs.first()
        val emit: (SshConnectEvent.Stage, String) -> Unit = { stage, msg ->
            onEvent?.invoke(SshConnectEvent(first.host, first.port, stage, msg))
        }
        // 已有活连接就复用：同主机多个 tab 共享一条连接，各自 openShell。快路径不进锁；复用前先探测。
        reusableAfterProbe(id, emit)?.let { return it }
        // 另一个 connect 在飞时提示在等。isLocked 只是快照，只用来出提示，不参与决策。
        if (connectLockFor(id).isLocked) emit(SshConnectEvent.Stage.TCP, "另一连接尝试进行中，等待其完成 …")
        // 同 id 并发首连单飞：先到者建连接，后到者进锁复查见活连接即复用，不会各建一条互相顶掉。
        return connectLockFor(id).withLock {
            // 锁内复查：先到者已建好就复用，也发合成事件。不再探测：刚建好的连接是新鲜的，锁内也不该做网络往返。
            states[id]?.connection?.let {
                if (it.isConnected) {
                    emit(SshConnectEvent.Stage.ESTABLISHED, "复用已有连接（并发连接已就绪）")
                    return@withLock it
                }
            }
            doConnect(id, hostId, label, addressConfigs, onEvent, expectedGen)
        }
    }

    /**
     * 复用已有连接前的活性探测。`isConnected` 只读 sshj 本地标志，死 TCP 好一阵仍是 true，直接复用会卡死在 openShell。
     * - 探测通过：发合成 ESTABLISHED 后复用，连接日志不会空。
     * - 探测失败：按值比较摘除死连接、后台关，返回 null 走真连接。
     *
     * 摘除是替换，不是断开，所以不自增代号：自增会误弃带旧 expectedGen 的后台重连和在飞的 doConnect。
     * 探测在锁外；物理关闭走 [closeInBackground]。
     */
    private suspend fun reusableAfterProbe(
        id: String,
        emit: (SshConnectEvent.Stage, String) -> Unit,
    ): SshTransport? {
        val existing = states[id]?.connection ?: return null
        if (!existing.isConnected) return null // 本地标志已死：走真 connect，finalizeConnected 会替换掉旧连接
        emit(SshConnectEvent.Stage.TCP, "检测到已有连接，校验可用性 …")
        if (existing.probeAlive(FASTPATH_PROBE_TIMEOUT_MS)) {
            // 探测期间可能被并发 disconnect 或替换：仍是它且仍连着才复用，否则走锁路。
            val stillReusable = states[id]?.connection === existing && existing.isConnected
            if (stillReusable) emit(SshConnectEvent.Stage.ESTABLISHED, "复用已有连接")
            return existing.takeIf { stillReusable }
        }
        emit(SshConnectEvent.Stage.TCP, "既有连接已失效，重新建立 …")
        evictDead(id, existing) // 按值比较摘除、后台关，不自增代号
        return null
    }

    /**
     * 真正建连的临界区体，被 [connect] 的单飞锁包着。
     * 三处 throw 都是有意的：重抛取消、标 ERROR 后重抛、代号变了时弃用幽灵连接。
     */
    @Suppress("ThrowsCount")
    private suspend fun doConnect(
        id: String,
        hostId: String,
        label: String,
        addressConfigs: List<ConnectionConfig>,
        onEvent: ((SshConnectEvent) -> Unit)?,
        expectedGen: Int?,
    ): SshTransport {
        // 先造 transport（纯构造、无 socket），与 CONNECTING 一起原子登记为在飞连接，disconnect 之后必能关到它。
        val conn = transportFactory(knownHostStore)
        val myGen: Int
        synchronized(lockFor(id)) {
            val st = stateFor(id)
            throwIfEpochExpired(id, expectedGen) // 代号已变说明已被 disconnect：弃用
            myGen = st.generation // 之后 disconnect 自增代号即作废本次
            put(SessionState(id, hostId, label, SessionStatus.CONNECTING))
            st.configs = addressConfigs
            st.connecting = conn // disconnect 与看门狗靠它关掉阻塞中的握手
        }
        // 握手看门狗：黑洞对端连上 TCP 后 banner/KEX 读会无限阻塞（sshj 没有这一级超时），到点强关 transport 解阻塞。
        // 进度判据是任一目标地址认证成功（ESTABLISHED），不是 isConnected：isConnected 要等所有输家清理完才为真，
        // 期间赢家早已认证。只认目标地址，跳板的 ESTABLISHED 不算。
        val handshakeDone = AtomicBoolean(false)
        val targetHostPorts = addressConfigs.mapTo(HashSet()) { it.host to it.port }
        val onEventGuarded: (SshConnectEvent) -> Unit = { ev ->
            if (ev.stage == SshConnectEvent.Stage.ESTABLISHED && (ev.host to ev.port) in targetHostPorts) handshakeDone.set(true)
            onEvent?.invoke(ev)
        }
        // 下限 connectTimeoutMs + 15s，防 connectTimeoutMs 调得比 handshakeTimeoutMs 还大时看门狗先于合法握手触发。
        // 跳板 + 多地址是串行的，按地址数扩额；每次尝试的上界取 max(connectTimeoutMs, 30s)，
        // 因为经隧道的尝试不走 socket.connect，上界是 sshj 内部 channel-open 的 30s 等待。
        val first0 = addressConfigs.first()
        val isSerialJump = first0.jumpVia != null && addressConfigs.size > 1
        val perAttemptMs = if (isSerialJump) maxOf(first0.connectTimeoutMs.toLong(), 30_000L) else first0.connectTimeoutMs.toLong()
        val serialFactor = if (isSerialJump) addressConfigs.size else 1
        val deadlineMs = maxOf(first0.handshakeTimeoutMs, perAttemptMs * serialFactor + 15_000)
        val timedOut = AtomicBoolean(false)
        val watchdog =
            reconnectScope.launch {
                delay(deadlineMs)
                if (!handshakeDone.get()) {
                    timedOut.set(true)
                    runCatching { conn.close() }
                }
            }
        // 注意：close() 是网络写（SSH_MSG_DISCONNECT），在主线程调会抛 NetworkOnMainThreadException，
        // sshj 只 catch IOException，socket、读线程和远端会话随之泄漏，外层 runCatching 又会把异常吞掉。
        // connect 可能从主线程调进来，所以失败路径一律走 [closeInBackground]。
        try {
            conn.connect(addressConfigs, onEventGuarded)
        } catch (ce: CancellationException) {
            closeInBackground(listOf(conn))
            throw ce // 协程被取消：不标 ERROR，交给结构化取消
        } catch (e: Exception) {
            closeInBackground(listOf(conn)) // 失败也关，防半开 socket 与跳板链泄漏
            synchronized(lockFor(id)) {
                // 未被新的 disconnect 取代时才标 ERROR，否则会复活已被移除的会话条目。
                if (stateFor(id).generation == myGen) {
                    val msg =
                        if (timedOut.get()) {
                            "连接超时：握手未在 ${deadlineMs}ms 内完成"
                        } else {
                            e.message ?: e.toString()
                        }
                    put(SessionState(id, hostId, label, SessionStatus.ERROR, msg))
                }
            }
            throw e
        } finally {
            watchdog.cancel()
            // 按值比较清在飞登记，不误清更新一次 connect 的登记。
            synchronized(lockFor(id)) { stateFor(id).let { if (it.connecting === conn) it.connecting = null } }
        }
        return finalizeConnected(id, hostId, label, conn, myGen)
    }

    /**
     * 连接成功后的收尾：锁内只改状态，物理关闭一律锁外后台做。锁内做网络写会卡住在同一把锁上争用的主线程。
     * 代号变了（连接期间被 disconnect）就弃用并抛取消；否则装入，旧连接后台关。
     */
    private fun finalizeConnected(
        id: String,
        hostId: String,
        label: String,
        conn: SshTransport,
        myGen: Int,
    ): SshTransport {
        var supersededOld: SshTransport? = null
        var ghostDiscard = false
        synchronized(lockFor(id)) {
            val st = stateFor(id)
            if (st.generation != myGen) {
                ghostDiscard = true
            } else {
                supersededOld = st.connection // 重连路径：旧连接锁外后台关
                st.connection = conn
                put(SessionState(id, hostId, label, SessionStatus.CONNECTED))
            }
        }
        if (ghostDiscard) {
            closeInBackground(listOf(conn))
            throw CancellationException("会话在连接期间已被断开")
        }
        supersededOld?.let { closeInBackground(listOf(it)) }
        return conn
    }

    fun connection(id: String): SshTransport? = states[id]?.connection // 只读，不创建条目

    // 持有者集在 [HostConnectionState.holders]：多 tab 共享同一 id 的连接，只在 lockFor 内访问。
    // 用集合而不是计数：强制断开清集后，存活的 VM 重新 retain 即可恢复持有。

    /**
     * 登记一个持有者（会话屏或 tab，[holder] 是该 VM 的唯一 token），与 [release] 配对。按 holder 幂等，可安全重复 retain。
     *
     * 返回当前重连纪元，调用方应把它作为随后 [connect] 的 expectedGen：强制断开若落在 retain 与 connect 之间，
     * connect 会因纪元已变而弃用，不会装入一条无人持有的连接（那会让保活服务永不自停）。
     * retain 与读纪元在同一把锁内原子完成。
     */
    fun retain(
        id: String,
        holder: String,
    ): Int =
        synchronized(lockFor(id)) {
            val st = stateFor(id)
            st.holders.add(holder)
            st.generation
        }

    /** 释放一个持有者；该 id 没有持有者了才真断开，还有别的 tab 在用就不断。 */
    fun release(
        id: String,
        holder: String,
    ) {
        var doomed: List<SshTransport> = emptyList()
        synchronized(lockFor(id)) {
            // token 不在集中（重复 release、陈旧、已被强制清掉）时什么都不做：不误断之后重建的活连接，也不白白作废在飞 connect。
            val holders = stateFor(id).holders
            if (!holders.remove(holder)) return
            if (holders.isEmpty()) {
                doomed = disconnectLocked(id) // 无持有者：真断开
            }
        }
        closeInBackground(doomed) // 物理关闭在锁外后台做
    }

    /** 强制断开（用户断连、SFTP 收尾、disconnectAll）：无视持有者，清集并真断开。 */
    fun disconnect(id: String) {
        val doomed =
            synchronized(lockFor(id)) {
                stateFor(id).holders.clear() // 存活的 VM 重连时会重新 retain
                disconnectLocked(id)
            }
        closeInBackground(doomed) // 物理关闭在锁外后台做
    }

    /**
     * 真断开的临界区体，调用方须已持 [lockFor]：自增代号作废在飞 connect，摘除连接，清配置与会话条目。
     * 锁内只改状态（`connection(id)` 立刻对外可见为 null），不在这里 close：close 是网络写，主线程上会泄漏。
     * 摘下的 transport 返回给调用方在锁外后台关。
     */
    private fun disconnectLocked(id: String): List<SshTransport> {
        val st = stateFor(id)
        st.generation++ // 作废任何在飞 connect 的注册
        // 在飞连接也摘下来关：阻塞的 sshj 握手不响应取消，关 socket 是唯一的解法，单飞锁随之释放。
        val prevConnecting = st.connecting
        val prevConnection = st.connection
        st.connecting = null
        st.connection = null
        val doomed = listOfNotNull(prevConnecting, prevConnection)
        st.configs = null // 防重连复活手动断开的会话
        _sessions.update { it - id } // 不再是重连候选
        return doomed
    }

    /**
     * 后台关 transport，不等结果。用本类长命的 [reconnectScope]，不用调用方的 scope：
     * viewModelScope 在 onCleared 返回时就取消了，close 永远不会执行。
     */
    private fun closeInBackground(transports: List<SshTransport>) {
        if (transports.isEmpty()) return
        reconnectScope.launch { transports.forEach { t -> runCatching { t.close() } } }
    }

    /**
     * 探测判死后摘除死连接：按值比较，只在当前仍是它时才摘，不误摘并发换上的新连接；物理关闭走后台。
     * 摘除是替换，不自增代号。
     */
    private fun evictDead(
        id: String,
        dead: SshTransport,
    ) {
        synchronized(lockFor(id)) { stateFor(id).let { if (it.connection === dead) it.connection = null } }
        closeInBackground(listOf(dead))
    }

    // 连接失败留下 ERROR 条目但没有连接的会话也要断：取「有连接的 id」与会话条目的并集。
    // 不用 states.keys：那里含已清空的条目，会白白加锁、自增代号。
    fun disconnectAll() = (states.filterValues { it.connection != null }.keys + _sessions.value.keys).toSet().forEach(::disconnect)

    // 保活与重连
    private val reconnectScope = CoroutineScope(SupervisorJob() + reconnectDispatcher)
    // 重连与探测的去重标在 [HostConnectionState.reconnecting] / [HostConnectionState.probing]。

    /**
     * 用存好的配置重连单个会话（尽力而为，按 id 去重防风暴）。
     * 重建的是新连接：状态、SFTP 与新操作恢复，已有交互终端的旧流不会自动重挂。
     */
    private fun reconnect(st: SessionState) {
        // 配置与代号必须在同一把锁内一起读：disconnect 落在两次读之间时，会拿断开前的配置配断开后的代号，
        // 复活用户刚断开的会话。
        val cfgs: List<ConnectionConfig>
        val genAtStart: Int
        synchronized(lockFor(st.id)) {
            cfgs = stateFor(st.id).configs ?: return
            genAtStart = stateFor(st.id).generation
        }
        val policy = cfgs.first().reconnect
        if (!policy.auto) return // 该主机关了断线自动重连
        if (!stateFor(st.id).reconnecting.compareAndSet(false, true)) return // 已在重连：跳过
        reconnectScope.launch {
            try {
                reconnectWithBackoff(st, cfgs, policy, genAtStart)
            } finally {
                stateFor(st.id).reconnecting.set(false)
            }
        }
    }

    /**
     * 退避重试：失败按指数退避（baseDelay × 2^(attempt-1)，上限 maxDelay）重试，到 [ReconnectPolicy.maxAttempts] 停（0 为无限）；
     * 代号变了（用户断开）或已连上也停。
     */
    private suspend fun reconnectWithBackoff(
        st: SessionState,
        cfgs: List<ConnectionConfig>,
        policy: ReconnectPolicy,
        genAtStart: Int,
    ) {
        var attempt = 0
        while (tryMarkReconnecting(st, genAtStart)) {
            // 失败吞掉继续重试，但取消必须重抛。带 expectedGen：connect 首个临界区再核一次代号。
            val established =
                runCatching { connect(st.id, st.hostId, st.label, cfgs, expectedGen = genAtStart) }
                    .onFailure { if (it is CancellationException) throw it }
                    .isSuccess
            if (established) return
            attempt++
            if (policy.maxAttempts in 1..attempt) return // 0 为无限
            delay((policy.baseDelayMs shl (attempt - 1).coerceIn(0, MAX_BACKOFF_SHIFT)).coerceAtMost(policy.maxDelayMs))
        }
    }

    /**
     * 「仍需重连」的检查与标 RECONNECTING 在同一把锁内原子完成。分两步时，disconnect 插在中间会把一个
     * RECONNECTING 幽灵条目写回 [_sessions]，没有连接、永不再重连、无人清理。返回 false 表示已断开或已连上。
     */
    internal fun tryMarkReconnecting(
        st: SessionState,
        genAtStart: Int,
    ): Boolean =
        synchronized(lockFor(st.id)) {
            if (reconnectStillActive(st.id, genAtStart)) {
                put(st.copy(status = SessionStatus.RECONNECTING))
                true
            } else {
                false
            }
        }

    /** 代号没被 disconnect 自增，且尚未连上：继续重连。 */
    private fun reconnectStillActive(
        id: String,
        genAtStart: Int,
    ): Boolean = stateFor(id).let { it.generation == genAtStart && it.connection?.isConnected != true }

    /** 重连纪元守门：[expectedGen] 非空且当前代号已不同（期间被 disconnect）就抛取消。须在 [lockFor] 内调。 */
    private fun throwIfEpochExpired(
        id: String,
        expectedGen: Int?,
    ) {
        if (expectedGen != null && stateFor(id).generation != expectedGen) {
            throw CancellationException("重连纪元过期：会话已被断开")
        }
    }

    /** 对 ERROR 会话重连。 */
    fun requestReconnectAll() {
        _sessions.value.values
            .filter { it.isErrorReconnectCandidate() }
            .forEach { reconnect(it) }
    }

    /**
     * 前台或网络恢复时，重连底层已死但状态仍是 CONNECTED 的会话。
     * 本地 `isConnected` 对黑洞断网（无 FIN）好一阵仍是 true，所以本地标志还活着的会话走主动 [SshTransport.probeAlive]，
     * 探测失败才重连；本地标志已死的直接重连。探测按 id 去重，防网络抖动反复探。
     */
    fun probeAndReconnectStale() {
        _sessions.value.values
            .filter {
                states[it.id]
                    ?.configs
                    ?.firstOrNull()
                    ?.reconnect
                    ?.onNetworkChange != false
            } // 尊重该主机的「网络变化时重连」
            .filter {
                states[it.id]
                    ?.configs
                    ?.firstOrNull()
                    ?.reconnect
                    ?.auto != false
            } // 关了断线自动重连：不重连，也不探测
            .forEach { st ->
                val existing = states[st.id]?.connection
                when {
                    // 本地标志已死：直接重连。
                    st.isStaleReconnectCandidate(existing?.isConnected == true) -> reconnect(st)
                    // 本地标志仍活：主动探测，失败先摘除死连接（reconnectStillActive 才会放行）再重连。
                    existing != null && st.countsAsActive() && stateFor(st.id).probing.compareAndSet(false, true) ->
                        reconnectScope.launch {
                            try {
                                if (!existing.probeAlive(FASTPATH_PROBE_TIMEOUT_MS)) {
                                    evictDead(st.id, existing)
                                    reconnect(st)
                                }
                            } finally {
                                stateFor(st.id).probing.set(false)
                            }
                        }
                }
            }
    }

    /** 当前活跃会话数（CONNECTED），供保活服务出通知与自停。 */
    fun activeCount(): Int = _sessions.value.values.count { it.countsAsActive() }
}
