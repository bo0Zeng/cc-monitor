package com.ccmonitor.mobile.core.data.crypto

/**
 * 静态加密边界。IdentityRepository 写时 encrypt、读时 decrypt。
 * 生产绑 KeystoreCryptoBox（Tink AEAD + Keystore master key）。
 */
interface CryptoBox {
    fun encrypt(plain: ByteArray): ByteArray

    fun decrypt(blob: ByteArray): ByteArray
}
