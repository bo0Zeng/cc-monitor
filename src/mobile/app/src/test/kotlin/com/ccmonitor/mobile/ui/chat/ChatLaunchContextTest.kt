package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.transport.ClaudePaths
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 设置里那个「Claude 配置目录」要管得到起那侧。
 *
 * 读那侧（会话发现 / 阅读面）走 [ClaudePaths.resolveClaudeDir]：主机覆盖 > 应用默认 > 远端自己的。
 * 起那侧若什么都不设，覆盖过的用户就会「读的是 A、起出来那个 Claude 写的是 B」；
 * 远端做了账号隔离时，管道起的 Claude 还会是未登录态。
 */
class ChatLaunchContextTest {
    private val default = ClaudePaths.DEFAULT_CLAUDE_DIR

    /**
     * 判据是「解析结果 ≠ 默认表达式」，不是「主机字段非空」。
     *
     * 只看主机字段的话，应用级默认那一档会被漏掉 ——
     * 表现成「设置里明明填了，起出来那个 Claude 还是不认」，而且从代码上看不出哪里错了。
     */
    @Test
    fun anAppLevelDefaultCountsAsAnOverrideJustLikeAPerHostOne() {
        assertEquals("每主机覆盖要认", "/a", explicitClaudeDir(hostClaudeDir = "/a", appDefault = null))
        assertEquals("应用级默认同样要认", "/b", explicitClaudeDir(hostClaudeDir = null, appDefault = "/b"))
        assertEquals("主机覆盖优先于应用默认", "/a", explicitClaudeDir(hostClaudeDir = "/a", appDefault = "/b"))
    }

    /**
     * 没覆盖过就是 null —— 什么都不设。
     *
     * 默认值本身是个 shell 表达式（语义 = 「听远端自己的」）。
     * 这时硬塞一个值反而会把远端的选择覆盖掉，是比不设更糟的错。
     */
    @Test
    fun withoutAnOverrideItIsNullSoNothingGetsForcedOnTheRemote() {
        assertNull("都没设 ⇒ 听远端自己的", explicitClaudeDir(hostClaudeDir = null, appDefault = null))
        assertNull("空白等于没设", explicitClaudeDir(hostClaudeDir = "  ", appDefault = ""))
        // 前提：那个默认值确实是「听远端自己的」那种表达式，否则上面两条断言的理由就不成立
        assertEquals("前提：默认值是 shell 表达式", "\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}", default)
    }

    /** 与读那侧同一个解析函数 —— 两边各写一套迟早会漂。 */
    @Test
    fun itReusesTheSameResolutionAsTheReadingSide() {
        for (host in listOf(null, "", "/a")) {
            for (app in listOf(null, "", "/b")) {
                val resolved = ClaudePaths.resolveClaudeDir(host, app)
                val expected = resolved.takeIf { it != default }
                assertEquals("host=$host app=$app 时两边必须一致", expected, explicitClaudeDir(host, app))
            }
        }
    }
}
