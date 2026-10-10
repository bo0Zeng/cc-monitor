package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.crypto.CryptoBox
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostDao
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.IdentityDao
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ssh.AuthMethod
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** ProxyJump 链解析（递归 + 防环 + 逐跳身份）纯逻辑测。跳板身份用 keystore-backed（resolveAuthMethod 不解密）。 */
class ResolveJumpChainTest {
    private fun ksIdentity(id: String) =
        Identity(
            id = id,
            label = id,
            keyType = "ecdsa-p256",
            privateKeyEnc = ByteArray(0),
            publicKeyOpenSsh = "",
            fingerprintSha256 = "",
            createdAt = 0L,
            keystoreAlias = "alias-$id",
        )

    private fun host(
        id: String,
        jump: String? = null,
    ) = Host(id = id, label = id, host = "$id.example", port = 22, username = "u$id", authRef = "idk", proxyJumpHostId = jump)

    private fun repos(hosts: List<Host>): Pair<HostRepository, IdentityRepository> {
        val hostRepo = HostRepository(JumpFakeHostDao(hosts.associateBy { it.id }), JumpNoopCrypto)
        val idRepo = IdentityRepository(JumpFakeIdentityDao(mapOf("idk" to ksIdentity("idk"))), JumpNoopCrypto)
        return hostRepo to idRepo
    }

    @Test
    fun `no proxyJump returns null`() =
        runBlocking {
            val (hr, ir) = repos(listOf(host("A")))
            assertNull(resolveJumpChain(host("A"), hr, ir))
        }

    @Test
    fun `single hop resolves jump config`() =
        runBlocking {
            val a = host("A", jump = "B")
            val (hr, ir) = repos(listOf(a, host("B")))
            val chain = resolveJumpChain(a, hr, ir)!!
            assertEquals("B.example", chain.host)
            assertEquals("uB", chain.username)
            assertTrue(chain.auth is AuthMethod.KeystoreSigner)
            assertNull("单跳 → 跳板无再上级", chain.jumpVia)
        }

    @Test
    fun `multi hop A via B via C nests jumpVia`() =
        runBlocking {
            val a = host("A", jump = "B")
            val (hr, ir) = repos(listOf(a, host("B", jump = "C"), host("C")))
            val chain = resolveJumpChain(a, hr, ir)!!
            assertEquals("B.example", chain.host)
            assertEquals("C.example", chain.jumpVia!!.host)
            assertNull(chain.jumpVia!!.jumpVia)
        }

    @Test
    fun `dangling jump host throws (fail-closed, no silent bastion bypass)`() {
        val a = host("A", jump = "GHOST") // 跳板 id 指向不存在的主机（悬垂，如被删除）
        val (hr, ir) = repos(listOf(a))
        assertThrows(IllegalStateException::class.java) { runBlocking { resolveJumpChain(a, hr, ir) } }
    }

    @Test
    fun `identity-less jump host throws (fail-closed)`() {
        val a = host("A", jump = "B")
        val bNoIdentity = host("B").copy(authRef = null) // 跳板主机无身份
        val (hr, ir) = repos(listOf(a, bNoIdentity))
        assertThrows(IllegalStateException::class.java) { runBlocking { resolveJumpChain(a, hr, ir) } }
    }

    @Test
    fun `cycle A via B via A terminates`() =
        runBlocking {
            val a = host("A", jump = "B")
            val (hr, ir) = repos(listOf(a, host("B", jump = "A")))
            val chain = resolveJumpChain(a, hr, ir)!!
            assertEquals("B.example", chain.host)
            assertNull("环在 B→A 处停止（A 已访问）", chain.jumpVia) // 不无限递归
        }
}

private class JumpFakeHostDao(
    private val map: Map<String, Host>,
) : HostDao {
    override suspend fun get(id: String): Host? = map[id]

    override fun observeAll(): Flow<List<Host>> = throw NotImplementedError()

    override suspend fun upsert(host: Host) = throw NotImplementedError()

    override suspend fun upsertAll(hosts: List<Host>) = throw NotImplementedError()

    override suspend fun delete(host: Host) = throw NotImplementedError()

    override suspend fun deleteById(id: String) = throw NotImplementedError()
}

private class JumpFakeIdentityDao(
    private val map: Map<String, Identity>,
) : IdentityDao {
    override suspend fun get(id: String): Identity? = map[id]

    override fun observeAll(): Flow<List<Identity>> = throw NotImplementedError()

    override suspend fun upsert(identity: Identity) = throw NotImplementedError()

    override suspend fun delete(identity: Identity) = throw NotImplementedError()
}

private object JumpNoopCrypto : CryptoBox {
    override fun encrypt(plaintext: ByteArray): ByteArray = plaintext

    override fun decrypt(ciphertext: ByteArray): ByteArray = ciphertext

    override fun isEncrypted(blob: ByteArray): Boolean = true // 测试用；keystore 身份路径不触及
}
