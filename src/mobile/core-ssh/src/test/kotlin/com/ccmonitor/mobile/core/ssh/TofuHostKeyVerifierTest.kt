package com.ccmonitor.mobile.core.ssh

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.security.KeyPairGenerator
import java.security.PublicKey
import java.security.spec.ECGenParameterSpec
import java.util.Base64

/**
 * TofuHostKeyVerifier：host key 算法钉扎（findExistingAlgorithms）与 TOFU 比对语义（verify）。
 * verify 用 java.util.Base64，能在纯 JVM 上跑；这里同时守住编码与已存记录逐字节一致，以及首见、信任、变更三个分支。
 */
class TofuHostKeyVerifierTest {
    private fun kh(
        keyType: String,
        b64: String = "AAAA",
    ) = KnownHostRecord(host = "h", port = 22, keyType = keyType, publicKeyBase64 = b64, addedAt = 0L)

    /** 真 EC P-256 公钥（SunEC，无需 BC provider 即可被 sshj KeyType/Buffer 编码）。 */
    private fun ecP256Key(): PublicKey =
        KeyPairGenerator
            .getInstance("EC")
            .apply { initialize(ECGenParameterSpec("secp256r1")) }
            .generateKeyPair()
            .public

    @Test
    fun `no known hosts pins nothing (real first use, default negotiation)`() {
        val v = TofuHostKeyVerifier("h", 22, emptyList(), now = 0L)
        assertEquals(emptyList<String>(), v.findExistingAlgorithms("h", 22))
    }

    @Test
    fun `known host-key types are pinned so sshj prefers already-trusted algorithm`() {
        val v = TofuHostKeyVerifier("h", 22, listOf(kh("ecdsa-sha2-nistp256")), now = 0L)
        // 返回空时，服务器新增 ed25519 后合法重连会被当成 Mismatch 拒掉。
        assertEquals(listOf("ecdsa-sha2-nistp256"), v.findExistingAlgorithms("h", 22))
    }

    @Test
    fun `multiple types are all pinned and de-duplicated`() {
        val v =
            TofuHostKeyVerifier(
                "h",
                22,
                listOf(kh("ssh-ed25519"), kh("ecdsa-sha2-nistp256"), kh("ssh-ed25519", b64 = "BBBB")),
                now = 0L,
            )
        val pinned = v.findExistingAlgorithms("h", 22)
        assertEquals(2, pinned.size)
        assertTrue(pinned.containsAll(listOf("ssh-ed25519", "ecdsa-sha2-nistp256")))
    }

    // 首见分支：空 known 时接受并记 FirstUse，b64 编码成功。
    @Test
    fun `verify first use accepts and records non-empty b64 on jvm`() {
        val key = ecP256Key()
        val v = TofuHostKeyVerifier("h", 22, emptyList(), now = 7L)
        assertTrue("真·首见应接受（TOFU）", v.verify("h", 22, key))
        val outcome = v.outcome
        assertTrue("应记 FirstUse", outcome is TofuOutcome.FirstUse)
        val rec = (outcome as TofuOutcome.FirstUse).knownHost
        assertEquals("ecdsa-sha2-nistp256", rec.keyType)
        assertTrue("java.util.Base64 编码非空", rec.publicKeyBase64.isNotEmpty())
        assertEquals("addedAt=now", 7L, rec.addedAt)
    }

    // 首见记录的 b64 在重连时对同一把 key 应判 Trusted。
    @Test
    fun `verify trusts same key on reconnect round-tripping recorded b64`() {
        val key = ecP256Key()
        val first = TofuHostKeyVerifier("h", 22, emptyList(), now = 0L).apply { verify("h", 22, key) }
        val recorded = (first.outcome as TofuOutcome.FirstUse).knownHost
        // 用首见快照重建校验器（相当于重连前预取 KnownHostStore.forHost），同 key 必须 Trusted。
        val v = TofuHostKeyVerifier("h", 22, listOf(recorded), now = 1L)
        assertTrue("同 key 重连应信任", v.verify("h", 22, key))
        assertEquals(TofuOutcome.Trusted, v.outcome)
    }

    // 金向量：钉死 java.util.Base64 与 android.util.Base64(NO_WRAP) 字节等价。
    // round-trip 只证明自洽；误改成 getUrlEncoder() 或 withoutPadding() 时它仍过，但已存的指纹会全部判 Mismatch。
    // 这里用固定字节对已知标准 base64：标准字母表（含 + /）、= 填充、单行无尾换行。
    @Test
    fun `java Base64 encoder matches android NO_WRAP semantics (golden vectors)`() {
        val enc = { b: ByteArray -> Base64.getEncoder().encodeToString(b) }
        // 9 字节整除 3 → 无填充；含 0xFF/0x3E/0x3F 覆盖 + / 边界字符。
        assertEquals("AAECPj9A/4B/", enc(byteArrayOf(0, 1, 2, 62, 63, 64, -1, -128, 127)))
        // 4 字节 → 2 个 = 填充（钉死未传 NO_PADDING）。
        assertEquals("AAECPg==", enc(byteArrayOf(0, 1, 2, 62)))
        // 含 0xFF → '/' + 填充。
        assertEquals("AAEC/w==", enc(byteArrayOf(0, 1, 2, -1)))
    }

    // 变更分支：同 keyType 但存的 b64 不符，拒绝。
    @Test
    fun `verify rejects when stored b64 differs`() {
        val key = ecP256Key()
        val v = TofuHostKeyVerifier("h", 22, listOf(kh("ecdsa-sha2-nistp256", b64 = "TAMPERED")), now = 0L)
        assertFalse("同类型但 b64 不符 → 拒绝", v.verify("h", 22, key))
        assertTrue("应记 Mismatch", v.outcome is TofuOutcome.Mismatch)
    }

    // TOFU 恢复路径

    /**
     * Mismatch 之后忘掉该 host:port 的记录，下次连接回到 FirstUse 重新钉扎；服务器合法重装后靠它恢复。
     * verifier 吃的是连接前查好的快照，所以这里用换掉快照来模拟 forget 之后的状态。
     */
    @Test
    fun `forgetting the pinned key turns a mismatch back into first use`() {
        val key = ecP256Key()
        val keyType =
            net.schmizz.sshj.common.KeyType
                .fromKey(key)
                .toString()

        // 钉扎一把别的钥匙（相当于服务器重装前记住的那把）
        val stale = TofuHostKeyVerifier("h", 22, listOf(kh(keyType, b64 = "c3RhbGU=")), now = 0L)
        assertFalse("陈旧钥匙对不上，必须拒绝", stale.verify("h", 22, key))
        assertTrue("且必须判为 Mismatch（不是 FirstUse）", stale.outcome is TofuOutcome.Mismatch)

        // forget 之后该 host:port 快照为空，回到首见
        val afterForget = TofuHostKeyVerifier("h", 22, emptyList(), now = 0L)
        assertTrue("忘掉旧记录后必须能连上", afterForget.verify("h", 22, key))
        assertTrue("且必须是 FirstUse（会重新钉扎新钥匙）", afterForget.outcome is TofuOutcome.FirstUse)
    }

    /** Mismatch 必须带出新旧两把钥匙的 b64，恢复页要靠它算指纹给用户对比。 */
    @Test
    fun `mismatch carries both the expected and the offered key so the UI can show a diff`() {
        val key = ecP256Key()
        val keyType =
            net.schmizz.sshj.common.KeyType
                .fromKey(key)
                .toString()
        val v = TofuHostKeyVerifier("h", 22, listOf(kh(keyType, b64 = "c3RhbGU=")), now = 0L)
        v.verify("h", 22, key)

        val m = v.outcome as TofuOutcome.Mismatch
        assertEquals("expectedB64 应是我们记得的那把", "c3RhbGU=", m.expectedB64)
        assertTrue("offeredB64 应是服务器现在出示的那把（非空且与旧的不同）", m.offeredB64.isNotBlank())
        assertTrue("新旧必须不同——相同就不该判 Mismatch", m.expectedB64 != m.offeredB64)
    }

    /** 存的 b64 是 wire blob 的 base64，能算出 `SHA256:…` 形态的指纹（与 OpenSSH 首次连接提示同格式）。 */
    @Test
    fun `stored base64 converts to an ssh-style sha256 fingerprint`() {
        val key = ecP256Key()
        val blob =
            net.schmizz.sshj.common.Buffer
                .PlainBuffer()
                .putPublicKey(key)
                .compactData
        val b64 = Base64.getEncoder().encodeToString(blob)

        val fp =
            com.ccmonitor.mobile.core.data.crypto.SshWire
                .fingerprintSha256(Base64.getDecoder().decode(b64))
        assertTrue("应是 SHA256: 前缀的标准形态，实际=$fp", fp.startsWith("SHA256:"))
        assertTrue("指纹主体不该为空", fp.removePrefix("SHA256:").isNotBlank())
    }
}
