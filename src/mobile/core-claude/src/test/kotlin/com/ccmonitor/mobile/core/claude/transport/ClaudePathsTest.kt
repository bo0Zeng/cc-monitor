package com.ccmonitor.mobile.core.claude.transport

import org.junit.Assert.assertEquals
import org.junit.Test

class ClaudePathsTest {
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
}
