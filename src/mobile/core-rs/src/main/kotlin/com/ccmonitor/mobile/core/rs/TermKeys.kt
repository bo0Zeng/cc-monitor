package com.ccmonitor.mobile.core.rs

/**
 * 送进终端的字节由核心算（`core-rs/rust/keys.rs` 调 `cmd_wire::keys`，后端托管终端送键是同一张表）：Kotlin 这一侧只把一下写成记号交过去。
 *
 * 一下的**记号**：`t<字>`（照原样打的一段字）· `s<修饰>:<键>`（特殊键）· `c<修饰>:<字>`（带修饰的一个字）。
 * 修饰是 `ctrl` · `alt` · `shift` 照这个先后的逗号连写，可空；键是库里存的线上名（`up` · `pgdn` · `f5` …）。
 * 第一次碰到时才加载那份 .so；JVM 单测加载同一个 crate 的宿主编译（根 `build.gradle.kts` 指好 `java.library.path`）。
 */
object TermKeys {
    init {
        System.loadLibrary("mobcore")
    }

    /** 修饰的线上名，照记号里的先后。 */
    val MOD_ORDER: List<String> = listOf("ctrl", "alt", "shift")

    /** 照原样打的一段字。 */
    fun text(s: String): String = "t$s"

    /** 一颗特殊键（[key] 是线上名），[mods] 是修饰的线上名（先后不论）。 */
    fun special(
        key: String,
        mods: Collection<String>,
    ): String = "s${modWords(mods)}:$key"

    /** 带修饰的一个字（[char] 是一个码点）。 */
    fun chord(
        char: String,
        mods: Collection<String>,
    ): String = "c${modWords(mods)}:$char"

    private fun modWords(mods: Collection<String>) = MOD_ORDER.filter { it in mods }.joinToString(",")

    /** 几下（记号）⇒ 送进终端的字节，一下接一下拼起来；有一个记号核心读不出 ⇒ null。 */
    external fun bytes(tokens: Array<String>): ByteArray?
}
