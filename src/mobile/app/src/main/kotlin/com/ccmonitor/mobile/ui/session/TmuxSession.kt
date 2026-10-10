package com.ccmonitor.mobile.ui.session

/** 一个 tmux 会话。[command] 是活动 pane 当前的命令（免得盲杀正在干活的会话），缺则为空。 */
data class TmuxSession(
    val name: String,
    val windows: Int,
    val attached: Boolean,
    val command: String = "",
)

/**
 * 解析 `tmux ls -F '#{session_name}\t#{session_windows}\t#{?session_attached,1,0}\t#{pane_current_command}'` 的输出。
 * 用 tab 分隔（会话名可含空格与 `|`，但不含 tab）；
 * 字段不足 3 或会话名空 → 跳过；windows 非数字 → 0；attached=第 3 列为 "1"。无 server（stderr 被 2>/dev/null 吞）→ 空。
 */
fun parseTmuxList(raw: String): List<TmuxSession> =
    raw
        .lineSequence()
        .mapNotNull { line ->
            val p = line.split('\t')
            if (p.size < 3 || p[0].isBlank()) {
                null
            } else {
                TmuxSession(p[0], p[1].trim().toIntOrNull() ?: 0, p[2].trim() == "1", p.getOrNull(3)?.trim().orEmpty())
            }
        }.toList()
