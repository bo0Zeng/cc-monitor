package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.remote.ExecResult
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.SendChannel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.channels.trySendBlocking
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import net.schmizz.sshj.SSHClient
import net.schmizz.sshj.connection.channel.OpenFailException
import net.schmizz.sshj.connection.channel.direct.PTYMode
import net.schmizz.sshj.sftp.OpenMode
import net.schmizz.sshj.transport.verification.PromiscuousVerifier
import java.io.IOException
import java.io.InputStream
import java.util.EnumSet
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * sshj 连接封装：连接与认证、交互 shell（PTY + resize）、exec、SFTP。
 *
 * - 有 [KnownHostStore] 时逐跳 TOFU 校验 host key；没有时退回 [PromiscuousVerifier]。
 * - 阻塞 IO 全部在 `Dispatchers.IO` 上。
 * - resize 调 sshj `Session.changeWindowDimensions`，远端收到 SIGWINCH。
 */
class SshConnection(
    private val knownHostStore: KnownHostStore? = null,
) : SshTransport {
    @Volatile private var ssh: SSHClient? = null

    // 本连接打开的全部 sshj client（目标 + 跳板链）。close() 逐个关。
    private val hops = CopyOnWriteArrayList<SSHClient>()

    // 本连接是否已被 close（看门狗超时或断开）。三处配合，防 close 之后新建的 client 逃出追踪：
    // [buildAndRegister] 登记前后各查一次；[connect] 决出赢家后再查一次；[close] 直接关 [ssh]。
    // 注意：sshj `SocketClient.disconnect()` 对还没 socket 的 client 是 no-op，所以「被 sweep」不等于「被中止」。
    // 赢家必被关；黑洞地址的输家可能阻塞到 sshj 自己的 connectTimeout（有界，不泄漏）。close 幂等。
    private val closed = AtomicBoolean(false)

    override suspend fun connect(
        configs: List<ConnectionConfig>,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) = withContext(Dispatchers.IO) {
        require(configs.isNotEmpty()) { "no address to connect" }
        // 跳板只连一次：各地址共用同一 jumpVia，每个目标只在它的隧道上做最后一跳。jumpClient 进 hops，close 时统一关。
        val jumpClient = configs.first().jumpVia?.let { connectClient(it, onEvent) }
        val winner =
            // 跳板 + 多地址走串行（理由见 [connectSequentially]）；直连多地址或单地址走并发竞速。
            if (jumpClient != null && configs.size > 1) {
                connectSequentially(configs, jumpClient, onEvent)
            } else {
                connectRacing(configs, jumpClient, onEvent)
            }
        // 先发布赢家：即便下面的复查漏过某种交错，[close] 直关 ssh 也能关到它。
        ssh = winner
        // 竞速期间若已被 close，赢家的 socket 可能是 close 之后才开的；关掉它并按取消抛出，不返回幻影成功。
        closeWinnerIfClosedDuringRace(winner, closed.get()) { runCatching { it.disconnect() } }
        // 注意：在「登记之后、client.connect 之前」被 close 扫过的 client，其拨号会阻塞到 sshj 的 connectTimeout。
        // 这里只保证不返回幻影成功、赢家必被关。
    }

    /**
     * 直连多地址或单地址的并发竞速（happy-eyeballs）：首个成功者胜，输家只关目标、不碰共享跳板。
     * jumpClient 非空时这里只会有一个地址，没有输家。
     */
    private suspend fun connectRacing(
        configs: List<ConnectionConfig>,
        jumpClient: SSHClient?,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ): SSHClient =
        raceToFirstSuccess(
            items = configs,
            create = { buildAndRegister(it) }, // 早建并登记 hops，close 才能中止阻塞中的拨号
            connect = { cfg, client -> finishConnect(cfg, client, jumpClient, onEvent) },
            closeLoser = { loser ->
                hops.remove(loser)
                runCatching { loser.disconnect() } // 只关落败目标，不碰共享跳板
            },
            aggregate = { errs -> aggregateRaceError(errs) { "${it.host}:${it.port}" } },
        )

    /**
     * 经共享跳板的多地址串行连接，首个成功即止，不并发开隧道。
     *
     * 注意：sshj 关掉一条还在握手的隧道时只在本地 eof 它的流，不发 CHANNEL_CLOSE；
     * 之后到达的 CHANNEL_DATA 会让跳板的 Reader 线程抛错、整条跳板传输层死掉，连带已认证的赢家。
     * 串行下每个地址都跑到自然成败才拆，赢家胜出时是跳板上唯一的隧道。失败聚合与竞速同形。
     */
    @Suppress("TooGenericExceptionCaught") // 逐地址捕获任何失败记入 errors，最后聚合
    private suspend fun connectSequentially(
        configs: List<ConnectionConfig>,
        jumpClient: SSHClient,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ): SSHClient {
        val errors = mutableListOf<Pair<ConnectionConfig, Throwable>>()
        for (cfg in configs) {
            var client: SSHClient? = null
            try {
                client = buildAndRegister(cfg) // 已 close 时这里抛，剩余地址快速失败后聚合抛出
                finishConnect(cfg, client, jumpClient, onEvent)
                return client // 首成功即止：此刻跳板上只有这一条隧道
            } catch (e: CancellationException) {
                client?.let {
                    hops.remove(it)
                    runCatching { it.disconnect() }
                }
                throw e
            } catch (e: Throwable) {
                errors.add(cfg to e)
                client?.let {
                    hops.remove(it)
                    // 失败的尝试通常已被对端 EOF，拆除时没有在飞数据。
                    // 注意：本地先判死而对端仍在发（host-key 不匹配、非 SSH 端口）时，拆除仍可能拖垮跳板，
                    // 本轮剩余地址随之失败，重试可恢复；已成功的赢家不受影响。
                    runCatching { it.disconnect() }
                }
            }
        }
        throw aggregateRaceError(errors) { "${it.host}:${it.port}" }
    }

    /**
     * 建跳板链：连接单个 [config]（连接、校验 host key、认证），返回其 SSHClient。
     * [ConnectionConfig.jumpVia] 非空时先递归连跳板，再经其隧道 `connectVia` 本目标（ssh -J 语义），逐跳独立 TOFU。
     * 整条链登记进 [hops]。
     */
    private suspend fun connectClient(
        config: ConnectionConfig,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ): SSHClient {
        val jumpClient = config.jumpVia?.let { connectClient(it, onEvent) } // 递归先连跳板（支持多跳）
        val client = buildAndRegister(config)
        finishConnect(config, client, jumpClient, onEvent)
        return client
    }

    /**
     * 建 SSHClient 并立即登记 [hops]，让 [close] 能中止阻塞在拨号上的 client。
     * 登记前后各查一次 [closed]：已关就注销、关掉并抛错（该地址按失败聚合）。
     * 本函数返回之后、client.connect 之前被 close 的情形由 [connect] 的复查兜。
     */
    private fun buildAndRegister(config: ConnectionConfig): SSHClient {
        check(!closed.get()) { "连接已关闭（看门狗超时或已断开），拒绝新建 client" }
        val client = buildClient(config).also { hops.add(it) }
        if (closed.get()) { // close 恰在建与登记之间触发，sweep 可能错过本 client，自行收尾
            hops.remove(client)
            runCatching { client.disconnect() }
            error("连接已关闭（看门狗超时或已断开），弃用刚建的 client")
        }
        return client
    }

    /** 单个 client 的连接、host key 校验、认证与 TOFU 落库；经 [jumpClient] 或直连。 */
    private suspend fun finishConnect(
        config: ConnectionConfig,
        client: SSHClient,
        jumpClient: SSHClient?,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) {
        fun emit(
            stage: SshConnectEvent.Stage,
            msg: String,
        ) {
            onEvent?.invoke(SshConnectEvent(config.host, config.port, stage, msg))
        }

        val tofu = installTofuVerifier(client, config)
        openTransport(client, config, jumpClient, tofu, ::emit)
        when (tofu?.outcome) {
            is TofuOutcome.FirstUse -> emit(SshConnectEvent.Stage.HOSTKEY, "首次信任该主机密钥（TOFU）")
            is TofuOutcome.Trusted -> emit(SshConnectEvent.Stage.HOSTKEY, "主机密钥已信任")
            else -> {} // Mismatch 已在 openTransport 抛出；null=无 store
        }
        authenticate(client, config, onEvent)
        // 认证成功后启用 keepalive（Config 已装 KEEP_ALIVE provider），探测静默死掉的对端。
        // 设置失败不该让连接失败，但要打日志。
        runCatching { client.connection.keepAlive.keepAliveInterval = config.keepAliveIntervalSec }
            .onFailure { android.util.Log.w("SshConnection", "keepalive 设置失败（连接仍可用，但静默死对端探测降级）: ${it.message}") }
        // 首见的 host key 认证成功后才落库，免得未认证的端点往 TOFU 库里投毒。
        (tofu?.outcome as? TofuOutcome.FirstUse)?.let { knownHostStore?.record(it.knownHost) }
        emit(SshConnectEvent.Stage.ESTABLISHED, "${config.username}@${config.host}:${config.port} 连接已建立")
    }

    /** KeystoreSigner 连接用替换了 ecdsa KeyAlgorithm 的 Config（签名走 AndroidKeyStore），其余用默认。 */
    private fun buildClient(config: ConnectionConfig): SSHClient =
        SSHClient(
            if (config.auth is AuthMethod.KeystoreSigner) SshjConfigFactory.createForKeystoreSigner() else SshjConfigFactory.create(),
        ).apply { connectTimeout = config.connectTimeoutMs }

    /** 有 store 时装 TOFU 校验器并返回它（供判定 outcome）；没有 store 时装 PromiscuousVerifier、返回 null。 */
    private suspend fun installTofuVerifier(
        client: SSHClient,
        config: ConnectionConfig,
    ): TofuHostKeyVerifier? {
        val store = knownHostStore
        val tofu =
            if (store != null) {
                TofuHostKeyVerifier(config.host, config.port, store.forHost(config.host, config.port), System.currentTimeMillis())
            } else {
                android.util.Log.w("SshConnection", "TOFU 未启用(无 KnownHostStore) → PromiscuousVerifier，host key 不校验")
                null
            }
        client.addHostKeyVerifier(tofu ?: PromiscuousVerifier())
        return tofu
    }

    /** 打开传输层：有跳板 → 经隧道 `connectVia`；否则直连。host-key Mismatch → 抛 [HostKeyChangedException]。 */
    private fun openTransport(
        client: SSHClient,
        config: ConnectionConfig,
        jumpClient: SSHClient?,
        tofu: TofuHostKeyVerifier?,
        emit: (SshConnectEvent.Stage, String) -> Unit,
    ) {
        try {
            if (jumpClient != null) {
                emit(SshConnectEvent.Stage.TCP, "经跳板 ${config.jumpVia?.host} 打开到 ${config.host}:${config.port} 的隧道 …")
                client.connectVia(jumpClient.newDirectConnection(config.host, config.port)) // ssh -J
            } else {
                emit(SshConnectEvent.Stage.TCP, "连接 ${config.host}:${config.port} …")
                client.connect(config.host, config.port)
            }
        } catch (e: Exception) {
            val o = tofu?.outcome
            if (o is TofuOutcome.Mismatch) {
                emit(SshConnectEvent.Stage.ERROR, "主机密钥不匹配（疑 MITM/服务器重装）")
                runCatching { client.disconnect() }
                throw HostKeyChangedException(
                    host = config.host,
                    port = config.port,
                    keyType = o.keyType,
                    // 新旧指纹供恢复页展示。存的 b64 是 wire blob 的 base64，解码即可算 SHA256 指纹；
                    // 算不出就给空串，不让报错本身再崩。
                    expectedFingerprint = fingerprintOrEmpty(o.expectedB64),
                    offeredFingerprint = fingerprintOrEmpty(o.offeredB64),
                    cause = e,
                )
            }
            emit(SshConnectEvent.Stage.ERROR, "连接失败：${e.message ?: e.javaClass.simpleName}")
            throw e
        }
        emit(SshConnectEvent.Stage.NEGOTIATE, "协议协商与密钥交换完成")
    }

    /** 按认证方式认证：先发 AUTH 阶段事件，失败时发 ERROR 再抛。 */
    private fun authenticate(
        client: SSHClient,
        config: ConnectionConfig,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) {
        fun emit(
            stage: SshConnectEvent.Stage,
            msg: String,
        ) {
            onEvent?.invoke(SshConnectEvent(config.host, config.port, stage, msg))
        }
        try {
            when (val auth = config.auth) {
                is AuthMethod.Password -> {
                    emit(SshConnectEvent.Stage.AUTH, "密码认证中 …")
                    client.authPassword(config.username, auth.password)
                }
                is AuthMethod.PrivateKey -> {
                    emit(SshConnectEvent.Stage.AUTH, "公钥认证中 …")
                    client.authPublickey(config.username, client.loadKeys(auth.pem.toString(Charsets.UTF_8), null, null))
                }
                is AuthMethod.KeystoreSigner -> {
                    emit(SshConnectEvent.Stage.AUTH, "硬件密钥认证中（TEE 签名）…")
                    client.authPublickey(config.username, KeystoreEcdsaSigner(auth.alias))
                }
            }
        } catch (e: Exception) {
            emit(SshConnectEvent.Stage.ERROR, "认证失败：${e.message ?: e.javaClass.simpleName}")
            throw e
        }
    }

    override val isConnected: Boolean get() = ssh?.isConnected == true && ssh?.isAuthenticated == true

    /**
     * 活性探测：开关一次 session（CHANNEL_OPEN → CLOSE），真走网络，死 TCP 在 [timeoutMs] 内暴露
     * （[isConnected] 只读本地标志，死 TCP 好一阵仍是 true）。超时就关整条连接，解开同连接上其他阻塞读，判 false。
     * 通道级拒绝不等于连接死：CHANNEL_OPEN_FAILURE（如 sshd MaxSessions 打满）说明对端有应答，判活且不关连接。
     * 分型见 [probeExceptionMeansAlive]。
     */
    override suspend fun probeAlive(timeoutMs: Long): Boolean =
        withContext(Dispatchers.IO) {
            val client = ssh
            if (client == null || !isConnected) return@withContext false
            try {
                readWithDeadline(timeoutMs, onTimeout = { runCatching { close() } }) { client.startSession().close() }
                true
            } catch (ce: CancellationException) {
                throw ce
            } catch (e: Exception) {
                probeExceptionMeansAlive(e)
            }
        }

    /**
     * 开交互 shell（分配 PTY）。返回的通道暴露 in/out 流和 resize，供终端桥接。
     * 注意：startSession / allocatePTY / startShell 是阻塞往返，死连接上会无限阻塞且不响应协程取消，
     * 所以包在 [readWithDeadline] 里。超时就关整条连接（幂等），重试走真重连，也解开同连接上其他阻塞读。
     */
    override suspend fun openShell(
        term: String,
        cols: Int,
        rows: Int,
    ): ShellChannel =
        withContext(Dispatchers.IO) {
            readWithDeadline(
                OPEN_SHELL_TIMEOUT_MS,
                onTimeout = { runCatching { close() } },
                message = "打开终端通道超时（${OPEN_SHELL_TIMEOUT_MS / 1000} 秒）：连接可能已失效，已断开，请重试",
            ) {
                val session = client().startSession()
                session.allocatePTY(term, cols, rows, 0, 0, emptyMap<PTYMode, Int>())
                ShellChannel(session, session.startShell())
            }
        }

    /**
     * 一次性执行，带退出码与 stderr 尾部。流式长跑命令（`tail -F`）走 [execStream]，这条用于写操作与探测。
     *
     * 1. stderr 必须同时排空：sshj 的 stderr 与 stdout 共用同一个通道窗口，不读 stderr 会把 stdout 一起拖停。
     *    所以两个线程各读一路，不能先读完 stdout 再读 stderr。
     * 2. 不分配 PTY：`session.exec()` 本来就不分配。
     * 3. 退出码 null 不等于 0：连接断或命令被信号杀时 sshj 给 null，不能当成功。
     */
    override suspend fun execCapture(command: String): ExecResult =
        withContext(Dispatchers.IO) {
            val session = client().startSession()
            try {
                val cmd = session.exec(command)
                // 两路并行读；stderr 只留尾部。
                val errBuf = StringBuilder()
                val errReader =
                    Thread({
                        runCatching {
                            cmd.errorStream.bufferedReader().forEachLine { errBuf.appendLine(it) }
                        }
                    }, "ssh-exec-stderr").apply {
                        isDaemon = true
                        start()
                    }
                val out = cmd.inputStream.readBytes().toString(Charsets.UTF_8)
                cmd.join(EXEC_CAPTURE_TIMEOUT_SEC, TimeUnit.SECONDS)
                errReader.join(EXEC_CAPTURE_STDERR_JOIN_MS)
                ExecResult(
                    exitStatus = cmd.exitStatus,
                    stdout = out,
                    stderrTail = errBuf.toString().takeLast(STDERR_TAIL_BYTES),
                )
            } finally {
                runCatching { session.close() }
            }
        }

    /**
     * 流式执行长命令（如 `tail -F`），按到达顺序发 stdout 字节块。
     *
     * 阻塞的 socket `read()` 不响应协程取消，所以取消时显式关 session：关通道、杀远端命令、解开阻塞读。
     * 由专用 daemon 线程读（[pumpStreamToChannel]）。
     *
     * 缓冲有界并带背压：[EXEC_STREAM_BUFFER_CHUNKS] 封顶在途内存；满时读线程阻塞等待（不丢块），
     * 不再读就不补 sshj 本地窗口，远端随之停发。
     */
    override fun execStream(command: String): Flow<ByteArray> =
        callbackFlow {
            val session = client().startSession()
            val cmd =
                try {
                    session.exec(command)
                } catch (t: Throwable) {
                    runCatching { session.close() } // 启动失败也关 session，防通道泄漏
                    throw t
                }
            val input = cmd.inputStream
            val reader =
                Thread({
                    try {
                        pumpStreamToChannel(input, this@callbackFlow, EXEC_STREAM_READ_CHUNK)
                        close()
                    } catch (t: Throwable) {
                        close(t) // 取消导致的 session 关闭也走这里；flow 已在关闭则被忽略
                    }
                }, "ssh-exec-stream").apply {
                    isDaemon = true
                    start()
                }
            awaitClose {
                // 关 session 才能解开阻塞的 socket read（interrupt 无效），同时杀掉远端命令。
                // 读线程若阻塞在 trySendBlocking，flow 通道关闭会唤醒它，join 不会死锁。
                runCatching { cmd.close() }
                runCatching { session.close() }
                runCatching { reader.join(500) } // 等 reader 退出，teardown 可观测
            }
            // buffer(N) 与 callbackFlow 的通道融合，生产者通道容量即 N。
        }.buffer(EXEC_STREAM_BUFFER_CHUNKS).flowOn(Dispatchers.IO) // startSession / exec 是阻塞调用，离开主线程

    /** 列目录（滤掉 `.` 与 `..`；目录在前，再按名排序）。 */
    override suspend fun sftpList(path: String): List<SftpEntry> =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { sftp ->
                sftp
                    .ls(path)
                    .filter { it.name != "." && it.name != ".." }
                    .map { SftpEntry(it.name, it.path, it.isDirectory, it.attributes.size) }
                    .sortedWith(compareByDescending<SftpEntry> { it.isDir }.thenBy { it.name.lowercase() })
            }
        }

    /** 把路径规范化为绝对路径（文件浏览从 home `.` 起算、往上级导航时用）。 */
    override suspend fun sftpRealPath(path: String): String =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { it.canonicalize(path) }
        }

    /** 下载远端文件到本地 [local]。 */
    override suspend fun sftpDownload(
        remotePath: String,
        local: java.io.File,
    ): Unit =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { it.get(remotePath, local.absolutePath) }
        }

    /** 上传本地文件 [local] 到远端 [remotePath]。 */
    override suspend fun sftpUpload(
        local: java.io.File,
        remotePath: String,
    ): Unit =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { it.put(local.absolutePath, remotePath) }
        }

    /** 重命名或移动远端路径。 */
    override suspend fun sftpRename(
        from: String,
        to: String,
    ): Unit =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { it.rename(from, to) }
        }

    /** 删除远端文件或空目录（非空目录由 sshj 抛错，上层提示）。 */
    override suspend fun sftpDelete(
        path: String,
        isDir: Boolean,
    ): Unit =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { if (isDir) it.rmdir(path) else it.rm(path) }
        }

    /** 新建目录。 */
    override suspend fun sftpMkdir(path: String): Unit =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { it.mkdir(path) }
        }

    /** 读远端文本预览，最多 [maxBytes] 字节；超限时尾部加截断标记。 */
    override suspend fun sftpReadText(
        path: String,
        maxBytes: Int,
    ): String =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { sftp ->
                sftp.open(path).use { rf ->
                    val total = rf.length()
                    val cap = maxBytes.toLong().coerceAtMost(total).toInt()
                    val buf = ByteArray(cap)
                    var off = 0
                    while (off < cap) {
                        val n = rf.read(off.toLong(), buf, off, cap - off)
                        if (n < 0) break
                        off += n
                    }
                    val text = String(buf, 0, off, Charsets.UTF_8)
                    if (total > maxBytes) "$text\n…（已截断，共 $total 字节）" else text
                }
            }
        }

    /** 读供编辑的全量文本。超过 [maxBytes] 或含 NUL（疑似二进制）返回 null：拒绝编辑，免得截断或乱码写坏文件。 */
    override suspend fun sftpReadTextForEdit(
        path: String,
        maxBytes: Int,
    ): String? =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { sftp ->
                sftp.open(path).use { rf ->
                    val total = rf.length()
                    if (total > maxBytes) return@withContext null
                    val buf = ByteArray(total.toInt()) // total ≤ maxBytes，toInt() 安全
                    var off = 0
                    while (off < buf.size) {
                        val n = rf.read(off.toLong(), buf, off, buf.size - off)
                        if (n <= 0) break // <0 是 EOF；==0 防异常服务器回 0 长度包时死循环
                        off += n
                    }
                    // 只扫已填充前缀 [0,off)：文件在 length() 后被截短时尾部是零，整缓冲扫会误判二进制。
                    if ((0 until off).any { buf[it] == 0.toByte() }) return@withContext null // 含 NUL：疑似二进制
                    String(buf, 0, off, Charsets.UTF_8)
                }
            }
        }

    /** 以 UTF-8 覆写远端文件（WRITE|CREAT|TRUNC）。 */
    override suspend fun sftpWriteText(
        remotePath: String,
        text: String,
    ): Unit =
        withContext(Dispatchers.IO) {
            client().newSFTPClient().use { sftp ->
                sftp.open(remotePath, EnumSet.of(OpenMode.WRITE, OpenMode.CREAT, OpenMode.TRUNC)).use { rf ->
                    val bytes = text.toByteArray(Charsets.UTF_8)
                    // 分块写（每包 ≤32KB）：超大的单个 SFTP WRITE 包会被部分严格的服务器拒绝。
                    var off = 0
                    while (off < bytes.size) {
                        val n = minOf(WRITE_CHUNK, bytes.size - off)
                        rf.write(off.toLong(), bytes, off, n)
                        off += n
                    }
                }
            }
        }

    override fun close() {
        closed.set(true) // 先立旗：此后 buildAndRegister 拒绝或自关新 client
        hops.forEach { runCatching { it.disconnect() } } // 关目标与整条跳板链
        hops.clear()
        // 直关赢家：它可能在 hops 清空之后才被赋给 ssh。disconnect 幂等。
        runCatching { ssh?.disconnect() }
        ssh = null
    }

    // 「未连接」抛 IOException 而不是 IllegalStateException：断连一律以 IOException 上抛。
    private fun client(): SSHClient = ssh ?: throw IOException("SSH 连接不可用（未连接 / 已断开）")

    private companion object {
        /** SFTP WRITE 分包大小（SFTP 草案建议 ≤32KB）。 */
        const val WRITE_CHUNK = 32 * 1024

        /** execStream 单次 read 的块大小上限（字节）。 */
        const val EXEC_STREAM_READ_CHUNK = 8 * 1024

        /**
         * `execCapture` 等命令结束的上限。不是网络超时，是命令卡住时的兜底；这里跑的命令健康时都是毫秒级。
         * 超时后 `cmd.exitStatus` 为 null，按没有退出码处理，不当成功。
         */
        const val EXEC_CAPTURE_TIMEOUT_SEC = 30L

        /** 等 stderr 读线程收尾的上限。stdout 已到 EOF、命令已 join，这里只是把残余读干净。 */
        const val EXEC_CAPTURE_STDERR_JOIN_MS = 1_000L

        /** `ExecResult.stderrTail` 保留的尾部字节数。 */
        const val STDERR_TAIL_BYTES = 4 * 1024

        /**
         * execStream 生产者通道容量（块数）。在途上限 64 块 × ≤8KiB ≤ 512KiB。
         * 够大，吸收解析抖动；够小，几条 tail 并发时在途总量也只有 MiB 级。
         */
        const val EXEC_STREAM_BUFFER_CHUNKS = 64

        /**
         * openShell 全程（startSession + allocatePTY + startShell）的期限。三次往返，健康连接秒级完成；
         * 15s 给弱网留足余量，又低于握手看门狗与 sshj 内部超时。
         */
        const val OPEN_SHELL_TIMEOUT_MS = 15_000L
    }
}

/**
 * 竞速决出赢家后复查是否已被关闭。已关闭时赢家的 socket 可能是 close 之后才开的：
 * 关掉赢家（[closeWinner]）并抛 [CancellationException]，上层按取消处置，不返回幻影成功、不泄漏已认证连接。
 * [closed] 为 false 时原样放行。
 */
internal fun <C> closeWinnerIfClosedDuringRace(
    winner: C,
    closed: Boolean,
    closeWinner: (C) -> Unit,
) {
    if (closed) {
        closeWinner(winner)
        throw CancellationException("连接在竞速期间已被关闭（看门狗超时或断开）—弃用赢家，防幻影胜出泄漏")
    }
}

/**
 * 探测往返的异常分型：服务器有应答就算连接活。
 * [OpenFailException]（CHANNEL_OPEN_FAILURE，如 sshd MaxSessions 打满）判活；
 * 传输层异常、[readWithDeadline] 的超时 IOException 及其它 IO 错误判死。
 */
internal fun probeExceptionMeansAlive(e: Throwable): Boolean = e is OpenFailException

/**
 * 把一段阻塞读限制在 [timeoutMs] 内。到点由看门狗执行 [onTimeout]（调用方在这里关底层流：
 * 阻塞的 socket read 不响应取消和 interrupt），[read] 随之返回或抛错后以 [IOException] 报超时，
 * 不把被掐断的部分输出当成功。未超时则原样返回结果或重抛异常。[message] 可定制超时文案。
 * 读与看门狗用一个 CAS 抢定局，二者互斥，读刚好在期限前完成时不会再触发 [onTimeout]。
 * 注意：必须在多线程 dispatcher（如 IO）上调用。
 */
internal suspend fun <T> readWithDeadline(
    timeoutMs: Long,
    onTimeout: () -> Unit,
    message: String? = null,
    read: () -> T,
): T =
    coroutineScope {
        val settled = AtomicBoolean(false)
        val watchdog =
            launch {
                delay(timeoutMs)
                if (settled.compareAndSet(false, true)) onTimeout()
            }
        val result = runCatching(read)
        val wonByRead = settled.compareAndSet(false, true) // false：看门狗已先判超时
        watchdog.cancel()
        if (!wonByRead) throw IOException(message ?: "命令执行超时（${timeoutMs}ms），已中止")
        result.getOrThrow()
    }

/**
 * execStream 的读线程泵：在专用阻塞线程上把 [input] 按 ≤[chunkSize] 的块送进 [channel]，直到 EOF 或通道关闭。
 * - 背压、不丢块：通道满时 [trySendBlocking] 阻塞等待（丢一块就是 JSONL 断行）。
 * - 通道关闭即停、不自旋；阻塞中的 send 也会被关闭唤醒。
 * - EOF 正常返回；读错误原样上抛。
 */
internal fun pumpStreamToChannel(
    input: InputStream,
    channel: SendChannel<ByteArray>,
    chunkSize: Int,
) {
    val buf = ByteArray(chunkSize)
    var pumping = true
    while (pumping) {
        val n = input.read(buf)
        when {
            n < 0 -> pumping = false // EOF：远端命令结束或 session 被关
            n > 0 -> pumping = channel.trySendBlocking(buf.copyOf(n)).isSuccess
        }
    }
}

/** 把 known-host 里存的 base64 wire blob 换算成 `SHA256:…` 指纹。失败返回空串：只用于展示对比，不能让报错本身崩。 */
private fun fingerprintOrEmpty(b64: String): String =
    runCatching {
        com.ccmonitor.mobile.core.data.crypto.SshWire.fingerprintSha256(
            java.util.Base64
                .getDecoder()
                .decode(b64),
        )
    }.getOrDefault("")
