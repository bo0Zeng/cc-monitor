package com.ccmonitor.mobile.core.data.crypto

/**
 * 静态加密边界。IdentityRepository 写时 encrypt、读时 decrypt。
 * 生产绑 KeystoreCryptoBox（Tink AEAD + Keystore master key）。
 */
interface CryptoBox {
    fun encrypt(plain: ByteArray): ByteArray

    fun decrypt(blob: ByteArray): ByteArray

    fun isEncrypted(blob: ByteArray): Boolean
}

/** 直通，不加密。只给测试用。 */
class NoopCryptoBox : CryptoBox {
    override fun encrypt(plain: ByteArray): ByteArray = plain

    override fun decrypt(blob: ByteArray): ByteArray = blob

    override fun isEncrypted(blob: ByteArray): Boolean = false
}
