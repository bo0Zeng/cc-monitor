package com.ccmonitor.mobile.core.claude.transport

import org.junit.Assert.assertEquals
import org.junit.Test

class ClaudePathsTest {
    @Test fun realSamples() {
        // 真实目录名：每个非字母数字 → '-'
        assertEquals("-home-pi--claude", ClaudePaths.projectDirName("/home/pi/.claude"))
        assertEquals("-home-pi-Resilio-Sync", ClaudePaths.projectDirName("/home/pi/Resilio Sync"))
        assertEquals("-home-pi-project-android-terminal", ClaudePaths.projectDirName("/home/pi/project/android-terminal"))
        assertEquals("-home-pi-project-120", ClaudePaths.projectDirName("/home/pi/project/120"))
    }

    // claudeDir 解析与 glob
    @Test fun resolveClaudeDirPrecedence() {
        assertEquals("/custom/claude", ClaudePaths.resolveClaudeDir("/custom/claude", "/app/default")) // host 覆盖优先
        assertEquals("/app/default", ClaudePaths.resolveClaudeDir(null, "/app/default")) // host 空 → app 默认
        assertEquals("/app/default", ClaudePaths.resolveClaudeDir("  ", "/app/default")) // host 空白视作未设
        assertEquals(ClaudePaths.DEFAULT_CLAUDE_DIR, ClaudePaths.resolveClaudeDir(null, null)) // 都无 → 内置默认
        assertEquals(ClaudePaths.DEFAULT_CLAUDE_DIR, ClaudePaths.resolveClaudeDir("", "  ")) // 都空白 → 内置默认
    }

    @Test fun defaultClaudeDirProbesEnv() {
        assertEquals("\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}", ClaudePaths.DEFAULT_CLAUDE_DIR)
    }

    // projectsGlob 只 quote 前缀（覆写→单引号），保留 `*` glob 与 projectDirName 活性。
    @Test fun projectsGlobWithAndWithoutCwd() {
        assertEquals(
            "'~/.claude'/projects/-home-pi-project-android-terminal/*.jsonl",
            ClaudePaths.projectsGlob("~/.claude", "/home/pi/project/android-terminal"),
        )
        assertEquals("'~/.claude'/projects/*/*.jsonl", ClaudePaths.projectsGlob("~/.claude", null))
        assertEquals("'~/.claude'/projects/*/*.jsonl", ClaudePaths.projectsGlob("~/.claude", "  "))
        assertEquals("'/data/claude'/projects/-x/*.jsonl", ClaudePaths.projectsGlob("/data/claude", "/x"))
    }

    // 含空格/单引号的覆写 claudeDir 不能被 word-split（否则 glob 失配、静默丢会话）；默认值用双引号（要在远端展开）。
    @Test fun projectsGlobQuotesPrefixForSpaceSafety() {
        // 默认值 = 受信 shell 表达式 → 双引号（保留 $CLAUDE_CONFIG_DIR/$HOME 展开）+ glob 活。
        assertEquals(
            "\"${ClaudePaths.DEFAULT_CLAUDE_DIR}\"/projects/*/*.jsonl",
            ClaudePaths.projectsGlob(ClaudePaths.DEFAULT_CLAUDE_DIR, null),
        )
        // 含空格覆写 → 单引号包裹，不被 word-split。
        assertEquals("'/srv/c d'/projects/*/*.jsonl", ClaudePaths.projectsGlob("/srv/c d", null))
        assertEquals("'/srv/c d'/projects/-x/*.jsonl", ClaudePaths.projectsGlob("/srv/c d", "/x"))
        // 含单引号覆写 → POSIX 转义 `'a'\''b'`（注入安全）。
        assertEquals("'/a'\\''b'/projects/-x/*.jsonl", ClaudePaths.projectsGlob("/a'b", "/x"))
    }

    // resume 用的 projects 目录整体 quote（无 glob）。
    @Test fun projectsDirExprQuoting() {
        assertEquals("默认值=受信 shell 表达式 → 双引号展开", "\"${ClaudePaths.DEFAULT_CLAUDE_DIR}/projects\"", ClaudePaths.projectsDirExpr(ClaudePaths.DEFAULT_CLAUDE_DIR))
        assertEquals("覆写值=字面路径 → 单引号", "'/srv/c d/projects'", ClaudePaths.projectsDirExpr("/srv/c d"))
        assertEquals("覆写值含单引号 → POSIX 转义", "'/a'\\''b/projects'", ClaudePaths.projectsDirExpr("/a'b"))
    }
}
