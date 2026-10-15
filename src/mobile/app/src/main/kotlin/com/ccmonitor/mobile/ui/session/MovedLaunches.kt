package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.MovedLaunch

/*
 * 迁移说明里的一条（手机原来存的启动配置）：原样列出 · 可复制；带命令的可在那台建同名别名（建不建得了、怎么折成别名由那台判）。
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
