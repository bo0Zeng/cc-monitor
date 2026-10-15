package com.ccmonitor.mobile.core.ssh

import java.io.IOException

/**
 * 没够着那一台（解析不了 · TCP 连不上 · 一步进展都没有到期限）：网不通的那一种失败，不是那台拒了（认证 · 钥匙变了）。
 * SSH 那层照样把拨号请求交给共用那条重连循环（它在核心里自己再拨，网回来就接上）。
 */
class HostUnreachedException(
    message: String,
    cause: Throwable? = null,
) : IOException(message, cause)
