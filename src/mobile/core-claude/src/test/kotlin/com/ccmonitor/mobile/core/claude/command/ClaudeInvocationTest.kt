package com.ccmonitor.mobile.core.claude.command

import org.junit.Assert.assertEquals
import org.junit.Test

/** [ClaudeInvocation]：Claude 命令载荷。 */
class ClaudeInvocationTest {
    @Test
    fun sanitizeLaunchCommandAllowsArgsBlocksInjection() {
        assertEquals("cc", ClaudeInvocation.sanitizeLaunchCommand("cc"))
        assertEquals("env 前缀形态放行", "CLAUDE_CONFIG_DIR=~/.cw claude", ClaudeInvocation.sanitizeLaunchCommand("CLAUDE_CONFIG_DIR=~/.cw claude"))
        assertEquals("引号/括号/星号是合法 claude 参数，放行", """cc --allowedTools "Bash(*)"""", ClaudeInvocation.sanitizeLaunchCommand("""cc --allowedTools "Bash(*)""""))
        assertEquals("单引号带参放行", "cc --model 'opus'", ClaudeInvocation.sanitizeLaunchCommand("cc --model 'opus'"))
        assertEquals("claude", ClaudeInvocation.sanitizeLaunchCommand(null))
        assertEquals("claude", ClaudeInvocation.sanitizeLaunchCommand("   "))
        assertEquals("命令串联 ; → 回退 claude", "claude", ClaudeInvocation.sanitizeLaunchCommand("cc; rm -rf /"))
        assertEquals("&& → 回退", "claude", ClaudeInvocation.sanitizeLaunchCommand("cc && evil"))
        assertEquals("管道 → 回退", "claude", ClaudeInvocation.sanitizeLaunchCommand("cc | evil"))
        assertEquals("命令替换 $ → 回退", "claude", ClaudeInvocation.sanitizeLaunchCommand("\$(evil)"))
        assertEquals("反引号 → 回退", "claude", ClaudeInvocation.sanitizeLaunchCommand("cc `evil`"))
        assertEquals("重定向 > → 回退", "claude", ClaudeInvocation.sanitizeLaunchCommand("cc > /etc/x"))
        assertEquals("换行 → 回退", "claude", ClaudeInvocation.sanitizeLaunchCommand("cc\nevil"))
    }

    // 单候选链：调用方拼 `listOf(历史页选定, 发起 tab launcher) + 主机 launchers`，逐级下探。
    @Test
    fun resolveLaunchCommandStepsDownCandidateChain() {
        assertEquals("历史页显式选定最优先", "cct", ClaudeInvocation.resolveLaunchCommand(listOf("cct", "cc")))
        assertEquals("选定/preferred 缺席 → 首个非空候选", "cc", ClaudeInvocation.resolveLaunchCommand(listOf(null, "  ", "cc", "cct")))
        assertEquals("都没有 → 兜底 claude", "claude", ClaudeInvocation.resolveLaunchCommand(emptyList()))
        assertEquals("全 null → 兜底 claude", "claude", ClaudeInvocation.resolveLaunchCommand(listOf(null, null)))
        assertEquals("首候选非法 → 下探有效命令 cc", "cc", ClaudeInvocation.resolveLaunchCommand(listOf("cc;evil", "cc")))
        assertEquals("前两候选非法 → 下探到 cct", "cct", ClaudeInvocation.resolveLaunchCommand(listOf("a|b", "c;d", "cct")))
        assertEquals("全部非法 → claude", "claude", ClaudeInvocation.resolveLaunchCommand(listOf("a;b", "c|d")))
        assertEquals("四级链：选定 > 发起 tab launcher > 主机 launchers > claude", "cc", ClaudeInvocation.resolveLaunchCommand(listOf(null, null, "cc")))
    }

    @Test
    fun modelCommandIsSlashModel() {
        assertEquals("/model opus", ClaudeInvocation.modelCommand("opus"))
        assertEquals("/model claude-sonnet-5", ClaudeInvocation.modelCommand("claude-sonnet-5"))
    }
}
