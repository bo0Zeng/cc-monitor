package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 机器编辑器里的几组单选（Agent 种类 · 远端系统 · 认证方式 · 用哪把钥匙 · 跳板）：整行都能点。
 *
 * 模拟器冒烟撞上的：Claude / Codex 那组只有小圆圈能点，点字不选（字是旁边一个单独的 `Text`）。
 * 要的写法：那一行挂 `Modifier.selectable(…, role = Role.RadioButton)`，圆圈本身 `onClick = null`（只画状态，点击归那一行）。
 *
 * 判别力边界：读源码里的写法，不量真点上去选没选中（app 没引 Robolectric）；冒烟台架在模拟器上点字验。
 */
class HostChoiceRowsTest {
    private val code =
        KotlinSourceScanner.codeOnlyKeepingLiterals(
            (File(HOST_SCREENS).takeIf { it.isFile } ?: File("app/$HOST_SCREENS")).readText(),
        )

    @Test
    fun `每个单选圆圈都不自己接点击：点击归整行`() {
        val radios = Regex("""RadioButton\(([^)]*)\)""").findAll(code).map { it.groupValues[1] }.toList()
        assertTrue("前提：机器编辑器里得有单选（实得 ${radios.size}）", radios.isNotEmpty())
        val selfClicking = radios.filterNot { Regex("""onClick\s*=\s*null""").containsMatchIn(it) }
        assertEquals("这几个圆圈自己接点击（旁边的字点了不选）：$selfClicking", emptyList<String>(), selfClicking)
    }

    @Test
    fun `整行用 selectable、按单选的角色报给无障碍`() {
        assertTrue("没有 `.selectable(`：整行点不了", ".selectable(" in code)
        assertTrue("selectable 要带 role = Role.RadioButton", "role = Role.RadioButton" in code)
    }

    private companion object {
        const val HOST_SCREENS = "src/main/kotlin/com/ccmonitor/mobile/ui/host/HostScreens.kt"
    }
}
