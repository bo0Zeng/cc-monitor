package com.ccmonitor.mobile.core.ssh

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.security.KeyStore
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.util.Base64

/** Keystore 里生成的 P-256 钥匙：公钥那一行（核心照未压缩点写）对得上、私钥签得了。要那份 .so。 */
@RunWith(AndroidJUnit4::class)
class SshKeyGeneratorTest {
    private val alias = "ccm_test_ecdsa"

    @Before fun setup() = cleanAlias()

    @After fun teardown() = cleanAlias()

    private fun keystore() = KeyStore.getInstance(SshKeyGenerator.ANDROID_KEYSTORE).apply { load(null) }

    private fun cleanAlias() {
        val ks = keystore()
        if (ks.containsAlias(alias)) ks.deleteEntry(alias)
    }

    @Test
    fun generatesValidEcdsaP256OpenSshKey() {
        val key = SshKeyGenerator.generateEcdsaP256InKeystore(alias, "test@cc-monitor")
        assertTrue(key.publicKeyOpenSsh.startsWith("ecdsa-sha2-nistp256 "))
        assertTrue(key.publicKeyOpenSsh.endsWith(" test@cc-monitor"))
        assertTrue(key.fingerprintSha256.startsWith("SHA256:"))
        // emulator 无 StrongBox → 必回退软件
        assertFalse(key.strongBoxBacked)

        // 核心写的那一行里 wire blob 末尾 65 字节就是交给它的那个未压缩点 0x04||X||Y
        val pub = keystore().getCertificate(alias).publicKey as ECPublicKey
        val point = SshWire.uncompressedPoint(pub)
        assertEquals(65, point.size)
        assertEquals(0x04.toByte(), point[0])
        val blob = Base64.getDecoder().decode(key.publicKeyOpenSsh.split(' ')[1])
        assertArrayEquals(point, blob.copyOfRange(blob.size - 65, blob.size))
    }

    @Test
    fun keystoreKeySignsAndVerifies() {
        SshKeyGenerator.generateEcdsaP256InKeystore(alias)
        val ks = keystore()
        val entry = ks.getEntry(alias, null) as KeyStore.PrivateKeyEntry
        val msg = "challenge-bytes".toByteArray()

        val sig =
            Signature.getInstance("SHA256withECDSA").run {
                initSign(entry.privateKey)
                update(msg)
                sign()
            }
        val verified =
            Signature.getInstance("SHA256withECDSA").run {
                initVerify(ks.getCertificate(alias).publicKey)
                update(msg)
                verify(sig)
            }
        assertTrue("Keystore P-256 key must sign+verify", verified)
    }
}
