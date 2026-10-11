package com.ccmonitor.mobile.core.claude.command

import org.junit.Assert.assertEquals
import org.junit.Test

/** [CodexInvocation] resume 载荷——`codex resume <uuid>` 子命令（非 --resume、无 unset），默认 codex。 */
class CodexInvocationTest {
    private val uuid = "019f78e8-84dd-7ac0-b479-e9c1b9caec67"

    @Test fun resolveLaunchDefaultsCodex() {
        assertEquals("codex", CodexInvocation.resolveLaunchCommand(emptyList()))
        assertEquals("codex", CodexInvocation.resolveLaunchCommand(listOf(null, "  ")))
        assertEquals("my-codex", CodexInvocation.resolveLaunchCommand(listOf("my-codex", "other")))
        assertEquals("下探过第一个不安全的", "safe", CodexInvocation.resolveLaunchCommand(listOf("bad|cmd", "safe")))
    }
}
