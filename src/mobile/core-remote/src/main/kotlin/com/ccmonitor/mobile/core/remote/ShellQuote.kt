package com.ccmonitor.mobile.core.remote

/**
 * POSIX shell 单引号转义，全仓唯一实现。放在中立叶子模块 `core-remote`，core-claude 与 app 都看得见。
 *
 * 整串裹单引号，串内的 `'` 换成 `'\''`（闭合 → 转义单引号 → 重开）。
 * 结果对任意输入都是一个 shell 词，不会被 word-split、不会触发 glob/变量展开。
 *
 * ```
 * shellQuote("main")  == "'main'"
 * shellQuote("a b")   == "'a b'"
 * shellQuote("it's")  == "'it'\''s'"
 * ```
 *
 * 注意：被引用的值里 `~` / `$HOME` 不会展开。
 *
 * 这是 POSIX sh 方言；PowerShell 的转义规则不同（`'` → `''`），见 `ShellDialect`，别用这个函数。
 */
fun shellQuote(s: String): String = "'" + s.replace("'", "'\\''") + "'"
