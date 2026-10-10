package com.ccmonitor.mobile.core.data.repo

import com.ccmonitor.mobile.core.data.crypto.CryptoBox
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostDao
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.util.Base64

/**
 * HostRepository 密码加解密契约（纯 JVM）。用变换型 fake crypto（XOR 0x42）证明 passwordEnc 真过了
 * CryptoBox（非直存明文 Base64）+ withPassword/password 往返 + 空/null 归一。真加密由 KeystoreCryptoBox 负责（设备侧）。
 */
class HostRepositoryPasswordTest {
    private fun repo() = HostRepository(NoTouchHostDao, XorCrypto)

    private fun host() = Host(id = "h", label = "h", host = "1.1.1.1", port = 22, username = "u")

    @Test
    fun withPasswordEncryptsAndRoundTrips() {
        val r = repo()
        val stored = r.withPassword(host(), "s3cret")
        assertEquals("解密应还原明文", "s3cret", r.password(stored))
        // passwordEnc 不是明文的裸 Base64（证明真过了 crypto，而非直存）。
        val plainBase64 = Base64.getEncoder().encodeToString("s3cret".toByteArray())
        assertNotEquals("不应是明文 Base64（未加密）", plainBase64, stored.passwordEnc)
    }

    @Test
    fun emptyPasswordNormalizesToNull() {
        val r = repo()
        assertNull("空串=不用密码认证", r.withPassword(host(), "").passwordEnc)
        assertNull("null=不用密码认证", r.withPassword(host(), null).passwordEnc)
    }

    @Test
    fun passwordOfUnsetHostIsNull() {
        assertNull("passwordEnc 为 null → password() 为 null", repo().password(host()))
    }

    @Test
    fun clearingPasswordWipesEnc() {
        val r = repo()
        val withPw = r.withPassword(host(), "pw")
        val cleared = r.withPassword(withPw, null) // 切回身份认证
        assertNull(cleared.passwordEnc)
    }
}

/** 变换型 fake：证明 encrypt/decrypt 真被调用（非 Noop 直存）。 */
private object XorCrypto : CryptoBox {
    private fun xor(b: ByteArray) = ByteArray(b.size) { (b[it].toInt() xor 0x42).toByte() }

    override fun encrypt(plain: ByteArray) = xor(plain)

    override fun decrypt(blob: ByteArray) = xor(blob)

    override fun isEncrypted(blob: ByteArray) = true
}

/** withPassword/password 不碰 DAO——全 no-op。 */
private object NoTouchHostDao : HostDao {
    override fun observeAll(): Flow<List<Host>> = flowOf(emptyList())

    override suspend fun get(id: String): Host? = null

    override suspend fun upsert(host: Host) = Unit

    override suspend fun upsertAll(hosts: List<Host>) = Unit

    override suspend fun delete(host: Host) = Unit

    override suspend fun deleteById(id: String) = Unit
}
