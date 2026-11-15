package com.ccmonitor.mobile.core.data

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ccmonitor.mobile.core.data.crypto.KeystoreCryptoBox
import com.ccmonitor.mobile.core.data.crypto.rowBinding
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import java.security.GeneralSecurityException

/** 落库加密（[KeystoreCryptoBox]）：解得回来、密文绑在那一行。Keystore 里生成 SSH 钥匙的在 core-ssh 的 `SshKeyGeneratorTest`。 */
@RunWith(AndroidJUnit4::class)
class KeystoreCryptoTest {
    private val ctx: Context = ApplicationProvider.getApplicationContext()

    @Test
    fun aeadEncryptDecryptRoundTrip() {
        val box = KeystoreCryptoBox(ctx)
        val plain = "super-secret-private-key-bytes".toByteArray()
        val enc = box.encrypt(plain, rowBinding("identities", "k1"))
        assertFalse("ciphertext must differ from plaintext", enc.contentEquals(plain))
        assertArrayEquals(plain, box.decrypt(enc, rowBinding("identities", "k1")))
    }

    /** 密文挪到别的行（关联数据换成另一行的）就解不开。 */
    @Test
    fun ciphertextMovedToAnotherRowDoesNotDecrypt() {
        val box = KeystoreCryptoBox(ctx)
        val enc = box.encrypt("k".toByteArray(), rowBinding("identities", "k1"))
        assertThrows(GeneralSecurityException::class.java) { box.decrypt(enc, rowBinding("identities", "k2")) }
    }
}
