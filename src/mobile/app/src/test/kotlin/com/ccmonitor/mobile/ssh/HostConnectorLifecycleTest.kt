package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.crypto.NoopCryptoBox
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostDao
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.IdentityDao
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.remote.ExecResult
import com.ccmonitor.mobile.core.ssh.ConnectionConfig
import com.ccmonitor.mobile.core.ssh.SftpEntry
import com.ccmonitor.mobile.core.ssh.ShellChannel
import com.ccmonitor.mobile.core.ssh.SshConnectEvent
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.SshTransport
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 网关的「生」与「死」真的落在连接管理器上：行为那一半。
 *
 * `HostConnectorGatewayTest` 那几条全是源码扫描，只答「谁写了哪个名字」。一个 `fun release(hostId, holder) {}`
 * （空实现）能让那几条全绿，而连接永远不会被放手。所以生死两头都要有一条跑起来看结果的判据：
 *
 * | 真 bug | 本文件 |
 * |---|---|
 * | `HostConnector.release` 不落到 manager（空实现 / 转给了别的实例） | 红：`connection(id)` 该变 null 却没变 |
 * | `HostConnector.retain` 不落到 manager（返回常数纪元） | 红：陈旧纪元该被守门拒掉却放行了 |
 * | `HostConnector.connect` 没把解析结果交给 manager | 红：`connection(id)` 拿不到那条 transport |
 * | `retain` 返回的纪元不是 manager 当时的纪元 | 红（[staleEpochFromRetainIsRejectedByConnect]） |
 * | `HostConnector.connection` 写成空壳（`= null`）或不跟随断开 | 红（[connectionThroughTheGatewayTracksTheManagersHandle]） |
 * | `HostConnector.disconnectAll` 写成空壳 `{}`，或接成了「有持有者就不断」的 `release` 语义 | 红（[disconnectAllThroughTheGatewayForcesEveryHostDown]） |
 *
 * 判别力边界：
 * - 用的是真 [SshConnectionManager] + 假 [SshTransport]（`transportFactory` 注入）：
 *   manager 的锁/纪元/持有者集逻辑是真的跑了的，只有 socket 是假的。
 * - 物理 `close` 由 manager 派到 `reconnectScope`（后台）⇒ 本文件断言的是摘除（`connection(id)` 变 null，
 *   锁内同步可见），不是 transport 真被关。「close 落在哪条线程」由 `core-ssh` 的 `SshConnectionCloseThreadTest` 守。
 */
class HostConnectorLifecycleTest {
    private val hostId = "h1"
    private val holder = "test-holder"

    private fun identity() =
        Identity(
            id = "idk",
            label = "idk",
            keyType = "ecdsa-p256",
            privateKeyEnc = ByteArray(0),
            publicKeyOpenSsh = "",
            fingerprintSha256 = "",
            createdAt = 0L,
            keystoreAlias = "alias-idk", // keystore 身份 → 认证解析不解密、纯 JVM 可跑
        )

    private fun host(id: String = hostId) = Host(id = id, label = "H", host = "h.example", port = 22, username = "u", authRef = "idk")

    private fun fixture(): Pair<SshConnectionManager, HostConnector> = fixture(listOf(hostId))

    private fun fixture(ids: List<String>): Pair<SshConnectionManager, HostConnector> {
        val manager = SshConnectionManager(transportFactory = { LifecycleFakeTransport() })
        val connector =
            HostConnector(
                manager,
                HostRepository(LifecycleFakeHostDao(ids.associateWith { host(it) }), NoopCryptoBox()),
                IdentityRepository(LifecycleFakeIdentityDao(mapOf("idk" to identity())), NoopCryptoBox()),
            )
        return manager to connector
    }

    /**
     * 经网关建连 → 经网关放手 → 连接真的被摘掉。
     *
     * 「生与死都收进 `HostConnector`」的最小端到端：全程一次都不碰 manager 的生命周期 API。
     */
    @Test
    fun connectAndReleaseThroughTheGatewayReachTheManager() =
        runBlocking {
            val (manager, connector) = fixture()
            val epoch = connector.retain(hostId, holder)
            val transport = connector.connect(host(), expectedGen = epoch)
            assertTrue("前提：假 transport 该是连上的状态，否则下面的断言量不到东西", transport.isConnected)
            assertNotNull(
                "经网关建的连接没有进 manager：`HostConnector.connect` 没落到 `manager.connect` 上。",
                manager.connection(hostId),
            )
            assertEquals("进 manager 的该是网关交出去的那一条", transport, manager.connection(hostId))

            connector.release(hostId, holder)
            assertNull(
                "经网关放手后连接还在：`HostConnector.release` 没落到 `manager.release` 上" +
                    "（空实现 / 拿到了别的 manager 实例）。这正是源码扫描那几条抓不到的形态。",
                manager.connection(hostId),
            )
            assertTrue("放手后会话条目也该摘掉（manager 的既有语义，顺带钉住没被绕过）", manager.sessions.value.isEmpty())
        }

    /**
     * `retain` 返回的纪元是 manager 的真纪元，否则纪元守门形同虚设。
     *
     * `retain` 与随后的 `connect` 之间若发生 force-disconnect，那次 connect 必须被弃用（否则留下孤儿连接）。
     * 若 `HostConnector.retain` 自己编一个数（比如恒返回 0），这条守门就永远不触发，
     * 而源码扫描那几条一条都不会红。
     */
    @Test
    fun staleEpochFromRetainIsRejectedByConnect() =
        runBlocking {
            val (manager, connector) = fixture()
            val epoch = connector.retain(hostId, holder)
            manager.disconnect(hostId) // force-disconnect：纪元自增 ⇒ 手里那个 epoch 过期
            val boom =
                assertThrows(
                    "用过期纪元建连居然成功了：`HostConnector.retain` 返回的不是 manager 的真纪元，" +
                        "纪元守门整条形同虚设（force-disconnect 落在窄窗里就会留下孤儿连接）。",
                    CancellationException::class.java,
                ) { runBlocking { connector.connect(host(), expectedGen = epoch) } }
            assertTrue("抛的该是纪元守门那条（消息里要认得出来）：${boom.message}", boom.message != null)
            assertNull("被弃用的连接不许装进 manager", manager.connection(hostId))
        }

    /**
     * `retain` 交回来的必须是 manager 的「当前」纪元，否则一次正常建连会被误弃用。
     *
     * 注意：把 `HostConnector.retain` 改成「调一下 manager，然后恒返回 0」，[staleEpochFromRetainIsRejectedByConnect]
     * 是绿的：manager 的纪元初值恰好就是 0，真值与常数值撞在一起。本条先把纪元推离初值再问，
     * 并且先断言前提（`epoch != 0`），不然它会悄悄退化回上一条的无判别力状态。
     */
    @Test
    fun retainHandsBackTheManagersCurrentEpoch() =
        runBlocking {
            val (manager, connector) = fixture()
            manager.disconnect(hostId) // 纪元 0→1
            manager.disconnect(hostId) // 纪元 1→2：推离初值，常数实现与真值从此可辨
            val epoch = connector.retain(hostId, holder)
            assertTrue(
                "`HostConnector.retain` 交回来的纪元是 0，而 manager 这时的纪元已经被推到 2 了。\n" +
                    "  两种可能，都得查：① 网关没把 `manager.retain` 的返回值交出来（比如恒返回常数），这正是本条要抓的 bug；" +
                    "② manager 的纪元语义变了，本条推离初值的手法（连调两次 `disconnect`）不再有效 ⇒ 这条判据退化成零判别力，要改。",
                epoch != 0,
            )
            connector.connect(host(), expectedGen = epoch)
            assertNotNull(
                "用当前纪元建连被守门拒掉了：`HostConnector.retain` 交回来的不是 manager 的真纪元。" +
                    "真实后果：每次建连都被当成「纪元过期」弃用，连不上且无从查起。",
                manager.connection(hostId),
            )
        }

    /**
     * 阴性对照的对照：同一主机两个持有者时，放手一个不许断连。
     *
     * 没有这条的话，一个「`release` = 无条件 disconnect」的错实现也能让第一条绿。
     * 它钉住网关交出去的是 `release`（持有者语义）而不是 `disconnect`（强断语义）。
     */
    @Test
    fun releasingOneOfTwoHoldersKeepsTheConnection() =
        runBlocking {
            val (manager, connector) = fixture()
            val epoch = connector.retain(hostId, "holder-A")
            connector.retain(hostId, "holder-B")
            connector.connect(host(), expectedGen = epoch)
            assertNotNull("前提：得先连上", manager.connection(hostId))

            connector.release(hostId, "holder-A")
            assertNotNull(
                "还有一个持有者在，放手另一个居然就断了：网关把 `release` 接成了强断（`disconnect`）。" +
                    "真实后果：同主机开两个 tab，关掉一个就连带杀掉另一个正在跑的东西。",
                manager.connection(hostId),
            )
            connector.release(hostId, "holder-B")
            assertNull("最后一个持有者走了才该真断", manager.connection(hostId))
        }

    /**
     * 经网关拿到的句柄就是 manager 手里那一条，且跟着它生灭。
     *
     * 它要防的 bug 很具体：`fun connection(hostId: String): SshTransport? = null`（空壳）
     * 能让 `HostConnectorGatewayTest` 那几条源码扫描全绿：名字写对了、调用方钉上了、
     * 文件集也对。真实后果是 `SftpViewModel.ensureConn` 永远复用不到现成连接（每次重连）、
     * `HostLink` 永远走一遍建连、`AttachmentPicker` 上传当场报「这台主机现在用不了」。
     *
     * 两头都断：有连接时非空且同一对象（防空壳）+ 断开后变 null（防「把 connect 的返回值缓存住」）。
     */
    @Test
    fun connectionThroughTheGatewayTracksTheManagersHandle() =
        runBlocking {
            val (manager, connector) = fixture()
            assertNull("前提：还没连的时候两边都该是 null，否则下面量不出东西", connector.connection(hostId))
            val epoch = connector.retain(hostId, holder)
            val transport = connector.connect(host(), expectedGen = epoch)
            assertEquals(
                "经网关问句柄，拿到的不是 manager 手里那一条：`HostConnector.connection` 多半是空壳或自己缓存了一份。",
                transport,
                connector.connection(hostId),
            )
            connector.release(hostId, holder)
            assertNull(
                "连接已经被放手摘掉了，经网关问句柄却还拿得到：网关缓存了一份陈旧句柄。" +
                    "真实后果：拿着一条已死的 transport 去 sftp/exec，卡死而不报错。",
                connector.connection(hostId),
            )
        }

    /**
     * `disconnectAll` 真的把每一台都强断掉：无视持有者。
     *
     * 它要防的两个 bug：
     * ① 空壳 `fun disconnectAll() {}` ⇒ 用户把任务划走后连接全都还在，前台保活服务继续常驻耗电
     *    （`onTaskRemoved` 那一句存在的全部理由）。
     * ② 接成了 `release` 语义（「还有持有者就不断」）⇒ 有 tab 存活时划走任务什么也不会发生。
     *
     * 故本条刻意给两台主机各留一个在册持有者：`release` 语义下两台都断不掉，强断语义下两台都得掉。
     */
    @Test
    fun disconnectAllThroughTheGatewayForcesEveryHostDown() =
        runBlocking {
            val second = "h2"
            val (manager, connector) = fixture(listOf(hostId, second))
            for (id in listOf(hostId, second)) {
                val epoch = connector.retain(id, holder)
                connector.connect(host(id), expectedGen = epoch)
                assertNotNull("前提：$id 得先真连上，否则下面量不出东西", manager.connection(id))
            }

            connector.disconnectAll()

            assertNull(
                "`HostConnector.disconnectAll` 之后第一台还连着：空壳实现，或接成了「有持有者就不断」的 `release` 语义。",
                manager.connection(hostId),
            )
            assertNull(
                "第二台还连着：它断的不是「全部」。真实后果：用户划走任务后仍有连接常驻、前台服务不自停。",
                manager.connection(second),
            )
        }
}

/** [SshTransport] 的最小替身：只需「连得上、报 isConnected、关得掉」，manager 的生死逻辑才是被测对象。 */
private class LifecycleFakeTransport : SshTransport {
    @Volatile private var alive = false

    @Volatile private var closed = false

    override val isConnected: Boolean get() = alive && !closed

    override suspend fun connect(
        configs: List<ConnectionConfig>,
        onEvent: ((SshConnectEvent) -> Unit)?,
    ) {
        alive = true
    }

    override fun close() {
        closed = true
    }

    private fun nope(): Nothing = throw UnsupportedOperationException("本替身只服务连接生死，不做通道/SFTP")

    override suspend fun openShell(
        term: String,
        cols: Int,
        rows: Int,
    ): ShellChannel = nope()

    override fun execStream(command: String): Flow<ByteArray> = nope()

    override suspend fun execCapture(command: String): ExecResult = nope()

    override suspend fun sftpList(path: String): List<SftpEntry> = nope()

    override suspend fun sftpRealPath(path: String): String = nope()

    override suspend fun sftpDownload(
        remotePath: String,
        local: File,
    ) = nope()

    override suspend fun sftpUpload(
        local: File,
        remotePath: String,
    ) = nope()

    override suspend fun sftpRename(
        from: String,
        to: String,
    ) = nope()

    override suspend fun sftpDelete(
        path: String,
        isDir: Boolean,
    ) = nope()

    override suspend fun sftpMkdir(path: String) = nope()

    override suspend fun sftpReadText(
        path: String,
        maxBytes: Int,
    ): String = nope()

    override suspend fun sftpReadTextForEdit(
        path: String,
        maxBytes: Int,
    ): String? = nope()

    override suspend fun sftpWriteText(
        remotePath: String,
        text: String,
    ) = nope()
}

private class LifecycleFakeHostDao(
    private val map: Map<String, Host>,
) : HostDao {
    override suspend fun get(id: String): Host? = map[id]

    override fun observeAll(): Flow<List<Host>> = throw NotImplementedError()

    override suspend fun upsert(host: Host) = throw NotImplementedError()

    override suspend fun upsertAll(hosts: List<Host>) = throw NotImplementedError()

    override suspend fun delete(host: Host) = throw NotImplementedError()

    override suspend fun deleteById(id: String) = throw NotImplementedError()
}

private class LifecycleFakeIdentityDao(
    private val map: Map<String, Identity>,
) : IdentityDao {
    override suspend fun get(id: String): Identity? = map[id]

    override fun observeAll(): Flow<List<Identity>> = throw NotImplementedError()

    override suspend fun upsert(identity: Identity) = throw NotImplementedError()

    override suspend fun delete(identity: Identity) = throw NotImplementedError()
}
