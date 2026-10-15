package com.ccmonitor.mobile.core.ssh

/** 认证方式。 */
sealed interface AuthMethod {
    data class Password(
        val password: String,
    ) : AuthMethod {
        override fun toString(): String = "Password(****)" // 不让明文密码进日志与 toString
    }

    /** OpenSSH/PEM 私钥字节，可选 passphrase 解密。 */
    class PrivateKey(
        val pem: ByteArray,
        val passphrase: CharArray? = null,
    ) : AuthMethod

    /** 用 Android Keystore alias 下的 ECDSA-P256 密钥签名。 */
    data class KeystoreSigner(
        val alias: String,
    ) : AuthMethod
}

/** 重连策略：断了要不要跟着共用那条重连循环换一条（怎么重连 · 隔多久只在共用那一处）。 */
data class ReconnectPolicy(
    val auto: Boolean = true,
)

/** 一次 SSH 连接所需的配置。 */
data class ConnectionConfig(
    val host: String,
    val port: Int = 22,
    val username: String,
    val auth: AuthMethod,
    // 工作目录与 tmux 不进连接层：由 app 在终端 shell 通道里发初始命令。
    val reconnect: ReconnectPolicy = ReconnectPolicy(),
    // 经跳板机连接（ssh -J）：先连 [jumpVia] 并认证，再经其 direct-tcpip 隧道握手本目标。可嵌套成多跳；null 为直连。
    val jumpVia: ConnectionConfig? = null,
)
