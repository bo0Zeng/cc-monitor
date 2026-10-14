package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.MovedLaunch

/*
 * 迁移说明里的一条（手机原来存的启动配置）：原样列出 · 可复制；`claude` / `ccm` 那一类可在那台建同名别名。
 */

/** 列出来的那几格，原样（没写的不列）：目录 · 会话名 · 命令。 */
fun MovedLaunch.shownValues(): List<String> = listOfNotNull(workingDir, tmuxSession, command)

/** 屏上那几行：每格前面带它是哪一格。 */
fun MovedLaunch.shownLines(): List<String> =
    listOfNotNull(
        workingDir?.let { copyText("mobLaunch.migrate.dir", "dir" to it) },
        tmuxSession?.let { copyText("mobLaunch.migrate.tmux", "name" to it) },
        command?.let { copyText("mobLaunch.migrate.cmd", "cmd" to it) },
    )

/** 「复制」交给剪贴板的那一段：名字一行，下面照屏上那几行。 */
fun MovedLaunch.clipText(): String = (listOf(label) + shownLines()).joinToString("\n")

/**
 * 「建这条」交给那台的别名表单（别名页表单那一形）：命令是 `claude …`（后面那串原样交给 agent）或单一个 `ccm` ⇒ 同名一段，
 * 目录 ⇒「在哪个目录起」，会话名 ⇒ 固定名的 tmux；别的命令 · 带了自己选项的 `ccm` ⇒ `null`（只列出、可复制，不建：
 * ccm 选项怎么折成别名那几格只在那台）。
 */
fun MovedLaunch.aliasForm(): Map<String, Any?>? {
    val cmd = command?.trim().orEmpty()
    val head = cmd.substringBefore(' ')
    val rest = cmd.substringAfter(' ', "").trim()
    val args =
        when {
            head == AGENT_WORD -> rest.ifEmpty { null }
            head == CCM_WORD && rest.isEmpty() -> null
            else -> return null
        }
    return buildMap {
        put("name", label.trim())
        workingDir?.trim()?.takeIf { it.isNotEmpty() }?.let { put("cwd", it) }
        tmuxSession?.trim()?.takeIf { it.isNotEmpty() }?.let { put("tmux", mapOf("mode" to "fixed", "name" to it)) }
        args?.let { put("args", it) }
    }
}

private const val AGENT_WORD = "claude"
private const val CCM_WORD = "ccm"
