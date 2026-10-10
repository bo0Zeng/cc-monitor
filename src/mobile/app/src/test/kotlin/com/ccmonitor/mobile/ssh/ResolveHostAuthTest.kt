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
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** 认证分派单一来源——密码优先、否则按身份、都无则报错。 */
class ResolveHostAuthTest {
    private fun hostBase() = Host(id = "h1", label = "h", host = "1.2.3.4", port = 22, username = "u")

    private fun ksIdentity(id: String) =
        Identity(
            id = id,
            label = id,
            keyType = "ecdsa-p256",
            privateKeyEnc = ByteArray(0),
            publicKeyOpenSsh = "",
            fingerprintSha256 = "",
            createdAt = 0,
            keystoreAlias = id,
        )

    @Test
    fun passwordHostReturnsPasswordAuth() =
        runBlocking {
            val hostRepo = HostRepository(HAFakeHostDao(), NoopCrypto)
            val host = hostRepo.withPassword(hostBase(), "s3cret") // 加密进 passwordEnc
            val auth = resolveHostAuth(host, null, IdentityRepository(HAFakeIdentityDao(emptyMap<String, Identity>()), NoopCrypto), hostRepo)
            assertTrue("有密码 → Password", auth is AuthMethod.Password)
            assertEquals("s3cret", (auth as AuthMethod.Password).password)
        }

    @Test
    fun keyHostDispatchesByIdentity() =
        runBlocking {
            val host = hostBase().copy(authRef = "id1")
            val idRepo = IdentityRepository(HAFakeIdentityDao(mapOf("id1" to ksIdentity("id1"))), NoopCrypto)
            val auth = resolveHostAuth(host, null, idRepo, HostRepository(HAFakeHostDao(), NoopCrypto))
            assertTrue("无密码有身份 → 按身份路由（keystore→KeystoreSigner）", auth is AuthMethod.KeystoreSigner)
        }

    @Test
    fun pendingIdentityUsedWhenNoPasswordNoAuthRef() =
        runBlocking {
            val host = hostBase() // authRef null, passwordEnc null
            val idRepo = IdentityRepository(HAFakeIdentityDao(mapOf("pend" to ksIdentity("pend"))), NoopCrypto)
            val auth = resolveHostAuth(host, "pend", idRepo, HostRepository(HAFakeHostDao(), NoopCrypto)) // 本次选定
            assertTrue(auth is AuthMethod.KeystoreSigner)
        }

    @Test
    fun noPasswordNoAuthNoPendingThrows() {
        val host = hostBase()
        assertThrows(IllegalStateException::class.java) {
            runBlocking {
                resolveHostAuth(host, null, IdentityRepository(HAFakeIdentityDao(emptyMap<String, Identity>()), NoopCrypto), HostRepository(HAFakeHostDao(), NoopCrypto))
            }
        }
    }
}

private object NoopCrypto : CryptoBox {
    override fun encrypt(plain: ByteArray) = plain

    override fun decrypt(blob: ByteArray) = blob

    override fun isEncrypted(blob: ByteArray) = false
}

private class HAFakeHostDao : HostDao {
    override fun observeAll(): Flow<List<Host>> = flowOf(emptyList())

    override suspend fun get(id: String): Host? = null

    override suspend fun upsert(host: Host) = Unit

    override suspend fun upsertAll(hosts: List<Host>) = Unit

    override suspend fun delete(host: Host) = Unit

    override suspend fun deleteById(id: String) = Unit
}

private class HAFakeIdentityDao(
    private val map: Map<String, Identity>,
) : IdentityDao {
    override fun observeAll(): Flow<List<Identity>> = flowOf(map.values.toList())

    override suspend fun get(id: String): Identity? = map[id]

    override suspend fun upsert(identity: Identity) = Unit

    override suspend fun delete(identity: Identity) = Unit
}
