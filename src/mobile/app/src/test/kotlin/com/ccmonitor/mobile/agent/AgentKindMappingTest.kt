package com.ccmonitor.mobile.agent

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.data.db.Host
import org.junit.Assert.assertEquals
import org.junit.Test

/** `Host.agentKind`(String，core-data 存 enum.name) → [AgentKind] 的 :app 边界映射；null/未知 → ClaudeCode（保守缺省、前向兼容）。 */
class AgentKindMappingTest {
    private fun host(kind: String?) = Host(id = "h", label = "l", host = "x", username = "u", agentKind = kind)

    @Test fun mapsCodex() = assertEquals(AgentKind.Codex, host("Codex").agentKindOrDefault())

    @Test fun mapsClaudeExplicit() = assertEquals(AgentKind.ClaudeCode, host("ClaudeCode").agentKindOrDefault())

    @Test fun nullDefaultsClaude() = assertEquals(AgentKind.ClaudeCode, host(null).agentKindOrDefault())

    @Test fun unknownDefaultsClaude() = assertEquals("未知 wire/DB 值前向兼容→ClaudeCode", AgentKind.ClaudeCode, host("Gemini").agentKindOrDefault())
}
