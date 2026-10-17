package com.ccmonitor.mobile.core.data.crypto

import com.ccmonitor.mobile.core.data.db.HOSTS_TABLE
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostDao
import com.ccmonitor.mobile.core.data.db.IDENTITIES_TABLE
import com.ccmonitor.mobile.core.data.db.reseal
import com.ccmonitor.mobile.core.data.db.resealPassword
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.google.crypto.tink.Aead
import com.google.crypto.tink.KeyTemplates
import com.google.crypto.tink.KeysetHandle
import com.google.crypto.tink.aead.AeadConfig
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test
import java.security.GeneralSecurityException
import java.util.Base64

/** 密文绑在那一行 · 21→22 的重加（纯 JVM，Tink 真 AEAD，只把 Keystore 那把主钥换成一把临时的）。 */
class RowBindingTest {
    private val box = KeystoreCryptoBox(lazyOf(tinkAead()))

    @Test
    fun twoHostsPasswordsSwappedDoNotDecrypt() {
        val repo = HostRepository(NoTouchHostDao, box)
        val a = repo.withPassword(host("h1"), "pw-a")
        val b = repo.withPassword(host("h2"), "pw-b")
        assertEquals("pw-a", repo.password(a))
        val swapped = a.copy(passwordEnc = b.passwordEnc)
        assertThrows(GeneralSecurityException::class.java) { repo.password(swapped) }
    }

    @Test
    fun twoIdentitiesKeysSwappedDoNotDecrypt() {
        val k1 = box.encrypt("key-1".toByteArray(), rowBinding(IDENTITIES_TABLE, "k1"))
        assertArrayEquals("key-1".toByteArray(), box.decrypt(k1, rowBinding(IDENTITIES_TABLE, "k1")))
        assertThrows(GeneralSecurityException::class.java) { box.decrypt(k1, rowBinding(IDENTITIES_TABLE, "k2")) }
        assertThrows(GeneralSecurityException::class.java) { box.decrypt(k1, rowBinding(HOSTS_TABLE, "k1")) }
    }

    /** 21→22 那一跳的重加：旧密文（全库共用那份关联数据）按那一行重加，原文不变、只认那一行。 */
    @Test
    fun resealBindsAnOldCiphertextToItsRow() {
        val old = box.encrypt("key-1".toByteArray(), "ccm-private-key".toByteArray())
        val sealed = requireNotNull(reseal(box, old, IDENTITIES_TABLE, "k1"))
        assertArrayEquals("key-1".toByteArray(), box.decrypt(sealed, rowBinding(IDENTITIES_TABLE, "k1")))
        assertThrows(GeneralSecurityException::class.java) { box.decrypt(sealed, rowBinding(IDENTITIES_TABLE, "k2")) }
        assertThrows("迁过之后旧关联数据不再认", GeneralSecurityException::class.java) {
            box.decrypt(sealed, "ccm-private-key".toByteArray())
        }

        val oldPw = Base64.getEncoder().encodeToString(box.encrypt("pw".toByteArray(), "ccm-private-key".toByteArray()))
        val repo = HostRepository(NoTouchHostDao, box)
        assertEquals("pw", repo.password(host("h1").copy(passwordEnc = resealPassword(box, "h1", oldPw))))
    }

    /** 解不开的旧密文（坏了的 · 不是这把钥匙加的 · 不是 Base64）⇒ null，交迁移照实处置。 */
    @Test
    fun resealOfUnreadableOldDataIsNull() {
        assertNull(reseal(box, byteArrayOf(1, 2, 3), IDENTITIES_TABLE, "k1"))
        val foreign = KeystoreCryptoBox(lazyOf(tinkAead())).encrypt("x".toByteArray(), "ccm-private-key".toByteArray())
        assertNull(reseal(box, foreign, IDENTITIES_TABLE, "k1"))
        assertNull(resealPassword(box, "h1", "不是 Base64"))
        assertNull(resealPassword(box, "h1", Base64.getEncoder().encodeToString("坏了的".toByteArray())))
    }

    private fun host(id: String) = Host(id = id, label = id, host = "192.0.2.1", port = 22, username = "u")
}

internal fun tinkAead(): Aead {
    AeadConfig.register()
    return KeysetHandle.generateNew(KeyTemplates.get("AES256_GCM")).getPrimitive(Aead::class.java)
}

private object NoTouchHostDao : HostDao {
    override fun observeAll(): Flow<List<Host>> = flowOf(emptyList())

    override suspend fun get(id: String): Host? = null

    override suspend fun upsert(host: Host) = Unit

    override suspend fun upsertAll(hosts: List<Host>) = Unit

    override suspend fun delete(host: Host) = Unit

    override suspend fun deleteById(id: String) = Unit
}
