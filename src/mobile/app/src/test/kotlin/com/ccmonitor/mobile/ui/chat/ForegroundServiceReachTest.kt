package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 前台保活服务在新界面上也得被起起来。
 *
 * ### 它守的是什么
 *
 * `SshKeepAliveService` 的存活条件是「有活动会话 或 有在飞轮次」，后者由应用级的
 * `ChatController.hasInFlightTurn` 持有。服务得有人起：若只有终端屏（`SessionScreen`）调
 * `startForegroundService`，只走聊天屏的用户就从没起过它，「离开 app 后回复完成推送」
 * 不报错、就是不通知。
 *
 * ### 为什么是扫源码
 *
 * 缺的是一个调用点，不是一段可注入的逻辑：`ContextCompat.startForegroundService`
 * 要 `Context`、只能待在 Composable 里。要真验它得起 instrumented + 观察系统服务状态，
 * 那是另一个量级的装置。
 *
 * 所以这里守的是结构规则：两条入口（终端屏、聊天屏）都必须有那个调用。
 * 如实说明它的判别力边界：它能挡住「有人把聊天屏那行删了」，
 * 挡不住「调用还在但参数写错」，后者要靠真机。
 */
class ForegroundServiceReachTest {
    private fun appSources(): List<File> {
        val dir = File("src/main/kotlin").takeIf { it.isDirectory } ?: File("app/src/main/kotlin")
        return dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
    }

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 注意：必须剥注释。直接对全文 `contains("startForegroundService")` 的话，聊天屏那次调用删掉之后
     * 判据照样绿，因为 `ChatRoute.kt` 里有注释也写着这个名字。
     *
     * 它看不见什么（反射 / 按名拼接 / 死分支 / 模板插值里的引号翻面），逐条写在
     * [KotlinSourceScanner] 的头注里，这里不复述。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    @Test
    fun bothTheTerminalAndTheChatSurfaceStartTheKeepAliveService() {
        val files = appSources()
        assertTrue("前提：得真扫到源文件，否则这条测试是空跑", files.size > 20)

        val starters =
            files.filter { codeOnly(it.readText()).contains("startForegroundService") }.map { it.name }.toSet()
        assertTrue("前提：终端屏本来就有（否则这条判据的参照系不成立）", "SessionScreen.kt" in starters)
        assertTrue(
            "聊天屏也必须起前台保活服务，否则新界面上「离开 app 后回复完成推送」不成立。实得：$starters",
            "ChatRoute.kt" in starters,
        )
    }

    /**
     * 前提自检：剥注释这一步真的在起作用。
     *
     * 不验它的话，上面那条可能因为「把整份文件当注释剥光了」而恒绿 —— 那又是一条无判别力的判据。
     */
    @Test
    fun theCommentStrippingActuallyStripsCommentsAndKeepsCode() {
        val sample =
            """
            /** 这段 KDoc 里提 startForegroundService 是合法的 */
            // 这行注释里也提 startForegroundService
            val real = ContextCompat.startForegroundService(app, intent)
            """.trimIndent()
        val code = codeOnly(sample)
        assertTrue("代码里的必须留下来", code.contains("ContextCompat.startForegroundService"))
        assertFalse("KDoc 里的必须被剥掉", code.contains("这段 KDoc"))
        assertFalse("行注释里的必须被剥掉", code.contains("这行注释"))
    }
}
