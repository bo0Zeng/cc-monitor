package com.ccmonitor.mobile.core.ssh

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ccmonitor.mobile.core.data.crypto.SshKeyGenerator
import net.schmizz.sshj.common.KeyType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.nio.ByteBuffer
import java.security.KeyStore
import java.security.Signature

/**
 * 在真 Android 运行时上验证 KeystoreEcdsaSigner 的 TEE 签名与 SSH 编码，不需要 SSH 服务器：
 * 生成密钥、在 AndroidKeyStore 内签挑战、用公钥标准验签，并校验 SSH 线格式。
 */
@RunWith(AndroidJUnit4::class)
class KeystoreSignerInstrumentedTest {
    private fun keystore(): KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    @Before
    fun installBouncyCastle() {
        // 生产路径在 SshjConfigFactory.createForKeystoreSigner() 里先装全量 BC；这里也要先装，
        // 否则标准 SignatureECDSA 用到 Android 自带的裁剪版 "BC"（缺 SHA256withECDSA）会失败。
        BouncyCastleSetup.ensure()
    }

    @Test
    fun keystoreSignerSignsValidEcdsaAndEncodesSsh() {
        val alias = "aterm-f31-test-${System.nanoTime()}"
        try {
            SshKeyGenerator.generateEcdsaP256InKeystore(alias, comment = "f31-test")
            val signer = KeystoreEcdsaSigner(alias)
            assertEquals(KeyType.ECDSA256, signer.getType())
            // sshj 靠 KeyType.fromKey(私钥) 选 KeyAlgorithm：AndroidKeyStore 私钥必须被判成 ECDSA256，
            // 否则报 "No KeyAlgorithm for UNKNOWN"，认证接不上。
            assertEquals(KeyType.ECDSA256, KeyType.fromKey(signer.getPrivate()))

            val challenge = "f31-ssh-auth-challenge".toByteArray()
            val sig = KeystoreEcdsaP256Signature()
            sig.initSign(signer.getPrivate()) // AndroidKeyStore 句柄（不可导出）
            sig.update(challenge)
            val der = sig.sign() // 原始 DER，在 TEE 内签出

            // 密码学正确：DER 用公钥标准验签通过，服务器就会接受
            val verifier = Signature.getInstance("SHA256withECDSA")
            verifier.initVerify(signer.getPublic())
            verifier.update(challenge)
            assertTrue("AndroidKeyStore 签名应被公钥验签通过", verifier.verify(der))

            // SSH 格式：encode 产出 `mpint r ‖ mpint s` 两个 SSH string，无残留
            assertTwoSshStrings(sig.encode(der))
        } finally {
            runCatching { keystore().deleteEntry(alias) }
        }
    }

    @Test
    fun signatureRejectsTamperedChallenge() {
        val alias = "aterm-f31-neg-${System.nanoTime()}"
        try {
            SshKeyGenerator.generateEcdsaP256InKeystore(alias)
            val signer = KeystoreEcdsaSigner(alias)
            val sig = KeystoreEcdsaP256Signature()
            sig.initSign(signer.getPrivate())
            sig.update("real-challenge".toByteArray())
            val der = sig.sign()

            val verifier = Signature.getInstance("SHA256withECDSA")
            verifier.initVerify(signer.getPublic())
            verifier.update("tampered-challenge".toByteArray())
            assertFalse("篡改的挑战不应验签通过", verifier.verify(der))
        } finally {
            runCatching { keystore().deleteEntry(alias) }
        }
    }

    /** SSH ECDSA sig blob = string(mpint r) + string(mpint s)：确认恰是两个 length-prefixed string 且用尽整块。 */
    private fun assertTwoSshStrings(blob: ByteArray) {
        assertTrue("blob 太短", blob.size >= 8)
        val bb = ByteBuffer.wrap(blob)
        val lenR = bb.int
        assertTrue("r 长度越界", lenR in 1..(blob.size - 8))
        bb.position(bb.position() + lenR)
        val lenS = bb.int
        assertTrue("s 长度越界", lenS in 1..bb.remaining())
        bb.position(bb.position() + lenS)
        assertEquals("两个 SSH string 应恰好用完整个 blob", 0, bb.remaining())
    }
}
