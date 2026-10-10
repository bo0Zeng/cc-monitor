package com.ccmonitor.mobile.core.data.crypto

import android.content.Context
import com.google.crypto.tink.Aead
import com.google.crypto.tink.KeyTemplates
import com.google.crypto.tink.aead.AeadConfig
import com.google.crypto.tink.integration.android.AndroidKeysetManager

/**
 * Tink AEAD（AES-256-GCM）+ Android Keystore master key 包裹加密落盘。
 * keyset 存于 SharedPreferences，由 Keystore 中的 master key 加密：即使数据库被导出，
 * 没有本机 Keystore 也无法解密。
 */
class KeystoreCryptoBox(
    private val context: Context,
) : CryptoBox {
    private val aead: Aead by lazy { buildAead() }

    private fun buildAead(): Aead {
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

    override fun encrypt(plain: ByteArray): ByteArray = aead.encrypt(plain, ASSOCIATED_DATA)

    override fun decrypt(blob: ByteArray): ByteArray = aead.decrypt(blob, ASSOCIATED_DATA)

    /**
     * 启发式：Tink 密文不以 PEM '-'(0x2D) 或 DER SEQUENCE(0x30) 开头。
     * 用来区分已加密与明文（防御性）。
     */
    override fun isEncrypted(blob: ByteArray): Boolean {
        if (blob.isEmpty()) return false
        val first = blob[0]
        return first != '-'.code.toByte() && first != 0x30.toByte()
    }

    private companion object {
        const val KEYSET_NAME = "aterm_key_keyset"
        const val PREFERENCE_FILE = "aterm_key_keyset_prefs"
        const val MASTER_KEY_URI = "android-keystore://aterm_key_master"
        val ASSOCIATED_DATA = "aterm-private-key".toByteArray()
    }
}
