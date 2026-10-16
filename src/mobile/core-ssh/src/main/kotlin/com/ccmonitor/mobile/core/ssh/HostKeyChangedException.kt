package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.copy.copyText

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
) : Exception(copyText("mobSsh.hostKey.changed", "target" to "$host:$port"), cause)
