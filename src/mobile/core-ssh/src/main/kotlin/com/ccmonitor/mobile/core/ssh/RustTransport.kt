package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.remote.ConnectionDeadException
import com.ccmonitor.mobile.core.remote.ExecResult
import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.rs.ResidentEvents
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
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
    private val preset: JSONObject? = null,
    /** [preset] 那份身份的钥匙编号（[again] 已替这一条多握了一份）。 */
    held: List<String> = emptyList(),
) : SshTransport {
    /** 拨号身份那一份请求（不带用法）：每条链路照它开，核心按它复用同一条连接。 */
    @Volatile private var base: JSONObject? = preset

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
        return RustTransport(JSONObject(b.toString()), ids)
    }

    override fun onDrop(f: () -> Unit) {
        synchronized(drops) { drops += f }
        // 登记之前就已经收工了的那一条：当场叫（叫两次无妨：管理那一侧按「还是不是装着的那条」认）。
        if (hold?.isOpen == false && !closed.get()) f()
    }

    override suspend fun connect(
        configs: List<ConnectionConfig>,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) = withContext(Dispatchers.IO) {
        require(configs.isNotEmpty()) { "no address to connect" }
        val first = configs.first()
        preset?.let { return@withContext holdOn(it, first, onEvent) }
        val req =
            JSONObject()
                .put("host", first.host)
                .put("port", first.port)
                .put("user", first.username)
                .put("key_path", keyFor(first.auth))
                .put("endpoints", JSONArray(configs.map { JSONObject().put("host", it.host).put("port", it.port) }))
        first.jumpVia?.let { req.put("jump", hopOf(it)) }
        base = JSONObject(req.toString())
        holdOn(req, first, onEvent)
    }

    /**
     * 照 [req] 握一条连接（`use: hold`）、读到 ack。期限在核心里（按进展算，共用那条重连循环同一处），这里不等表。
     * 握着的那条收工（对端断 · 保活判死 · 核心收场）⇒ [isConnected] 随之变假、[onDrop] 那几个各叫一次（自己 [close] 的不叫）。
     * 三处 throw 都是有意的：链路没给 ack · ack 说没成 · 期间被关了。
     */
    @Suppress("ThrowsCount")
    private suspend fun holdOn(
        req: JSONObject,
        first: ConnectionConfig,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) {
        val link = RustCore.open(JSONObject(req.toString()).put("use", "hold").put("stages", onEvent != null))
        val ack =
            try {
                link.awaitAck { onEvent?.invoke(eventOf(it, first)) }
            } catch (e: IOException) {
                link.close()
                throw HostUnreachedException(e.message ?: copyText("reason.link.lost"), e)
            }
        if (!ack.optBoolean("ok")) {
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

    override suspend fun execCapture(command: String): ExecResult =
        withContext(Dispatchers.IO) {
            val link = child(JSONObject().put("use", "capture").put("command", command).put("capture", JSONObject().put("max_bytes", CAPTURE_MAX)))
            try {
                val line = link.readLine() ?: throw IOException(copyText("reason.link.lost"))
                val got = JSONObject(line)
                ExecResult(
                    exitStatus = if (got.isNull("exit_status")) null else got.getInt("exit_status"),
                    stdout = got.optString("stdout"),
                    stderrTail = got.optString("stderr").takeLast(STDERR_TAIL),
                )
            } finally {
                link.close()
            }
        }

    override fun execStream(command: String): Flow<ByteArray> =
        callbackFlow {
            val link = child(JSONObject().put("use", "stream").put("command", command))
            launch {
                while (true) {
                    val chunk = link.readChunk() ?: break
                    send(chunk)
                }
                close()
            }
            awaitClose { link.close() }
        }.flowOn(Dispatchers.IO)

    override val dialRequest: String? get() = base?.toString()

    override suspend fun openShell(
        term: String,
        cols: Int,
        rows: Int,
    ): ShellChannel =
        withContext(Dispatchers.IO) {
            val pty = JSONObject().put("term", term).put("cols", cols).put("rows", rows)
            RustShell(child(JSONObject().put("use", "shell").put("pty", pty)))
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
    private suspend fun child(use: JSONObject): RustLink {
        val b = base
        if (b == null || !isConnected) throw ConnectionDeadException(copyText("reason.link.lost"))
        val req = JSONObject(b.toString())
        use.keys().forEach { k -> req.put(k, use.get(k)) }
        val link = RustCore.open(req)
        children += link
        link.onClose { children -= link }
        val ack = link.awaitAck {}
        if (!ack.optBoolean("ok")) {
            link.close()
            throw IOException(ack.optString("error"))
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
                    Core.putKeystore(auth.alias, uncompressedPoint(pub)) ?: error(copyText("mobSsh.keystore.noCert"))
                }
            } ?: throw IOException(copyText("reason.link.lost"))
        val kept = synchronized(keys) { !closed.get() && keys.add(id) }
        if (!kept) {
            Core.holdKey(id, false)
            throw IOException(copyText("reason.link.lost"))
        }
        return id
    }

    private suspend fun hopOf(cfg: ConnectionConfig): JSONObject {
        val hop =
            JSONObject()
                .put("host", cfg.host)
                .put("port", cfg.port)
                .put("user", cfg.username)
                .put("key_path", keyFor(cfg.auth))
                .put("label", "${cfg.host}:${cfg.port}")
        cfg.jumpVia?.let { hop.put("jump", hopOf(it)) }
        return hop
    }

    /** ack 带着库里那一把的指纹 ⇒ 钥匙对不上被拒（核心判的、指纹也是核心写的）；库读不出（含旧表没迁完）只是 `host_key` 那一句。 */
    private fun failureOf(
        ack: JSONObject,
        cfg: ConnectionConfig,
    ): Exception =
        if (ack.has("expected_fingerprint")) {
            HostKeyChangedException(
                host = cfg.host,
                port = cfg.port,
                keyType = ack.optString("key_type"),
                expectedFingerprint = ack.optString("expected_fingerprint"),
                offeredFingerprint = ack.optString("fingerprint"),
            )
        } else if (ack.optString("reason") in UNREACHED) {
            HostUnreachedException(ack.optString("error").ifEmpty { copyText("mobSsh.log.connectFailed") })
        } else {
            IOException(ack.optString("error").ifEmpty { copyText("mobSsh.log.connectFailed") })
        }

    private fun eventOf(
        stage: JSONObject,
        cfg: ConnectionConfig,
    ): SshConnectEvent {
        val ep = stage.optString("endpoint")
        val host = ep.substringBeforeLast(':', cfg.host)
        val port = ep.substringAfterLast(':', "").toIntOrNull() ?: cfg.port
        val (kind, message) =
            when (stage.optString("kind")) {
                "dialing" -> SshConnectEvent.Stage.TCP to copyText("mobSsh.log.connecting", "target" to ep)
                "hostKey" -> SshConnectEvent.Stage.HOSTKEY to stage.optString("fingerprint")
                "won" -> SshConnectEvent.Stage.NEGOTIATE to copyText("mobSsh.log.negotiated")
                "auth" ->
                    if (stage.optBoolean("ok")) {
                        SshConnectEvent.Stage.AUTH to copyText("mobSsh.log.authKey")
                    } else {
                        SshConnectEvent.Stage.ERROR to stage.optString("detail").ifEmpty { copyText("mobSsh.log.authFailed") }
                    }
                "established" -> SshConnectEvent.Stage.ESTABLISHED to copyText("mobSsh.log.established", "target" to "${cfg.host}:${cfg.port}")
                else -> SshConnectEvent.Stage.ERROR to stage.optString("reason")
            }
        return SshConnectEvent(host, port, kind, message)
    }

    internal companion object {
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val CAPTURE_MAX = 16 * 1024 * 1024
        const val STDERR_TAIL = 4096

        /** 拨号核心 ack 里「没够着那一台」的那几个原因码（`ssh-dial` 的 `why`：解析 · 拨不通 · 一步都没动到期限 · 跳板那一台断了）。 */
        val UNREACHED = setOf("resolve", "unreachable", "timeout", "jump")

        /** P-256 公钥 ⇒ 未压缩点 `04 ‖ X ‖ Y`（各 32 字节，左补零）。 */
        fun uncompressedPoint(pub: ECPublicKey): ByteArray {
            fun fixed(n: java.math.BigInteger): ByteArray {
                val raw = n.toByteArray().dropWhile { it == 0.toByte() }.toByteArray()
                return ByteArray(P256_COORD - raw.size) + raw
            }
            return byteArrayOf(4) + fixed(pub.w.affineX) + fixed(pub.w.affineY)
        }

        private const val P256_COORD = 32
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

    fun open(req: JSONObject): RustLink {
        ensure()
        val handle = Core.open(req.toString())
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

    /** 读到 ack 那一行（之前的阶段行交给 [onStage]）；没等到就收工 ⇒ 抛。 */
    suspend fun awaitAck(onStage: (JSONObject) -> Unit): JSONObject {
        while (true) {
            val line = readLine() ?: throw IOException(copyText("reason.link.lost"))
            val v = JSONObject(line)
            val stage = v.optJSONObject("stage")
            if (stage != null) onStage(stage) else return v
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
