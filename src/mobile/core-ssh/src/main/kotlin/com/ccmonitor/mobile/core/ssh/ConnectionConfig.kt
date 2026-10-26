package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.wire.gen.DialRequest

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

/**
 * 一次 SSH 连接所需的配置：拨号核心照机器表组好的那一份请求（地址展开 · 多跳跳板都在里面，`Core.dialPlan`）＋ 路上每一台的认证。
 * 请求里每一跳的 `keyPath` 是那一台的编号（机器表交核心时写的），[auths] 照它给认证；连的那一刻换成核心的钥匙编号。
 * 工作目录与 tmux 不进连接层：由 app 在终端 shell 通道里发初始命令。
 */
data class ConnectionConfig(
    val request: DialRequest,
    val auths: Map<String, AuthMethod>,
    val reconnect: ReconnectPolicy = ReconnectPolicy(),
) {
    /** 目标那一台的首选地址（日志 · 「钥匙变了」页说的是它）。 */
    val host: String get() = request.host

    val port: Int get() = request.port
}
