package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind
import org.junit.Assert.assertEquals
import org.junit.Test

/** [SessionLocator] 按种类解析 agent-dir。 */
class SessionLocatorTest {
    @Test fun codexResolveDirOverrideElseDefault() {
        assertEquals("/custom/codex", CodexPaths.resolveCodexDir("/custom/codex", null))
        assertEquals(CodexPaths.DEFAULT_CODEX_DIR, CodexPaths.resolveCodexDir(null, null))
        assertEquals("/app/def", CodexPaths.resolveCodexDir("  ", "/app/def")) // 空覆盖 → 应用默认
    }

    @Test fun selectorRoutesPerKind() {
        assertEquals(ClaudeSessionLocator, AgentProfile.of(AgentKind.ClaudeCode).sessionLocator)
        assertEquals(CodexSessionLocator, AgentProfile.of(AgentKind.Codex).sessionLocator)
        assertEquals(ClaudePaths.DEFAULT_CLAUDE_DIR, AgentProfile.of(AgentKind.ClaudeCode).sessionLocator.resolveAgentDir(null, null))
        assertEquals(CodexPaths.DEFAULT_CODEX_DIR, AgentProfile.of(AgentKind.Codex).sessionLocator.resolveAgentDir(null, null))
    }
}
