package com.ccmonitor.mobile.core.ssh

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ccmonitor.mobile.core.data.crypto.SshKeyGenerator
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** 从私钥 PEM 派生公钥（Rust 核心那一份，要 .so ⇒ 在设备上跑）。 */
@RunWith(AndroidJUnit4::class)
class PublicKeyDeriverTest {
    @Test
    fun derives_ed25519_public_key_matching_the_generated_one() {
        val gen = SshKeyGenerator.generateEd25519("f40-test")
        val derived = PublicKeyDeriver.fromPrivatePem(gen.privateKeyOpenSshPem, comment = "f40-test")
        assertNotNull("应能从生成的 Ed25519 PEM 派生公钥", derived)
        derived!!
        assertEquals("ssh-ed25519", derived.keyType)
        // 派生的 OpenSSH 公钥的 base64 部分应与生成时一致（type + base64 两段比对，comment 可不同）
        val genParts = gen.publicKeyOpenSsh.split(" ")
        val derParts = derived.openSsh.split(" ")
        assertEquals(genParts[0], derParts[0])
        assertEquals("公钥 base64 应与生成一致", genParts[1], derParts[1])
        assertEquals("指纹应与生成一致", gen.fingerprintSha256, derived.fingerprintSha256)
        assertTrue(derived.fingerprintSha256.startsWith("SHA256:"))
    }

    @Test
    fun garbage_input_returns_null_instead_of_throwing() {
        assertNull(PublicKeyDeriver.fromPrivatePem("not a private key".toByteArray()))
        assertNull(PublicKeyDeriver.fromPrivatePem(ByteArray(0)))
    }
}
