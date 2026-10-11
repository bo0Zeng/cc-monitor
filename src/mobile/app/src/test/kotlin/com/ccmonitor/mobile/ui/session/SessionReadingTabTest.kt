package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.ssh.PosixDialect
import com.ccmonitor.mobile.ssh.PowerShellDialect
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * 终端 tab 里那个「阅读」子页：今天那一页读的是这台 Claude 的记录树（`ReadingPaneViewModel`），所以只有 Claude 主机、
 * 且远端有 `tail -F` 的方言才出。Codex 主机不出 —— 手机不再自己解 Codex 的盘上格式，看 Codex 的记录走「对话」⇒ 聊天屏（读核心的通用记录）。
 */
class SessionReadingTabTest {
    @Test
    fun `只有 Claude 主机且方言读得了才出阅读子页`() {
        val got =
            AgentKind.entries.flatMap { k ->
                listOf(PosixDialect, PowerShellDialect).map { d -> "$k/${d::class.simpleName}=${showsReadingTab(d, k)}" }
            }
        assertEquals(
            listOf(
                "ClaudeCode/PosixDialect=true",
                "ClaudeCode/PowerShellDialect=false",
                "Codex/PosixDialect=false",
                "Codex/PowerShellDialect=false",
            ),
            got,
        )
    }
}
