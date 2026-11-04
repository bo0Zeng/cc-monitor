package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 一条会话的目录（⋯「打开工作目录」去的那一处，在它里面「新建会话」也起在那里）是会话帧上的项目目录 `projectDir`，
 * 不是进程起在哪的 `cwd`（那一格只拿来认「刚起的那条起来了」）。
 * ⋯ 那一项点了去哪：导航层开一个文件标签、落在那个目录（app 不引 Robolectric，点菜单那一下门禁里量不了，这里钉源码里那一条接线）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatOpenDirTest {
    private val file = FakeCoreLink.File(mutableListOf(human("u1", "你好")))

    @Test
    fun `会话帧上进程目录与项目目录不同 ⇒ 会话的目录是项目目录`() =
        runTest {
            val link = FakeCoreLink()
            link.report("s1", cwd = "/h/proj/sub", projectDir = "/h/proj")
            link.answer = { cmd, args -> if (cmd == "history-facts") Reply.LinkDown else file.answer(cmd, args) ?: Reply.LinkDown }
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }
            runCurrent()
            assertEquals("/h/proj", s.state.value.cwd)
        }

    @Test
    fun `⋯「打开工作目录」：有目录才出、字同桌面；导航层把它接到文件标签、落在那个目录`() {
        val route = KotlinSourceScanner.codeOnlyKeepingLiterals(source("src/main/kotlin/com/ccmonitor/mobile/ui/chat/ChatRoute.kt"))
        assertTrue("⋯ 里没有那一项（字 tabBarView.tab.cwdHint）", route.contains("copyText(\"tabBarView.tab.cwdHint\")"))
        assertTrue("目录还不知道也出了那一项", route.contains("openDir = state.cwd?.let"))
        val nav = KotlinSourceScanner.codeOnlyKeepingLiterals(source("src/main/kotlin/com/ccmonitor/mobile/ui/nav/AppNavHost.kt"))
        val at = nav.indexOf("onOpenDir =")
        assertTrue("导航层没接 onOpenDir", at >= 0)
        val line = nav.substring(at, nav.indexOf('\n', at))
        assertTrue("没开文件标签落在那个目录：$line", line.contains("TabTarget.Files") && line.contains("Screen.Session.of("))
    }

    private fun source(relative: String): String =
        (File(relative).takeIf { it.isFile } ?: File("app/$relative")).let {
            assertTrue("前提：得找得到 $relative（实际找的是 ${it.absolutePath}）", it.isFile)
            it.readText()
        }
}
