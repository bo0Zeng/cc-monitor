package com.ccmonitor.mobile.core.data.crypto

import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.rs.Core
import org.json.JSONObject
import java.security.KeyPairGenerator
import java.security.ProviderException
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.util.Base64

/**
 * 在 Android Keystore 生成 ECDSA-P256（secp256r1）不可导出签名密钥。
 * 私钥不出 TEE，签名经 Keystore。StrongBox 优先（API 28+），不可用回退软件 Keystore。
 * 用 P-256：Keystore 对 Ed25519 的支持不可靠，硬件背书只能 ECDSA-P256/384/521。
 */
object SshKeyGenerator {
    const val ANDROID_KEYSTORE = "AndroidKeyStore"

    data class KeystoreSshKey(
        val alias: String,
        val publicKeyOpenSsh: String,
        val fingerprintSha256: String,
        val strongBoxBacked: Boolean,
    )

    /** 生成并存入 Keystore（alias），返回可贴到 authorized_keys 的 OpenSSH 公钥。 */
    fun generateEcdsaP256InKeystore(
        alias: String,
        comment: String = "",
    ): KeystoreSshKey {
        val (publicKey, strongBox) = generateKeyPair(alias)
        val blob = SshWire.encodeEcdsaP256(publicKey)
        return KeystoreSshKey(
            alias = alias,
            publicKeyOpenSsh = SshWire.ecdsaP256ToOpenSsh(publicKey, comment),
            fingerprintSha256 = SshWire.fingerprintSha256(blob),
            strongBoxBacked = strongBox,
        )
    }

    /**
     * 软件生成的 SSH 密钥（公钥已 OpenSSH 编码、私钥为 OpenSSH PEM，交 CryptoBox 加密落库）。
     * 不用 data class：持 ByteArray，自动生成的 equals 会按引用比较。
     */
    class SoftwareSshKey(
        val publicKeyOpenSsh: String,
        val fingerprintSha256: String,
        val privateKeyOpenSshPem: ByteArray,
    )

    /**
     * 软件生成 Ed25519 身份：在手机嵌的 Rust 核心里生成（`Core.generateEd25519`，与连接时解这把钥匙的是同一份）。
     * 私钥是 OpenSSH PEM（openssh-key-v1，无 passphrase），交上层 CryptoBox 加密落库；指纹由公钥 wire blob 用 SshWire 出。
     * 走软件而非 Keystore：Android Keystore 对 Ed25519 的支持不可靠。
     */
    fun generateEd25519(comment: String = ""): SoftwareSshKey {
        val got = JSONObject(Core.generateEd25519(comment) ?: error(copyText("mobIdentity.gen.failed")))
        val public = got.getString("public")
        val blob = Base64.getDecoder().decode(public.split(' ')[1])
        return SoftwareSshKey(
            publicKeyOpenSsh = public,
            fingerprintSha256 = SshWire.fingerprintSha256(blob),
            privateKeyOpenSshPem = got.getString("pem").toByteArray(),
        )
    }

    /**
     * 返回 (公钥, 是否 StrongBox 背书)。SDK≥28 先试 StrongBox，失败则回退软件 Keystore。
     *
     * 捕 `ProviderException`（`StrongBoxUnavailableException` 的父类）而非仅子类：部分 OEM（三星/Pixel 等）
     * 在 StrongBox key slot 耗尽 / keymaster 内部错误时抛的是**裸 `ProviderException`** 而非
     * `StrongBoxUnavailableException`，只 catch 子类会让硬件身份生成在这些设备上直接崩溃且不回退。
     */
    private fun generateKeyPair(alias: String): Pair<ECPublicKey, Boolean> {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            try {
                return buildKeyPair(alias, strongBox = true) to true
            } catch (_: ProviderException) {
                // StrongBox 不可用 / slot 耗尽 / keymaster 错误 → 回退软件 Keystore
            }
        }
        return buildKeyPair(alias, strongBox = false) to false
    }

    private fun buildKeyPair(
        alias: String,
        strongBox: Boolean,
    ): ECPublicKey {
        val builder =
            KeyGenParameterSpec
                .Builder(alias, KeyProperties.PURPOSE_SIGN)
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                .setDigests(KeyProperties.DIGEST_SHA256)
        if (strongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            builder.setIsStrongBoxBacked(true)
        }
        val kpg = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, ANDROID_KEYSTORE)
        kpg.initialize(builder.build())
        return kpg.generateKeyPair().public as ECPublicKey
    }
}
