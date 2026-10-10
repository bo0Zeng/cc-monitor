package com.ccmonitor.mobile.ui.identity

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.data.crypto.SshKeyGenerator
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ssh.PublicKeyDeriver
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import java.util.UUID

/** 身份生成/导入的操作反馈状态，失败不静默。 */
sealed interface IdentityOp {
    data object Idle : IdentityOp

    data object Working : IdentityOp

    data class Done(
        val message: String,
    ) : IdentityOp

    data class Failed(
        val message: String,
    ) : IdentityOp
}

class IdentityViewModel(
    private val repo: IdentityRepository,
    private val io: CoroutineDispatcher = Dispatchers.IO, // 可注入，便于 JVM 测试
) : ViewModel() {
    val identities: StateFlow<List<Identity>> =
        repo.observeAll().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // 三个入口的进度/结果：UI 显示 loading 与成功/失败提示（keystore 硬件异常、PEM 解析失败都可见）。
    private val _op = MutableStateFlow<IdentityOp>(IdentityOp.Idle)
    val op: StateFlow<IdentityOp> = _op.asStateFlow()

    /** UI 消费完一次终态（Done/Failed）后复位，避免重组时重复弹提示。 */
    fun ackOp() {
        _op.value = IdentityOp.Idle
    }

    /** 包裹一次身份写操作：Working → Done(successMsg) / Failed(异常信息)。 */
    @Suppress("TooGenericExceptionCaught") // 任何失败（keystore 硬件异常/PEM 解析/落库）都要变成可见的 Failed
    private fun runOp(
        successMsg: String,
        block: suspend () -> Unit,
    ) = viewModelScope.launch(io) {
        _op.value = IdentityOp.Working
        _op.value =
            try {
                block()
                IdentityOp.Done(successMsg)
            } catch (e: Exception) {
                IdentityOp.Failed(e.message ?: e.toString())
            }
    }

    /** 生成 ECDSA-P256 Keystore 不可导出密钥并存为身份（私钥留 Keystore，不落库）。 */
    fun generateEcdsa(label: String) =
        runOp("已生成硬件密钥") {
            val alias = "aterm-key-" + UUID.randomUUID()
            val key = SshKeyGenerator.generateEcdsaP256InKeystore(alias, comment = label.ifBlank { "aterm" })
            repo.saveKeystoreBacked(
                Identity(
                    id = alias,
                    label = label.ifBlank { "key" },
                    keyType = "ecdsa-p256",
                    privateKeyEnc = ByteArray(0),
                    publicKeyOpenSsh = key.publicKeyOpenSsh,
                    fingerprintSha256 = key.fingerprintSha256,
                    createdAt = System.currentTimeMillis(),
                    keystoreAlias = alias,
                ),
            )
        }

    /** 软件生成 Ed25519 身份（私钥 OpenSSH PEM 经 CryptoBox 加密落库，走 imported/PrivateKey 路径）。 */
    fun generateEd25519(label: String) =
        runOp("已生成 Ed25519 密钥") {
            val key = SshKeyGenerator.generateEd25519(comment = label.ifBlank { "aterm" })
            repo.save(
                Identity(
                    id = "ed25519-" + UUID.randomUUID(),
                    label = label.ifBlank { "ed25519" },
                    keyType = "ed25519",
                    privateKeyEnc = ByteArray(0),
                    publicKeyOpenSsh = key.publicKeyOpenSsh,
                    fingerprintSha256 = key.fingerprintSha256,
                    createdAt = System.currentTimeMillis(),
                ),
                plainPrivateKey = key.privateKeyOpenSshPem,
            )
        }

    /** 导入已有私钥（route A：CryptoBox 加密落盘）。pem = OpenSSH/PEM 私钥字节。 */
    fun importPrivateKey(
        label: String,
        pem: ByteArray,
    ) = runOp("已导入私钥") {
        // 从 PEM 派生公钥与指纹。派生失败（带 passphrase/格式不识别）回退空串，身份照样导入：
        // 私钥仍可用于连接，只是「复制公钥」为空。keyType 保持 "imported"。
        val derived = PublicKeyDeriver.fromPrivatePem(pem, comment = label.ifBlank { "imported" })
        repo.save(
            Identity(
                id = "import-" + UUID.randomUUID(),
                label = label.ifBlank { "imported" },
                keyType = "imported",
                privateKeyEnc = ByteArray(0),
                publicKeyOpenSsh = derived?.openSsh ?: "",
                fingerprintSha256 = derived?.fingerprintSha256 ?: "",
                createdAt = System.currentTimeMillis(),
            ),
            plainPrivateKey = pem,
        )
    }

    fun delete(identity: Identity) = viewModelScope.launch(io) { repo.delete(identity) }
}
