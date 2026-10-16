package com.ccmonitor.mobile.core.rs

/**
 * 手机嵌的 Rust 核心（`libmobcore.so`）在 Kotlin 这一侧的唯一入口：手写 JNI、窄口、走消息。
 *
 * 一条**链路**就是拨号核心的那一条：[open] 交一份拨号请求（JSON，同后端 `link-open` 组好的那一形），之后上行字节经 [send]，
 * 下行字节（先是阶段行 ＋ 一行 ack，再按用法）经 [Sink.data] 回来，收工时回一个 `null`。改 PTY 窗口经 [resize]，收掉经 [close]。
 * 钥匙交进来回一个编号（[putPassword] · [putPem] · [putKeystore]，同样的一把只存一份），拨号请求的 `key_path` 写那个编号；
 * 每交一次 ＝ 握一份，[holdKey] 多握 / 放掉一份，最后一份放掉核心就删了它。
 *
 * 一台的**常驻流**（重连 · 握手 · 版本门 · 在看名单 · 一问一答，桌面同一份 `resident-client`）整条在核心里：[residentOpen] 看一台，
 * [residentSsh] 交 SSH 那层此刻的拨号请求，[residentCall] 问一句，[residentWatch] 报在看哪条，[residentKick] 叫它马上再接，[residentClose] 不看了；
 * 它交的事经 [Sink.resident] 回来（`state` · `frame` · `watch_from` · `ssh` · `reply` · `deploy`）。给一台装 / 更新后端经 [deploy]。
 *
 * 第一次碰到时才加载那份 .so；JVM 单测不碰它（真连接那一条路才用，单测走假传输）。
 */
object Core {
    init {
        System.loadLibrary("mobcore")
    }

    /** 核心回调 Kotlin 的四个口（在核心的工作线程上调；不许阻塞太久，不许抛）。 */
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

        /** 一台常驻流交的事：[kind] 是 `state` · `frame` · `watch_from` · `reply`，[body] 是那一段文本（JSON 或原样一帧）。 */
        fun resident(
            handle: Long,
            kind: String,
            body: String,
        )
    }

    /** 这一份核心的身份那一行（JSON：拨号核心的 ack 协议版本 · 认得的用法）。 */
    external fun version(): String

    /** 记下 [sink]、装上手机那一套（只认第一次）。 */
    external fun init(sink: Sink)

    /** 握一份密码，回它的编号；读不动 ⇒ `null`。 */
    external fun putPassword(password: String): String?

    /** 握一份私钥 PEM（[passphrase] 是它的口令），回编号；读不动 ⇒ `null`。 */
    external fun putPem(
        pem: ByteArray,
        passphrase: String?,
    ): String?

    /** 握一份 Keystore 钥匙（[point] = P-256 公钥的未压缩点 `04 ‖ X ‖ Y`），回编号；认不出那个点 ⇒ `null`。 */
    external fun putKeystore(
        alias: String,
        point: ByteArray,
    ): String?

    /** 编号 [id] 那把钥匙多握一份（[holding]）/ 放掉一份；编号不在 ⇒ `false`。 */
    external fun holdKey(
        id: String,
        holding: Boolean,
    ): Boolean

    /** 一段私钥 PEM（不带口令）⇒ 它的公钥那一行（`<算法> <base64> [comment]`）；认不出 ⇒ `null`。 */
    external fun publicKeyOf(
        pem: ByteArray,
        comment: String,
    ): String?

    /** 现生成一把软件 Ed25519：JSON `{pem, public}`（私钥 openssh-key-v1 无口令 · 公钥那一行）；生成不了 ⇒ `null`。 */
    external fun generateEd25519(comment: String): String?

    /** 一段 `~/.ssh/config` 文本 ⇒ 可导入主机的 JSON 数组（`alias` · `host_name` · `port` · `user` · `identity_file`）。 */
    external fun parseSshConfig(text: String): String

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

    /** 看一台：[machine] 是它在屏上的名字（句子里称呼它）· [mine] 手上这一版的身份（版本门的对照物）· [tz] 看的这一台的时区。回编号。 */
    external fun residentOpen(
        machine: String,
        mine: String?,
        tz: String?,
    ): Long

    /** SSH 那层此刻的连接（拨号请求 JSON，同 [open] 那一形，不带用法）；`null` ⇒ 没通。请求读不动 ⇒ `false`。 */
    external fun residentSsh(
        handle: Long,
        request: String?,
    ): Boolean

    /** 问一句（流没通就先等它接上，等的那一段也算在 [withinMs] 里）；回问答编号，结局经 [Sink.resident] 的 `reply` 回来。这台不在 ⇒ 0。 */
    external fun residentCall(
        handle: Long,
        cmd: String,
        args: String,
        withinMs: Long,
    ): Long

    /** 多一处（[holding]）/ 少一处在看 [sid] 这条会话。 */
    external fun residentWatch(
        handle: Long,
        sid: String,
        holding: Boolean,
    )

    /** [retry] ⇒ ［重试］：断开这一条、马上整份重来；否则（回前台 · 网络变了）断着才马上再接。 */
    external fun residentKick(
        handle: Long,
        retry: Boolean,
    )

    external fun residentClose(handle: Long)

    /**
     * 给那台装 / 更新后端（判定与做法是桌面后端同一份 `deploy`）：[slots] = 包里那几格解到的文件
     * （JSON `[{os, arch, id, path}]`）。走到哪 · 结局 · 失败经 [Sink.resident] 的 `deploy` 那一种回来（话是核心写好的）。
     * 那台不在 · 正在装 ⇒ `false`。
     */
    external fun deploy(
        handle: Long,
        slots: String,
    ): Boolean
}
