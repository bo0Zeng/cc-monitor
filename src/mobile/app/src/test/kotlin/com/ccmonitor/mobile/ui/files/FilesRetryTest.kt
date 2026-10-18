package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 文件页那一屏的［重试］与会话列表问题条的［重试］走同一条（`CoreLink.retry`：流断着 ⇒ 整份重接，通着 ⇒ 重问）。
 * 冒烟看到的那一处：那台记成「未装」（流停在要人动手那一步）以后，文件页的［重试］只重问一遍 ⇒ 核心当场回「离线」、一条命令都不发；
 * 会话列表那一条才重拨得回。
 *
 * 判别力边界：这一屏的视图模型连着 SSH 那层与核心，门禁里起不来（app 没引 Robolectric）；`CoreLink.retry` 那一判本身由
 * `ResidentFeedTest` 照行为钉。这里钉的是两处［重试］都经它、文件页那颗按钮接的是它。
 */
class FilesRetryTest {
    @Test
    fun `文件页的［重试］接的是视图模型的 retry，不是只重问那一下`() {
        val screen = code("src/main/kotlin/com/ccmonitor/mobile/ui/files/FilesScreen.kt")
        val at = screen.indexOf("\"front.act.retry\"")
        assertTrue("前提：文件页得找得到［重试］那颗按钮", at >= 0)
        val button = screen.substring(screen.lastIndexOf("SecondaryButton(", at), at)
        assertTrue(
            "文件页［重试］没走 vm.retry（只重问一遍、断在要人动手那一步时不重拨）：$button",
            button.contains("vm::retry") || button.contains("vm.retry()"),
        )
    }

    @Test
    fun `两处［重试］都经 CoreLink retry（一处实现）`() {
        val vm = bodyOf(code("src/main/kotlin/com/ccmonitor/mobile/ui/files/FilesViewModel.kt"), "fun retry(")
        assertTrue("文件页的 retry 没经 feed.retry(…)：$vm", vm.contains("feed.retry("))
        val chats = bodyOf(code("src/main/kotlin/com/ccmonitor/mobile/ui/chats/HostChats.kt"), "fun retry(")
        assertTrue("会话列表的 retry 没经 link.retry(…)：$chats", chats.contains("link.retry("))
    }

    /** 从 [head] 起到同一层下一个声明之前那一段（注释已剥）。 */
    private fun bodyOf(
        code: String,
        head: String,
    ): String {
        val at = code.indexOf(head)
        assertTrue("前提：得找得到 $head", at >= 0)
        val next = Regex("\n    (fun |val |private |override |internal )").find(code, at + head.length)?.range?.first ?: code.length
        return code.substring(at, next)
    }

    private fun code(relative: String): String =
        (File(relative).takeIf { it.isFile } ?: File("app/$relative")).let {
            assertTrue("前提：得找得到 $relative（实际找的是 ${it.absolutePath}）", it.isFile)
            KotlinSourceScanner.codeOnlyKeepingLiterals(it.readText())
        }
}
