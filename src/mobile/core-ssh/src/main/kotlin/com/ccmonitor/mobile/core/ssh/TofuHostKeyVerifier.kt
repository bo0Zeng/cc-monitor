package com.ccmonitor.mobile.core.ssh

import net.schmizz.sshj.common.Buffer
import net.schmizz.sshj.common.KeyType
import net.schmizz.sshj.transport.verification.HostKeyVerifier
import java.security.PublicKey
import java.util.Base64

/** host-key 校验结果（TOFU）。 */
sealed interface TofuOutcome {
    data object Trusted : TofuOutcome

    data class FirstUse(
        val knownHost: KnownHostRecord,
    ) : TofuOutcome

    data class Mismatch(
        val keyType: String,
        val expectedB64: String,
        val offeredB64: String,
    ) : TofuOutcome
}

/**
 * host key 变更或降级（疑似中间人或服务器重装）时拒绝连接。
 * 带新旧指纹，恢复页才能给用户对比，判断是自己重装的还是有人冒充。
 */
class HostKeyChangedException(
    val host: String,
    val port: Int,
    val keyType: String,
    /** 已记住的那把密钥的指纹（`SHA256:…`）；空串表示算不出。 */
    val expectedFingerprint: String = "",
    /** 现在这台服务器出示的指纹。 */
    val offeredFingerprint: String = "",
    cause: Throwable? = null,
) : Exception("主机密钥已变更（$host:$port，$keyType）—— 可能是中间人攻击或服务器重装。已拒绝连接。", cause)

/**
 * TOFU（Trust-On-First-Use）host key 校验。
 *
 * [known] 是该 host:port 已存密钥的内存快照，连接前用 [KnownHostStore.forHost] 查好；[verify] 在 sshj 握手线程同步调用，
 * 不碰存储。首见接受并记 [TofuOutcome.FirstUse]（调用方认证成功后 [KnownHostStore.record]）；
 * 同类型且相同为 [TofuOutcome.Trusted]；不同则拒绝并记 [TofuOutcome.Mismatch]。
 */
class TofuHostKeyVerifier(
    private val host: String,
    private val port: Int,
    private val known: List<KnownHostRecord>,
    private val now: Long,
) : HostKeyVerifier {
    @Volatile
    var outcome: TofuOutcome? = null
        private set

    override fun verify(
        hostname: String,
        port: Int,
        key: PublicKey,
    ): Boolean {
        val keyType = KeyType.fromKey(key).toString()
        // 用 java.util.Base64（标准字母表、带填充、单行），与已存记录逐字节一致，也能在纯 JVM 测试里跑。
        val b64 =
            Base64.getEncoder().encodeToString(
                Buffer.PlainBuffer().putPublicKey(key).compactData,
            )
        val existing = known.firstOrNull { it.keyType == keyType }
        return when {
            // 同类型且相同：信任
            existing != null && existing.publicKeyBase64 == b64 -> {
                outcome = TofuOutcome.Trusted
                true
            }
            // 同类型但变了：拒绝
            existing != null -> {
                outcome = TofuOutcome.Mismatch(keyType, existing.publicKeyBase64, b64)
                false
            }
            // 该主机完全没有记录：首见，接受
            known.isEmpty() -> {
                outcome = TofuOutcome.FirstUse(KnownHostRecord(host, this.port, keyType, b64, now))
                true
            }
            // 该主机已有其它类型的密钥，却来了新类型：疑似降级攻击，拒绝（OpenSSH 语义）
            else -> {
                outcome = TofuOutcome.Mismatch(keyType, "(该主机已有其它类型密钥，无 $keyType 记录)", b64)
                false
            }
        }
    }

    /**
     * 返回已钉扎的 host key 算法名（如 `ssh-ed25519`），让 sshj 协商时优先这些类型（OpenSSH `HostKeyAlgorithms` 语义）。
     *
     * 不能返回空：返回空时 sshj 按默认偏好协商，服务器若在首连后新增了更高偏好的类型，
     * 重连会请求那个新类型，库里没有记录，[verify] 走降级分支把合法重连当攻击拒掉。真正的密钥轮换仍会触发 Mismatch。
     */
    override fun findExistingAlgorithms(
        hostname: String,
        port: Int,
    ): List<String> = known.map { it.keyType }.distinct()
}
