package com.ccmonitor.mobile.core.data.crypto

import org.bouncycastle.crypto.params.Ed25519PrivateKeyParameters
import org.bouncycastle.crypto.params.Ed25519PublicKeyParameters
import org.bouncycastle.crypto.signers.Ed25519Signer
import org.bouncycastle.crypto.util.OpenSSHPrivateKeyUtil
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Base64

/** Ed25519 公钥 wire 编码 + 软件生成（纯 BouncyCastle，JVM 可测）。 */
class Ed25519Test {
    private fun beInt(
        b: ByteArray,
        off: Int,
    ): Int =
        ((b[off].toInt() and 0xFF) shl 24) or ((b[off + 1].toInt() and 0xFF) shl 16) or
            ((b[off + 2].toInt() and 0xFF) shl 8) or (b[off + 3].toInt() and 0xFF)

    @Test fun encodeEd25519Structure() {
        val pub = ByteArray(32) { 1 }
        val blob = SshWire.encodeEd25519(pub)
        assertEquals(4 + 11 + 4 + 32, blob.size) // string("ssh-ed25519") + string(pub)
        assertEquals(11, beInt(blob, 0))
        assertEquals("ssh-ed25519", String(blob, 4, 11))
        assertEquals(32, beInt(blob, 15))
        assertArrayEquals(pub, blob.copyOfRange(19, 51))
    }

    @Test fun ed25519ToOpenSshFormat() {
        val pub = ByteArray(32) { 2 }
        val line = SshWire.ed25519ToOpenSsh(pub, "cmt")
        assertTrue(line.startsWith("ssh-ed25519 "))
        assertTrue(line.endsWith(" cmt"))
        assertArrayEquals(SshWire.encodeEd25519(pub), Base64.getDecoder().decode(line.split(" ")[1]))
    }

    @Test fun generateEd25519RoundtripsThroughOpenSshPem() {
        val key = SshKeyGenerator.generateEd25519("k")
        assertTrue(key.publicKeyOpenSsh.startsWith("ssh-ed25519 "))
        assertTrue(key.fingerprintSha256.startsWith("SHA256:"))
        val pem = String(key.privateKeyOpenSshPem)
        assertTrue(pem.startsWith("-----BEGIN OPENSSH PRIVATE KEY-----"))
        assertTrue(pem.trimEnd().endsWith("-----END OPENSSH PRIVATE KEY-----"))

        // 公钥 OpenSSH 串 → wire blob 尾 32B = 公钥
        val wire = Base64.getDecoder().decode(key.publicKeyOpenSsh.split(" ")[1])
        val encodedPub = wire.copyOfRange(wire.size - 32, wire.size)

        // PEM → 内层 → BC 解析回私钥；其派生公钥应与上一致（keypair 一致）+ 签/验通过（key 有效）
        val inner =
            Base64.getDecoder().decode(
                pem.lineSequence().filterNot { it.startsWith("-----") || it.isBlank() }.joinToString(""),
            )
        val priv = OpenSSHPrivateKeyUtil.parsePrivateKeyBlob(inner) as Ed25519PrivateKeyParameters
        assertArrayEquals(encodedPub, priv.generatePublicKey().encoded)

        val msg = "f33-ed25519".toByteArray()
        val signer = Ed25519Signer().apply { init(true, priv) }
        signer.update(msg, 0, msg.size)
        val sig = signer.generateSignature()
        val verifier = Ed25519Signer().apply { init(false, Ed25519PublicKeyParameters(encodedPub, 0)) }
        verifier.update(msg, 0, msg.size)
        assertTrue("生成的 Ed25519 keypair 应签验通过", verifier.verifySignature(sig))
    }
}
