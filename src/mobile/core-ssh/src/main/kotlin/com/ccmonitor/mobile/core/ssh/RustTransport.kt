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
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.security.KeyStore
import java.security.MessageDigest
import java.security.PrivateKey
import java.security.interfaces.ECPublicKey
import java.util.Base64
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean

/**
 * [SshTransport] 走手机嵌的 Rust 核心（`:core-rs`，拨号核心 `ssh-dial` 与后端同一份）。
 *
 * - **连上**＝开一条握着连接的链路（`use: hold`）：竞速 · 跳板（多跳）· host key · 鉴权 · 保活都在核心里；之后每一次 exec /
 *   shell / SFTP 各开一条链路，按同一份拨号身份复用那条连接（核心的连接池）。握着的那条收工（对端断 · 保活判死）⇒ [isConnected] 变假。
 * - host key：这台记过的全部钥匙交进请求，照 OpenSSH 比（记过的类型协商排前、按协商出的那一种严格比、那一种没记过 ⇒ 拒）；
 *   一把都没记过 ⇒ 首见接受，**鉴权过了**才落 [KnownHostStore]。
 * - 钥匙按编号交进核心（密码 · PEM · Keystore 别名＋公钥）；Keystore 的签名在这里做（回 DER，核心换成 SSH 的格式）。
 */
class RustTransport(
    private val knownHostStore: KnownHostStore? = null,
    /** 照这一份拨号身份再握一条（[again]）；`null` ⇒ [connect] 照配置组一份新的。 */
    private val preset: JSONObject? = null,
) : SshTransport {
    /** 拨号身份那一份请求（不带用法）：每条链路照它开，核心按它复用同一条连接。 */
    @Volatile private var base: JSONObject? = preset

    @Volatile private var hold: RustLink? = null
    private val children: MutableSet<RustLink> = ConcurrentHashMap.newKeySet()
    private val filesLock = Mutex()
    private var files: RustLink? = null
    private val closed = AtomicBoolean(false)
    private val watcher = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private val drops = mutableListOf<() -> Unit>()

    override val isConnected: Boolean get() = !closed.get() && hold?.isOpen == true

    override fun again(): SshTransport? = base?.let { RustTransport(knownHostStore, JSONObject(it.toString())) }

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
        preset?.let { return@withContext holdOn(it, first, emptyList(), emptyList(), onEvent) }
        val known = knownOf(first.host, first.port)
        val req =
            JSONObject()
                .put("host", first.host)
                .put("port", first.port)
                .put("user", first.username)
                .put("key_path", keyFor(first.auth))
                .put("endpoints", JSONArray(configs.map { JSONObject().put("host", it.host).put("port", it.port) }))
        if (known.isNotEmpty()) req.put("known_keys", JSONArray(known.map { "${it.keyType} ${it.publicKeyBase64}" }))
        first.jumpVia?.let { req.put("jump", hopOf(it)) }
        base = JSONObject(req.toString())
        val chain = generateSequence(first) { it.jumpVia }.map { it.host to it.port }.toList()
        chain.forEach { RustCore.forgetSeen(it.first, it.second) }
        holdOn(req, first, known, chain, onEvent)
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
        known: List<KnownHostRecord>,
        chain: List<Pair<String, Int>>,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) {
        val link = RustCore.open(JSONObject(req.toString()).put("use", "hold").put("stages", onEvent != null))
        val ack =
            try {
                link.awaitAck { onEvent?.invoke(eventOf(it, first)) }
            } catch (e: IOException) {
                link.close()
                throw HostUnreachedException(e.message ?: copyText("mobSsh.conn.unavailable"), e)
            }
        if (!ack.optBoolean("ok")) {
            link.close()
            throw failureOf(ack, first, known)
        }
        recordFirstUse(chain)
        if (closed.get()) {
            link.close()
            throw IOException(copyText("mobSsh.conn.closedNew"))
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
                val line = link.readLine() ?: throw IOException(copyText("mobSsh.conn.unavailable"))
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

    override suspend fun sftpList(path: String): List<SftpEntry> {
        val dir = sftpRealPath(path)
        val got = ask(JSONObject().put("op", "list").put("path", dir))
        val rows = got.optJSONArray("entries") ?: JSONArray()
        return (0 until rows.length()).map { i ->
            val e = rows.getJSONObject(i)
            val name = e.getString("name")
            SftpEntry(name, joinPath(dir, name), e.optBoolean("dir"), e.optLong("size"))
        }
    }

    override suspend fun sftpRealPath(path: String): String = ask(JSONObject().put("op", "realpath").put("path", path)).getString("path")

    override suspend fun sftpDownload(
        remotePath: String,
        local: File,
    ) {
        val bytes = readRemote(remotePath, MAX_FILE_BYTES) ?: throw IOException(remotePath)
        withContext(Dispatchers.IO) { local.writeBytes(bytes) }
    }

    override suspend fun sftpUpload(
        local: File,
        remotePath: String,
    ) = put(remotePath, withContext(Dispatchers.IO) { local.readBytes() })

    override suspend fun sftpRename(
        from: String,
        to: String,
    ) {
        ask(JSONObject().put("op", "rename").put("from", from).put("to", to))
    }

    override suspend fun sftpDelete(
        path: String,
        isDir: Boolean,
    ) {
        ask(JSONObject().put("op", if (isDir) "rmdir" else "remove").put("path", path))
    }

    override suspend fun sftpMkdir(path: String) {
        ask(JSONObject().put("op", "mkdirs").put("path", path))
    }

    override suspend fun sftpReadText(
        path: String,
        maxBytes: Int,
    ): String {
        val bytes = readRemote(path, MAX_FILE_BYTES) ?: return ""
        if (bytes.size <= maxBytes) return bytes.toString(Charsets.UTF_8)
        return bytes.copyOf(maxBytes).toString(Charsets.UTF_8) + copyText("mobSsh.read.truncated", "n" to bytes.size.toString())
    }

    override suspend fun sftpReadTextForEdit(
        path: String,
        maxBytes: Int,
    ): String? {
        val bytes = readRemote(path, maxBytes.toLong()) ?: return null
        if (bytes.any { it == 0.toByte() }) return null
        return bytes.toString(Charsets.UTF_8)
    }

    override suspend fun sftpWriteText(
        remotePath: String,
        text: String,
    ) = put(remotePath, text.toByteArray(Charsets.UTF_8))

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        files?.close()
        children.forEach { it.close() }
        children.clear()
        hold?.close()
        watcher.cancel()
    }

    // ── 链路 ─────────────────────────────────────────────────────────────────────

    /** 照拨号身份开一条用法链路、读到 ack；ack 不成 ⇒ 抛（连接没了 ⇒ [ConnectionDeadException]）。 */
    private suspend fun child(use: JSONObject): RustLink {
        val b = base
        if (b == null || !isConnected) throw ConnectionDeadException(copyText("mobSsh.conn.unavailable"))
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

    /** files 链路上的一问一答（链路惰性开、一问接一问）。应答带 `code` ⇒ 抛那一句。 */
    private suspend fun ask(
        req: JSONObject,
        bytes: ByteArray? = null,
    ): JSONObject =
        withContext(Dispatchers.IO) {
            filesLock.withLock {
                val link = files?.takeIf { it.isOpen } ?: child(JSONObject().put("use", "files")).also { files = it }
                link.send((req.toString() + "\n").toByteArray(Charsets.UTF_8))
                bytes?.let { link.send(it) }
                val line = link.readLine() ?: throw IOException(copyText("mobSsh.conn.unavailable"))
                val got = JSONObject(line)
                if (got.has("code")) throw IOException(got.optString("message"))
                got
            }
        }

    /** 整份读回来；比 [max] 大 / 不在 ⇒ `null`。 */
    private suspend fun readRemote(
        path: String,
        max: Long,
    ): ByteArray? {
        val got =
            try {
                ask(JSONObject().put("op", "read").put("path", path).put("max", max))
            } catch (e: IOException) {
                return null.also { if (!isConnected) throw e }
            }
        if (got.isNull("data")) return null
        return Base64.getDecoder().decode(got.getString("data"))
    }

    private suspend fun put(
        path: String,
        bytes: ByteArray,
    ) {
        ask(
            JSONObject()
                .put("op", "put")
                .put("path", path)
                .put("size", bytes.size)
                .put("mode", FILE_MODE),
            bytes,
        )
    }

    // ── 钥匙 · host key ─────────────────────────────────────────────────────────

    /** 把一样鉴权交进核心，回它的编号（拨号请求的 `key_path`）。 */
    private fun keyFor(auth: AuthMethod): String {
        RustCore.ensure()
        val id = "k-" + UUID.randomUUID()
        when (auth) {
            is AuthMethod.Password -> Core.putPassword(id, auth.password)
            is AuthMethod.PrivateKey -> Core.putPem(id, auth.pem, auth.passphrase?.concatToString())
            is AuthMethod.KeystoreSigner -> {
                val ks = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
                val pub =
                    ks.getCertificate(auth.alias)?.publicKey as? ECPublicKey
                        ?: error(copyText("mobSsh.keystore.noCert", "alias" to auth.alias))
                check(Core.putKeystore(id, auth.alias, uncompressedPoint(pub))) { copyText("mobSsh.keystore.noCert", "alias" to auth.alias) }
            }
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
        val known = knownOf(cfg.host, cfg.port)
        if (known.isNotEmpty()) hop.put("known_keys", JSONArray(known.map { "${it.keyType} ${it.publicKeyBase64}" }))
        cfg.jumpVia?.let { hop.put("jump", hopOf(it)) }
        return hop
    }

    /**
     * 这台记过的全部钥匙（可多种类型）。交给核心照 OpenSSH 的做法比：协商时这几种类型排前，按协商出的那一种严格比；
     * 那一种没记过（而别的记过）⇒ 拒，同「钥匙变了」；一把都没记过 ⇒ 首见。
     */
    private suspend fun knownOf(
        host: String,
        port: Int,
    ): List<KnownHostRecord> = knownHostStore?.forHost(host, port).orEmpty()

    /** 鉴权过了：这一趟首见的那几台落 [KnownHostStore]（已经记过的不动）。 */
    private suspend fun recordFirstUse(chain: List<Pair<String, Int>>) {
        val store = knownHostStore ?: return
        for ((host, port) in chain) {
            val parts = RustCore.takeSeen(host, port)?.split(' ').orEmpty()
            if (parts.size >= 2 && store.forHost(host, port).isEmpty()) {
                store.record(KnownHostRecord(host, port, parts[0], parts[1], System.currentTimeMillis()))
            }
        }
    }

    private fun failureOf(
        ack: JSONObject,
        cfg: ConnectionConfig,
        known: List<KnownHostRecord>,
    ): Exception =
        if (ack.optString("reason") == "host_key" && known.isNotEmpty()) {
            val type = ack.optString("key_type")
            HostKeyChangedException(
                host = cfg.host,
                port = cfg.port,
                keyType = type,
                // 同类型记过的那一把（没有 ⇒ 这一种从没记过：空串）。
                expectedFingerprint = known.firstOrNull { it.keyType == type }?.let { fingerprintOf(it.publicKeyBase64) }.orEmpty(),
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
        const val MAX_FILE_BYTES = 64L shl 20
        const val FILE_MODE = 0x1A4 // 0644

        /** 拨号核心 ack 里「没够着那一台」的那几个原因码（`ssh-dial` 的 `why`：解析 · 拨不通 · 一步都没动到期限 · 跳板那一台断了）。 */
        val UNREACHED = setOf("resolve", "unreachable", "timeout", "jump")

        /** SSH 公钥 blob（base64）⇒ `SHA256:<无填充 base64>`（与 OpenSSH / 核心报的同形）。 */
        fun fingerprintOf(blobB64: String): String {
            val digest = MessageDigest.getInstance("SHA-256").digest(Base64.getDecoder().decode(blobB64))
            return "SHA256:" + Base64.getEncoder().withoutPadding().encodeToString(digest)
        }

        /** P-256 公钥 ⇒ 未压缩点 `04 ‖ X ‖ Y`（各 32 字节，左补零）。 */
        fun uncompressedPoint(pub: ECPublicKey): ByteArray {
            fun fixed(n: java.math.BigInteger): ByteArray {
                val raw = n.toByteArray().dropWhile { it == 0.toByte() }.toByteArray()
                return ByteArray(P256_COORD - raw.size) + raw
            }
            return byteArrayOf(4) + fixed(pub.w.affineX) + fixed(pub.w.affineY)
        }

        fun joinPath(
            dir: String,
            name: String,
        ): String = if (dir.endsWith('/')) dir + name else "$dir/$name"

        private const val P256_COORD = 32
    }
}

/** 核心装上（记下 Kotlin 这一侧的回调、装上手机那一套）；只认第一次。开常驻流之前要它（常驻流交的事经同一个回调回来）。 */
fun ensureRustCore() = RustCore.ensure()

/** 进程里那一份 Rust 核心的 Kotlin 一侧：链路编号 → 收件箱 · Keystore 签名 · 认下的 host key · 常驻流交的事。 */
internal object RustCore : Core.Sink {
    private val inboxes = ConcurrentHashMap<Long, Channel<ByteArray>>()
    private val seen = ConcurrentHashMap<String, String>()

    @Volatile private var ready = false

    fun ensure() {
        if (ready) return
        synchronized(this) {
            if (!ready) {
                Core.init(this)
                ready = true
            }
        }
    }

    fun open(req: JSONObject): RustLink {
        ensure()
        val handle = Core.open(req.toString())
        if (handle == 0L) throw IOException(copyText("mobSsh.conn.unavailable"))
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

    override fun hostKey(
        host: String,
        port: Int,
        key: String,
    ) {
        seen["$host:$port"] = key
    }

    fun forgetSeen(
        host: String,
        port: Int,
    ) {
        seen.remove("$host:$port")
    }

    fun takeSeen(
        host: String,
        port: Int,
    ): String? = seen.remove("$host:$port")

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
            val line = readLine() ?: throw IOException(copyText("mobSsh.conn.unavailable"))
            val v = JSONObject(line)
            val stage = v.optJSONObject("stage")
            if (stage != null) onStage(stage) else return v
        }
    }

    fun send(bytes: ByteArray) {
        if (!Core.send(handle, bytes)) throw IOException(copyText("mobSsh.conn.unavailable"))
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
