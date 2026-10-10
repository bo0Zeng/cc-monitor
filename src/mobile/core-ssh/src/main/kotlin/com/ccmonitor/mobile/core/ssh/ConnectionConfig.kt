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

/** 重连策略。 */
data class ReconnectPolicy(
    val auto: Boolean = true,
    val onNetworkChange: Boolean = true,
    val maxAttempts: Int = 0, // 0 = 无限
    val baseDelayMs: Long = 2_000,
    val maxDelayMs: Long = 30_000,
)

/** 一次 SSH 连接所需的配置。 */
data class ConnectionConfig(
    val host: String,
    val port: Int = 22,
    val username: String,
    val auth: AuthMethod,
    // 工作目录与 tmux 不进连接层：由 app 在终端 shell 通道里发初始命令。
    val reconnect: ReconnectPolicy = ReconnectPolicy(),
    val connectTimeoutMs: Int = 15_000,
    // 整条握手（TCP、banner/KEX、认证）的看门狗上限：超时仍无任一目标地址认证成功，就强关在飞 transport 报超时。
    // 与只管 TCP 的 [connectTimeoutMs] 不同：黑洞对端连上 TCP 后 banner 读会无限阻塞，sshj 没有这一级超时。
    // 默认放宽到 45s，长于 sshj 内部跳板约 30s 的等待；manager 另以 connectTimeoutMs + 15s 为下限。
    val handshakeTimeoutMs: Long = 45_000,
    // SSH keepalive 间隔（秒），连上后启用，用来发现静默死掉的对端（半开 TCP 没有 FIN）。
    // sshj 默认连丢 5 次才断，死对端约 5 × interval 内被发现。
    val keepAliveIntervalSec: Int = 15,
    // 经跳板机连接（ssh -J）：先连 [jumpVia] 并认证，再经其 direct-tcpip 隧道握手本目标。可嵌套成多跳；null 为直连。
    val jumpVia: ConnectionConfig? = null,
)
