package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostDao
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.IdentityDao
import com.ccmonitor.mobile.core.data.db.wire
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ssh.AuthMethod
import com.ccmonitor.mobile.core.wire.gen.DialPlan
import com.ccmonitor.mobile.core.wire.gen.DialRequest
import com.ccmonitor.mobile.core.wire.gen.DialUse
import com.ccmonitor.mobile.core.wire.gen.Endpoint
import com.ccmonitor.mobile.core.wire.gen.JumpHop
import com.ccmonitor.mobile.testing.PlainCryptoBox
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 手机这一端照机器表组拨号（[planDial]）：组法在核心（这里换成记下交了什么的假核心），Kotlin 只交表、照请求里每一跳写的那台 id 解认证。
 * 跳板身份用 keystore-backed（[resolveAuthMethod] 不解密）。
 */
class DialPlanTest {
    private fun host(
        id: String,
        jump: String = "",
    ) = Host(id = id, label = id, host = "$id.example", port = 22, username = "u$id", authRef = "idk", jump = jump)

    /** 假核心：记下交进来的那张表（线上那一形），回 [answer]。 */
    private class Seen(
        val answer: DialPlan,
    ) : DialPlanner {
        var table: List<com.ccmonitor.mobile.core.wire.gen.Host> = emptyList()

        override fun plan(
            hosts: List<Host>,
            target: Host,
        ): DialPlan = answer.also { table = hosts.map { it.wire() } }
    }

    private fun request(
        keyPath: String,
        jump: JumpHop? = null,
    ) = DialRequest(
        "A.example",
        22,
        "uA",
        keyPath,
        "",
        listOf(Endpoint("A.example", 22)),
        jump,
        DialUse.STREAM,
        null,
        null,
        null,
        false,
        false,
        null,
        false,
    )

    private fun hop(
        keyPath: String,
        jump: JumpHop? = null,
    ) = JumpHop("$keyPath.example", 22, "u$keyPath", keyPath, keyPath, jump)

    private val identities = ConstIdentityDao()

    private fun plan(
        hosts: List<Host>,
        planner: DialPlanner,
        target: Host = hosts.first(),
    ) = runBlocking {
        planDial(
            target,
            hosts,
            null,
            IdentityRepository(identities, PlainCryptoBox),
            HostRepository(NoHostDao, PlainCryptoBox),
            planner,
        )
    }

    /** Room 里 A→B→A：环原样交到核心（之前 Kotlin 自己截成 A→B 照连）；核心拒 ⇒ 不连、［编辑］改 A，一把钥匙都不解。 */
    @Test
    fun `a jump loop in the room table reaches the core intact and is refused`() {
        val core = Seen(DialPlan.Refused("成环了"))
        val e = assertThrows(JumpBroken::class.java) { plan(listOf(host("A", jump = "B"), host("B", jump = "A")), core) }
        assertEquals(listOf("A" to "B", "B" to "A"), core.table.map { it.label to it.jump })
        assertEquals("每一行的 keyPath 写那台的 id", listOf("A", "B"), core.table.map { it.keyPath })
        assertEquals("成环了", e.message)
        assertEquals("A", e.fixHostId)
        assertEquals("拒了就不解认证", 0, identities.reads)
    }

    /** 核心组好的两跳：每一跳写的那台 id ⇒ 那台的认证；地址 · 跳板照核心那一份原样交给连接层。 */
    @Test
    fun `each hop of the plan gets the auth of the machine it names`() {
        val planned = request("A", hop("B"))
        val cfg = plan(listOf(host("A", jump = "B"), host("B")), Seen(DialPlan.Ready(planned)))
        assertEquals(planned, cfg.request)
        assertEquals(setOf("A", "B"), cfg.auths.keys)
        assertTrue(cfg.auths.values.all { it is AuthMethod.KeystoreSigner })
    }

    /** 跳板机没身份也没密码 ⇒ 拒，［编辑］改跳板那一台；请求里写着表里没有的那台 ⇒ 拒，改这一台。 */
    @Test
    fun `a jump host without auth or missing from the table is refused`() {
        val noAuth = host("B").copy(authRef = null)
        val e = assertThrows(JumpBroken::class.java) { plan(listOf(host("A", jump = "B"), noAuth), Seen(DialPlan.Ready(request("A", hop("B"))))) }
        assertEquals("B", e.fixHostId)
        val gone = assertThrows(JumpBroken::class.java) { plan(listOf(host("A", jump = "B")), Seen(DialPlan.Ready(request("A", hop("B"))))) }
        assertEquals("A", gone.fixHostId)
    }

    /** 交进来的那一台（调用方手里的）替掉库里同 id 的那一行，顺序不动。 */
    @Test
    fun `the machine in hand replaces its row in the table`() {
        val core = Seen(DialPlan.Ready(request("A")))
        plan(listOf(host("A"), host("B")), core, target = host("A").copy(host = "new.example"))
        assertEquals(listOf("new.example", "B.example"), core.table.map { it.host })
    }
}

private class ConstIdentityDao : IdentityDao {
    var reads = 0

    override suspend fun get(id: String): Identity? =
        Identity(
            id = id,
            label = id,
            keyType = "ecdsa-p256",
            privateKeyEnc = ByteArray(0),
            publicKeyOpenSsh = "",
            fingerprintSha256 = "",
            createdAt = 0L,
            keystoreAlias = "alias-$id",
        ).also { reads++ }

    override fun observeAll(): Flow<List<Identity>> = throw NotImplementedError()

    override suspend fun upsert(identity: Identity) = throw NotImplementedError()

    override suspend fun delete(identity: Identity) = throw NotImplementedError()
}

private object NoHostDao : HostDao {
    override suspend fun get(id: String): Host? = throw NotImplementedError()

    override fun observeAll(): Flow<List<Host>> = throw NotImplementedError()

    override suspend fun upsert(host: Host) = throw NotImplementedError()

    override suspend fun upsertAll(hosts: List<Host>) = throw NotImplementedError()
}
