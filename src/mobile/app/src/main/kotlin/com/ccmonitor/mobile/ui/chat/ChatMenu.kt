package com.ccmonitor.mobile.ui.chat

/**
 * 聊天屏上屏文案里不许出现的内部词，大小写不敏感比对。
 *
 * 分三类：路由名、类名与技术构件、黑话。
 * 「会话」「连接」不在表里：它们有产品义，技术义的复合词（`tmux 会话`、`SSH 会话`）已由 `tmux` / `ssh` 拦住。
 */
val CHAT_MENU_BANNED_WORDS: List<String> =
    // 路由名
    listOf("settings", "chat", "conversations", "sessions", "hosts", "route", "launch") +
        // 类名 / 技术构件
        listOf("screen", "navhost", "composable", "viewmodel", "scaffold", "dropdown") +
        // 黑话
        listOf("ssh", "tmux", "bridge", "daemon", "pipe", "ndjson", "offset", "exec", "log") +
        // 这四个没有产品义，上屏就是黑话。
        listOf("管道", "路由", "实例", "进程")
