package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.data.crypto.SshWire
import com.ccmonitor.mobile.core.rs.Core
import java.util.Base64

/**
 * 从导入的私钥 PEM 派生 OpenSSH 公钥串与 SHA256 指纹；推送公钥到 `authorized_keys` 前需要它。
 *
 * 解析在手机嵌的 Rust 核心里（`Core.publicKeyOf`：与连接时解这把钥匙的是同一份代码，认得的格式也就是连得上的格式）；
 * 指纹由公钥 wire blob 用 [SshWire.fingerprintSha256] 出。要那份 .so ⇒ 判据在 androidTest。
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
     * 从私钥 PEM 派生公钥。带 passphrase、格式不识别或没有公钥时返回 null（调用方当场拒，不存）。
     */
    fun fromPrivatePem(
        pem: ByteArray,
        comment: String = "",
    ): Derived? {
        val line = Core.publicKeyOf(pem, comment) ?: return null
        val parts = line.split(' ')
        if (parts.size < 2) return null
        val wire = runCatching { Base64.getDecoder().decode(parts[1]) }.getOrNull() ?: return null
        return Derived(line, SshWire.fingerprintSha256(wire), parts[0])
    }
}
