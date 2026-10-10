package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.crypto.NoopCryptoBox
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.IdentityDao
import com.ccmonitor.mobile.core.data.db.isKeystoreBacked
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ssh.AuthMethod
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 连接路由最终形态的两个纯逻辑零件——
 * ① [isKeystoreBacked] 决定走 KeystoreSigner（不裸解密）还是 PrivateKey；
 * ② [toConnectionConfigs] 把选定的 auth 施于每个竞速 endpoint。
 * （VM 内 connect 的实际编排要创建 termlib 原生会话，纯 JVM 测不了，由设备端测试覆盖。）
 */
class AuthRoutingTest {
    private fun identity(keystoreAlias: String? = null) =
        Identity(
            id = "i",
            label = "i",
            keyType = if (keystoreAlias != null) "ecdsa-p256" else "imported",
            privateKeyEnc = ByteArray(0),
            publicKeyOpenSsh = "",
            fingerprintSha256 = "",
            createdAt = 0L,
            keystoreAlias = keystoreAlias,
        )

    private fun host(extra: String? = null) =
        Host(
            id = "h",
            label = "h",
            host = "192.0.2.2",
            port = 22,
            username = "pi",
            extraAddresses = extra,
        )

    // === 身份类型判定（keystore 身份不许裸解密：它的私钥字节是空的）===
    @Test fun keystoreIdentityIsKeystoreBacked() = assertTrue(identity(keystoreAlias = "aterm-key-1").isKeystoreBacked())

    @Test fun importedIdentityNotKeystoreBacked() = assertFalse(identity(keystoreAlias = null).isKeystoreBacked())

    // === toConnectionConfigs 把同一 auth 施于每个 endpoint（竞速，先连通者胜）===
    @Test fun privateKeyAuthAppliedToSingleEndpoint() {
        val auth = AuthMethod.PrivateKey("key".toByteArray())
        val configs = host().toConnectionConfigs(auth)
        assertEquals(1, configs.size)
        assertSame(auth, configs[0].auth)
        assertEquals("192.0.2.2", configs[0].host)
        assertEquals("pi", configs[0].username)
    }

    @Test fun keystoreSignerAuthAppliedToEndpoint() {
        val auth = AuthMethod.KeystoreSigner("aterm-key-1")
        val configs = host().toConnectionConfigs(auth)
        assertEquals(1, configs.size)
        assertSame(auth, configs[0].auth)
    }

    @Test fun sameAuthAppliedToAllRacedEndpoints() {
        val auth = AuthMethod.PrivateKey("key".toByteArray())
        val configs = host(extra = "192.0.2.9").toConnectionConfigs(auth)
        assertEquals(2, configs.size)
        configs.forEach { assertSame(auth, it.auth) }
    }

    // === resolveAuthMethod：路由单一来源——keystore→KeystoreSigner（不解密）/ 否则→PrivateKey（解密）===
    @Test fun resolveAuthMethodRoutesKeystoreIdentityToSigner() {
        runBlocking {
            val repo = IdentityRepository(FakeIdentityDao(identity(keystoreAlias = "aterm-key-1")), NoopCryptoBox())
            val auth = resolveAuthMethod("i", repo)
            assertTrue(auth is AuthMethod.KeystoreSigner)
            assertEquals("aterm-key-1", (auth as AuthMethod.KeystoreSigner).alias)
        }
    }

    @Test fun resolveAuthMethodRoutesImportedIdentityToPrivateKey() {
        runBlocking {
            val pem = "PEM-PRIVATE-KEY".toByteArray()
            // NoopCryptoBox 直通：privateKeyEnc 即明文私钥字节 → decrypt 后原样。
            val repo = IdentityRepository(FakeIdentityDao(identity().copy(privateKeyEnc = pem)), NoopCryptoBox())
            val auth = resolveAuthMethod("i", repo)
            assertTrue(auth is AuthMethod.PrivateKey)
            assertArrayEquals(pem, (auth as AuthMethod.PrivateKey).pem)
        }
    }
}

/** 只喂一个身份（id="i"）的最小 [IdentityDao]，供 resolveAuthMethod 路由测试。 */
private class FakeIdentityDao(
    private val stored: Identity,
) : IdentityDao {
    override fun observeAll(): Flow<List<Identity>> = flowOf(listOf(stored))

    override suspend fun get(id: String): Identity? = stored.takeIf { it.id == id }

    override suspend fun upsert(identity: Identity) = error("unused in test")

    override suspend fun delete(identity: Identity) = error("unused in test")
}
