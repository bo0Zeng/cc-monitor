package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.nav.Screen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 设置屏「机器与账号」那几条：入口是一份数据（[settingsEntries]），设置屏只渲染它；每条都指向真注册的路由；
 * 读那台的那几条（账号 · 轮换）在那台读不了时灰着。终端不在设置里（只从抽屉进）。
 */
class SettingsEntriesTest {
    @Test
    fun theFourEntriesAreMachinesAccountsRotationKeysInThatOrder() {
        assertEquals(
            listOf(Screen.Hosts.route, Screen.Accounts.route, Screen.RotationHere.route, Screen.Identities.route),
            settingsEntries().map { it.route },
        )
        assertEquals("读那台的是账号 · 轮换；机器 · 密钥住手机", listOf(false, true, true, false), settingsEntries().map { it.readsMachine })
        assertEquals("行尾数的是机器表与保险箱", listOf(EntryCount.MACHINES, null, null, EntryCount.KEYS), settingsEntries().map { it.count })
    }

    /** 打错一个路由串在 Compose 里是运行期才炸的（`navigate` 找不到目的地）：这条把它提到门禁里。 */
    @Test
    fun everyEntrancePointsAtARouteThatIsActuallyRegistered() {
        val registered = registeredRoutes()
        for (e in settingsEntries()) {
            assertTrue("入口「${e.title}」指向的 ${e.route} 没有在 AppNavHost 里注册：$registered", e.route in registered)
        }
        assertFalse("终端只从抽屉进", settingsEntries().any { it.route == Screen.SessionsOverview.route || it.route == Screen.Session.route })
    }

    /** 入口不许写回 composable 里：屏上看着一样，而数据里那一项没了，上面两条就守不住任何东西。 */
    @Test
    fun theScreenRendersTheDataAndScrolls() {
        val code = KotlinSourceScanner.codeOnlyKeepingLiterals(source("SettingsScreen.kt"))
        assertTrue("设置屏必须去读那份数据", code.contains("settingsEntries()"))
        assertFalse("设置屏不该自己认得任何一条路由", code.contains("Screen."))
        for (e in settingsEntries()) assertFalse("入口「${e.title}」的字被写死在设置屏里了", code.contains("\"${e.title}\""))
        assertTrue("那一列要能滚：不滚的话下面几节落在屏外", code.contains("verticalScroll(rememberScrollState"))
    }

    /** `AppNavHost` 里所有 `composable(<x>)` 的第一个实参解成路由串（认不出的那种写法会误红，误红是安全的一侧）。 */
    private fun registeredRoutes(): Set<String> {
        val code = KotlinSourceScanner.codeOnlyKeepingLiterals(source("AppNavHost.kt"))
        val args = Regex("""composable\(\s*([^,)\s]+)""").findAll(code).map { it.groupValues[1] }.toList()
        assertTrue("前提：得真认出 `composable(` 调用（实得 ${args.size} 处）", args.size >= SCREENS.size)
        return args
            .mapNotNull { token ->
                when {
                    token.startsWith("\"") -> token.trim('"')
                    token.startsWith("Screen.") && token.endsWith(".route") -> SCREENS[token.removePrefix("Screen.").removeSuffix(".route")]
                    else -> null
                }
            }.toSet()
    }

    private fun source(name: String): String {
        val dir = File("src/main/kotlin").takeIf { it.isDirectory } ?: File("app/src/main/kotlin")
        return dir.walkTopDown().first { it.isFile && it.name == name }.readText()
    }

    private companion object {
        /** `Screen` 的每个 data object ⇒ 它的路由串（反射拿不到 sealed 子类的稳定顺序，手写；少一条 ⇒ 那条注册认不出 ⇒ 误红）。 */
        val SCREENS: Map<String, String> =
            mapOf(
                "Launch" to Screen.Launch.route,
                "Hosts" to Screen.Hosts.route,
                "HostEdit" to Screen.HostEdit.route,
                "Identities" to Screen.Identities.route,
                "Keys" to Screen.Keys.route,
                "Session" to Screen.Session.route,
                "SessionsOverview" to Screen.SessionsOverview.route,
                "Conversations" to Screen.Conversations.route,
                "Chat" to Screen.Chat.route,
                "NewSessionHostPicker" to Screen.NewSessionHostPicker.route,
                "Settings" to Screen.Settings.route,
                "Rotation" to Screen.Rotation.route,
                "RotationHere" to Screen.RotationHere.route,
                "Accounts" to Screen.Accounts.route,
            )
    }
}
