package com.ccmonitor.mobile.core.rs

/**
 * 手机嵌的 Rust 核心（`libmobcore.so`）在 Kotlin 这一侧的唯一入口：手写 JNI、窄口、走消息。
 *
 * 一条**链路**就是拨号核心的那一条：[open] 交一份拨号请求（JSON，同后端 `link-open` 组好的那一形），之后上行字节经 [send]，
 * 下行字节（先是阶段行 ＋ 一行 ack，再按用法）经 [Sink.data] 回来，收工时回一个 `null`。改 PTY 窗口经 [resize]，收掉经 [close]。
 * 钥匙按编号交进来（[putPassword] · [putPem] · [putKeystore]），拨号请求的 `key_path` 写那个编号。
 *
 * 第一次碰到时才加载那份 .so；JVM 单测不碰它（真连接那一条路才用，单测走假传输）。
 */
object Core {
    init {
        System.loadLibrary("mobcore")
    }

    /** 核心回调 Kotlin 的三个口（在核心的工作线程上调；不许阻塞太久，不许抛）。 */
    interface Sink {
        /** 一条链路的下行字节；`null` ⇒ 这条链路收工了。 */
        fun data(
            handle: Long,
            bytes: ByteArray?,
        )

        /** 用 Keystore 里 [alias] 那把 P-256 对 [data] 签名，回 DER；签不了 ⇒ `null`。 */
        fun sign(
            alias: String,
            data: ByteArray,
        ): ByteArray?

        /** 握手时认下的一把 host key（[key] = 算法 ＋ base64，同 OpenSSH 那一行的前两段）。 */
        fun hostKey(
            host: String,
            port: Int,
            key: String,
        )
    }

    /** 这一份核心的身份那一行（JSON：拨号核心的 ack 协议版本 · 认得的用法）。 */
    external fun version(): String

    /** 记下 [sink]、装上手机那一套（只认第一次）。 */
    external fun init(sink: Sink)

    external fun putPassword(
        id: String,
        password: String,
    )

    external fun putPem(
        id: String,
        pem: ByteArray,
        passphrase: String?,
    )

    /** [point] = P-256 公钥的未压缩点（`04 ‖ X ‖ Y`）。认不出 ⇒ `false`。 */
    external fun putKeystore(
        id: String,
        alias: String,
        point: ByteArray,
    ): Boolean

    /** 一段私钥 PEM（不带口令）⇒ 它的公钥那一行（`<算法> <base64> [comment]`）；认不出 ⇒ `null`。 */
    external fun publicKeyOf(
        pem: ByteArray,
        comment: String,
    ): String?

    /** 开一条链路，回编号；请求读不动 ⇒ 0。 */
    external fun open(request: String): Long

    external fun send(
        handle: Long,
        bytes: ByteArray,
    ): Boolean

    external fun resize(
        handle: Long,
        cols: Int,
        rows: Int,
    )

    external fun close(handle: Long)
}
