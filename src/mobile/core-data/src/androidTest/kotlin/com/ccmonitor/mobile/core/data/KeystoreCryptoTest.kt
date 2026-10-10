package com.ccmonitor.mobile.core.data

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ccmonitor.mobile.core.data.crypto.KeystoreCryptoBox
import com.ccmonitor.mobile.core.data.crypto.SshKeyGenerator
import com.ccmonitor.mobile.core.data.crypto.SshWire
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

@RunWith(AndroidJUnit4::class)
class KeystoreCryptoTest {
    private val ctx: Context = ApplicationProvider.getApplicationContext()
    private val alias = "aterm_test_ecdsa"

    @Before fun setup() = cleanAlias()

    @After fun teardown() = cleanAlias()

    private fun cleanAlias() {
        val ks = KeyStore.getInstance(SshKeyGenerator.ANDROID_KEYSTORE).apply { load(null) }
        if (ks.containsAlias(alias)) ks.deleteEntry(alias)
    }

    @Test
    fun aeadEncryptDecryptRoundTrip() {
        val box = KeystoreCryptoBox(ctx)
        val plain = "super-secret-private-key-bytes".toByteArray()
        val enc = box.encrypt(plain)
        assertFalse("ciphertext must differ from plaintext", enc.contentEquals(plain))
        assertArrayEquals(plain, box.decrypt(enc))
        assertTrue(box.isEncrypted(enc))
        assertFalse(box.isEncrypted("-----BEGIN OPENSSH PRIVATE KEY-----".toByteArray()))
    }

    @Test
    fun generatesValidEcdsaP256OpenSshKey() {
        val key = SshKeyGenerator.generateEcdsaP256InKeystore(alias, "test@aterm")
        assertTrue(key.publicKeyOpenSsh.startsWith("ecdsa-sha2-nistp256 "))
        assertTrue(key.publicKeyOpenSsh.endsWith(" test@aterm"))
        assertTrue(key.fingerprintSha256.startsWith("SHA256:"))
        // emulator 无 StrongBox → 必回退软件
        assertFalse(key.strongBoxBacked)

        // wire blob 末尾 65 字节为未压缩点 0x04||X||Y
        val ks = KeyStore.getInstance(SshKeyGenerator.ANDROID_KEYSTORE).apply { load(null) }
        val pub = ks.getCertificate(alias).publicKey as ECPublicKey
        val blob = SshWire.encodeEcdsaP256(pub)
        assertEquals(0x04.toByte(), blob[blob.size - 65])
    }

    @Test
    fun keystoreKeySignsAndVerifies() {
        SshKeyGenerator.generateEcdsaP256InKeystore(alias)
        val ks = KeyStore.getInstance(SshKeyGenerator.ANDROID_KEYSTORE).apply { load(null) }
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
