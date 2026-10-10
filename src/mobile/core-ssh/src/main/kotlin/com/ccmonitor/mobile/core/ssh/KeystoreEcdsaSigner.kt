package com.ccmonitor.mobile.core.ssh

import com.hierynomus.sshj.key.KeyAlgorithms
import net.schmizz.sshj.common.Factory
import net.schmizz.sshj.common.KeyType
import net.schmizz.sshj.signature.Signature
import net.schmizz.sshj.signature.SignatureECDSA
import net.schmizz.sshj.userauth.keyprovider.KeyProvider
import java.security.KeyStore
import java.security.PrivateKey
import java.security.PublicKey

private const val ANDROID_KEYSTORE = "AndroidKeyStore"
internal const val ECDSA_NISTP256 = "ecdsa-sha2-nistp256" // 与 SshjConfigFactory 共用，防算法名两处不一致
private const val ECDSA_JCA_ALGO = "SHA256withECDSA"

/**
 * 用 Android Keystore alias 下不可导出的 ECDSA-P256 私钥做 SSH publickey 认证的 sshj [KeyProvider]。
 * 私钥不出 TEE，只把句柄交给 [KeystoreEcdsaP256Signature] 在 Keystore 内签名。
 * 公钥取自 Keystore 自签证书（生成密钥时一并落的证书）。
 */
class KeystoreEcdsaSigner(
    private val alias: String,
) : KeyProvider {
    private val keystore: KeyStore by lazy { KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) } }

    override fun getType(): KeyType = KeyType.ECDSA256

    override fun getPublic(): PublicKey =
        keystore.getCertificate(alias)?.publicKey ?: error("AndroidKeyStore 无 alias 证书: $alias")

    override fun getPrivate(): PrivateKey =
        (keystore.getKey(alias, null) as? PrivateKey) ?: error("AndroidKeyStore 无 alias 私钥: $alias")
}

/**
 * `ecdsa-sha2-nistp256` 的 sshj [Signature]：签名走 AndroidKeyStore，verify 与 encode 用标准实现。
 *
 * sshj 的 [SignatureECDSA] 强制 BouncyCastle provider，BC 不能用不可导出的 key 句柄签名（`initSign` 抛 InvalidKeyException），
 * 所以签名用不指定 provider 的 `java.security.Signature.getInstance("SHA256withECDSA")`，由 JCA 按 key 路由到 AndroidKeyStore。
 *
 * - `sign()` 返回原始 DER（sshj 的认证层会调 `encode(sign())`）。
 * - `encode()` 用标准实现把 DER 转成 SSH 格式（`mpint r ‖ mpint s`）。
 * - host key 校验（`initVerify` / `verify`）委托标准实现，那是普通公钥，BC 可用。
 */
internal class KeystoreEcdsaP256Signature : Signature {
    private val standard = SignatureECDSA(ECDSA_JCA_ALGO, ECDSA_NISTP256) // verify 与 encode（DER → SSH）
    private var androidSigner: java.security.Signature? = null // 只有签名走 AndroidKeyStore

    override fun getSignatureName(): String = standard.signatureName

    override fun initVerify(publicKey: PublicKey) = standard.initVerify(publicKey)

    override fun initSign(privateKey: PrivateKey) {
        // getInstance 不指定 provider，拿到延迟选择的 Signature；initSign 时 BC 抛 InvalidKeyException，JCA 才回退到 AndroidKeyStore。
        // 注意：不要显式传 provider（如 "BC"），那会绑死 BC，initSign 直接失败。
        val signer = java.security.Signature.getInstance(ECDSA_JCA_ALGO)
        signer.initSign(privateKey)
        androidSigner = signer
    }

    override fun update(h: ByteArray) {
        val s = androidSigner
        if (s != null) s.update(h) else standard.update(h)
    }

    override fun update(
        h: ByteArray,
        off: Int,
        len: Int,
    ) {
        val s = androidSigner
        if (s != null) s.update(h, off, len) else standard.update(h, off, len)
    }

    override fun sign(): ByteArray = (androidSigner ?: error("initSign 未调用")).sign() // 原始 DER

    override fun encode(signatureBlob: ByteArray): ByteArray = standard.encode(signatureBlob) // DER → SSH

    override fun verify(sig: ByteArray): Boolean = standard.verify(sig)
}

/** 产出 [KeystoreEcdsaP256Signature] 的 sshj signature 工厂。 */
internal class KeystoreEcdsaSignatureFactory : Factory.Named<Signature> {
    override fun getName(): String = ECDSA_NISTP256

    override fun create(): Signature = KeystoreEcdsaP256Signature()
}

/**
 * `ecdsa-sha2-nistp256` 的 KeyAlgorithm 工厂：公钥处理用标准 [KeyType.ECDSA256]，只把签名换成 [KeystoreEcdsaSignatureFactory]。
 * 注入连接级 Config 的 keyAlgorithms。
 */
internal fun keystoreEcdsaKeyAlgorithm(): KeyAlgorithms.Factory =
    KeyAlgorithms.Factory(ECDSA_NISTP256, KeystoreEcdsaSignatureFactory(), KeyType.ECDSA256)
