package com.ccmonitor.mobile.core.claude.command

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** [CodexInvocation] resume 载荷——`codex resume <uuid>` 子命令（非 --resume、无 unset），默认 codex。 */
class CodexInvocationTest {
    private val uuid = "019f78e8-84dd-7ac0-b479-e9c1b9caec67"

    @Test fun resumeIsSubcommandNoFlagNoUnset() {
        val payload = CodexInvocation.resumeInvocation(null, uuid)
        assertEquals("codex resume $uuid", payload)
        assertFalse("非 --resume flag", payload.contains("--resume"))
        assertFalse("无 unset 嵌套环境（Codex 无此病）", payload.contains("unset"))
        assertFalse("无 CLAUDECODE", payload.contains("CLAUDECODE"))
    }

    @Test fun resumeUsesCustomLaunchWhenSafe() {
        assertEquals("cct resume $uuid", CodexInvocation.resumeInvocation("cct", uuid))
        // 含注入元字符 → fail-closed 回退 codex。
        assertEquals("codex resume $uuid", CodexInvocation.resumeInvocation("codex; rm -rf /", uuid))
    }

    @Test fun resolveLaunchDefaultsCodex() {
        assertEquals("codex", CodexInvocation.resolveLaunchCommand(emptyList()))
        assertEquals("codex", CodexInvocation.resolveLaunchCommand(listOf(null, "  ")))
        assertEquals("my-codex", CodexInvocation.resolveLaunchCommand(listOf("my-codex", "other")))
        assertEquals("下探过第一个不安全的", "safe", CodexInvocation.resolveLaunchCommand(listOf("bad|cmd", "safe")))
    }

    @Test fun sessionNameIsCxPrefixed() {
        assertEquals("cx-019f78e8", CodexInvocation.resumeSessionName(uuid))
    }

    @Test fun validSessionIdAcceptsUuidRejectsInjection() {
        assertTrue(CodexInvocation.isValidSessionId(uuid))
        assertFalse(CodexInvocation.isValidSessionId("$uuid; rm"))
        assertFalse(CodexInvocation.isValidSessionId(""))
    }

    @Test fun resumeRejectsInvalidSessionId() {
        val ex = runCatching { CodexInvocation.resumeInvocation("codex", "bad;id") }.exceptionOrNull()
        assertTrue("非法 sid → IllegalArgumentException", ex is IllegalArgumentException)
    }
}
