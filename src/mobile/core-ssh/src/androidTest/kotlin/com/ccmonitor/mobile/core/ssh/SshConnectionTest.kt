package com.ccmonitor.mobile.core.ssh

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.InputStream

/**
 * 端到端：模拟器连真实 SSH 服务器（[host]），验证 sshj + BouncyCastle 在 Android 上能握手、分配 PTY、
 * resize 送达 SIGWINCH、走 SFTP。
 *
 * 认证用 androidTest/assets/test_key（本地放置，不提交）。服务器不可达时本测试失败。
 * 服务器地址用 instrumentation 参数 `sshHost` 给（`-Pandroid.testInstrumentationRunnerArguments.sshHost=<地址>`）；
 * 没给时落到文档段占位地址，必然连不上。
 */
@RunWith(AndroidJUnit4::class)
class SshConnectionTest {
    private val host = InstrumentationRegistry.getArguments().getString("sshHost") ?: "192.0.2.2"
    private val port = 22
    private val user = "pi"
    private val conn = SshConnection()

    @After fun teardown() = conn.close()

    private fun loadKey(): ByteArray =
        InstrumentationRegistry
            .getInstrumentation()
            .context.assets
            .open("test_key")
            .use(InputStream::readBytes)

    private fun config() =
        ConnectionConfig(
            host = host,
            port = port,
            username = user,
            auth = AuthMethod.PrivateKey(loadKey()),
        )

    private fun connect() = runBlocking { conn.connect(config(), null) }

    /** 内存版 [KnownHostStore]。 */
    private class FakeKnownHostStore : KnownHostStore {
        private val store = mutableListOf<KnownHostRecord>()

        override suspend fun forHost(
            host: String,
            port: Int,
        ): List<KnownHostRecord> =
            store.filter { it.host == host && it.port == port }

        override suspend fun record(knownHost: KnownHostRecord) {
            store.removeAll { it.host == knownHost.host && it.port == knownHost.port && it.keyType == knownHost.keyType }
            store.add(knownHost)
        }

        override suspend fun forget(
            host: String,
            port: Int,
        ) {
            store.removeAll { it.host == host && it.port == port }
        }
    }

    /** 读 shell 输出累积 millis 毫秒，返回文本。 */
    private fun readFor(
        input: InputStream,
        millis: Long,
    ): String {
        val sb = StringBuilder()
        val buf = ByteArray(4096)
        val deadline = System.currentTimeMillis() + millis
        while (System.currentTimeMillis() < deadline) {
            if (input.available() > 0) {
                val n = input.read(buf)
                if (n > 0) sb.append(String(buf, 0, n, Charsets.UTF_8))
            } else {
                Thread.sleep(50)
            }
        }
        return sb.toString()
    }

    @Test
    fun connectsAndAuthenticates() {
        connect()
        assertTrue("sshj 应已连接并认证", conn.isConnected)
    }

    @Test
    fun sftpListsHomeNonEmpty() =
        runBlocking {
            connect()
            val names = conn.sftpList(".")
            assertTrue("SFTP ls home 应非空: $names", names.isNotEmpty())
        }

    /** 多地址竞速：不可达地址列在前，仍应很快连上可达地址，不等不可达地址的超时。 */
    @Test
    fun raceConnectPrefersReachableNotWaitingDeadAddress() {
        runBlocking {
            val mgr = SshConnectionManager() // 无 store 时不校验 host key，足够测竞速
            val key = loadKey()
            val dead = ConnectionConfig("203.0.113.1", 22, user, AuthMethod.PrivateKey(key), connectTimeoutMs = 12_000)
            val good = ConnectionConfig(host, port, user, AuthMethod.PrivateKey(key))
            try {
                val t0 = System.currentTimeMillis()
                val conn = mgr.connect("race", "h", "L", listOf(dead, good)) // 不可达列在前
                val dt = System.currentTimeMillis() - t0
                assertTrue("应连上可达地址", conn.isConnected)
                assertTrue("应秒连不等死地址超时(dt=$dt ms, 远小于 12s)", dt < 9_000)
            } finally {
                mgr.disconnect("race")
            }
        }
    }

    /** 上传临时文件到 /tmp、列出、再下载回来，字节一致；用 /tmp 不碰真实用户文件，跑完清理。 */
    @Test
    fun sftpUploadListDownloadRoundTrip() {
        runBlocking {
            connect()
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            val payload = "roundtrip-${System.nanoTime()}".toByteArray()
            val remote = "/tmp/aterm_roundtrip_test.txt"
            val up = File.createTempFile("aterm_up", ".txt", ctx.cacheDir).apply { writeBytes(payload) }
            try {
                conn.sftpUpload(up, remote)
                val listed = conn.sftpList("/tmp")
                assertTrue("上传后 /tmp 应能列到该文件", listed.any { it.name == "aterm_roundtrip_test.txt" && !it.isDir })
                val down = File.createTempFile("aterm_down", ".txt", ctx.cacheDir)
                conn.sftpDownload(remote, down)
                assertTrue("下载字节应与上传一致", down.readBytes().contentEquals(payload))
                down.delete()
            } finally {
                execText("rm -f $remote")
                up.delete()
            }
        }
    }

    @Test
    fun resizeTriggersSigwinchOnRemote() =
        runBlocking {
            connect()
            conn.openShell(term = "xterm-256color", cols = 80, rows = 24).use { shell ->
                Thread.sleep(600) // 等 shell prompt 稳定
                shell.output.write("echo MARK1=$(tput cols)\n".toByteArray())
                shell.output.flush()
                val out1 = readFor(shell.input, 2500)
                assertTrue("初始 PTY 80 列，tput cols 应为 80。实际输出:\n$out1", out1.contains("MARK1=80"))

                shell.resize(cols = 132, rows = 40) // → 远端 SIGWINCH
                Thread.sleep(400)
                shell.output.write("echo MARK2=$(tput cols)\n".toByteArray())
                shell.output.flush()
                val out2 = readFor(shell.input, 2500)
                assertTrue("resize 后 tput cols 应为 132（SIGWINCH 已达远端）。实际输出:\n$out2", out2.contains("MARK2=132"))
            }
        }

    private val jsonlDir = "/home/pi/.claude/projects/-home-pi-project-android-terminal"

    // 用 execStream 收成 stdout 文本的助手。
    private suspend fun execText(cmd: String): String =
        conn.execStream(cmd).toList().joinToString("") { String(it, Charsets.UTF_8) }

    private fun latestJsonl(): String =
        runBlocking { execText("ls -t $jsonlDir/*.jsonl 2>/dev/null | head -1") }.trim()

    /** execStream 流式读真实 jsonl（cat，无 -F，自然结束），应收到含 JSON 的多行内容。 */
    @Test
    fun execStreamReadsRealJsonl() =
        runBlocking {
            connect()
            val path = latestJsonl()
            assertTrue("应找到真实 jsonl 路径: '$path'", path.endsWith(".jsonl"))
            val bytes = conn.execStream("cat '$path'").toList().fold(ByteArray(0)) { a, b -> a + b }
            val text = bytes.toString(Charsets.UTF_8)
            assertTrue("execStream 应收到文件内容（含 \"type\" 字段）", text.contains("\"type\""))
            assertTrue("应有多行 jsonl", text.lines().size > 3)
        }

    /** tail -F 是无限流；take(1) 拿到首块后取消，awaitClose 关 session 杀掉远端 tail，能返回即证明可取消。 */
    @Test
    fun execStreamTailFollowIsCancellable() =
        runBlocking {
            connect()
            val path = latestJsonl()
            val first =
                withTimeout(10_000) {
                    conn.execStream("tail -n +1 -F '$path'").take(1).toList()
                }
            assertTrue("tail -F 应至少流出一块字节", first.isNotEmpty() && first[0].isNotEmpty())
        }

    /** TOFU：首连记录 host key，断开后用同一把 key 再连应匹配通过。 */
    @Test
    fun tofuFirstUseStoresThenMatches() =
        runBlocking {
            val store = FakeKnownHostStore()
            SshConnection(store).use {
                it.connect(config(), null)
                assertTrue("首连应成功（TOFU 首见接受）", it.isConnected)
            }
            assertEquals("首连应记录 1 条 known host", 1, store.forHost(host, port).size)
            SshConnection(store).use {
                it.connect(config(), null) // 同 key → Trusted
                assertTrue("再连应成功（host key 匹配）", it.isConnected)
            }
            assertEquals("再连不应重复记录", 1, store.forHost(host, port).size)
        }

    /** TOFU：篡改已存的 host key 后再连应被拒（HostKeyChangedException）。 */
    @Test
    fun tofuMismatchRejected() =
        runBlocking {
            val store = FakeKnownHostStore()
            SshConnection(store).use { it.connect(config(), null) } // 学到真 key+type 并存
            val stored = store.forHost(host, port).first()
            store.record(stored.copy(publicKeyBase64 = "TAMPERED" + stored.publicKeyBase64)) // 同 PK 覆盖，污染 b64
            SshConnection(store).use {
                try {
                    it.connect(config(), null)
                    fail("篡改 host key 后应拒绝连接")
                } catch (e: HostKeyChangedException) {
                    assertTrue("应针对协商出的 keyType 报变更", e.keyType.isNotEmpty())
                }
            }
        }
}
