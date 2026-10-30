package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 前台保活服务只在一处起：应用级的 `KeepAliveWork.startOnWork`（「有活」那一下），不挂在哪一屏上。
 *
 * ### 它守的是什么
 *
 * `SshKeepAliveService` 的存活条件是「有活动会话 · 有在飞轮次 · 核心里有在传的」（`KeepAlivePolicy.hasWork`）。
 * 服务得有人起：挂在某一屏上起（以前是终端屏 · 聊天屏连上那两处），文件页下一组文件 · 附件在传时就没人起它，
 * 切到后台进程随时被收，也不报错。
 *
 * ### 为什么是扫源码
 *
 * 缺的是一个调用点，不是一段可注入的逻辑：`ContextCompat.startForegroundService` 要 `Context`。
 * 要真验它得起 instrumented + 观察系统服务状态，那是另一个量级的装置。
 *
 * 所以这里守的是结构规则：起它的只有 `KeepAliveWork.kt`，且应用起来时挂上它（`CcMonitorApp.kt` 调 `startOnWork`）。
 * 如实说明它的判别力边界：挡得住「又在哪一屏上另起一处」「应用没挂上」，挡不住「调用还在但参数写错」，后者要靠真机。
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
    fun theKeepAliveServiceIsStartedOnlyWhereWorkIsJudged() {
        val files = appSources()
        assertTrue("前提：得真扫到源文件，否则这条测试是空跑", files.size > 20)

        val starters =
            files.filter { codeOnly(it.readText()).contains("startForegroundService") }.map { it.name }.toSet()
        assertEquals(
            "起保活服务的只许是「有活」那一处（不挂在哪一屏上：文件页下载 · 附件在传也得起得来）",
            setOf("KeepAliveWork.kt"),
            starters,
        )
        val app = files.single { it.name == "CcMonitorApp.kt" }
        assertTrue("应用起来时得挂上它", codeOnly(app.readText()).contains("startOnWork("))
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
