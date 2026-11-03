package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.SshPublicKey

/**
 * 从导入的私钥 PEM 派生公钥（那一行 · 算法名 · 指纹）；推送公钥到 `authorized_keys` 前需要它。
 *
 * 全在手机嵌的 Rust 核心里（`Core.publicKeyOf`：与连接时解这把钥匙的是同一份代码，认得的格式也就是连得上的格式；
 * 指纹是拨号核心那一份，与桌面同一个）。要那份 .so ⇒ 判据在 androidTest。
 */
object PublicKeyDeriver {
    /**
     * 从私钥 PEM 派生公钥。带 passphrase、格式不识别或没有公钥时返回 null（调用方当场拒，不存）。
     */
    fun fromPrivatePem(
        pem: ByteArray,
        comment: String = "",
    ): SshPublicKey? = Core.publicKeyOf(pem, comment)?.let { Wire.read<SshPublicKey>(it) }
}
