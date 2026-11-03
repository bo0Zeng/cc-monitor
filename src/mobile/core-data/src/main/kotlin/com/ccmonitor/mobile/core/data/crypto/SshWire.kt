package com.ccmonitor.mobile.core.data.crypto

import java.math.BigInteger
import java.security.interfaces.ECPublicKey

/**
 * Keystore 那把公钥交核心之前的那一步：P-256 公钥 ⇒ 未压缩点。公钥那一行 · 指纹 · 握手都在核心里（`Core.publicKeyOfPoint` · `Core.putKeystore`）。
 * 纯函数，无 Keystore 依赖。
 */
object SshWire {
    /** P-256 公钥 ⇒ 未压缩点 `04 ‖ X ‖ Y`（各 32 字节，左补零）。 */
    fun uncompressedPoint(pub: ECPublicKey): ByteArray {
        fun fixed(n: BigInteger): ByteArray {
            val raw = n.toByteArray().dropWhile { it == 0.toByte() }.toByteArray()
            return ByteArray(P256_COORD - raw.size) + raw
        }
        return byteArrayOf(4) + fixed(pub.w.affineX) + fixed(pub.w.affineY)
    }

    private const val P256_COORD = 32
}
