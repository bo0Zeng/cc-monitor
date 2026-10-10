package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.data.crypto.SshWire
import net.schmizz.sshj.SSHClient
import net.schmizz.sshj.common.Buffer
import net.schmizz.sshj.common.KeyType
import java.util.Base64

/**
 * 从导入的私钥 PEM 派生 OpenSSH 公钥串与 SHA256 指纹；推送公钥到 `authorized_keys` 前需要它。
 *
 * 用 sshj 的 `loadKeys` 解析主流私钥格式（OpenSSH-v1 / PKCS#8 / PKCS#1 / SEC1；Ed25519 / ECDSA / RSA），
 * 再取公钥 wire blob，用 [SshWire.fingerprintSha256] 出指纹。纯 JVM，无 Android 依赖。
 */
object PublicKeyDeriver {
    data class Derived(
        /** OpenSSH 一行：`<type> <base64> [comment]`，可直接贴进 authorized_keys。 */
        val openSsh: String,
        /** `SHA256:<base64-no-pad>`。 */
        val fingerprintSha256: String,
        /** SSH 算法名，如 `ssh-ed25519` / `ecdsa-sha2-nistp256` / `ssh-rsa`。 */
        val keyType: String,
    )

    /**
     * 从私钥 PEM 派生公钥。带 passphrase、格式不识别或没有公钥时返回 null，调用方退回空串；
     * 派生失败不能让导入失败，私钥仍可用于连接。
     */
    fun fromPrivatePem(
        pem: ByteArray,
        comment: String = "",
    ): Derived? =
        runCatching {
            SSHClient(SshjConfigFactory.create()).use { client ->
                val kp = client.loadKeys(String(pem, Charsets.UTF_8), null, null)
                val pub = kp.public ?: return null
                val wire = Buffer.PlainBuffer().putPublicKey(pub).compactData
                val type = KeyType.fromKey(pub).toString()
                val b64 = Base64.getEncoder().encodeToString(wire)
                val openSsh = "$type $b64" + if (comment.isNotBlank()) " $comment" else ""
                Derived(openSsh, SshWire.fingerprintSha256(wire), type)
            }
        }.getOrNull()
}
