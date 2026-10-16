package com.ccmonitor.mobile.core.ssh

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.ccmonitor.mobile.core.data.crypto.SshKeyGenerator
import com.ccmonitor.mobile.core.rs.Core
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyStore
import java.util.Base64

/**
 * 端到端：模拟器上的 [RustTransport]（手机嵌的 Rust 核心）连一台**私起的 sshd**（冒烟台架 `tests/mobile-smoke/rig.sh`，
 * 只听宿主机回环的私有口，模拟器经它的网关够到它），走一遍连上 · exec · 长 exec · PTY 改窗口 · TOFU · Keystore 签名。
 *
 * 参数（instrumentation，`-Pandroid.testInstrumentationRunnerArguments.<名>=…`）：`sshHost`（模拟器那一侧宿主机的网关地址）· `sshPort` ·
 * `sshKey`（台架登录钥匙的 PEM，base64）。没给 `sshKey` / `sshHost` ⇒ 整份跳过（入口脚本 `scripts/android-test.sh` 不起台架时就是这样）。
 */
@RunWith(AndroidJUnit4::class)
class RustTransportTest {
    private val args = InstrumentationRegistry.getArguments()
    private val host = args.getString("sshHost").orEmpty()
    private val port = args.getString("sshPort")?.toIntOrNull() ?: 22422
    private val pem: ByteArray? = args.getString("sshKey")?.let { Base64.getDecoder().decode(it) }
    private val open = mutableListOf<RustTransport>()

    /** 主机钥匙那份库：测试 APK 自己的私有目录（核心一个进程只装一次，路径以第一次为准）。 */
    private val book = File(InstrumentationRegistry.getInstrumentation().targetContext.filesDir, "known_hosts")

    @Before fun setup() = installRustCore(book)

    @After fun teardown() = open.forEach { it.close() }

    private fun config(auth: AuthMethod = AuthMethod.PrivateKey(requireNotNull(pem))) =
        ConnectionConfig(host = host, port = port, username = user(), auth = auth)

    /** 台架只认跑它的那个用户（宿主机上的用户名由入口交进来；缺 ⇒ 读 `sshUser`）。 */
    private fun user(): String = requireNotNull(args.getString("sshUser")) { "缺 sshUser" }

    private fun connected(auth: AuthMethod? = null): RustTransport {
        assumeTrue("没给台架钥匙与地址（sshKey · sshHost）", pem != null && host.isNotEmpty())
        val t = RustTransport().also { open += it }
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
    }

    /**
     * 流式 exec 与常驻流交给核心的那一份拨号请求：连上过就有（关了也还在），照它开的链路复用这一条连接；同一份身份上再握的那条请求一字不差。
     * 钥匙跟着身份走：这一条与再握的那条都关了，核心里那把就没了（再握一份 ⇒ 编号不在）。
     */
    @Test
    fun a_stream_rides_the_same_connection_and_the_keys_go_with_the_last_of_its_identity() {
        val t = connected()
        val out = runBlocking { withTimeout(WALL_CEILING_MS) { t.execStream("printf 'a\\nb\\n'").toList() } }
        assertEquals("a\nb\n", out.joinToString("") { it.toString(Charsets.UTF_8) })
        val req = org.json.JSONObject(t.dialRequest ?: error("连着却没有拨号请求"))
        assertTrue(req.toString(), req.has("host") && !req.has("use"))
        val next = t.again() ?: error("同一份身份上再握不了")
        assertEquals(req.toString(), next.dialRequest)
        t.close()
        assertEquals(req.toString(), t.dialRequest)
        next.close()
        assertFalse("身份全关了，钥匙还在核心里", Core.holdKey(req.getString("key_path"), true))
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

    /** 首见记进库文件（鉴权过了才记）；之后按记下的那一把严格比；那一把被换了 ⇒ 拒并带库里那一把的指纹；忘掉之后又是首见。 */
    @Test
    fun first_use_is_recorded_then_enforced_and_forgetting_starts_over() {
        assumeTrue("没给台架钥匙与地址（sshKey · sshHost）", pem != null && host.isNotEmpty())
        assertTrue(forgetHostKey(host, port))
        connected()
        val line = book.readLines().single { it.isNotBlank() }
        connected()
        val (names, alg, b64) = line.split(' ')
        book.writeText("$names $alg ${flipped(b64)}\n")
        val e = runCatching { connected() }.exceptionOrNull()
        assertTrue("$e", e is HostKeyChangedException && e.expectedFingerprint.startsWith("SHA256:"))
        assertTrue(forgetHostKey(host, port))
        connected()
        assertEquals(line, book.readLines().single { it.isNotBlank() })
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

    private companion object {
        const val WALL_CEILING_MS = 300_000L
    }
}
