package com.ccmonitor.mobile.core.rs

/**
 * 手机嵌的 Rust 核心（`libmobcore.so`）在 Kotlin 这一侧的唯一入口：手写 JNI、窄口、走消息。
 *
 * 第一次碰到时才加载那份 .so（`System.loadLibrary`）；JVM 单测不碰它（真连接那一条路才用，单测走假传输）。
 */
object Core {
    init {
        System.loadLibrary("mobcore")
    }

    /** 这一份核心的身份那一行（JSON：拨号核心的 ack 协议版本 · 认得的用法）。 */
    external fun version(): String
}
