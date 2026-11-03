package com.ccmonitor.mobile.core.copy

/**
 * 手机取文的唯一入口：经 JNI 调手机那份 `.so` 里的 `copy-core::copy_text`（同桌面 Rust 那一份，表编译期内嵌）。
 *
 * 键必须是字面量、参数是 `"名" to 值` 的对，名与表里 `args` 两向相等（`copy-table.vitest.ts` 按调用点判）；
 * 表里没有的键写成〔键〕，同 Rust 读口。核心来的句子（`message` · `…Text`）原样显示，不经这里。
 */
fun copyText(
    key: String,
    vararg args: Pair<String, Any>,
): String = CopyCore.text(key, args.flatMap { listOf(it.first, it.second.toString()) }.toTypedArray())

/**
 * `libmobcore.so` 里写字的那几个口（`core-rs/rust/copy.rs`）。App 里那份 `.so` 由 core-rs 打进包；
 * JVM 单测加载同一个 crate 的宿主编译（根 `build.gradle.kts` 给每个模块的单测指好 `java.library.path`）。
 */
internal object CopyCore {
    init {
        System.loadLibrary("mobcore")
    }

    /** [args] 按「名 · 值 · 名 · 值 …」平铺。 */
    external fun text(
        key: String,
        args: Array<String>,
    ): String

    /** 负数按 0。 */
    external fun shortDuration(ms: Long): String

    /** 已到 ⇒ `null`。 */
    external fun relDuration(aheadMs: Long): String?
}
