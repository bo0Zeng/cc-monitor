package com.ccmonitor.mobile.core.ssh

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.ccmonitor.mobile.core.data.crypto.SshKeyGenerator
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyStore
import java.util.Base64

/**
 * 端到端：模拟器上的 [RustTransport]（手机嵌的 Rust 核心）连一台**私起的 sshd**（冒烟台架 `tests/mobile-smoke/rig.sh`，
 * 只听宿主机回环的私有口，模拟器经网关 `10.0.2.2` 够到它），走一遍连上 · exec · 长 exec · PTY 改窗口 · SFTP · TOFU · Keystore 签名。
 *
 * 参数（instrumentation，`-Pandroid.testInstrumentationRunnerArguments.<名>=…`）：`sshHost`（缺 = 10.0.2.2）· `sshPort` ·
 * `sshKey`（台架登录钥匙的 PEM，base64）。没给 `sshKey` ⇒ 整份跳过（入口脚本 `scripts/android-test.sh` 不起台架时就是这样）。
 */
@RunWith(AndroidJUnit4::class)
class RustTransportTest {
    private val args = InstrumentationRegistry.getArguments()
    private val host = args.getString("sshHost") ?: "10.0.2.2"
    private val port = args.getString("sshPort")?.toIntOrNull() ?: 22422
    private val pem: ByteArray? = args.getString("sshKey")?.let { Base64.getDecoder().decode(it) }
    private val open = mutableListOf<RustTransport>()

    @After fun teardown() = open.forEach { it.close() }

    private fun config(auth: AuthMethod = AuthMethod.PrivateKey(requireNotNull(pem))) =
        ConnectionConfig(host = host, port = port, username = user(), auth = auth)

    /** 台架只认跑它的那个用户（宿主机上的用户名由入口交进来；缺 ⇒ 读 `sshUser`）。 */
    private fun user(): String = requireNotNull(args.getString("sshUser")) { "缺 sshUser" }

    private fun connected(
        store: KnownHostStore? = null,
        auth: AuthMethod? = null,
    ): RustTransport {
        assumeTrue("没给台架钥匙（sshKey）", pem != null)
        val t = RustTransport(store).also { open += it }
        runBlocking { withTimeout(WALL_CEILING_MS) { t.connect(if (auth == null) config() else config(auth)) } }
        return t
    }

    @Test
    fun connects_and_runs_a_command_with_its_exit_status() {
        val t = connected()
        assertTrue(t.isConnected)
        val r = runBlocking { t.execCapture("echo hi; echo oops 1>&2; exit 3") }
        assertEquals("hi\n", r.stdout)
        assertEquals(3, r.exitStatus)
        assertTrue(r.stderrTail, r.stderrTail.contains("oops"))
        assertTrue(runBlocking { t.probeAlive(WALL_CEILING_MS) })
    }

    @Test
    fun a_stream_and_a_duplex_ride_the_same_connection() {
        val t = connected()
        val out = runBlocking { withTimeout(WALL_CEILING_MS) { t.execStream("printf 'a\\nb\\n'").toList() } }
        assertEquals("a\nb\n", out.joinToString("") { it.toString(Charsets.UTF_8) })
        val d = runBlocking { t.execDuplex("head -n 1") }
        runBlocking { d.write("ping\n".toByteArray()) }
        val got = runBlocking { withTimeout(WALL_CEILING_MS) { d.stdout.toList() } }
        assertEquals("ping\n", got.joinToString("") { it.toString(Charsets.UTF_8) })
        d.close()
    }

    @Test
    fun a_shell_gets_its_pty_and_follows_a_resize() {
        val t = connected()
        val sh = runBlocking { t.openShell("xterm-256color", 100, 30) }
        sh.resize(120, 40)
        sh.output.write("stty size; exit\n".toByteArray())
        sh.output.flush()
        val text = sh.input.readBytes().toString(Charsets.UTF_8)
        assertTrue(text, text.contains("40 120"))
        sh.close()
    }

    @Test
    fun files_round_trip_through_sftp() {
        val t = connected()
        val home = runBlocking { t.sftpRealPath(".") }
        val dir = "$home/work/rust-sftp-${System.nanoTime()}"
        runBlocking {
            t.sftpMkdir(dir)
            t.sftpWriteText("$dir/a.txt", "你好\n")
            assertEquals("你好\n", t.sftpReadTextForEdit("$dir/a.txt"))
            val local = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "up.bin")
            local.writeBytes(ByteArray(70_000) { (it % 251).toByte() })
            t.sftpUpload(local, "$dir/b.bin")
            val back = File(local.parentFile, "down.bin")
            t.sftpDownload("$dir/b.bin", back)
            assertTrue(back.readBytes().contentEquals(local.readBytes()))
            t.sftpRename("$dir/a.txt", "$dir/c.txt")
            val names = t.sftpList(dir).map { it.name to it.isDir }.toSet()
            assertEquals(setOf("c.txt" to false, "b.bin" to false), names)
            t.sftpDelete("$dir/c.txt", isDir = false)
            t.sftpDelete("$dir/b.bin", isDir = false)
            t.sftpDelete(dir, isDir = true)
        }
    }

    /** 首见记下（鉴权过了才落库）；之后按记下的那一把严格比；那一把被换了 ⇒ 拒并带新旧指纹。 */
    @Test
    fun first_use_is_recorded_then_enforced() {
        val store = MemoryStore()
        connected(store)
        val rec = runBlocking { store.forHost(host, port) }.single()
        connected(store)
        runBlocking {
            store.forget(host, port)
            store.record(rec.copy(publicKeyBase64 = flipped(rec.publicKeyBase64)))
        }
        val e = runCatching { connected(store) }.exceptionOrNull()
        assertTrue("$e", e is HostKeyChangedException)
    }

    /** Keystore 里不可导出的 P-256：先用台架钥匙把它的公钥放进台架的 authorized_keys，再只凭 Keystore 签名登上。 */
    @Test
    fun a_keystore_key_logs_in_through_the_core() {
        val alias = "ccm-rust-ks-${System.nanoTime()}"
        try {
            val key = SshKeyGenerator.generateEcdsaP256InKeystore(alias, comment = "rust-ks")
            val admin = connected()
            val add = runBlocking { admin.execCapture("printf '%s\\n' '${key.publicKeyOpenSsh}' >> \"\$CCM_SMOKE_BOX/sshd/authorized_keys\"") }
            assertEquals(0, add.exitStatus)
            val t = connected(auth = AuthMethod.KeystoreSigner(alias))
            assertEquals("ks\n", runBlocking { t.execCapture("echo ks") }.stdout)
        } finally {
            runCatching { KeyStore.getInstance("AndroidKeyStore").apply { load(null) }.deleteEntry(alias) }
        }
    }

    private fun flipped(b64: String): String {
        val raw = Base64.getDecoder().decode(b64)
        raw[raw.size - 1] = (raw[raw.size - 1].toInt() xor 1).toByte()
        return Base64.getEncoder().encodeToString(raw)
    }

    private class MemoryStore : KnownHostStore {
        private val rows = mutableListOf<KnownHostRecord>()

        override suspend fun forHost(
            host: String,
            port: Int,
        ) = synchronized(rows) { rows.filter { it.host == host && it.port == port } }

        override suspend fun record(knownHost: KnownHostRecord) {
            synchronized(rows) { rows += knownHost }
        }

        override suspend fun forget(
            host: String,
            port: Int,
        ) {
            synchronized(rows) { rows.removeAll { it.host == host && it.port == port } }
        }
    }

    private companion object {
        const val WALL_CEILING_MS = 300_000L
    }
}
