package com.ccmonitor.mobile.testing

import com.ccmonitor.mobile.core.data.crypto.CryptoBox

/** 直通的 [CryptoBox]：落的就是明文，单测里读写仓库用（真加密是 KeystoreCryptoBox，设备侧测）。 */
object PlainCryptoBox : CryptoBox {
    override fun encrypt(plain: ByteArray): ByteArray = plain

    override fun decrypt(blob: ByteArray): ByteArray = blob
}
