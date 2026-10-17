package com.ccmonitor.mobile.core.data.crypto

import android.content.Context
import com.google.crypto.tink.Aead
import com.google.crypto.tink.KeyTemplates
import com.google.crypto.tink.aead.AeadConfig
import com.google.crypto.tink.integration.android.AndroidKeysetManager

/**
 * Tink AEAD（AES-256-GCM）+ Android Keystore master key 包裹加密落盘。
 * keyset 存于 SharedPreferences，由 Keystore 中的 master key 加密：即使数据库被导出，
 * 没有本机 Keystore 也无法解密。关联数据由调用方给（那一行，见 [rowBinding]）。
 * 内部那个构造把 AEAD 换掉：JVM 单测用一把临时的 Tink AEAD，别的照生产走。
 */
class KeystoreCryptoBox internal constructor(
    aead: Lazy<Aead>,
) : CryptoBox {
    constructor(context: Context) : this(lazy { keystoreAead(context) })

    private val primitive: Aead by aead

    override fun encrypt(
        plain: ByteArray,
        associatedData: ByteArray,
    ): ByteArray = primitive.encrypt(plain, associatedData)

    override fun decrypt(
        blob: ByteArray,
        associatedData: ByteArray,
    ): ByteArray = primitive.decrypt(blob, associatedData)

    private companion object {
        fun keystoreAead(context: Context): Aead {
            AeadConfig.register()
            val keysetHandle =
                AndroidKeysetManager
                    .Builder()
                    .withSharedPref(context, KEYSET_NAME, PREFERENCE_FILE)
                    .withKeyTemplate(KeyTemplates.get("AES256_GCM"))
                    .withMasterKeyUri(MASTER_KEY_URI)
                    .build()
                    .keysetHandle
            return keysetHandle.getPrimitive(Aead::class.java)
        }

        const val KEYSET_NAME = "ccm_key_keyset"
        const val PREFERENCE_FILE = "ccm_key_keyset_prefs"
        const val MASTER_KEY_URI = "android-keystore://ccm_key_master"
    }
}
