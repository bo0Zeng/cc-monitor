package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.copy.copyText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap

/**
 * 多 SSH 会话管理（终端 · 文件走的那一层）。按 id 持有 [SshTransport]，对外暴露 [sessions]；负责注册、状态、连与断。
 *
 * **重连不在这里**：手机上只有一条重连循环，是共用的那一条（嵌的核心里的常驻流，`resident-client`，桌面同一份：拨号按进展计时 ·
 * 宿主报事就重拨 · 心跳判死）。这一层握着的那条死了（核心的保活判死 ⇒ [SshTransport.onDrop]）⇒ 名义状态「重连中」，
 * 等共用那条报「够着那一台了」（[loopReached]）才在同一份拨号身份上换一条（[SshTransport.again]：池里有活的就复用）。
 * 自己不拨、不判死、不退避；开链路的期限也在核心里（按进展算），这里一个数都不写。
 *
 * 注意：连接的生死只该经 app 层的 HostConnector 网关进出，别处不要直接调本类的
 * [connect] / [retain] / [release] / [disconnect] / [disconnectAll] / [connection]。
 */
class SshConnectionManager(
    private val knownHostStore: KnownHostStore? = null,
    // 传输工厂：走手机嵌的 Rust 核心（[RustTransport]，拨号核心与后端同一份）；测试注入假实现。
    private val transportFactory: (KnownHostStore?) -> SshTransport = { RustTransport(it) },
    // 跟着共用循环换连接 · 后台关连接的协程跑在哪；测试注入 TestDispatcher。
    reconnectDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    private val _sessions = MutableStateFlow<Map<String, SessionState>>(emptyMap())
    val sessions: StateFlow<Map<String, SessionState>> = _sessions.asStateFlow()

    // 每个 id 一份 [HostConnectionState]（锁、在飞连接、连接、配置、代号、持有者）。
    // computeIfAbsent 原子创建；条目永不移除：锁与代号必须跨 disconnect 稳定，disconnect 只清字段。
    private val states = ConcurrentHashMap<String, HostConnectionState>()

    /** 写路径与临界区用：原子取或建。只读查询用 `states[id]?.x`，别在陌生 id 上白建条目。 */
    private fun stateFor(id: String): HostConnectionState = states.computeIfAbsent(id) { HostConnectionState(it) }

    // 两把锁：synchronized 短临界区锁串行化 connect 注册与 disconnect 清理；suspend 锁让同 id 的连接单飞。
    // disconnect 只用 [lockFor]，不等 [connectLockFor]，所以作废在飞 connect 不会被它阻塞。
    private fun lockFor(id: String): Any = stateFor(id).lock

    private fun connectLockFor(id: String): Mutex = stateFor(id).connectLock

    private fun put(state: SessionState) = _sessions.update { it + (state.id to state) }

    private val scope = CoroutineScope(SupervisorJob() + reconnectDispatcher)

    /** 单地址连接（委托给多地址版；竞速本身在核心里）。 */
    suspend fun connect(
        id: String,
        hostId: String,
        label: String,
        config: ConnectionConfig,
        onEvent: ((SshConnectEvent) -> Unit)? = null,
    ): SshTransport = connect(id, hostId, label, listOf(config), onEvent)

    /**
     * 连接一个会话：多地址竞速与跳板都在核心里。全失败置 ERROR 并抛出。[onEvent] 透传连接阶段事件。
     * 已有活连接 ⇒ 复用。握着的那条已死、配置没变 ⇒ 在同一份拨号身份上再握一条（[SshTransport.again]），拨号请求不变。
     * [expectedGen] 非空时守代号：进入首个临界区时代号已变（期间被 disconnect）就弃用，不写回任何状态。
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
        val first = addressConfigs.first()
        val emit: (String) -> Unit = { msg -> onEvent?.invoke(SshConnectEvent(first.host, first.port, SshConnectEvent.Stage.ESTABLISHED, msg)) }
        states[id]?.connection?.takeIf { it.isConnected }?.let {
            emit(copyText("mobSsh.log.reused"))
            return it
        }
        if (connectLockFor(id).isLocked) onEvent?.invoke(SshConnectEvent(first.host, first.port, SshConnectEvent.Stage.TCP, copyText("mobSsh.log.waitOther")))
        // 同 id 并发连接单飞：先到者建，后到者进锁复查见活连接即复用，不会各建一条互相顶掉。
        return connectLockFor(id).withLock {
            states[id]?.connection?.takeIf { it.isConnected }?.let {
                emit(copyText("mobSsh.log.reusedConcurrent"))
                return@withLock it
            }
            doConnect(id, hostId, label, addressConfigs, onEvent, expectedGen)
        }
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
        val myGen: Int
        val conn: SshTransport
        synchronized(lockFor(id)) {
            val st = stateFor(id)
            throwIfEpochExpired(id, expectedGen) // 代号已变说明已被 disconnect：弃用
            myGen = st.generation // 之后 disconnect 自增代号即作废本次
            // 同一份配置上那条死了 ⇒ 同一份拨号身份上再来一条（常驻流照的那份拨号请求不变）；否则造一条新的。
            conn = st.connection?.takeIf { st.configs == addressConfigs }?.again() ?: transportFactory(knownHostStore)
            put(SessionState(id, hostId, label, SessionStatus.CONNECTING))
            st.configs = addressConfigs
            st.connecting = conn // disconnect 靠它关掉在飞的那一条
        }
        // 注意：close() 是网络写，在主线程调会抛 NetworkOnMainThreadException；connect 可能从主线程调进来，失败路径一律走 [closeInBackground]。
        try {
            conn.connect(addressConfigs, onEvent)
        } catch (ce: CancellationException) {
            closeInBackground(listOf(conn))
            throw ce // 协程被取消：不标 ERROR，交给结构化取消
        } catch (e: Exception) {
            closeInBackground(listOf(conn)) // 失败也关，防半开链路泄漏
            synchronized(lockFor(id)) {
                // 未被新的 disconnect 取代时才标 ERROR，否则会复活已被移除的会话条目。
                if (stateFor(id).generation == myGen) put(SessionState(id, hostId, label, SessionStatus.ERROR, e.message ?: e.toString()))
            }
            throw e
        } finally {
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
                supersededOld = st.connection?.takeIf { it !== conn }
                st.connection = conn
                conn.dialRequest?.let { req -> dials.update { it + (id to req) } }
                put(SessionState(id, hostId, label, SessionStatus.CONNECTED))
            }
        }
        if (ghostDiscard) {
            closeInBackground(listOf(conn))
            throw CancellationException("会话在连接期间已被断开")
        }
        conn.onDrop { dropped(id, conn) }
        supersededOld?.let { closeInBackground(listOf(it)) }
        return conn
    }

    fun connection(id: String): SshTransport? = states[id]?.connection // 只读，不创建条目

    /**
     * 每台的拨号请求（JSON，不带用法）：连上过就一直在，直到断开（[disconnect] · 持有者放光）。握着的那条死了也还在 ——
     * 共用那条重连循环照它在核心里自己拨（池里那条死了它拨一条新的），这一层再跟着它换。
     */
    private val dials = MutableStateFlow<Map<String, String>>(emptyMap())

    /** 有拨号请求的那几台（每台要有一条共用循环在跑）。 */
    val dialing: Flow<Set<String>> = dials.map { it.keys }.distinctUntilChanged()

    /** 这台的拨号请求（按值去重：同一份身份上换了一条，值不变）；没连上过 · 已断开 ⇒ `null`。 */
    fun dialOf(id: String): Flow<String?> = dials.map { it[id] }.distinctUntilChanged()

    /**
     * 共用那条重连循环报：这台此刻够没够着（流通着，或停在要人动手的那一步——那一步也是登上了那一台才知道的）。
     * 够着了、这一层握着的那条已死 ⇒ 在同一份拨号身份上换一条（池里复用它那条）。宿主只转发，这里不判。
     */
    fun loopReached(
        id: String,
        reached: Boolean,
    ) {
        stateFor(id).reached = reached
        if (reached) follow(id)
    }

    /** 握着的那条死了（核心报的）：还是装着的那条 ⇒ 名义状态「重连中」；共用那条此刻够着 ⇒ 当场跟着换一条。 */
    private fun dropped(
        id: String,
        conn: SshTransport,
    ) {
        val follows =
            synchronized(lockFor(id)) {
                val st = stateFor(id)
                val entry = _sessions.value[id]
                if (st.connection !== conn || entry == null || !autoOf(st)) return
                put(entry.copy(status = SessionStatus.RECONNECTING, error = null))
                st.reached
            }
        if (follows) follow(id)
    }

    /** 跟着共用那条换一条：装着的那条已死、有配置、这台开着自动重连才换；同一台同时只换一次，没换成就等共用那条下一次报。 */
    private fun follow(id: String) {
        val st = states[id] ?: return
        val (cfgs, entry, gen) =
            synchronized(lockFor(id)) {
                val dead = st.connection?.let { !it.isConnected } == true
                val cfgs = st.configs?.takeIf { dead && autoOf(st) } ?: return
                val entry = _sessions.value[id] ?: return
                Triple(cfgs, entry, st.generation)
            }
        if (!st.following.compareAndSet(false, true)) return
        scope.launch {
            try {
                connect(id, entry.hostId, entry.label, cfgs, expectedGen = gen)
            } catch (ce: CancellationException) {
                throw ce // 期间被断开（代号过期）· 收场：这一次落空
            } catch (
                @Suppress("TooGenericExceptionCaught") ignored: Exception,
            ) {
                // 没换成：状态已标 ERROR（那一句是核心的），等共用那条下一次报够着再换。
            } finally {
                st.following.set(false)
            }
        }
    }

    private fun autoOf(st: HostConnectionState): Boolean =
        st.configs
            ?.firstOrNull()
            ?.reconnect
            ?.auto != false

    // 持有者集在 [HostConnectionState.holders]：多 tab 共享同一 id 的连接，只在 lockFor 内访问。
    // 用集合而不是计数：强制断开清集后，存活的 VM 重新 retain 即可恢复持有。

    /**
     * 登记一个持有者（会话屏或 tab，[holder] 是该 VM 的唯一 token），与 [release] 配对。按 holder 幂等，可安全重复 retain。
     *
     * 返回当前代号，调用方应把它作为随后 [connect] 的 expectedGen：强制断开若落在 retain 与 connect 之间，
     * connect 会因代号已变而弃用，不会装入一条无人持有的连接（那会让保活服务永不自停）。
     * retain 与读代号在同一把锁内原子完成。
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
     * 真断开的临界区体，调用方须已持 [lockFor]：自增代号作废在飞 connect，摘除连接，清配置 · 拨号请求 · 会话条目。
     * 锁内只改状态（`connection(id)` 立刻对外可见为 null），不在这里 close：close 是网络写，主线程上会泄漏。
     * 摘下的 transport 返回给调用方在锁外后台关。
     */
    private fun disconnectLocked(id: String): List<SshTransport> {
        val st = stateFor(id)
        st.generation++ // 作废任何在飞 connect 的注册
        val doomed = listOfNotNull(st.connecting, st.connection)
        st.connecting = null
        st.connection = null
        dials.update { it - id } // 共用那条随之停在断开
        st.configs = null // 防跟着共用循环复活手动断开的会话
        _sessions.update { it - id }
        return doomed
    }

    /**
     * 后台关 transport，不等结果。用本类长命的 [scope]，不用调用方的 scope：
     * viewModelScope 在 onCleared 返回时就取消了，close 永远不会执行。
     */
    private fun closeInBackground(transports: List<SshTransport>) {
        if (transports.isEmpty()) return
        scope.launch { transports.forEach { t -> runCatching { t.close() } } }
    }

    // 连接失败留下 ERROR 条目但没有连接的会话也要断：取「有连接的 id」与会话条目的并集。
    // 不用 states.keys：那里含已清空的条目，会白白加锁、自增代号。
    fun disconnectAll() = (states.filterValues { it.connection != null }.keys + _sessions.value.keys).toSet().forEach(::disconnect)

    /** 代号守门：[expectedGen] 非空且当前代号已不同（期间被 disconnect）就抛取消。须在 [lockFor] 内调。 */
    private fun throwIfEpochExpired(
        id: String,
        expectedGen: Int?,
    ) {
        if (expectedGen != null && stateFor(id).generation != expectedGen) {
            throw CancellationException("重连纪元过期：会话已被断开")
        }
    }

    /** 当前活跃会话数（CONNECTED），供保活服务出通知与自停。 */
    fun activeCount(): Int = _sessions.value.values.count { it.countsAsActive() }
}
