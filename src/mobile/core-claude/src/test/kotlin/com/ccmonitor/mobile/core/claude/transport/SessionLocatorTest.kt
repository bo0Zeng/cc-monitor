package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [SessionLocator] per-kind 会话定位与 [CodexPaths]。用真 ~/.codex rollout 文件名的形状验 UUID 抽取。
 */
class SessionLocatorTest {
    // ---- CodexPaths.sessionIdOf：末尾 UUID（rollout-<ISO-ts>-<uuid>.jsonl[.zst]）----
    @Test fun codexSessionIdIsTrailingUuid() {
        // 真机文件名形状（ISO ts 里也有 `-`，故取末个 UUID 匹配、不误取时间段）。
        val real = "/home/u/.codex/sessions/2026/07/18/rollout-2026-07-18T07-34-25-019f75a6-5e87-72c1-ac71-fc9a790f0be3.jsonl"
        assertEquals("019f75a6-5e87-72c1-ac71-fc9a790f0be3", CodexPaths.sessionIdOf(real))
    }

    @Test fun codexSessionIdHandlesZstAndFallback() {
        assertEquals("019f75a6-5e87-72c1-ac71-fc9a790f0be3", CodexPaths.sessionIdOf("/p/rollout-2026-07-18T07-34-25-019f75a6-5e87-72c1-ac71-fc9a790f0be3.jsonl.zst"))
        // 无 UUID → 去后缀兜底（防御）。
        assertEquals("weird-name", CodexPaths.sessionIdOf("/p/weird-name.jsonl"))
    }

    @Test fun codexSessionsGlobIsDatePartitionedInclZst() {
        val g = CodexPaths.sessionsGlob(CodexPaths.DEFAULT_CODEX_DIR)
        assertTrue("日期三级 glob", g.contains("/sessions/*/*/*/rollout-*.jsonl*"))
        assertTrue(".jsonl* 尾兼 .zst", g.endsWith("rollout-*.jsonl*"))
    }

    @Test fun codexResolveDirOverrideElseDefault() {
        assertEquals("/custom/codex", CodexPaths.resolveCodexDir("/custom/codex", null))
        assertEquals(CodexPaths.DEFAULT_CODEX_DIR, CodexPaths.resolveCodexDir(null, null))
        assertEquals("/app/def", CodexPaths.resolveCodexDir("  ", "/app/def")) // 空覆盖 → 应用默认
    }

    // ---- SessionLocator per-kind ----
    @Test fun claudeLocatorScansProjectsByCwd() {
        val cmd = ClaudeSessionLocator.listCommand("/c/.claude", "/home/pi/proj")
        assertTrue("ls -t + projects", cmd.startsWith("ls -t ") && cmd.contains("/projects/"))
        assertTrue("cwd 编码进 glob", cmd.contains("-home-pi-proj"))
        assertEquals("s1", ClaudeSessionLocator.sessionIdOf("/a/b/s1.jsonl"))
    }

    @Test fun codexLocatorScansSessionsByDateIgnoringCwd() {
        // Codex 日期分区、cwd 不进 glob（cwd 过滤靠读 session_meta·catalog 层）。
        val withCwd = CodexSessionLocator.listCommand("/c/.codex", "/home/pi/proj")
        val noCwd = CodexSessionLocator.listCommand("/c/.codex", null)
        assertEquals("cwd 忽略：两命令同", withCwd, noCwd)
        assertTrue(withCwd.startsWith("ls -t ") && withCwd.contains("/sessions/*/*/*/rollout-*.jsonl*"))
    }

    // ---- 按 sid 找回会话文件（resume 脚手架的 locate 段）----

    @Test fun claudeFindsTheRecordByExactNameTwoLevelsDown() {
        assertEquals(
            "find \"\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}/projects\" -maxdepth 2 -name 'abc-1.jsonl' -print -quit 2>/dev/null",
            ClaudeSessionLocator.findBySessionIdCommand(ClaudePaths.DEFAULT_CLAUDE_DIR, "abc-1"),
        )
        assertEquals(
            "用户覆写的目录作字面路径单引号",
            "find '/srv/c d/projects' -maxdepth 2 -name 'abc-1.jsonl' -print -quit 2>/dev/null",
            ClaudeSessionLocator.findBySessionIdCommand("/srv/c d", "abc-1"),
        )
    }

    @Test fun codexFindsTheRolloutByTrailingUuidFourLevelsDown() {
        // rollout-<ISO-ts>-<uuid>.jsonl[.zst]：uuid 在尾 ⇒ `*<uuid>.jsonl*`（兼吃 .zst）；年/月/日三级 ⇒ maxdepth 4。
        assertEquals(
            "find '/h/.codex/sessions' -maxdepth 4 -name '*019f78e8-84dd-7ac0-b479-e9c1b9caec67.jsonl*' -print -quit 2>/dev/null",
            CodexSessionLocator.findBySessionIdCommand("/h/.codex", "019f78e8-84dd-7ac0-b479-e9c1b9caec67"),
        )
    }

    /** 按 kind 经 `AgentProfile.of(kind).sessionLocator` 取到对应定位器。 */
    @Test fun selectorRoutesPerKind() {
        assertEquals(ClaudeSessionLocator, AgentProfile.of(AgentKind.ClaudeCode).sessionLocator)
        assertEquals(CodexSessionLocator, AgentProfile.of(AgentKind.Codex).sessionLocator)
        assertEquals(ClaudePaths.DEFAULT_CLAUDE_DIR, AgentProfile.of(AgentKind.ClaudeCode).sessionLocator.defaultAgentDir)
        assertEquals(CodexPaths.DEFAULT_CODEX_DIR, AgentProfile.of(AgentKind.Codex).sessionLocator.defaultAgentDir)
    }
}
