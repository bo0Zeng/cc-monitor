package com.ccmonitor.mobile.core.ssh

import net.schmizz.keepalive.KeepAliveProvider
import net.schmizz.sshj.Config
import net.schmizz.sshj.DefaultConfig

/**
 * sshj Config 工厂。先做 BouncyCastle 替换，再用 sshj 的 [DefaultConfig]。
 * 不用 sshj 的 `AndroidConfig`（它依赖停更的 SpongyCastle）；装上全量 BC 后 DefaultConfig 即可工作。
 */
object SshjConfigFactory {
    fun create(): Config {
        BouncyCastleSetup.ensure()
        return DefaultConfig().withKeepAlive()
    }

    /**
     * 启用 sshj KEEP_ALIVE：用全局请求 `keepalive@openssh.com` 探活，连丢 maxAliveCount（默认 5）次就断开传输，
     * [SshConnection.isConnected] 随之变 false。间隔在连上后按 [ConnectionConfig.keepAliveIntervalSec] 设。
     * 不用 HEARTBEAT：它只发 IGNORE，不判活。
     */
    private fun Config.withKeepAlive(): Config = apply { keepAliveProvider = KeepAliveProvider.KEEP_ALIVE }

    /**
     * KeystoreSigner 认证用的 Config：把 `ecdsa-sha2-nistp256` 的 KeyAlgorithm 原位替换成签名走 AndroidKeyStore 的版本
     * （[keystoreEcdsaKeyAlgorithm]）。只有 KeystoreSigner 连接用它；原位替换保留算法协商顺序。
     */
    fun createForKeystoreSigner(): Config {
        BouncyCastleSetup.ensure()
        val config = DefaultConfig()
        val algos = config.keyAlgorithms.toMutableList()
        val idx = algos.indexOfFirst { it.name == ECDSA_NISTP256 }
        // DefaultConfig 必含该算法；缺了说明 sshj 升级移除了它，宁可报错，也不悄悄插到首位打乱协商顺序。
        require(idx >= 0) { "DefaultConfig 缺 $ECDSA_NISTP256 KeyAlgorithm" }
        algos[idx] = keystoreEcdsaKeyAlgorithm()
        config.keyAlgorithms = algos
        return config.withKeepAlive() // KeystoreSigner 连接也启用 keepalive
    }
}
