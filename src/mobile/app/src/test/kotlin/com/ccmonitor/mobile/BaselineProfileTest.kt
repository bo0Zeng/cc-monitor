package com.ccmonitor.mobile

import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 进仓的 Baseline Profile（`:baselineprofile` 的生成器录出来的那一份）盖住录的两条路，app 带着装它的 profileinstaller。
 *
 * - 冷启到主屏：Application · 那一个 Activity · 导航宿主各有规则。
 * - 打开一条长会话往上滑：会话列表 · 聊天屏 · 阅读面 · 正文排版各有规则。
 * 一条路的类整片不在 ⇒ 生成器没走到那一屏（或那一屏改了名、该重生成）。
 * profile 只在 profileinstaller 在的时候对侧装的包起效（首启后台装）：它的两个入口类要在 app 的运行时依赖里。
 */
class BaselineProfileTest {
    private val profile = File("src/main/generated/baselineProfiles/baseline-prof.txt")

    /** 规则里出现过的类（`L…;`，方法规则取 `->` 之前那一截，前缀的 HSP 标记去掉）。 */
    private val classes: Set<String> by lazy {
        profile
            .readLines()
            .map { it.trim().trimStart('H', 'S', 'P').substringBefore("->") }
            .filter { it.startsWith("L") && it.endsWith(";") }
            .toSet()
    }

    private fun assertCovered(
        path: String,
        wanted: List<String>,
    ) {
        assertTrue("$profile 不在：先跑 src/mobile/scripts/baseline-profile.sh gen", profile.isFile)
        val missing = wanted.filterNot { w -> classes.any { it == "L$w;" || it.startsWith("L$w$") } }
        assertTrue("$path 那条路的类没进 profile：$missing", missing.isEmpty())
    }

    @Test
    fun coldStartToMainScreenIsInProfile() =
        assertCovered(
            "冷启到主屏",
            listOf(
                "com/ccmonitor/mobile/CcMonitorApp",
                "com/ccmonitor/mobile/MainActivity",
                "com/ccmonitor/mobile/ui/nav/AppNavHostKt",
            ),
        )

    @Test
    fun openLongSessionAndScrollUpIsInProfile() =
        assertCovered(
            "打开长会话往上滑",
            listOf(
                "com/ccmonitor/mobile/ui/chats/HostChats",
                "com/ccmonitor/mobile/ui/chat/ChatRecord",
                "com/ccmonitor/mobile/ui/claude/ClaudeReadingPaneKt",
                "com/ccmonitor/mobile/ui/claude/MdDocKt",
            ),
        )

    @Test
    fun appCarriesProfileInstaller() {
        listOf("androidx.profileinstaller.ProfileInstallerInitializer", "androidx.profileinstaller.ProfileInstallReceiver").forEach {
            assertTrue("app 的运行时依赖里没有 $it", runCatching { Class.forName(it) }.isSuccess)
        }
    }
}
