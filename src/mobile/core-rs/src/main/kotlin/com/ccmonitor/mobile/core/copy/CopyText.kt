package com.ccmonitor.mobile.core.copy

import com.ccmonitor.mobile.core.rs.Core

/**
 * 手机取文的唯一入口：经 [Core.copyTextFlat] 调手机那份 `.so` 里的 `copy-core::copy_text`（同桌面 Rust 那一份，表编译期内嵌）。
 *
 * 键必须是字面量、参数是 `"名" to 值` 的对，名与表里 `args` 两向相等（`copy-table.vitest.ts` 按调用点判）；
 * 表里没有的键写成〔键〕，同 Rust 读口。核心来的句子（`message` · `…Text`）原样显示，不经这里。
 */
fun copyText(
    key: String,
    vararg args: Pair<String, Any>,
): String = Core.copyTextFlat(key, args.flatMap { listOf(it.first, it.second.toString()) }.toTypedArray())
