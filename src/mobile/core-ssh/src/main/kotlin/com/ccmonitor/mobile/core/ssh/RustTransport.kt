package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.crypto.SshWire
import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.rs.ResidentEvents
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.CaptureOpts
import com.ccmonitor.mobile.core.wire.gen.Captured
import com.ccmonitor.mobile.core.wire.gen.DialAck
import com.ccmonitor.mobile.core.wire.gen.DialRequest
import com.ccmonitor.mobile.core.wire.gen.DialUse
import com.ccmonitor.mobile.core.wire.gen.JumpHop
import com.ccmonitor.mobile.core.wire.gen.PtyOpts
import com.ccmonitor.mobile.core.wire.gen.Stage
import com.ccmonitor.mobile.core.wire.gen.StageRow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.security.KeyStore
import java.security.PrivateKey
import java.security.interfaces.ECPublicKey
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean

/**
 * [SshTransport] 走手机嵌的 Rust 核心（`:core-rs`，拨号核心 `ssh-dial` 与后端同一份）。
 *
 * - **连上**＝开一条握着连接的链路（`use: hold`）：竞速 · 跳板（多跳）· host key · 鉴权 · 保活都在核心里；之后每一次 exec /
 *   shell 各开一条链路，按同一份拨号身份复用那条连接（核心的连接池）。握着的那条收工（对端断 · 保活判死）⇒ [isConnected] 变假。
 * - host key：读 · 判 · 记都在核心里（App 私有目录那份库，[installRustCore] 时交的路径）；「钥匙变了」照 ack 说（[HostKeyChangedException]）。
 * - 钥匙交进核心、拨号请求写它的编号（密码 · PEM · Keystore 别名＋公钥）；每一条都握着自己那份身份的钥匙，[close] 时放掉 ——
 *   身份的最后一条收掉，核心里那几把就没了。Keystore 的签名在这里做（回 DER，核心换成 SSH 的格式）。
 */
class RustTransport(
    /** 照这一份拨号身份再握一条（[again]）；`null` ⇒ [connect] 照配置组一份新的。 */
    private val preset: DialRequest? = null,
    /** [preset] 那份身份的钥匙编号（[again] 已替这一条多握了一份）。 */
    held: List<String> = emptyList(),
) : SshTransport {
    /** 拨号身份那一份请求（不带用法）：每条链路照它开，核心按它复用同一条连接。 */
    @Volatile private var base: DialRequest? = preset

    @Volatile private var hold: RustLink? = null
    private val children: MutableSet<RustLink> = ConcurrentHashMap.newKeySet()
    private val closed = AtomicBoolean(false)
    private val watcher = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private val drops = mutableListOf<() -> Unit>()

    /** 这一条握着的钥匙编号（[close] 时放掉）。 */
    private val keys = held.toMutableList()

    override val isConnected: Boolean get() = !closed.get() && hold?.isOpen == true

    override fun again(): SshTransport? {
        val b = base ?: return null
        val ids = synchronized(keys) { keys.takeUnless { closed.get() }?.toList() } ?: return null
        val kept = ids.takeWhile { Core.holdKey(it, true) }
        if (kept.size < ids.size) {
            kept.forEach { Core.holdKey(it, false) }
            return null
        }
        return RustTransport(b, ids)
    }

    override fun onDrop(f: () -> Unit) {
        synchronized(drops) { drops += f }
        // 登记之前就已经收工了的那一条：当场叫（叫两次无妨：管理那一侧按「还是不是装着的那条」认）。
        if (hold?.isOpen == false && !closed.get()) f()
    }

    override suspend fun connect(
        config: ConnectionConfig,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) = withContext(Dispatchers.IO) {
        preset?.let { return@withContext holdOn(it, config, onEvent) }
        // 核心组好的那一份，只把每一跳写的那台编号换成交进核心的钥匙编号。
        val plan = config.request
        val req = plan.copy(keyPath = keyOf(plan.keyPath, config), jump = plan.jump?.let { hopOf(it, config) })
        base = req
        holdOn(req, config, onEvent)
    }

    /**
     * 照 [req] 握一条连接（`use: hold`）、读到 ack。期限在核心里（按进展算，共用那条重连循环同一处），这里不等表。
     * 握着的那条收工（对端断 · 保活判死 · 核心收场）⇒ [isConnected] 随之变假、[onDrop] 那几个各叫一次（自己 [close] 的不叫）。
     * 三处 throw 都是有意的：链路没给 ack · ack 说没成 · 期间被关了。
     */
    @Suppress("ThrowsCount")
    private suspend fun holdOn(
        req: DialRequest,
        first: ConnectionConfig,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) {
        val link = RustCore.open(req.copy(use = DialUse.HOLD, stages = onEvent != null))
        val ack =
            try {
                link.awaitAck { st -> eventOf(st, first)?.let { onEvent?.invoke(it) } }
            } catch (e: IOException) {
                link.close()
                throw HostUnreachedException(e.message ?: copyText("reason.link.lost"), e)
            }
        if (!ack.ok) {
            link.close()
            throw failureOf(ack, first)
        }
        if (closed.get()) {
            link.close()
            throw IOException(copyText("reason.link.lost"))
        }
        hold = link
        watcher.launch {
            while (link.readChunk() != null) Unit
            if (!closed.get()) synchronized(drops) { drops.toList() }.forEach { it() }
        }
    }

    override suspend fun execCapture(command: String): Captured =
        withContext(Dispatchers.IO) {
            val link = child { it.copy(use = DialUse.CAPTURE, command = command, capture = CaptureOpts(CAPTURE_MAX.toLong(), null, null)) }
            try {
                val line = link.readLine() ?: throw IOException(copyText("reason.link.lost"))
                // 收全的那一行认不出 ⇒ 两端契约对不上（不补默认、不当成功）。
                Wire.read<Captured>(line) ?: throw IOException(copyText("beServer.words.cantConnect"))
            } finally {
                link.close()
            }
        }

    override val dialRequest: String? get() = base?.let { Wire.line(it) }

    override suspend fun openShell(
        term: String,
        cols: Int,
        rows: Int,
    ): ShellChannel =
        withContext(Dispatchers.IO) {
            RustShell(child { it.copy(use = DialUse.SHELL, pty = PtyOpts(term, cols, rows)) })
        }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        children.forEach { it.close() }
        children.clear()
        hold?.close()
        watcher.cancel()
        val ids = synchronized(keys) { keys.toList().also { keys.clear() } }
        ids.forEach { Core.holdKey(it, false) }
    }

    // ── 链路 ─────────────────────────────────────────────────────────────────────

    /** 照拨号身份开一条用法链路、读到 ack；ack 不成 ⇒ 抛（连接没了 ⇒ [ConnectionDeadException]）。 */
    private suspend fun child(use: (DialRequest) -> DialRequest): RustLink {
        val b = base
        if (b == null || !isConnected) throw ConnectionDeadException(copyText("reason.link.lost"))
        val link = RustCore.open(use(b))
        children += link
        link.onClose { children -= link }
        val ack = link.awaitAck {}
        if (!ack.ok) {
            link.close()
            throw IOException(ack.said.orEmpty())
        }
        return link
    }

    // ── 钥匙 · host key ─────────────────────────────────────────────────────────

    /** 把一样鉴权交进核心（这一条握一份），回它的编号（拨号请求的 `key_path`）；这一条已经关了 ⇒ 当场放掉、抛。 */
    private fun keyFor(auth: AuthMethod): String {
        val id =
            when (auth) {
                is AuthMethod.Password -> Core.putPassword(auth.password)
                is AuthMethod.PrivateKey -> Core.putPem(auth.pem, auth.passphrase?.concatToString())
                is AuthMethod.KeystoreSigner -> {
                    val ks = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
                    val pub =
                        ks.getCertificate(auth.alias)?.publicKey as? ECPublicKey
                            ?: error(copyText("mobSsh.keystore.noCert"))
                    Core.putKeystore(auth.alias, SshWire.uncompressedPoint(pub)) ?: error(copyText("mobSsh.keystore.noCert"))
                }
            } ?: throw IOException(copyText("reason.link.lost"))
        val kept = synchronized(keys) { !closed.get() && keys.add(id) }
        if (!kept) {
            Core.holdKey(id, false)
            throw IOException(copyText("reason.link.lost"))
        }
        return id
    }

    /** 那一跳写的那台编号 ⇒ 交进核心的钥匙编号（[ConnectionConfig.auths] 里没有那一台 ⇒ 不拨，不当成走 agent）。 */
    private fun keyOf(
        which: String?,
        cfg: ConnectionConfig,
    ): String = keyFor(cfg.auths[which] ?: error(copyText("mobConnect.host.noAuth")))

    private fun hopOf(
        hop: JumpHop,
        cfg: ConnectionConfig,
    ): JumpHop = hop.copy(keyPath = keyOf(hop.keyPath, cfg), jump = hop.jump?.let { hopOf(it, cfg) })

    /**
     * ack 带着库里那一把的指纹 ⇒ 钥匙对不上被拒（核心判的、指纹也是核心写的）；库读不出（含旧表没迁完）只是 `host_key` 那一句。
     * 没够着那一台（解析 · 拨不通 · 到期限 · 跳板断了）也是核心判的（[DialAck.unreached]）。
     */
    private fun failureOf(
        ack: DialAck,
        cfg: ConnectionConfig,
    ): Exception {
        val said = ack.said?.ifEmpty { null } ?: copyText("beServer.words.cantConnect")
        val expected = ack.expectedFingerprint
        return when {
            expected != null ->
                HostKeyChangedException(
                    host = cfg.host,
                    port = cfg.port,
                    keyType = ack.keyType.orEmpty(),
                    expectedFingerprint = expected,
                    offeredFingerprint = ack.fingerprint.orEmpty(),
                )
            ack.unreached -> HostUnreachedException(said)
            else -> IOException(said)
        }
    }

    /**
     * 一条拨号阶段 ⇒ 连接日志那一条：字是拨号核心写好的那一句，主机与端口取阶段带的那一台（鉴权 · 就绪两支不带 ⇒ 请求那一台）。
     * 「过了一步」不上日志（手机不要 `progress`，只在这里明写）⇒ `null`。
     */
    private fun eventOf(
        stage: Stage,
        cfg: ConnectionConfig,
    ): SshConnectEvent? {
        val (at, kind, said) =
            when (stage) {
                is Stage.Step -> return null
                is Stage.Dialing -> Triple(stage.endpoint, SshConnectEvent.Stage.TCP, stage.said)
                is Stage.HostKey -> Triple(stage.endpoint, SshConnectEvent.Stage.HOSTKEY, stage.said)
                is Stage.Won -> Triple(stage.endpoint, SshConnectEvent.Stage.NEGOTIATE, stage.said)
                is Stage.Failed -> Triple(stage.endpoint, SshConnectEvent.Stage.ERROR, stage.said)
                is Stage.Auth -> Triple(null, if (stage.ok) SshConnectEvent.Stage.AUTH else SshConnectEvent.Stage.ERROR, stage.said)
                is Stage.Established -> Triple(null, SshConnectEvent.Stage.ESTABLISHED, stage.said)
            }
        return SshConnectEvent(at?.host ?: cfg.host, at?.port ?: cfg.port, kind, said)
    }

    internal companion object {
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val CAPTURE_MAX = 16 * 1024 * 1024
    }
}

/**
 * 核心装上（记下 Kotlin 这一侧的回调、装上手机那一套）：主机钥匙那份库在 [knownHosts]。只认第一次；App 一起来就装，
 * 之后才迁旧表、才拨号（常驻流交的事经同一个回调回来）。
 */
fun installRustCore(knownHosts: File) = RustCore.install(knownHosts)

/** 核心装过了（没装就拨 ⇒ 抛：没装的核心一把主机钥匙都不认、也不记）。 */
fun ensureRustCore() = RustCore.ensure()

/** 忘掉这台的主机钥匙（「钥匙变了」页上点了才调；规矩在核心）。写不成 ⇒ `false`。 */
fun forgetHostKey(
    host: String,
    port: Int,
): Boolean {
    RustCore.ensure()
    return Core.forgetHost(host, port)
}

/** 进程里那一份 Rust 核心的 Kotlin 一侧：链路编号 → 收件箱 · Keystore 签名 · 常驻流交的事。 */
internal object RustCore : Core.Sink {
    private val inboxes = ConcurrentHashMap<Long, Channel<ByteArray>>()

    @Volatile private var ready = false

    fun install(knownHosts: File) {
        synchronized(this) {
            if (!ready) {
                Core.init(this, knownHosts.path)
                ready = true
            }
        }
    }

    fun ensure() = check(ready)

    fun open(req: DialRequest): RustLink {
        ensure()
        val handle = Core.open(Wire.line(req))
        if (handle == 0L) throw IOException(copyText("reason.link.lost"))
        return RustLink(handle, inboxOf(handle))
    }

    private fun inboxOf(handle: Long): Channel<ByteArray> = inboxes.computeIfAbsent(handle) { Channel(Channel.UNLIMITED) }

    fun drop(handle: Long) {
        inboxes.remove(handle)?.close()
    }

    override fun data(
        handle: Long,
        bytes: ByteArray?,
    ) {
        val inbox = inboxOf(handle)
        if (bytes == null) inbox.close() else inbox.trySend(bytes)
    }

    @Suppress("TooGenericExceptionCaught") // Keystore 什么都可能抛；回 null ＝ 签不了，核心按鉴权失败报
    override fun sign(
        alias: String,
        data: ByteArray,
    ): ByteArray? =
        try {
            val ks = KeyStore.getInstance(RustTransport.ANDROID_KEYSTORE).apply { load(null) }
            val key = ks.getKey(alias, null) as PrivateKey
            java.security.Signature.getInstance("SHA256withECDSA").run {
                initSign(key)
                update(data)
                sign()
            }
        } catch (e: Exception) {
            // 回 null ＝ 签不了：核心照鉴权失败报（那一句带原因），这里留一行日志。
            android.util.Log.w("mobcore", "Keystore 签名失败（$alias）", e)
            null
        }

    /** 常驻流交的事：按那台的编号分过去（`ResidentEvents`）。 */
    override fun resident(
        handle: Long,
        kind: String,
        body: String,
    ) = ResidentEvents.deliver(handle, kind, body)
}

/** 一条链路（Kotlin 这一侧）：下行字节按序读（行 / 块），上行经核心送。 */
internal class RustLink(
    private val handle: Long,
    private val inbox: Channel<ByteArray>,
) {
    private var leftover = ByteArray(0)
    private val ended = AtomicBoolean(false)

    /** 核心那头收工了（读到了「没了」）。 */
    private val closedByCore = AtomicBoolean(false)
    private val onClose = mutableListOf<() -> Unit>()

    val isOpen: Boolean get() = !ended.get() && !closedByCore.get()

    fun onClose(f: () -> Unit) {
        synchronized(onClose) { onClose += f }
    }

    /** 下一块下行字节；链路收工 ⇒ `null`。 */
    suspend fun readChunk(): ByteArray? {
        if (leftover.isNotEmpty()) return leftover.also { leftover = ByteArray(0) }
        return inbox.receiveCatching().getOrNull().also { if (it == null) closedByCore.set(true) }
    }

    /** 下一行（不含 `\n`）；收工 ⇒ `null`。 */
    suspend fun readLine(): String? {
        var buf = leftover
        leftover = ByteArray(0)
        while (true) {
            val nl = buf.indexOf('\n'.code.toByte())
            if (nl >= 0) {
                leftover = buf.copyOfRange(nl + 1, buf.size)
                return buf.copyOfRange(0, nl).toString(Charsets.UTF_8)
            }
            val more = inbox.receiveCatching().getOrNull()
            if (more == null) {
                closedByCore.set(true)
                return null
            }
            buf += more
        }
    }

    /**
     * 读到 ack 那一行（[DialAck]；之前的阶段行 `{"stage":…}` 交给 [onStage]）；没等到就收工 ⇒ 抛。
     * 哪一行认不出（两端契约对不上）⇒ 抛，不补默认。
     */
    suspend fun awaitAck(onStage: (Stage) -> Unit): DialAck {
        while (true) {
            val line = readLine() ?: throw IOException(copyText("reason.link.lost"))
            val row =
                Wire.read<StageRow>(line)
                    ?: return Wire.read<DialAck>(line) ?: throw IOException(copyText("beServer.words.cantConnect"))
            onStage(row.stage)
        }
    }

    fun send(bytes: ByteArray) {
        if (!Core.send(handle, bytes)) throw IOException(copyText("reason.link.lost"))
    }

    fun resize(
        cols: Int,
        rows: Int,
    ) = Core.resize(handle, cols, rows)

    fun close() {
        if (!ended.compareAndSet(false, true)) return
        Core.close(handle)
        RustCore.drop(handle)
        synchronized(onClose) { onClose.toList() }.forEach { it() }
    }
}

/** PTY 上的交互 shell（`use: shell`）：给终端桥接的那一对阻塞流。 */
internal class RustShell(
    private val link: RustLink,
) : ShellChannel {
    private var pending = ByteArray(0)
    private var at = 0

    override val input: InputStream =
        object : InputStream() {
            override fun read(): Int {
                val one = ByteArray(1)
                return if (read(one, 0, 1) < 0) -1 else one[0].toInt() and BYTE_MASK
            }

            override fun read(
                b: ByteArray,
                off: Int,
                len: Int,
            ): Int {
                if (len == 0) return 0
                if (at >= pending.size) {
                    pending = runBlocking { link.readChunk() } ?: return -1
                    at = 0
                }
                val n = minOf(len, pending.size - at)
                System.arraycopy(pending, at, b, off, n)
                at += n
                return n
            }
        }

    override val output: OutputStream =
        object : OutputStream() {
            override fun write(b: Int) = write(byteArrayOf(b.toByte()), 0, 1)

            override fun write(
                b: ByteArray,
                off: Int,
                len: Int,
            ) {
                if (len > 0) link.send(b.copyOfRange(off, off + len))
            }
        }

    override fun resize(
        cols: Int,
        rows: Int,
    ) = link.resize(cols, rows)

    override fun close() = link.close()

    private companion object {
        const val BYTE_MASK = 0xff
    }
}
