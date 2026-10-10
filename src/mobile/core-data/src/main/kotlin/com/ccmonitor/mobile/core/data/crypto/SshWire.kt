package com.ccmonitor.mobile.core.data.crypto

import java.security.MessageDigest
import java.security.interfaces.ECPublicKey
import java.util.Base64

/**
 * SSH 公钥线格式编码（RFC 4253 / 4716）。
 * 纯函数，无 Keystore 依赖，可独立测试。
 */
object SshWire {
    /** ECPublicKey(secp256r1) → OpenSSH 一行：`ecdsa-sha2-nistp256 <base64> [comment]`。 */
    fun ecdsaP256ToOpenSsh(
        key: ECPublicKey,
        comment: String = "",
    ): String {
        val blob = encodeEcdsaP256(key)
        val b64 = Base64.getEncoder().encodeToString(blob)
        return "ecdsa-sha2-nistp256 $b64" + if (comment.isNotEmpty()) " $comment" else ""
    }

    /** SSH wire blob：string("ecdsa-sha2-nistp256") + string("nistp256") + string(0x04||X||Y)。 */
    fun encodeEcdsaP256(key: ECPublicKey): ByteArray {
        val keyType = "ecdsa-sha2-nistp256".toByteArray()
        val curveName = "nistp256".toByteArray()
        val x =
            key.w.affineX
                .toByteArray()
                .padStartTo(32)
        val y =
            key.w.affineY
                .toByteArray()
                .padStartTo(32)
        val point = ByteArray(1 + 32 + 32)
        point[0] = 0x04 // 未压缩点
        x.copyInto(point, 1)
        y.copyInto(point, 33)

        val buf = ByteArray(4 + keyType.size + 4 + curveName.size + 4 + point.size)
        var off = 0
        off = writeBytes(buf, off, keyType)
        off = writeBytes(buf, off, curveName)
        writeBytes(buf, off, point)
        return buf
    }

    /** Ed25519 公钥 wire blob：string("ssh-ed25519") + string(pub 32B)。 */
    fun encodeEd25519(pub: ByteArray): ByteArray {
        val keyType = "ssh-ed25519".toByteArray()
        val buf = ByteArray(4 + keyType.size + 4 + pub.size)
        var off = 0
        off = writeBytes(buf, off, keyType)
        writeBytes(buf, off, pub)
        return buf
    }

    /** Ed25519 公钥 → OpenSSH 一行：`ssh-ed25519 <base64> [comment]`。 */
    fun ed25519ToOpenSsh(
        pub: ByteArray,
        comment: String = "",
    ): String {
        val b64 = Base64.getEncoder().encodeToString(encodeEd25519(pub))
        return "ssh-ed25519 $b64" + if (comment.isNotEmpty()) " $comment" else ""
    }

    /** OpenSSH SHA256 指纹：`SHA256:<base64-no-pad>`，对公钥 wire blob 取 SHA-256。 */
    fun fingerprintSha256(publicKeyBlob: ByteArray): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(publicKeyBlob)
        val b64 = Base64.getEncoder().withoutPadding().encodeToString(digest)
        return "SHA256:$b64"
    }

    private fun writeBytes(
        buf: ByteArray,
        offset: Int,
        data: ByteArray,
    ): Int {
        buf[offset] = (data.size ushr 24 and 0xFF).toByte()
        buf[offset + 1] = (data.size ushr 16 and 0xFF).toByte()
        buf[offset + 2] = (data.size ushr 8 and 0xFF).toByte()
        buf[offset + 3] = (data.size and 0xFF).toByte()
        data.copyInto(buf, offset + 4)
        return offset + 4 + data.size
    }

    /** BigInteger.toByteArray 可能带符号前导 0 或不足位 → 规整到固定 length（左侧补 0 / 去前导 0）。 */
    private fun ByteArray.padStartTo(length: Int): ByteArray {
        if (size == length) return this
        if (size > length) return copyOfRange(size - length, size) // 去掉符号前导字节
        val padded = ByteArray(length)
        copyInto(padded, length - size)
        return padded
    }
}
