package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.data.crypto.SshKeyGenerator
import net.schmizz.sshj.SSHClient
import net.schmizz.sshj.common.Buffer
import net.schmizz.sshj.common.KeyType
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test
import java.util.Base64

/**
 * 生成的 OpenSSH Ed25519 私钥能被 sshj 加载为 ED25519，与 SshConnection 认证时同样调用 `loadKeys(pem, null, null)`。
 * sshj 经传递依赖 net.i2p.crypto:eddsa 支持 ed25519。
 */
class Ed25519SshjLoadTest {
    @Test
    fun generatedEd25519KeyLoadsInSshjAsEd25519() {
        BouncyCastleSetup.ensure() // 与生产一致：SshjConfigFactory.create() 内部也会 ensure()
        val key = SshKeyGenerator.generateEd25519("f33-test")
        val client = SSHClient(SshjConfigFactory.create())
        val kp = client.loadKeys(String(key.privateKeyOpenSshPem), null, null)
        assertEquals(KeyType.ED25519, kp.type)
        assertNotNull("私钥应可用于签名", kp.getPrivate())
        // sshj 加载出的公钥 wire 编码应与生成的一致
        val loadedWire = Buffer.PlainBuffer().putPublicKey(kp.getPublic()).compactData
        val generatedWire = Base64.getDecoder().decode(key.publicKeyOpenSsh.split(" ")[1])
        assertArrayEquals("sshj 公钥应与生成一致", generatedWire, loadedWire)
    }
}
