package com.ccmonitor.mobile.core.data.crypto

/**
 * 静态加密边界。仓库写时 encrypt、读时 decrypt。
 * 生产绑 KeystoreCryptoBox（Tink AEAD + Keystore master key）。
 *
 * 关联数据由调用方给，平常一律是 [rowBinding]（哪张表 · 哪一行）：密文挪到别的行就解不开。
 */
interface CryptoBox {
    fun encrypt(
        plain: ByteArray,
        associatedData: ByteArray,
    ): ByteArray

    fun decrypt(
        blob: ByteArray,
        associatedData: ByteArray,
    ): ByteArray
}

/** 那一行密文的关联数据：表名与行 id。表名都是固定的几个、不含 `/` ⇒ 不会两行拼出同一串。 */
fun rowBinding(
    table: String,
    rowId: String,
): ByteArray = "$table/$rowId".toByteArray(Charsets.UTF_8)
